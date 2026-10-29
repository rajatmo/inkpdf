//! Stress tests: hostile and random input thrown at every layer that takes it, with invariants
//! checked as it goes. Seeded, so a failure replays exactly.
//!
//! Run them bigger with `INKPDF_STRESS=20 cargo test -p inkpdf stress -- --nocapture` (the number
//! multiplies how many events and iterations each test runs).

use std::cell::Cell;
use std::time::Instant;

use crate::doc::Doc;
use crate::editor::{Editor, Request};
use crate::figure::NodeAction;
use crate::geometry::Shape;
use crate::ink::{InkStore, P, PageStroke, Style};
use crate::input::{ButtonState, Device, Intent, MouseButton, PenButtons, Phase, PointerEvent, Sample, Wheel};
use crate::links::{Link, Target};
use crate::pages::PageEdit;
use crate::smooth::{PenFilter, curve_through};
use crate::testutil::sample_pdf;
use crate::tool::ToolAction;
use crate::tool::{MAX_WIDTH, MIN_WIDTH, Tool};
use crate::toolbar::{Channel, ContextItem, Item, Slider, Ui};
use crate::tools::{Dir, Key};
use crate::view::{Layout, MAX_ZOOM, MIN_ZOOM};

/// A small, fast, seedable random source (xorshift64*).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// A coordinate: mostly sensible, sometimes the kind of value that breaks arithmetic.
    pub fn coord(&mut self, extent: f32) -> f32 {
        match self.below(20) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => 1.0e9,
            4 => -1.0e9,
            5 => f32::MAX,
            6 => 0.0,
            7 => -0.0,
            8 => f32::MIN_POSITIVE,
            9 => -5.0,
            _ => self.range(0.0, extent),
        }
    }

    /// Like [`Rng::coord`] but always finite (what a real device reports).
    pub fn finite(&mut self, extent: f32) -> f32 {
        if self.chance(0.05) { self.pick(&[-1.0e6, 1.0e6, 0.0, -3.0]) } else { self.range(-0.1 * extent, 1.1 * extent) }
    }
}

/// How many times bigger to run, from `INKPDF_STRESS` (default 1).
/// The shell's part of an edit of the pages: the document follows what the editor did (as it asked), and the two have the
/// pages laid out alike.
fn follow_pages(ed: &mut Editor, doc: &mut Doc) {
    for edit in ed.requests.take_page_edits() {
        assert!(doc.apply(&edit), "the document follows {edit:?}");
    }
    let (a, b) = (doc.layout(), ed.layout.clone());
    assert_eq!(a.len(), b.len(), "the editor and the document have as many pages");
    for p in 0..a.len() {
        assert_eq!(a.page(p), b.page(p), "page {p} is laid out alike by both");
    }
}

fn scale() -> usize {
    std::env::var("INKPDF_STRESS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).max(1)
}

// ── the editor ───────────────────────────────────────────────────────────────────────────────

thread_local! {
    /// Blank pages the simulated shell added during a test, so a test can tell the pull really ran.
    static PAGES_ADDED: Cell<usize> = const { Cell::new(0) };
}

const PHASES: [Phase; 6] = [Phase::Down, Phase::Move, Phase::Up, Phase::Cancel, Phase::Hover, Phase::Leave];
const BUTTONS: [MouseButton; 4] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Other];

fn random_samples(rng: &mut Rng, size: [f32; 2], hostile: bool) -> Vec<Sample> {
    let n = rng.below(8);
    let t0 = rng.range(0.0, 1.0e6) as f64;
    (0..n)
        .map(|i| {
            let pos = if hostile { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.finite(size[0]), rng.finite(size[1])] };
            Sample { pos, pressure: rng.unit(), t_ms: t0 + i as f64 * rng.range(0.0, 20.0) as f64 }
        })
        .collect()
}

fn random_event(rng: &mut Rng, size: [f32; 2], hostile: bool) -> PointerEvent {
    let pen = rng.chance(0.5);
    PointerEvent {
        device: if pen { Device::Pen } else { Device::Touch },
        id: if rng.chance(0.9) { rng.below(4) as u32 } else { rng.next() as u32 },
        phase: rng.pick(&PHASES),
        samples: random_samples(rng, size, hostile),
        pen: if pen {
            PenButtons { in_range: rng.chance(0.9), contact: rng.chance(0.7), eraser: rng.chance(0.2), barrel: rng.chance(0.2) }
        } else {
            PenButtons::default()
        },
        contact_px: if rng.chance(0.9) { rng.range(5.0, 60.0) } else { rng.coord(500.0) },
    }
}

/// The pen at one spot, the pieces a tap or a drag is made of.
fn pen_at(phase: Phase, pos: [f32; 2]) -> PointerEvent {
    PointerEvent {
        device: Device::Pen,
        id: 1,
        phase,
        samples: vec![Sample { pos, pressure: 0.5, t_ms: 1000.0 }],
        pen: PenButtons { in_range: true, contact: phase != Phase::Up, eraser: false, barrel: false },
        contact_px: 0.0,
    }
}

/// What a finger needs to be taken seriously: no pen nearby (the palm guard goes by the clock, which
/// a fast test outruns) and no other finger half-way through something.
fn fresh_touch(ed: &mut Editor) {
    ed.palm = crate::input::PalmGuard::default();
    ed.fingers = crate::input::TouchGestures::default();
}

/// A whole, well-formed gesture, so that the interesting paths (drawing, erasing, lassoing, moving a
/// selection) are reached far more often than random events would reach them.
fn gesture(ed: &mut Editor, rng: &mut Rng) -> String {
    let size = ed.view.viewport;
    let mut at = || [rng.range(0.0, size[0]), rng.range(0.0, size[1])];
    let (start, end) = (at(), at());
    let steps = 2 + rng.below(25);
    let along = |i: usize| {
        let t = i as f32 / steps as f32;
        [start[0] + (end[0] - start[0]) * t + (t * 9.0).sin() * 6.0, start[1] + (end[1] - start[1]) * t + (t * 7.0).cos() * 6.0]
    };
    let sample = |i: usize, rng: &mut Rng| Sample { pos: along(i), pressure: rng.unit(), t_ms: 1000.0 + i as f64 * 4.0 };
    let kind = rng.below(8);
    let buttons = match kind {
        1 => PenButtons { in_range: true, contact: true, eraser: true, barrel: false },
        2 => PenButtons { in_range: true, contact: true, eraser: false, barrel: true },
        _ => PenButtons { in_range: true, contact: true, eraser: false, barrel: false },
    };
    match kind {
        0..=2 => {
            let event = |phase, samples: Vec<Sample>| PointerEvent { device: Device::Pen, id: 1, phase, samples, pen: buttons, contact_px: 0.0 };
            ed.pointer_event(event(Phase::Down, vec![sample(0, rng)]));
            for i in 1..steps {
                ed.pointer_event(event(Phase::Move, vec![sample(i, rng)]));
            }
            ed.pointer_event(event(Phase::Up, vec![sample(steps, rng)]));
        }
        3 => {
            fresh_touch(ed);
            let touch = |phase, i: usize| PointerEvent {
                device: Device::Touch,
                id: 2,
                phase,
                samples: vec![Sample { pos: along(i), pressure: 1.0, t_ms: crate::clock::now_ms() }],
                pen: PenButtons::default(),
                contact_px: 30.0,
            };
            ed.pointer_event(touch(Phase::Down, 0));
            for i in 1..steps {
                ed.pointer_event(touch(Phase::Move, i));
            }
            ed.pointer_event(touch(Phase::Up, steps));
        }
        6 => {
            // A finger tap, on a link when there is one.
            fresh_touch(ed);
            let at = ed.links.first().map_or(start, |l| ed.view.doc_to_screen([(l.rect[0] + l.rect[2]) / 2.0, (l.rect[1] + l.rect[3]) / 2.0]));
            let touch = |phase| PointerEvent {
                device: Device::Touch,
                id: 3,
                phase,
                samples: vec![Sample { pos: at, pressure: 1.0, t_ms: crate::clock::now_ms() }],
                pen: PenButtons::default(),
                contact_px: 30.0,
            };
            ed.pointer_event(touch(Phase::Down));
            ed.pointer_event(touch(Phase::Up));
        }
        7 => {
            // A finger pulling up at the end of the document.
            fresh_touch(ed);
            ed.pan_px(0.0, -1.0e7);
            let touch = |phase, y: f32| PointerEvent {
                device: Device::Touch,
                id: 4,
                phase,
                samples: vec![Sample { pos: [start[0], y], pressure: 1.0, t_ms: crate::clock::now_ms() }],
                pen: PenButtons::default(),
                contact_px: 30.0,
            };
            ed.pointer_event(touch(Phase::Down, size[1] * 0.9));
            for i in 1..=steps {
                ed.pointer_event(touch(Phase::Move, size[1] * 0.9 - 14.0 * i as f32));
            }
            ed.pointer_event(touch(Phase::Up, size[1] * 0.9 - 14.0 * steps as f32));
        }
        _ => {
            ed.mouse_moved(along(0));
            ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
            for i in 1..steps {
                ed.mouse_moved(along(i));
            }
            ed.mouse_button(ButtonState::Released, MouseButton::Left);
        }
    }
    format!("gesture {kind} {start:?} -> {end:?}")
}

/// Something done to the selected shape's anchors, if a shape is selected: one taken hold of and
/// dragged (anywhere, including far off the page), double tapped, its outline double tapped, or
/// pressed with the barrel button.
fn anchor_poke(ed: &mut Editor, rng: &mut Rng) -> String {
    let size = ed.view.viewport;
    let mut what = String::new();
    if let Some((_, figure)) = ed.editing() {
        let controls = figure.controls();
        let (control, at) = controls[rng.below(controls.len())];
        let handle = ed.view.doc_to_screen(at);
        let to = if rng.chance(0.2) { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.finite(size[0]), rng.finite(size[1])] };
        what = format!("the shape: {control:?} at {handle:?}");
        let barrel = |phase, pos| PointerEvent {
            device: Device::Pen,
            id: 1,
            phase,
            samples: vec![Sample { pos, pressure: 0.5, t_ms: 1000.0 }],
            pen: PenButtons { in_range: true, contact: phase != Phase::Up, eraser: false, barrel: true },
            contact_px: 0.0,
        };
        match rng.below(8) {
            0 | 1 => {
                ed.pointer_event(pen_at(Phase::Down, handle));
                ed.pointer_event(pen_at(Phase::Move, to));
                ed.pointer_event(pen_at(Phase::Up, to));
            }
            2 => {
                for _ in 0..2 {
                    ed.pointer_event(pen_at(Phase::Down, handle));
                    ed.pointer_event(pen_at(Phase::Up, handle));
                }
            }
            3 => {
                let on = figure.nearest_on_path(at).map_or(at, |o| o.at);
                let spot = ed.view.doc_to_screen(on);
                for _ in 0..2 {
                    ed.pointer_event(pen_at(Phase::Down, spot));
                    ed.pointer_event(pen_at(Phase::Up, spot));
                }
            }
            4 => {
                // The barrel button on the anchor: it is taken away (when the shape can spare it).
                ed.pointer_event(barrel(Phase::Down, handle));
                ed.pointer_event(barrel(Phase::Up, handle));
            }
            5 => {
                // The barrel button tapped on empty page: a node added to a run of lines.
                let spot = [rng.finite(size[0]), rng.finite(size[1])];
                ed.pointer_event(barrel(Phase::Down, spot));
                ed.pointer_event(barrel(Phase::Up, spot));
            }
            _ => {
                // One end of the shape dragged onto its other end (which closes a broken line).
                let n = figure.anchor_count();
                let (from, onto) = if rng.chance(0.5) { (0, n - 1) } else { (n - 1, 0) };
                if let (Some(a), Some(b)) = (figure.anchor_pos(from), figure.anchor_pos(onto)) {
                    let (a, b) = (ed.view.doc_to_screen(a), ed.view.doc_to_screen(b));
                    let near = [b[0] + rng.range(-3.0, 3.0), b[1] + rng.range(-3.0, 3.0)];
                    ed.pointer_event(pen_at(Phase::Down, a));
                    ed.pointer_event(pen_at(Phase::Move, near));
                    ed.pointer_event(pen_at(Phase::Up, near));
                }
            }
        }
    }
    what
}
/// The text tool and the keyboard: typing (anything, including what a keyboard should never send),
/// the pen tapping empty page or a text box, and a text box dragged about.
fn text_poke(ed: &mut Editor, rng: &mut Rng) -> String {
    let size = ed.view.viewport;
    let words = ["Hello", " ", "\n", "wörld", "✓", "日本語", "\u{7}", "\t", "\r\n", "😀", "", "  ", "x"];
    match rng.below(6) {
        0 | 1 => {
            let typed: String = (0..rng.below(8)).map(|_| rng.pick(&words)).collect();
            ed.type_text(&typed);
            format!("type {typed:?}")
        }
        2 => {
            if rng.chance(0.5) {
                ed.backspace()
            } else {
                ed.finish_text()
            }
            "backspace or finish".into()
        }
        3 => {
            // A huge paste.
            if rng.chance(0.1) {
                ed.type_text(&"w".repeat(rng.below(3 * crate::text::MAX_CHARS)));
            }
            "a long text".into()
        }
        _ => {
            // The mouse on an existing box, or anywhere.
            let boxes: Vec<[f32; 4]> = ed.ink.strokes().iter().filter(|s| s.text.is_some()).map(|s| s.bbox()).collect();
            let at = match boxes.len() {
                0 => [rng.range(0.0, size[0]), rng.range(0.0, size[1])],
                n => {
                    let b = boxes[rng.below(n)];
                    ed.view.doc_to_screen([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0])
                }
            };
            let to = [at[0] + rng.range(-200.0, 200.0), at[1] + rng.range(-200.0, 200.0)];
            // (And, now and then, the stylus too, which does nothing to the page with the text tool.)
            match rng.below(4) {
                0 => {
                    ed.pointer_event(pen_at(Phase::Down, at));
                    ed.pointer_event(pen_at(Phase::Up, at));
                }
                1 => pen_drag(ed, at, to, 1 + rng.below(6)),
                2 => mouse_click(ed, at),
                _ => mouse_drag(ed, at, to, 1 + rng.below(6)),
            }
            format!("the mouse or pen at {at:?} to {to:?}")
        }
    }
}

/// One random thing a user, a device or the system could do.
fn poke(ed: &mut Editor, rng: &mut Rng, hostile: bool) -> String {
    let size = ed.view.viewport;
    let mut what = String::new();
    // The machine is turned into a tablet or back, the window loses the focus, a shortcut is pressed.
    if rng.chance(0.02) {
        let mode = rng.pick(&[crate::input::Mode::Desktop, crate::input::Mode::Tablet]);
        ed.set_mode(mode);
        return format!("mode {mode:?}");
    }
    if rng.chance(0.01) {
        ed.focus_lost();
        return "focus lost".into();
    }
    if rng.chance(0.03) {
        let (letter, shift) = (rng.pick(&["z", "y", "s", "q", "a", "c", "x", "v", "d"]), rng.chance(0.3));
        ed.shortcut(letter, shift);
        return format!("shortcut {letter} shift {shift}");
    }
    if rng.chance(0.3) {
        return gesture(ed, rng);
    }
    match rng.below(36) {
        32 | 33 => {
            // A button of the bar under the tools (select, erase, delete, copy, paste, a shape's points, the style).
            let items = [
                ContextItem::Intent(Intent::Draw),
                ContextItem::Intent(Intent::Select),
                ContextItem::Intent(Intent::Erase),
                ContextItem::Duplicate,
                ContextItem::Copy,
                ContextItem::Paste,
                ContextItem::Delete,
                ContextItem::Action(ToolAction::Node(NodeAction::Remove)),
                ContextItem::Action(ToolAction::Node(NodeAction::Add)),
                ContextItem::Action(ToolAction::Node(NodeAction::Smooth)),
                ContextItem::Style,
            ];
            if let Some(bar) = ed.context_bar() {
                let item = rng.pick(&items);
                let at = bar.centre_of(item);
                what = format!("tap {item:?} on the bar at {at:?}");
                ed.pointer_event(pen_at(Phase::Down, at));
                ed.pointer_event(pen_at(Phase::Up, at));
            }
        }
        34 => {
            // A key: the arrows, Delete, Home and End, Ctrl and a letter, a letter, with or without Shift.
            let shift = rng.chance(0.4);
            let key = match rng.below(9) {
                0 => Key::Delete,
                1 => Key::Backspace,
                2 => Key::Escape,
                3 => Key::Arrow(rng.pick(&[Dir::Left, Dir::Right, Dir::Up, Dir::Down]), shift),
                4 => Key::Home(shift),
                5 => Key::End(shift),
                6 => Key::Chord(rng.pick(&['a', 'c', 'x', 'v', 'q'])),
                7 => Key::Text("q é\n"),
                _ => Key::Text("word "),
            };
            what = format!("key {key:?}");
            ed.press(key);
        }
        35 => {
            // Words arrive on the system clipboard from another program.
            let words = rng.pick(&["from elsewhere", "two\nlines", "tab\there", "   ", ""]);
            what = format!("clipboard words {words:?}");
            ed.text_clipboard.set(words);
        }
        29 | 30 => what = anchor_poke(ed, rng),
        31 => what = text_poke(ed, rng),
        26 => {
            // A tap on a toolbar button: the tools, the document's buttons, or full screen.
            let items = [
                Item::Close,
                Item::Undo,
                Item::Redo,
                Item::Save,
                Item::Tool(Tool::Pen),
                Item::Tool(Tool::Geometry),
                Item::Tool(Tool::Text),
                Item::Tool(Tool::Text),
                Item::Tool(Tool::Laser),
                Item::Fullscreen,
                Item::AddPage,
                Item::DeletePage,
                Item::ModeSwitch,
            ];
            let at = ed.toolbar().centre_of(rng.pick(&items));
            what = format!("tap the toolbar at {at:?}");
            ed.pointer_event(pen_at(Phase::Down, at));
            ed.pointer_event(pen_at(Phase::Up, at));
        }
        27 | 28 => {
            // Something in the pen panel, when it is open: a slider dragged or a colour tapped.
            let editing = ed.chrome.panel.is_some_and(|s| s.editing.is_some());
            if let Some(p) = ed.toolbar().tool_panel(ed.tool, ed.view.viewport[0], editing).filter(|_| ed.chrome.panel.is_some()) {
                let sliders = [
                    Slider::Width,
                    Slider::Smoothing,
                    Slider::Channel(Channel::Hue),
                    Slider::Channel(Channel::Saturation),
                    Slider::Channel(Channel::Value),
                ];
                let (from, to) = (rng.unit(), rng.unit());
                let (start, end) = if rng.chance(0.4) {
                    let at = if ed.tool == Tool::Geometry && rng.chance(0.5) {
                        p.shape_centre(rng.pick(&crate::geometry::SHAPES))
                    } else {
                        p.swatch_centre(rng.below(5))
                    };
                    (at, at)
                } else {
                    let slider = rng.pick(&sliders);
                    (p.point_on(slider, from), p.point_on(slider, to))
                };
                what = format!("the panel: {start:?} -> {end:?}");
                ed.pointer_event(pen_at(Phase::Down, start));
                ed.pointer_event(pen_at(Phase::Move, end));
                ed.pointer_event(pen_at(Phase::Up, end));
            }
        }
        0..=7 => {
            let ev = random_event(rng, size, hostile);
            what = format!("{ev:?}");
            ed.pointer_event(ev);
        }
        8 | 9 => {
            let pos = if hostile { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.finite(size[0]), rng.finite(size[1])] };
            what = format!("mouse_moved {pos:?}");
            ed.mouse_moved(pos);
        }
        10 | 11 => ed.mouse_button(if rng.chance(0.5) { ButtonState::Pressed } else { ButtonState::Released }, rng.pick(&BUTTONS)),
        12 => {
            let (x, y) = if hostile { (rng.coord(10.0), rng.coord(10.0)) } else { (rng.finite(10.0), rng.finite(10.0)) };
            what = format!("wheel {x} {y}");
            ed.wheel(if rng.chance(0.5) { Wheel::Lines(x, y) } else { Wheel::Pixels(x * 30.0, y * 30.0) });
        }
        13 | 14 => {
            // The mouse presses and releases wherever it is, with any button, while a tool is held.
            let (x, y) = (rng.finite(size[0]), rng.finite(size[1]));
            ed.mouse_moved([x, y]);
            ed.mouse_button(ButtonState::Pressed, rng.pick(&BUTTONS));
            ed.mouse_moved([x + rng.range(-50.0, 50.0), y + rng.range(-50.0, 50.0)]);
            ed.mouse_button(ButtonState::Released, rng.pick(&BUTTONS));
        }
        15 => {
            let size = [rng.pick(&[0.0, 1.0, 50.0, 480.0, 1100.0, 3840.0, 20000.0]), rng.pick(&[0.0, 1.0, 50.0, 600.0, 900.0, 2160.0, 20000.0])];
            what = format!("set_viewport {size:?}");
            ed.set_viewport(size);
        }
        16 => ed.dpr = rng.pick(&[0.5, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0]),
        17 => ed.mouse_left(),
        18 => {
            let phase = rng.pick(&[Phase::Down, Phase::Move, Phase::Up, Phase::Cancel]);
            let (id, pos) =
                (rng.below(4) as u32, if hostile { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.finite(size[0]), rng.finite(size[1])] });
            what = format!("touch from the toolkit {phase:?} {id} {pos:?}");
            ed.pointer_event(PointerEvent {
                device: Device::Touch,
                id,
                phase,
                samples: vec![Sample { pos, pressure: 1.0, t_ms: crate::clock::now_ms() }],
                pen: PenButtons::default(),
                contact_px: 0.0,
            });
        }
        19 => {
            let dt = rng.range(0.0, 0.1);
            what = format!("step {dt}");
            ed.step_view(dt);
        }
        20 if rng.chance(0.1) => {
            let pages = rng.below(6);
            let sizes: Vec<[f32; 2]> =
                (0..pages).map(|_| [rng.pick(&[0.0, 1.0, 100.0, 612.0, 5000.0]), rng.pick(&[0.0, 1.0, 100.0, 792.0, 5000.0])]).collect();
            what = format!("load {sizes:?}");
            ed.load(Layout::new(&sizes), vec![]);
        }
        21 => {
            // The shell servicing what the editor asked for.
            ed.requests.take(&Request::Save);
            ed.requests.take(&Request::Fullscreen);
            ed.redraw = false;
            ed.requests.take(&Request::SettingsChanged);
            for edit in ed.requests.take_page_edits() {
                match edit {
                    PageEdit::Insert { at, .. } => {
                        what = format!("add a page: now {} (at {at})", ed.layout.len());
                        PAGES_ADDED.with(|n| n.set(n.get() + 1));
                    }
                    other => what = format!("page edit by the shell: {other:?}"),
                }
            }
            if std::mem::take(&mut ed.requests.has(&Request::Close)) {
                what = "close: a blank document".into();
                ed.load(Layout::new(&[[595.0, 842.0]]), vec![]);
            }
            ed.requests.clear_urls();
        }
        22 => ed.chrome.fullscreen = rng.chance(0.5),
        24 if rng.chance(0.2) => {
            what = "the shell asks about unsaved work".into();
            ed.ask_unsaved();
        }
        25 => {
            // The shell acting on an answer, or the person dismissing the prompt another way.
            if let Some(choice) = ed.requests.take_answer() {
                what = format!("the shell takes the answer {choice:?}");
            } else if ed.chrome.prompt.is_some() && rng.chance(0.4) {
                what = "the shell drops the prompt".into();
                ed.chrome.prompt = None;
            }
        }
        23 => {
            // Links anywhere, some of them nonsense.
            ed.links = (0..rng.below(8))
                .map(|_| Link {
                    rect: [rng.coord(1000.0), rng.coord(2000.0), rng.coord(1000.0), rng.coord(2000.0)],
                    target: if rng.chance(0.5) { Target::Page(rng.below(12)) } else { Target::Url("https://example.com/".into()) },
                })
                .collect();
            what = format!("{} links", ed.links.len());
        }
        _ => {}
    }
    what
}

/// What must hold after any sequence of events.
/// The cheap part, run after every event, so a failure names the event that caused it.
fn check_view(ed: &Editor, what: &str, event: &str) {
    let v = &ed.view;
    assert!(
        v.zoom.is_finite() && v.scroll.iter().all(|c| c.is_finite()),
        "{what}: after {event}: zoom {} scroll {:?} (viewport {:?}, layout {}x{}, dpr {})",
        v.zoom,
        v.scroll,
        v.viewport,
        ed.layout.width(),
        ed.layout.height(),
        ed.dpr
    );
    assert!(
        ed.selection.iter().all(|id| ed.ink.get(*id).is_some()),
        "{what}: after {event}: a selected stroke does not exist ({:?}, act {:?})",
        ed.selection,
        ed.chrome.act
    );
}

/// What each kind of shape promises, whatever was done to it: straight lines are exactly their nodes
/// (nothing bends them), a rectangle is axis-aligned and its diagonal's ends are its anchors, an
/// ellipse fills its bounding rectangle, axes are straight lines through their crossing.
fn check_kind(figure: &crate::figure::Figure, pts: &[P], what: &str) {
    use crate::figure::Figure;
    let near = |a: P, b: P| (a[0] - b[0]).abs() <= 1e-3 * (1.0 + a[0].abs()) && (a[1] - b[1]).abs() <= 1e-3 * (1.0 + a[1].abs());
    match figure {
        Figure::Poly { points, closed } => {
            assert!(
                pts.len() == points.len() + usize::from(*closed) && points.iter().zip(pts).all(|(a, b)| near(*a, *b)),
                "{what}: straight lines were bent: {figure:?} vs {pts:?}"
            );
            assert!(!*closed || near(pts[0], pts[pts.len() - 1]), "{what}: a polygon that does not close");
        }
        Figure::Rect { a, b } => {
            assert!(pts.len() == 5 && near(pts[0], *a) && near(pts[2], *b), "{what}: a rectangle that is not the box of its diagonal: {pts:?}");
            assert!(pts.windows(2).all(|w| w[0][0] == w[1][0] || w[0][1] == w[1][1]), "{what}: a rectangle with a slanted edge: {pts:?}");
        }
        Figure::Ellipse { a, b } => {
            let (lo, hi) = ([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]);
            let tol = 1e-2 * (1.0 + (hi[0] - lo[0]).abs().max((hi[1] - lo[1]).abs()));
            let bb = pts
                .iter()
                .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |bb, p| [bb[0].min(p[0]), bb[1].min(p[1]), bb[2].max(p[0]), bb[3].max(p[1])]);
            assert!(
                (bb[0] - lo[0]).abs() <= tol && (bb[1] - lo[1]).abs() <= tol && (bb[2] - hi[0]).abs() <= tol && (bb[3] - hi[1]).abs() <= tol,
                "{what}: an ellipse that does not fill its box: {bb:?} vs {lo:?} {hi:?}"
            );
        }
        Figure::Axes { origin, ends } => {
            // Every axis is a straight line through the crossing; with two they meet at a right angle.
            let dirs: Vec<P> = ends.iter().map(|e| [e[1][0] - e[0][0], e[1][1] - e[0][1]]).collect();
            for (e, d) in ends.iter().zip(&dirs) {
                let len = d[0].hypot(d[1]);
                let off = (e[0][0] - origin[0]) * d[1] / len.max(1e-6) - (e[0][1] - origin[1]) * d[0] / len.max(1e-6);
                assert!(len < 1e-3 || off.abs() <= 1e-2 * (1.0 + len), "{what}: an axis that misses the crossing by {off}: {figure:?}");
            }
            if let [x, y] = &dirs[..] {
                let (lx, ly) = (x[0].hypot(x[1]), y[0].hypot(y[1]));
                assert!(
                    lx < 1e-3 || ly < 1e-3 || (x[0] * y[0] + x[1] * y[1]).abs() <= 1e-3 * lx * ly,
                    "{what}: axes that are not perpendicular: {figure:?}"
                );
            }
        }
        Figure::Path { .. } => {}
    }
}

fn check_editor(ed: &Editor, what: &str) {
    let v = &ed.view;
    assert!(v.zoom.is_finite() && (MIN_ZOOM..=MAX_ZOOM).contains(&v.zoom), "{what}: zoom {}", v.zoom);
    assert!(v.scroll.iter().all(|c| c.is_finite()), "{what}: scroll {:?}", v.scroll);
    assert!(v.viewport.iter().all(|c| c.is_finite() && *c >= 1.0), "{what}: viewport {:?}", v.viewport);
    for s in ed.ink.strokes() {
        assert!(!s.pts.is_empty() && s.pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{what}: stroke {} has bad points", s.id);
        assert!(s.style.width.is_finite() && (s.style.width > 0.0 || s.text.is_some()), "{what}: stroke {} width {}", s.id, s.style.width);
        if let Some(block) = &s.text {
            let [w, h] = crate::text::measure(&block.text, block.size);
            let (first, corner) = (s.pts.first().copied().unwrap_or_default(), s.pts.get(2).copied().unwrap_or_default());
            assert!(
                !block.text.trim().is_empty() && block.text.chars().count() <= crate::text::MAX_CHARS,
                "{what}: text box {} holds {:?}",
                s.id,
                block.text
            );
            assert!(block.size.is_finite() && block.size > 0.0 && block.size <= 400.0, "{what}: text size {}", block.size);
            assert!(s.pts.len() == 5 && s.pts[0] == s.pts[4], "{what}: text box {} is not a closed rectangle: {:?}", s.id, s.pts);
            assert!(
                (corner[0] - first[0] - w).abs() < 0.05 + 1e-4 * w && (corner[1] - first[1] - h).abs() < 0.05 + 1e-4 * h,
                "{what}: text box {} is not the size of its text",
                s.id
            );
        }
    }
    if let Some(draft) = ed.draft() {
        assert!(ed.tool == Tool::Geometry, "{what}: drawing a stroke by taps with the {:?} tool", ed.tool);
        assert!(!draft.nodes.is_empty() && draft.nodes.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{what}: a bad draft {draft:?}");
        assert!((draft.page as usize) < ed.layout.len(), "{what}: drawing on page {} of {}", draft.page, ed.layout.len());
        assert!(
            ed.draft_preview().is_some_and(|(outline, nodes)| !nodes.is_empty() && outline.iter().all(|p| p[0].is_finite() && p[1].is_finite())),
            "{what}: a draft that cannot be drawn"
        );
    }
    if let Some(edit) = ed.text_edit() {
        assert!(ed.tool == Tool::Text, "{what}: typing with the {:?} tool", ed.tool);
        assert!(
            edit.text.chars().count() <= crate::text::MAX_CHARS && edit.top_left.iter().all(|c| c.is_finite()) && edit.size.is_finite(),
            "{what}: a bad text edit {edit:?}"
        );
        assert!(edit.target.is_none_or(|id| ed.ink.get(id).is_some_and(|s| s.text.is_some())), "{what}: editing a box that does not exist");
        let chars = edit.text.chars().count();
        assert!(
            edit.caret <= chars && edit.anchor.is_none_or(|a| a <= chars),
            "{what}: the caret {} or selection {:?} is outside {chars} characters",
            edit.caret,
            edit.anchor
        );
        assert!((edit.page as usize) < ed.layout.len(), "{what}: editing on page {} of {}", edit.page, ed.layout.len());
        // Typed text stays within its page's right edge (a space may hang over it).
        if let Some(page) = ed.layout.page(edit.page as usize).filter(|p| p.w >= 100.0 * edit.size.max(1.0) / 20.0) {
            let [w, _] = crate::text::measure(&edit.text, edit.size);
            assert!(
                edit.top_left[0] + w <= page.x + page.w + 0.4 * edit.size + 0.01 && edit.top_left[0] >= page.x - 0.01,
                "{what}: typed text runs off the page: left {} width {w} page {}..{}",
                edit.top_left[0],
                page.x,
                page.x + page.w
            );
        }
    }
    for id in &ed.selection {
        assert!(ed.ink.get(*id).is_some(), "{what}: selected stroke {id} does not exist");
    }
    for style in [ed.pen, ed.shape_style] {
        assert!(
            style.width.is_finite() && (MIN_WIDTH..=MAX_WIDTH).contains(&style.width) && style.colour < ed.palette.len(),
            "{what}: style {style:?}"
        );
    }
    let text = ed.text_style;
    assert!(
        text.width.is_finite() && (crate::tool::TEXT_SIZES.0..=crate::tool::TEXT_SIZES.1).contains(&text.width) && text.colour < ed.palette.len(),
        "{what}: text style {text:?}"
    );
    assert!((0.0..=1.0).contains(&ed.smoothing), "{what}: smoothing {}", ed.smoothing);
    assert!(ed.chrome.colour_hsv.iter().all(|c| c.is_finite()), "{what}: hsv {:?}", ed.chrome.colour_hsv);
    assert!(ed.tool != crate::tool::Tool::Laser || ed.chrome.panel.is_none(), "{what}: a panel is open with the laser chosen");
    for s in ed.ink.strokes() {
        if let Some(figure) = &s.figure {
            let drawn = figure.points();
            let close =
                |a: &[f32; 2], b: &[f32; 2]| (a[0] - b[0]).abs() <= 1e-3 * (1.0 + a[0].abs()) && (a[1] - b[1]).abs() <= 1e-3 * (1.0 + a[1].abs());
            let worst = drawn.iter().zip(&s.pts).enumerate().find(|(_, (a, b))| !close(a, b)).map(|(i, (a, b))| (i, *a, *b));
            assert!(
                figure.valid() && drawn.len() == s.pts.len() && worst.is_none(),
                "{what}: stroke {} no longer matches its shape: drawn {} points vs stored {}; first difference {worst:?}; figure {figure:?}",
                s.id,
                drawn.len(),
                s.pts.len()
            );
            check_kind(figure, &s.pts, what);
        }
    }
    if let Some((id, figure)) = ed.editing() {
        assert!(ed.selection == [id] && figure.valid(), "{what}: editing something that is not selected");
        assert!(ed.selected_anchor().is_none_or(|i| i < figure.anchor_count()), "{what}: a selected anchor that is not there");
    }
    // What would be written for the next session reads back as the same choices.
    let saved = ed.settings();
    let back = crate::settings::Settings::parse(&saved.to_text());
    assert_eq!(
        (back.tool, back.shape, back.pen.colour, back.shape_style.colour, back.text_style.colour, back.palette),
        (saved.tool, saved.shape, saved.pen.colour, saved.shape_style.colour, saved.text_style.colour, saved.palette),
        "{what}: settings do not survive a restart"
    );
    assert!(
        (back.pen.width - saved.pen.width).abs() <= 0.006
            && (back.shape_style.width - saved.shape_style.width).abs() <= 0.006
            && (back.text_style.width - saved.text_style.width).abs() <= 0.006
            && (back.smoothing - saved.smoothing).abs() <= 0.006,
        "{what}: settings drifted"
    );
    assert!(ed.laser().head().is_none_or(|h| h[0].is_finite() && h[1].is_finite()), "{what}: laser head {:?}", ed.laser().head());
    assert!(
        ed.laser().pieces(1.0e6).all(|(a, b, life)| a.iter().chain(&b).all(|c| c.is_finite()) && (0.0..=1.0).contains(&life)),
        "{what}: a bad laser trail"
    );
    assert!(!(ed.chrome.prompt.is_some() && ed.requests.answer().is_some()), "{what}: a question and its answer at once");
    assert!(ed.pull.is_finite() && ed.pull >= 0.0 && (0.0..=1.0).contains(&ed.pull_progress()), "{what}: pull {}", ed.pull);
    // What the shell draws from it must not blow up either.
    let mut ui = Ui::default();
    ed.toolbar().draw(&ed.toolbar_state(), &mut ui);
    if let (Some(view), Some(p)) =
        (ed.panel_view(), ed.toolbar().tool_panel(ed.tool, ed.view.viewport[0], ed.chrome.panel.is_some_and(|s| s.editing.is_some())))
    {
        p.draw(&view, &mut ui);
    }
    assert!(
        ui.lines.iter().all(|c| c.a.iter().chain(&c.b).chain(&[c.width]).all(|x| x.is_finite())),
        "{what}: the interface drew a non-finite shape"
    );
}

fn editor_with_pages(pages: usize) -> Editor {
    let mut ed = Editor::new([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&vec![[612.0, 792.0]; pages]), vec![]);
    ed
}

#[test]
fn stress_the_editor_survives_a_million_hostile_events_and_keeps_its_invariants() {
    let events = 60_000 * scale();
    for seed in 1..=4u64 {
        let mut rng = Rng::new(seed);
        let mut ed = editor_with_pages(1 + seed as usize);
        for i in 0..events {
            let event = poke(&mut ed, &mut rng, true);
            check_view(&ed, &format!("seed {seed}, event {i}"), &event);
            if i % 500 == 0 {
                check_editor(&ed, &format!("seed {seed}, event {i}"));
            }
        }
        check_editor(&ed, &format!("seed {seed}, end"));
    }
}

#[test]
fn stress_realistic_input_draws_lots_of_ink_that_always_undoes_cleanly() {
    // Events a device could really send (finite, in and around the window): strokes get made, erased,
    // selected and moved. Whatever happens, undoing everything must leave an empty, clean page.
    let mut rng = Rng::new(99);
    let mut ed = editor_with_pages(3);
    PAGES_ADDED.with(|n| n.set(0));
    let (mut created, mut last_len) = (0usize, 0usize);
    for i in 0..(80_000 * scale()) {
        let event = poke(&mut ed, &mut rng, false);
        created += ed.ink.len().saturating_sub(last_len);
        last_len = ed.ink.len();
        check_view(&ed, &format!("event {i}"), &event);
        if i % 1000 == 0 {
            check_editor(&ed, &format!("event {i}"));
        }
    }
    ed.pointer_event(PointerEvent { device: Device::Pen, id: 0, phase: Phase::Cancel, samples: vec![], pen: PenButtons::default(), contact_px: 0.0 });
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    eprintln!("strokes made during the storm: {created}; alive at the end: {}", ed.ink.len());
    assert!(created > 300, "the storm should have drawn plenty of ink ({created})");
    let added = PAGES_ADDED.with(Cell::get);
    eprintln!("blank pages added by pulling: {added}");
    assert!(added > 5, "the storm should have pulled in new pages ({added})");
    let mut steps = 0;
    while ed.ink.can_undo() && steps < 100_000 {
        ed.undo();
        steps += 1;
    }
    check_editor(&ed, "after undoing everything");
    if steps < 500 {
        assert_eq!(ed.ink.len(), 0, "undoing every step leaves no ink (took {steps} steps)");
    }
}

// ── the ink store ────────────────────────────────────────────────────────────────────────────

fn random_stroke(rng: &mut Rng, pages: u32) -> (u32, Style, Vec<P>) {
    let n = 1 + rng.below(40);
    let (mut x, mut y) = (rng.range(0.0, 600.0), rng.range(0.0, 2000.0));
    let pts = (0..n)
        .map(|_| {
            x += rng.range(-8.0, 8.0);
            y += rng.range(-8.0, 8.0);
            [x, y]
        })
        .collect();
    (rng.below(pages as usize) as u32, Style { rgba: [rng.below(256) as u8, 0, 0, 255], width: rng.range(0.25, 24.0) }, pts)
}

/// The same strokes, to within the rounding a move and its undo leave behind.
fn same_ink(a: &[crate::ink::Stroke], b: &[crate::ink::Stroke]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.id == y.id
                && x.page == y.page
                && x.style == y.style
                && x.pts.len() == y.pts.len()
                && x.pts.iter().zip(&y.pts).all(|(p, q)| (p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3)
        })
}

#[test]
fn stress_the_ink_store_undoes_and_redoes_exactly_whatever_was_done() {
    for seed in 1..=20u64 {
        let mut rng = Rng::new(seed);
        let mut store = InkStore::new();
        let initial: Vec<PageStroke> = (0..rng.below(30))
            .map(|_| {
                let (page, style, pts) = random_stroke(&mut rng, 3);
                (page, style, pts, None, None)
            })
            .collect();
        store.load(initial);
        let start = store.strokes().to_vec();
        let ops = 100 + rng.below(200 * scale());
        for _ in 0..ops {
            let ids: Vec<u64> = store.strokes().iter().map(|s| s.id).collect();
            match rng.below(7) {
                0 | 1 => {
                    let (page, style, pts) = random_stroke(&mut rng, 3);
                    store.add(page, style, pts);
                }
                2 if !ids.is_empty() => {
                    let pick: Vec<u64> = (0..1 + rng.below(3)).map(|_| rng.pick(&ids)).collect();
                    store.remove(&pick);
                }
                3 if !ids.is_empty() => {
                    let pick: Vec<u64> = (0..1 + rng.below(3)).map(|_| rng.pick(&ids)).collect();
                    let to_page = rng.chance(0.3).then(|| rng.below(3) as u32);
                    store.move_by(&pick, rng.range(-50.0, 50.0), rng.range(-50.0, 50.0), to_page);
                }
                4 => {
                    store.undo();
                }
                5 => {
                    store.redo();
                }
                _ => {
                    // Hostile points are refused, not stored.
                    store.add(0, Style { rgba: [0; 4], width: 1.0 }, vec![[f32::NAN, 1.0], [f32::INFINITY, 2.0]]);
                }
            }
            assert!(store.dirty_pages().iter().all(|&p| p < 3), "dirty pages stay real pages");
        }
        // Rewind completely, then replay completely.
        let end = store.strokes().to_vec();
        let mut undone = 0;
        while store.undo() {
            undone += 1;
        }
        // The store keeps the last 500 steps. A run with fewer operations than that lost none of them
        // (a long run that undid its way back below 500 steps may still have dropped its oldest).
        if ops < 500 {
            assert!(same_ink(store.strokes(), &start), "seed {seed}: undoing everything restores the start");
            // Redo exactly the steps just undone (an earlier undo may have left more redo behind).
            for _ in 0..undone {
                assert!(store.redo(), "seed {seed}: a step that was undone can be redone");
            }
            assert!(same_ink(store.strokes(), &end), "seed {seed}: redoing them restores the end");
        }
    }
}

#[test]
fn stress_a_huge_drawing_stays_fast() {
    let mut rng = Rng::new(5);
    let mut store = InkStore::new();
    let (strokes, per_page) = (20_000 * scale().min(5), 50);
    let t = Instant::now();
    for i in 0..strokes {
        let (_, style, pts) = random_stroke(&mut rng, 1);
        store.add((i / per_page) as u32, style, pts);
    }
    let added = t.elapsed();
    let t = Instant::now();
    let mut hits = 0;
    for _ in 0..2000 {
        hits += usize::from(store.hit_test([rng.range(0.0, 600.0), rng.range(0.0, 2000.0)], 5.0).is_some());
    }
    let hit_test = t.elapsed() / 2000;
    let t = Instant::now();
    let many = store.in_polygon(&[[0.0, 0.0], [600.0, 0.0], [600.0, 2000.0], [0.0, 2000.0]]).len();
    let lassoed = t.elapsed();
    eprintln!("{strokes} strokes: add {added:?}; hit test {hit_test:?} each ({hits} hit); lasso of {many} took {lassoed:?}");
    assert!(hit_test.as_micros() < 5_000, "a hit test stays well under a frame: {hit_test:?}");
    assert!(added.as_secs() < 5, "{added:?}");
}

// ── smoothing ────────────────────────────────────────────────────────────────────────────────

#[test]
fn stress_smoothing_never_panics_and_stays_finite_for_finite_input() {
    let mut rng = Rng::new(7);
    for _ in 0..(300 * scale()) {
        let amount = rng.pick(&[f32::NAN, -1.0, 0.0, 0.3, 0.5, 1.0, 5.0, f32::INFINITY]);
        let mut filter = PenFilter::new(amount);
        let mut t = rng.range(0.0, 1.0e6) as f64;
        let finite_only = rng.chance(0.7);
        let mut out = Vec::new();
        let mut raw = Vec::new();
        for _ in 0..rng.below(300) {
            t += rng.pick(&[0.0, 0.0, 1.0, 4.0, 16.0, 500.0, -3.0]);
            let p = if finite_only { [rng.finite(1000.0), rng.finite(1000.0)] } else { [rng.coord(1000.0), rng.coord(1000.0)] };
            raw.push(p);
            out.push(filter.filter(p, t));
        }
        if finite_only && amount.is_finite() {
            assert!(out.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "amount {amount}: the filter produced a non-finite point");
        }
        let curve = curve_through(&raw, rng.pick(&[0.75, 0.0, -1.0, 1.0e-9, f32::NAN, 1.0e6]));
        assert!(curve.len() <= raw.len() * 25 + 2, "a curve through {} points made {} points", raw.len(), curve.len());
        if finite_only {
            assert!(curve.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        }
    }
}

// ── files ────────────────────────────────────────────────────────────────────────────────────

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("inkpdf-stress-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn stress_corrupted_pdfs_are_an_error_or_a_document_never_a_crash() {
    let dir = temp_dir("corrupt");
    let seeds = [sample_pdf(), crate::testutil::pdf_with_links()];
    let mut rng = Rng::new(11);
    let (mut opened, mut refused) = (0, 0);
    for i in 0..(150 * scale()) {
        let mut bytes = seeds[i % seeds.len()].clone();
        match rng.below(5) {
            0 => {
                for _ in 0..1 + rng.below(30) {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.next() as u8;
                }
            }
            1 => bytes.truncate(rng.below(bytes.len())),
            2 => {
                let (a, b) = (rng.below(bytes.len()), rng.below(bytes.len()));
                let (a, b) = (a.min(b), a.max(b));
                let chunk = bytes[a..b].to_vec();
                let at = rng.below(bytes.len() + 1);
                bytes.splice(at..at, chunk);
            }
            3 => bytes = (0..rng.below(2000)).map(|_| rng.next() as u8).collect(),
            _ => {
                let at = rng.below(bytes.len());
                bytes.drain(at..(at + rng.below(200)).min(bytes.len()));
            }
        }
        let path = dir.join(format!("c{i}.pdf"));
        std::fs::write(&path, &bytes).expect("write");
        match Doc::open(&path) {
            Ok((mut doc, _strokes)) => {
                opened += 1;
                // Asking for pages from a damaged file must not take the process down either.
                let mut store = InkStore::new();
                store.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[50.0, 50.0], [90.0, 90.0]]);
                let _ = doc.save(&store);
            }
            Err(_) => refused += 1,
        }
    }
    eprintln!("corrupted files: {opened} opened, {refused} refused");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_saving_and_reopening_keeps_every_stroke() {
    let dir = temp_dir("roundtrip");
    let mut rng = Rng::new(21);
    for round in 0..(8 * scale()) {
        let path = dir.join(format!("r{round}.pdf"));
        std::fs::write(&path, sample_pdf()).expect("write");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let mut store = InkStore::new();
        let count = 1 + rng.below(300);
        let page = doc.layout().page(0).expect("page");
        for _ in 0..count {
            let n = 2 + rng.below(30);
            let (mut x, mut y) = (page.x + rng.range(20.0, 500.0), page.y + rng.range(20.0, 700.0));
            let pts: Vec<P> = (0..n)
                .map(|_| {
                    x = (x + rng.range(-6.0, 6.0)).clamp(page.x + 5.0, page.x + 600.0);
                    y = (y + rng.range(-6.0, 6.0)).clamp(page.y + 5.0, page.y + 780.0);
                    [x, y]
                })
                .collect();
            store.add(0, Style { rgba: [rng.below(256) as u8, rng.below(256) as u8, 0, 255], width: rng.range(0.5, 12.0) }, pts);
        }
        doc.save(&store).expect("save");
        let (_, back) = Doc::open(&path).expect("reopen");
        assert_eq!(back.len(), store.len(), "round {round}: every stroke survives a save and reopen");
        // A second save after erasing half replaces, not adds to, the first.
        let ids: Vec<u64> = store.strokes().iter().step_by(2).map(|s| s.id).collect();
        store.remove(&ids);
        doc.save(&store).expect("second save");
        let (_, after) = Doc::open(&path).expect("reopen again");
        assert_eq!(after.len(), store.len(), "round {round}: the erased half stays erased");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_saving_thousands_of_strokes_is_quick_and_complete() {
    let dir = temp_dir("big");
    let path = dir.join("big.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut rng = Rng::new(31);
    let mut store = InkStore::new();
    let page = doc.layout().page(0).expect("page");
    let count = 5_000 * scale().min(4);
    for _ in 0..count {
        let (x, y) = (page.x + rng.range(20.0, 500.0), page.y + rng.range(20.0, 700.0));
        store.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, (0..20).map(|i| [x + i as f32, y + (i as f32).sin() * 4.0]).collect());
    }
    let t = Instant::now();
    doc.save(&store).expect("save");
    let saved = t.elapsed();
    let size = std::fs::metadata(&path).expect("meta").len();
    let t = Instant::now();
    let (_, back) = Doc::open(&path).expect("reopen");
    let reopened = t.elapsed();
    eprintln!("{count} strokes: save {saved:?}, {} KB on disk, reopen {reopened:?}", size / 1024);
    assert_eq!(back.len(), count);
    assert!(saved.as_secs() < 10 && reopened.as_secs() < 10, "save {saved:?}, reopen {reopened:?}");
    // For trying the real app on a heavily inked file: `INKPDF_STRESS_KEEP=path.pdf`.
    if let Some(keep) = std::env::var_os("INKPDF_STRESS_KEEP") {
        std::fs::copy(&path, keep).expect("keep the file");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_rebuilding_the_ink_buffer_for_a_crowded_page_is_cheap_enough() {
    // What the shell does when ink changes: flatten every stroke into capsules again.
    let mut rng = Rng::new(3);
    let strokes: Vec<Vec<P>> = (0..5_000).map(|_| random_stroke(&mut rng, 1).2).collect();
    let t = Instant::now();
    let capsules: usize = strokes.iter().map(|pts| curve_through(pts, 0.75).len()).sum();
    let took = t.elapsed();
    eprintln!("flattening {} strokes into {capsules} segments took {took:?}", strokes.len());
    assert!(took.as_millis() < 100, "{took:?}");
}
#[test]
fn stress_documents_grow_save_and_reopen_without_losing_a_page_or_a_stroke() {
    let dir = temp_dir("model");
    let mut rng = Rng::new(77);
    for round in 0..(8 * scale()) {
        let path = dir.join(format!("m{round}.pdf"));
        // Half the documents start blank (and unsaved), half from a file.
        let mut saved = !rng.chance(0.5);
        let mut doc = if saved {
            std::fs::write(&path, sample_pdf()).expect("write");
            Doc::open(&path).expect("open").0
        } else {
            Doc::blank(crate::doc::BLANK_PAGE).expect("blank")
        };
        let mut store = InkStore::new();
        let mut strokes_on: Vec<usize> = vec![0; doc.page_count()];
        let check = |doc: &Doc, strokes_on: &[usize], what: &str| {
            let (back, strokes) = Doc::open(doc.path().expect("saved")).expect("reopen");
            assert_eq!(back.page_count(), strokes_on.len(), "round {round} {what}: pages");
            for (page, &n) in strokes_on.iter().enumerate() {
                assert_eq!(strokes.iter().filter(|s| s.0 as usize == page).count(), n, "round {round} {what}: strokes on page {page}");
            }
        };
        for _ in 0..(12 + rng.below(30)) {
            match rng.below(5) {
                0 => {
                    if let Some(page) = doc.add_blank_page() {
                        strokes_on.push(0);
                        store.mark_dirty(page as u32);
                    }
                }
                1..=2 => {
                    let page = rng.below(doc.page_count());
                    let rect = doc.layout().page(page).expect("page");
                    let (x, y) = (rect.x + rng.range(20.0, rect.w - 60.0), rect.y + rng.range(20.0, rect.h - 60.0));
                    store.add(
                        page as u32,
                        Style { rgba: [0, 0, 0, 255], width: 2.0 },
                        (0..8).map(|i| [x + i as f32 * 3.0, y + (i as f32).sin() * 5.0]).collect(),
                    );
                    strokes_on[page] += 1;
                }
                _ => {
                    if saved {
                        doc.save(&store).expect("save");
                    } else {
                        doc.save_as(&path, &store).expect("save as");
                        saved = true;
                    }
                    store.mark_clean();
                    check(&doc, &strokes_on, "after a save");
                }
            }
        }
        if !saved {
            doc.save_as(&path, &store).expect("save as");
        } else {
            doc.save(&store).expect("save");
        }
        check(&doc, &strokes_on, "at the end");
    }
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn stress_saving_a_document_with_very_many_added_pages_is_quick_enough() {
    let dir = temp_dir("manypages");
    let mut doc = Doc::blank(crate::doc::BLANK_PAGE).expect("blank");
    let mut store = InkStore::new();
    let count = 500 * scale().min(8);
    let t = Instant::now();
    for _ in 0..count {
        let page = doc.add_blank_page().expect("add");
        store.mark_dirty(page as u32);
    }
    let added = t.elapsed();
    let path = dir.join("many.pdf");
    let t = Instant::now();
    doc.save_as(&path, &store).expect("save as");
    let saved = t.elapsed();
    let t = Instant::now();
    let (back, _) = Doc::open(&path).expect("reopen");
    let reopened = t.elapsed();
    eprintln!(
        "{count} pages added: {added:?}; saved in {saved:?} ({} KB); reopened in {reopened:?}",
        std::fs::metadata(&path).expect("meta").len() / 1024
    );
    assert_eq!(back.page_count(), count + 1);
    assert!(saved.as_secs() < 20, "saving {count} added pages took {saved:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_while_the_prompt_is_up_nothing_underneath_changes_whatever_arrives() {
    let mut rng = Rng::new(404);
    let mut ed = editor_with_pages(3);
    // Some ink to protect, and a selection.
    for i in 0..20 {
        ed.ink.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[50.0 + i as f32 * 9.0, 100.0], [80.0 + i as f32 * 9.0, 160.0]]);
    }
    let (mut blocked, mut answered) = (0, 0);
    for i in 0..(60_000 * scale()) {
        if ed.chrome.prompt.is_none() {
            // Leave nothing pending from before, so any flag that appears came from the events.
            ed.requests.take(&Request::Save);
            ed.requests.take(&Request::Close);
            ed.requests.take(&Request::Fullscreen);
            ed.requests.take_page_edits();
            ed.requests.clear_urls();
            ed.requests.take(&Request::SettingsChanged);
            if let Some(choice) = ed.requests.take_answer() {
                let _ = choice;
                answered += 1;
            }
            ed.ask_unsaved();
        }
        let (revision, strokes) = (ed.ink.revision(), ed.ink.len());
        let event = if rng.chance(0.3) { gesture(&mut ed, &mut rng) } else { poke(&mut ed, &mut rng, true) };
        // Events that replace the whole document or stand in for the shell are not the person's input.
        let shell_stepped_in = event.starts_with("load") || event.starts_with("close") || event.starts_with("add a page") || event.contains("shell");
        if ed.chrome.prompt.is_some() && !shell_stepped_in {
            blocked += 1;
            assert_eq!((ed.ink.revision(), ed.ink.len()), (revision, strokes), "event {i} ({event}) changed the ink under the prompt");
            assert!(
                !(ed.requests.has(&Request::Save)
                    || ed.requests.has(&Request::Close)
                    || ed.requests.has(&Request::Fullscreen)
                    || ed.requests.page_edit().is_some())
                    && ed.requests.url().is_none(),
                "event {i} ({event}) asked the shell for something under the prompt"
            );
        }
        if i % 500 == 0 {
            check_editor(&ed, &format!("event {i}"));
        }
    }
    eprintln!("events heard by the prompt alone: {blocked}; answers given: {answered}");
    assert!(blocked > 1000 && answered > 20, "the prompt should have been up and answered plenty ({blocked}, {answered})");
}

// ── the shape tool ───────────────────────────────────────────────────────────────────────────

/// A pen drag from `a` to `b` through a few steps, as the shape tool is used.
fn pen_drag(ed: &mut Editor, a: [f32; 2], b: [f32; 2], steps: usize) {
    ed.pointer_event(pen_at(Phase::Down, a));
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        ed.pointer_event(pen_at(Phase::Move, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]));
    }
    ed.pointer_event(pen_at(Phase::Up, b));
}

#[test]
fn stress_shapes_drawn_by_the_pen_stay_on_their_page_undo_redo_exactly_and_survive_save_and_reopen() {
    let dir = temp_dir("shapes");
    let path = dir.join("shapes.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::new([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    ed.tool = Tool::Geometry;
    let mut rng = Rng::new(404);
    let (mut made, mut edits) = (0usize, 0usize);
    for round in 0..(4_000 * scale()) {
        let size = ed.view.viewport;
        // (A random press can land on a toolbar button; the storm is about the shape tool. Back to it the way the
        // toolbar changes tools, which ends what the other was doing: the mode button can make the stylus place text.)
        if ed.tool != Tool::Geometry {
            use_tool(&mut ed, Tool::Geometry);
        }
        // (Nor is the bar under the tools to leave the tip choosing to select or erase.)
        ed.intent = crate::input::Intent::Draw;
        ed.shape = rng.pick(&crate::geometry::SHAPES);
        ed.shape_style = crate::tool::ToolStyle { colour: rng.below(5), width: rng.range(MIN_WIDTH, MAX_WIDTH) };
        // Anywhere in the window, off the page, or far beyond it.
        let spot = |rng: &mut Rng| {
            if rng.chance(0.1) { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.range(0.0, size[0]), rng.range(0.0, size[1])] }
        };
        let (a, b) = (spot(&mut rng), spot(&mut rng));
        let before = ed.ink.len();
        pen_drag(&mut ed, a, b, 1 + rng.below(12));
        made += ed.ink.len().saturating_sub(before);
        // Edit what was just drawn by its anchors: drag them about, double tap them, add and delete.
        for _ in 0..rng.below(5) {
            if !anchor_poke(&mut ed, &mut rng).is_empty() {
                edits += 1;
            }
        }
        // Now and then: undo, redo, or an eraser sweep through everything.
        match rng.below(12) {
            0 => ed.undo(),
            1 => ed.redo(),
            2 => {
                let from = spot(&mut rng);
                ed.pointer_event(PointerEvent {
                    device: Device::Pen,
                    id: 1,
                    phase: Phase::Down,
                    samples: vec![Sample { pos: from, pressure: 1.0, t_ms: 1000.0 }],
                    pen: PenButtons { in_range: true, contact: true, eraser: true, barrel: false },
                    contact_px: 0.0,
                });
                ed.pointer_event(PointerEvent {
                    device: Device::Pen,
                    id: 1,
                    phase: Phase::Up,
                    samples: vec![Sample { pos: spot(&mut rng), pressure: 1.0, t_ms: 1010.0 }],
                    pen: PenButtons { in_range: true, contact: false, eraser: true, barrel: false },
                    contact_px: 0.0,
                });
            }
            3 => {
                let dy = rng.range(-300.0, 300.0);
                ed.pan_px(0.0, dy);
            }
            _ => {}
        }
        // (A random press can also add or delete a page, or undo one: the document follows, as the shell makes it.)
        follow_pages(&mut ed, &mut doc);
        if round % 100 == 0 {
            check_editor(&ed, &format!("round {round}"));
            for s in ed.ink.strokes() {
                assert!(s.pts.len() >= 2 && s.pts.len() <= crate::figure::MAX_ANCHORS * 50, "round {round}: a shape of {} points", s.pts.len());
                assert!(s.figure.is_some(), "round {round}: only shapes were drawn");
            }
        }
    }
    eprintln!("shapes made: {made} ({edits} anchor edits); alive at the end: {}", ed.ink.len());
    // (A press on the outline of the shape that is selected grabs it instead, and many of these are huge.)
    assert!(made > 1000 * scale(), "the storm should have made plenty of shapes ({made})");
    assert!(edits > 1000, "and edited plenty of anchors ({edits})");

    // Save, reopen: every shape comes back with the points it had.
    doc.save(&ed.ink).expect("save");
    let (_, back) = Doc::open(&path).expect("reopen");
    assert_eq!(back.len(), ed.ink.len(), "every shape survives a save and reopen");
    // (The file keeps the ink page by page, each page's strokes in the order they were drawn.)
    let mut by_page: Vec<_> = ed.ink.strokes().iter().collect();
    by_page.sort_by_key(|s| s.page);
    for (stroke, (page, style, pts, figure, _)) in by_page.into_iter().zip(&back) {
        assert_eq!((stroke.page, stroke.style.width.round()), (*page, style.width.round()));
        assert_eq!(stroke.pts.len(), pts.len(), "a shape keeps all its points");
        // And its anchors: it can still be edited after reopening.
        let figure = figure.as_ref().expect("the shape came back as a shape");
        let before = stroke.figure.as_ref().expect("a shape");
        assert_eq!(before.anchor_count(), figure.anchor_count());
        assert!(
            (0..before.anchor_count()).all(|i| {
                let (a, b) = (before.anchor_pos(i).expect("anchor"), figure.anchor_pos(i).expect("anchor"));
                (a[0] - b[0]).abs() < 0.02 && (a[1] - b[1]).abs() < 0.02
            }),
            "the anchors moved"
        );
        assert!(stroke.pts.iter().zip(pts).all(|(a, b)| (a[0] - b[0]).abs() < 0.02 && (a[1] - b[1]).abs() < 0.02), "the points moved");
    }

    // Undo everything the history still holds, then redo it all: the same ink.
    let end: Vec<_> = ed.ink.strokes().iter().map(|s| (s.id, s.pts.len())).collect();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        undone += 1;
    }
    for _ in 0..undone {
        ed.redo();
    }
    let again: Vec<_> = ed.ink.strokes().iter().map(|s| (s.id, s.pts.len())).collect();
    assert_eq!(again, end, "redoing everything undone restores the ink exactly");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_settings_text_of_any_kind_never_stops_the_app_and_real_settings_come_back_exactly() {
    let mut rng = Rng::new(808);
    let words = [
        "tool",
        "pen",
        "geometry",
        "laser",
        "shape",
        "shape-slot",
        "shape-width",
        "text-slot",
        "text-size",
        "text",
        "slot",
        "width",
        "smoothing",
        "color",
        "0",
        "1",
        "4",
        "5",
        "-1",
        "2.5",
        "NaN",
        "inf",
        "-inf",
        "1e40",
        "ff00ff",
        "#00ff00",
        "zz",
        "é",
        "line",
        "axes3d",
        "ellipse",
        "curve",
        "",
        "999999999999999999999",
    ];
    for _ in 0..(20_000 * scale()) {
        let lines = rng.below(10);
        let text: String =
            (0..lines).map(|_| (0..rng.below(5)).map(|_| rng.pick(&words)).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n");
        let s = crate::settings::Settings::parse(&text);
        for style in [s.pen, s.shape_style] {
            assert!(
                style.colour < s.palette.len() && style.width.is_finite() && (MIN_WIDTH..=MAX_WIDTH).contains(&style.width),
                "{text:?} gave {style:?}"
            );
        }
        let t = s.text_style;
        assert!(
            t.colour < s.palette.len() && t.width.is_finite() && (crate::tool::TEXT_SIZES.0..=crate::tool::TEXT_SIZES.1).contains(&t.width),
            "{text:?} gave {t:?}"
        );
        assert!((0.0..=1.0).contains(&s.smoothing), "{text:?}");
        // Whatever it read, writing it out and reading it back is stable.
        let again = crate::settings::Settings::parse(&s.to_text());
        assert_eq!(
            (again.tool, again.shape, again.pen.colour, again.shape_style.colour, again.palette),
            (s.tool, s.shape, s.pen.colour, s.shape_style.colour, s.palette),
            "{text:?}"
        );
    }
    // Settings made the way the app makes them come back exactly (widths to two decimals).
    for _ in 0..(2_000 * scale()) {
        let hundredths = |v: f32| (v * 100.0).round() / 100.0;
        let mut s = crate::settings::Settings {
            tool: rng.pick(&[Tool::Pen, Tool::Geometry, Tool::Text, Tool::Laser]),
            shape: rng.pick(&crate::geometry::SHAPES),
            pen: crate::tool::ToolStyle { colour: rng.below(5), width: hundredths(rng.range(MIN_WIDTH, MAX_WIDTH)) },
            shape_style: crate::tool::ToolStyle { colour: rng.below(5), width: hundredths(rng.range(MIN_WIDTH, MAX_WIDTH)) },
            text_style: crate::tool::ToolStyle {
                colour: rng.below(5),
                width: hundredths(rng.range(crate::tool::TEXT_SIZES.0, crate::tool::TEXT_SIZES.1)),
            },
            smoothing: hundredths(rng.unit()),
            ..crate::settings::Settings::default()
        };
        for c in &mut s.palette {
            *c = [rng.below(256) as u8, rng.below(256) as u8, rng.below(256) as u8, 255];
        }
        let back = crate::settings::Settings::parse(&s.to_text());
        assert_eq!(
            (back.tool, back.shape, back.pen.colour, back.shape_style.colour, back.text_style.colour, back.palette),
            (s.tool, s.shape, s.pen.colour, s.shape_style.colour, s.text_style.colour, s.palette)
        );
        assert!(
            (back.pen.width - s.pen.width).abs() < 0.006
                && (back.shape_style.width - s.shape_style.width).abs() < 0.006
                && (back.text_style.width - s.text_style.width).abs() < 0.006
                && (back.smoothing - s.smoothing).abs() < 0.006,
            "{s:?} vs {back:?}"
        );
    }
}

#[test]
fn stress_pages_inserted_anywhere_among_ink_keep_every_stroke_on_its_page_through_undo_save_and_reopen() {
    let dir = temp_dir("insert-pages");
    let path = dir.join("pages.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::new([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    let mut rng = Rng::new(606);
    let (mut inserted, mut deleted, mut saves, mut moved) = (0, 0, 0, 0);
    for round in 0..(300 * scale().min(4)) {
        match rng.below(14) {
            0..=2 if doc.page_count() < 60 => {
                // A page somewhere among the others, the size of one of them.
                let (at, like) = (rng.below(doc.page_count() + 1), rng.below(doc.page_count()));
                let size = doc.page_size(like as u32);
                assert!(ed.edit_pages(PageEdit::Insert { at, size }));
                follow_pages(&mut ed, &mut doc);
                inserted += 1;
            }
            3..=6 => {
                let page = rng.below(doc.page_count()) as u32;
                let r = doc.layout().page(page as usize).expect("page");
                let (x, y) = (r.x + rng.range(10.0, r.w - 10.0), r.y + rng.range(10.0, r.h - 10.0));
                ed.ink.add(
                    page,
                    Style { rgba: [rng.below(256) as u8, 0, 0, 255], width: rng.range(0.5, 6.0) },
                    (0..2 + rng.below(8))
                        .map(|i| [(x + i as f32 * 3.0).min(r.x + r.w - 1.0), (y + (i as f32).sin() * 4.0).min(r.bottom() - 1.0)])
                        .collect(),
                );
            }
            7 => {
                ed.undo();
                follow_pages(&mut ed, &mut doc);
            }
            8 => {
                ed.redo();
                follow_pages(&mut ed, &mut doc);
            }
            // A page goes, somewhere among the others, with the ink on it.
            9 | 10 if doc.page_count() > 1 => {
                let at = rng.below(doc.page_count());
                assert!(ed.edit_pages(PageEdit::delete(at)), "page {at} of {} is deleted", doc.page_count());
                follow_pages(&mut ed, &mut doc);
                deleted += 1;
            }
            // Some pages are taken out and put back somewhere else.
            11 | 12 if doc.page_count() > 1 => {
                let n = doc.page_count();
                let pages: Vec<usize> = (0..1 + rng.below(3.min(n - 1))).map(|_| rng.below(n)).collect();
                let mut distinct = pages.clone();
                distinct.sort_unstable();
                distinct.dedup();
                let to = rng.below(n - distinct.len() + 1);
                // (Putting them where they were is no edit: it is refused, and nothing happens.)
                if ed.edit_pages(PageEdit::Move { pages, to }) {
                    moved += 1;
                }
                follow_pages(&mut ed, &mut doc);
            }
            _ => {
                doc.save(&ed.ink).expect("save");
                ed.document_saved();
                saves += 1;
                let (back, strokes) = Doc::open(&path).expect("reopen");
                assert_eq!(back.page_count(), doc.page_count(), "round {round}: the pages are all in the file");
                assert_eq!(strokes.len(), ed.ink.len(), "round {round}: every stroke came back");
                // (The file keeps ink page by page, so compare as sets: each stroke by its page and first point.)
                let key = |page: u32, p: &[f32; 2]| (page, (p[0] * 10.0).round() as i64, (p[1] * 10.0).round() as i64);
                let mut want: Vec<_> = ed.ink.strokes().iter().map(|s| key(s.page, &s.pts[0])).collect();
                let mut got: Vec<_> = strokes.iter().map(|(page, _, pts, ..)| key(*page, &pts[0])).collect();
                want.sort_unstable();
                got.sort_unstable();
                assert_eq!(want, got, "round {round}: the strokes are not on the pages they were");
            }
        }
        // Whatever was done: every stroke is on the page it says it is.
        for s in ed.ink.strokes() {
            let r = doc.layout().page(s.page as usize).expect("the stroke's page exists");
            assert!(
                s.pts.iter().all(|p| p[0] >= r.x - 0.01 && p[0] <= r.x + r.w + 0.01 && p[1] >= r.y - 0.01 && p[1] <= r.bottom() + 0.01),
                "round {round}: stroke {} is off page {}",
                s.id,
                s.page
            );
        }
    }
    eprintln!("pages inserted: {inserted}; deleted: {deleted}; moved: {moved}; saves checked: {saves}; strokes at the end: {}", ed.ink.len());
    assert!(inserted > 20 && deleted > 10 && moved > 10 && saves > 5);
    // Everything the history holds can be undone and redone, with the document following, and the pages stay laid out alike.
    ed.finish_text();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        follow_pages(&mut ed, &mut doc);
        undone += 1;
    }
    for _ in 0..undone {
        ed.redo();
        follow_pages(&mut ed, &mut doc);
    }
    doc.save(&ed.ink).expect("save at the end");
    assert_eq!(Doc::open(&path).expect("reopen").0.page_count(), doc.page_count(), "and the file has the pages the document has");
    let _ = std::fs::remove_dir_all(dir);
}

// ── every tool together ──────────────────────────────────────────────────────────────────────

/// The pen tapped at `at` at time `t_ms`.
fn tap_pen(ed: &mut Editor, at: [f32; 2], t_ms: f64) {
    let ev = |phase| PointerEvent {
        device: Device::Pen,
        id: 1,
        phase,
        samples: vec![Sample { pos: at, pressure: 0.5, t_ms }],
        pen: PenButtons { in_range: true, contact: phase != Phase::Up, eraser: false, barrel: false },
        contact_px: 0.0,
    };
    ed.pointer_event(ev(Phase::Down));
    ed.pointer_event(ev(Phase::Up));
}

/// A click of the mouse at `at`, and a drag of it from `a` to `b` through a few steps.
fn mouse_click(ed: &mut Editor, at: [f32; 2]) {
    ed.mouse_moved(at);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
}

fn mouse_drag(ed: &mut Editor, a: [f32; 2], b: [f32; 2], steps: usize) {
    ed.mouse_moved(a);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        ed.mouse_moved([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
}

/// Switch tools the way the toolbar does: typing ends, and the selection is let go.
fn use_tool(ed: &mut Editor, tool: Tool) {
    // (As the toolbar does it: what the old tool had pending, typing or a polyline begun by taps, becomes part of the
    // document before the tool changes.)
    ed.settle();
    ed.selection.clear();
    ed.chrome.panel = None;
    ed.tool = tool;
    // (A random press can land on the bar under the tools and choose to select or erase; choosing a tool starts
    // with the tip drawing again.)
    ed.intent = crate::input::Intent::Draw;
}

/// The stylus's other ends: the eraser end or the barrel button, dragged along `path`.
fn pen_path(ed: &mut Editor, buttons: PenButtons, path: &[[f32; 2]]) {
    let at = |phase, pos| PointerEvent {
        device: Device::Pen,
        id: 1,
        phase,
        samples: vec![Sample { pos, pressure: 0.5, t_ms: 1000.0 }],
        pen: buttons,
        contact_px: 0.0,
    };
    ed.pointer_event(at(Phase::Down, path[0]));
    for &p in &path[1..] {
        ed.pointer_event(at(Phase::Move, p));
    }
    ed.pointer_event(at(Phase::Up, *path.last().unwrap_or(&path[0])));
}

/// What a text box says, how big, and where (page and top-left corner), for comparing before and
/// after a save.
fn text_boxes(strokes: impl Iterator<Item = (u32, String, f32, [f32; 2])>) -> Vec<(u32, String, i64, i64, i64)> {
    let mut out: Vec<_> = strokes
        .map(|(page, text, size, p)| (page, text, (size * 10.0).round() as i64, (p[0] * 10.0).round() as i64, (p[1] * 10.0).round() as i64))
        .collect();
    out.sort();
    out
}

/// Every tool, and everything that happens between them, from one seed.
fn every_tool_together(seed: u64, rounds: usize) {
    let dir = temp_dir(&format!("all-tools-{seed}"));
    let path = dir.join("all.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::new([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    let mut rng = Rng::new(seed);
    let words = ["Hello", "wörld", "✓ done", "a longer line of text to type", "x", "日本語", "line\nbreak", "  spaced  "];
    let (mut typed, mut shapes, mut inks, mut saves, mut inserted, mut deleted) = (0, 0, 0, 0, 0, 0);
    for round in 0..rounds {
        let size = ed.view.viewport;
        // Below the toolbar, so a press is a press on the page.
        let spot = |rng: &mut Rng| [rng.range(20.0, size[0] - 20.0), rng.range(130.0, size[1] - 20.0)];
        let boxes: Vec<[f32; 4]> = ed.ink.strokes().iter().filter(|s| s.text.is_some()).map(|s| s.bbox()).collect();
        let on_box = |rng: &mut Rng, ed: &Editor| {
            let b = boxes[rng.below(boxes.len())];
            ed.view.doc_to_screen([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0])
        };
        let event = match rng.below(16) {
            0..=3 => {
                // A box: a tap on the page (or on a box, to edit it), then the keyboard.
                use_tool(&mut ed, Tool::Text);
                let at = if !boxes.is_empty() && rng.chance(0.4) { on_box(&mut rng, &ed) } else { spot(&mut rng) };
                mouse_click(&mut ed, at);
                if ed.is_typing() {
                    typed += 1;
                    for _ in 0..1 + rng.below(3) {
                        ed.type_text(rng.pick(&words));
                        if rng.chance(0.2) {
                            ed.backspace();
                        }
                    }
                }
                "text"
            }
            4 => {
                if !boxes.is_empty() {
                    use_tool(&mut ed, Tool::Text);
                    let from = on_box(&mut rng, &ed);
                    let to = [from[0] + rng.range(-300.0, 300.0), from[1] + rng.range(-300.0, 300.0)];
                    mouse_drag(&mut ed, from, to, 2 + rng.below(8));
                }
                "move a box"
            }
            5 | 6 => {
                use_tool(&mut ed, Tool::Pen);
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                let before = ed.ink.len();
                pen_drag(&mut ed, a, b, 3 + rng.below(20));
                inks += ed.ink.len().saturating_sub(before);
                "ink"
            }
            7 | 8 => {
                use_tool(&mut ed, Tool::Geometry);
                ed.shape = rng.pick(&crate::geometry::SHAPES);
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                let before = ed.ink.len();
                if matches!(ed.shape, Shape::Line | Shape::Curve) && rng.chance(0.6) {
                    // A polyline or spline by taps, ended by a double tap (near the first node, now and then).
                    let nodes: Vec<[f32; 2]> = std::iter::once(a).chain((0..2 + rng.below(4)).map(|_| spot(&mut rng))).collect();
                    for (i, &n) in nodes.iter().enumerate() {
                        tap_pen(&mut ed, n, 10_000.0 * i as f64);
                    }
                    let end = if rng.chance(0.4) { [a[0] + 3.0, a[1] - 2.0] } else { *nodes.last().unwrap_or(&a) };
                    tap_pen(&mut ed, end, 10_000.0 * nodes.len() as f64);
                    tap_pen(&mut ed, end, 10_000.0 * nodes.len() as f64 + 150.0);
                } else {
                    pen_drag(&mut ed, a, b, 1 + rng.below(8));
                }
                shapes += ed.ink.len().saturating_sub(before);
                for _ in 0..rng.below(3) {
                    anchor_poke(&mut ed, &mut rng);
                }
                "shape"
            }
            9 => {
                // The eraser end through whatever is there, with any tool.
                use_tool(&mut ed, rng.pick(&[Tool::Pen, Tool::Geometry, Tool::Text]));
                let eraser = PenButtons { in_range: true, contact: true, eraser: true, barrel: false };
                pen_path(&mut ed, eraser, &[spot(&mut rng), spot(&mut rng), spot(&mut rng)]);
                "erase"
            }
            10 => {
                // The barrel button: a lasso round something, then the selection dragged.
                use_tool(&mut ed, rng.pick(&[Tool::Pen, Tool::Geometry, Tool::Text]));
                let barrel = PenButtons { in_range: true, contact: true, eraser: false, barrel: true };
                let c = spot(&mut rng);
                let r = rng.range(40.0, 400.0);
                pen_path(
                    &mut ed,
                    barrel,
                    &[[c[0] - r, c[1] - r], [c[0] + r, c[1] - r], [c[0] + r, c[1] + r], [c[0] - r, c[1] + r], [c[0] - r, c[1] - r]],
                );
                if let Some(b) = ed.ink.bounds_of(&ed.selection) {
                    let from = ed.view.doc_to_screen([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
                    pen_drag(&mut ed, from, [from[0] + rng.range(-200.0, 200.0), from[1] + rng.range(-200.0, 200.0)], 3);
                }
                "lasso and move"
            }
            11 | 12 => {
                // Undo and redo, also with typing under way.
                for _ in 0..1 + rng.below(3) {
                    if rng.chance(0.6) {
                        ed.undo()
                    } else {
                        ed.redo()
                    }
                    follow_pages(&mut ed, &mut doc);
                }
                "undo/redo"
            }
            13 => {
                if rng.chance(0.4) && doc.page_count() > 1 {
                    let at = rng.below(doc.page_count());
                    assert!(ed.edit_pages(PageEdit::delete(at)));
                    follow_pages(&mut ed, &mut doc);
                    deleted += 1;
                    "delete a page"
                } else {
                    if doc.page_count() < 30 {
                        let (at, like) = (rng.below(doc.page_count() + 1), rng.below(doc.page_count()));
                        let size = doc.page_size(like as u32);
                        assert!(ed.edit_pages(PageEdit::Insert { at, size }));
                        follow_pages(&mut ed, &mut doc);
                        inserted += 1;
                    }
                    "insert a page"
                }
            }
            14 => {
                // The view moves: scroll, a finger's pan, the wheel.
                ed.pan_px(rng.range(-200.0, 200.0), rng.range(-600.0, 600.0));
                ed.wheel(Wheel::Lines(0.0, rng.range(-5.0, 5.0)));
                if rng.chance(0.3) {
                    ed.step_view(0.5);
                }
                "scroll"
            }
            _ if rng.chance(0.6) => {
                ed.pan_px(0.0, rng.range(-300.0, 300.0));
                "scroll"
            }
            _ => {
                // Save and reopen, as the shell does it: typing ends first.
                ed.finish_text();
                doc.save(&ed.ink).expect("save");
                ed.document_saved();
                saves += 1;
                let (back, strokes) = Doc::open(&path).expect("reopen");
                assert_eq!(back.page_count(), doc.page_count(), "round {round}: the pages are all in the file");
                assert_eq!(strokes.len(), ed.ink.len(), "round {round}: every stroke and text box came back");
                let want = text_boxes(ed.ink.strokes().iter().filter_map(|s| s.text.as_ref().map(|t| (s.page, t.text.clone(), t.size, s.pts[0]))));
                let got = text_boxes(strokes.iter().filter_map(|(page, _, pts, _, t)| t.as_ref().map(|t| (*page, t.text.clone(), t.size, pts[0]))));
                assert_eq!(want, got, "round {round}: the text did not come back as it was");
                "save and reopen"
            }
        };
        check_view(&ed, &format!("round {round}"), event);
        if round % 25 == 0 {
            check_editor(&ed, &format!("round {round} after {event}"));
            // Every stroke is on a page that exists.
            for s in ed.ink.strokes() {
                assert!(doc.layout().page(s.page as usize).is_some(), "round {round}: stroke {} is on page {} of {}", s.id, s.page, doc.page_count());
            }
        }
    }
    eprintln!(
        "text boxes begun: {typed}; shapes: {shapes}; ink: {inks}; saves: {saves}; pages inserted: {inserted}; pages deleted: {deleted}; alive: {}",
        ed.ink.len()
    );
    assert!(typed > 200 && shapes > 100 && inks > 100 && saves > 10 && inserted > 10 && deleted > 5, "the storm should have used every tool");
    // Undoing everything the history holds leaves a consistent editor, and so does redoing it all.
    ed.finish_text();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        follow_pages(&mut ed, &mut doc);
        undone += 1;
    }
    check_editor(&ed, "after undoing everything");
    for _ in 0..undone {
        ed.redo();
        follow_pages(&mut ed, &mut doc);
    }
    check_editor(&ed, "after redoing everything");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_every_tool_and_interaction_together_stays_consistent_through_undo_pages_save_and_reopen() {
    // (The ink piles up, and every save writes it all: more than three times the work is slow, not better.)
    for seed in [2024, 7, 31_337] {
        every_tool_together(seed, 2_000 * scale().min(3));
    }
}

// ── the pages overview ───────────────────────────────────────────────────────────────────────

/// What must hold of the overview whatever was done to it.
fn check_overview(ed: &Editor, doc: &Doc, what: &str) {
    assert_eq!(ed.layout.len(), doc.page_count(), "{what}: the editor and the document have the same pages");
    for s in ed.ink.strokes() {
        assert!(ed.layout.page(s.page as usize).is_some(), "{what}: stroke {} is on page {} of {}", s.id, s.page, ed.layout.len());
    }
    let Some(state) = ed.chrome.overview.as_ref() else { return };
    let layout = ed.overview_layout();
    let count = ed.layout.len();
    assert!(
        state.scroll.is_finite() && state.scroll >= 0.0 && state.scroll <= layout.max_scroll(count) + 0.01,
        "{what}: the pages are scrolled to {}",
        state.scroll
    );
    let rows = ed.toc_rows().len();
    assert!(
        state.toc_scroll >= 0.0 && state.toc_scroll <= layout.toc_max_scroll(rows) + 0.01,
        "{what}: the table is scrolled to {}",
        state.toc_scroll
    );
    assert!(state.selected.iter().all(|&p| p < count), "{what}: pages chosen that are not there: {:?} of {count}", state.selected);
    assert!(state.edit || state.selected.is_empty(), "{what}: pages are chosen outside edit mode");
    // It draws, and what it draws is a number.
    let mut ui = Ui::default();
    ed.draw_overview(&mut ui);
    assert!(ui.panels.iter().all(|p| p.rect.iter().all(|v| v.is_finite())), "{what}");
    assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite()) && c.width.is_finite()), "{what}");
    assert!(ui.labels.iter().all(|l| l.at.iter().all(|v| v.is_finite()) && l.size > 0.0), "{what}");
    assert!(ui.images.len() <= count, "{what}: a picture for every page at most");
}

/// What must hold of everything that moves, whatever the frames between were like: a number, within its ends.
fn check_motion(ed: &Editor, what: &str) {
    let unit = |v: f32| v.is_finite() && (-1e-3..=1.0 + 1e-3).contains(&v);
    let look = ed.toolbar_look();
    assert!(look.hover.iter().chain(&look.enabled).chain(&look.ripple).all(|&v| unit(v)), "{what}: {look:?}");
    assert!(look.tool.is_finite() && (-1e-3..=3.0 + 1e-3).contains(&look.tool) && unit(look.unsaved), "{what}: {look:?}");
    assert!(unit(ed.overview_appear()) && unit(ed.overview_leaving()) && unit(ed.motions.bar_amount()), "{what}");
    assert!(ed.prompt_look().is_none_or(|(_, a)| unit(a)) && ed.panel_look().is_none_or(|(_, a)| unit(a)), "{what}");
    if ed.overview_open() {
        let mut ui = Ui::default();
        ed.draw_overview(&mut ui);
        assert!(ui.labels.iter().all(|l| unit(l.alpha)) && ui.images.iter().all(|i| unit(i.alpha)), "{what}: opacity");
        assert!(ui.panels.iter().all(|p| p.rect.iter().all(|v| v.is_finite()) && p.radius.is_finite()), "{what}");
    }
}

#[test]
fn stress_the_pages_overview_under_hostile_input_keeps_the_pages_the_selection_and_the_file_consistent() {
    let dir = temp_dir("overview");
    let path = dir.join("outline.pdf");
    std::fs::write(&path, crate::testutil::pdf_with_outline()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::new([1280.0, 800.0], 1.25, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    ed.outline = doc.take_outline();
    ed.links = doc.take_links();
    let mut rng = Rng::new(808);
    // (With motion on, and frames of any length in between: what moves must stay a number and within its ends.)
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    let (mut opened, mut edits, mut saves, mut carried) = (0, 0, 0, 0);
    // (More rounds than this find nothing new: a document of this size saves slower and slower, not differently.)
    for round in 0..(8_000 * scale().min(3)) {
        let what = format!("round {round}");
        ed.step_view(rng.pick(&[0.0, 0.004, 0.016, 0.016, 0.033, 0.5, 5.0, -1.0, f32::NAN, f32::INFINITY]));
        check_motion(&ed, &what);
        let was_open = ed.overview_open();
        // (A real document does not grow without end; a long storm would otherwise spend its time on thousands of pages.)
        if ed.layout.len() > 60 {
            let n = ed.layout.len();
            ed.edit_pages(PageEdit::Delete { pages: (30..n).collect(), grave: 0 });
            follow_pages(&mut ed, &mut doc);
        }
        let size = ed.view.viewport;
        let layout = ed.overview_layout();
        // Where a pointer might be: mostly in the card, sometimes anywhere, sometimes nowhere a pointer can be.
        let spot = |rng: &mut Rng| -> [f32; 2] {
            if rng.chance(0.8) {
                [rng.range(layout.card[0], layout.card[2]), rng.range(layout.card[1], layout.card[3])]
            } else {
                [rng.finite(size[0]), rng.finite(size[1])]
            }
        };
        match rng.below(26) {
            0 | 1 => {
                if !ed.overview_open() {
                    ed.open_overview();
                    opened += 1;
                }
            }
            2..=5 => {
                let at = spot(&mut rng);
                mouse_click(&mut ed, at);
            }
            6 | 7 => {
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                mouse_drag(&mut ed, a, b, 1 + rng.below(8));
                carried += 1;
            }
            8 | 9 => {
                let at = spot(&mut rng);
                tap_pen(&mut ed, at, 1000.0);
            }
            10 => {
                let path: Vec<[f32; 2]> = (0..2 + rng.below(6)).map(|_| spot(&mut rng)).collect();
                pen_path(&mut ed, PenButtons { in_range: true, contact: true, eraser: rng.chance(0.2), barrel: rng.chance(0.2) }, &path);
            }
            11 => {
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                ed.palm = crate::input::PalmGuard::default();
                let touch = |phase, pos| PointerEvent {
                    device: Device::Touch,
                    id: 5,
                    phase,
                    samples: vec![Sample { pos, pressure: 1.0, t_ms: 1000.0 }],
                    pen: PenButtons::default(),
                    contact_px: 20.0,
                };
                ed.pointer_event(touch(Phase::Down, a));
                ed.pointer_event(touch(Phase::Move, b));
                ed.pointer_event(touch(Phase::Up, b));
            }
            12 => {
                let (x, y) = (rng.range(-3.0, 3.0), rng.range(-6.0, 6.0));
                ed.mouse_moved(spot(&mut rng));
                ed.wheel(Wheel::Lines(x, y));
            }
            13 => {
                let key = match rng.below(8) {
                    0 => Key::Escape,
                    1 => Key::Delete,
                    2 => Key::Home(false),
                    3 => Key::End(false),
                    4 => Key::Arrow(Dir::Down, false),
                    5 => Key::Arrow(Dir::Up, false),
                    6 => Key::Backspace,
                    _ => Key::Text("x"),
                };
                ed.press(key);
            }
            14 | 15 => {
                let letter = rng.pick(&["a", "d", "z", "y", "c", "x", "v"]);
                ed.shortcut(letter, rng.chance(0.2));
            }
            16 => {
                // A page is added, deleted, copied or moved by something other than the overview's own controls.
                let n = ed.layout.len();
                let edit = match rng.below(4) {
                    0 => PageEdit::Insert { at: rng.below(n + 1), size: [612.0, 792.0] },
                    1 => PageEdit::delete(rng.below(n)),
                    2 => PageEdit::Duplicate { pages: vec![rng.below(n), rng.below(n)] },
                    _ => PageEdit::Move { pages: vec![rng.below(n)], to: rng.below(n) },
                };
                if n < 40 || matches!(edit, PageEdit::Delete { .. }) {
                    edits += usize::from(ed.edit_pages(edit));
                }
            }
            17 | 18 => {
                if rng.chance(0.5) {
                    ed.undo()
                } else {
                    ed.redo()
                }
            }
            19 if ed.ink.len() < 300 => {
                // Ink on a page, as when the overview is not up.
                let page = rng.below(ed.layout.len());
                let r = ed.layout.page(page).expect("page");
                ed.ink.add(page as u32, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r.x + 30.0, r.y + 40.0], [r.x + 90.0, r.y + 60.0]]);
            }
            20 => {
                // The window changes size or scale.
                ed.set_viewport([rng.range(300.0, 2400.0), rng.range(260.0, 1500.0)]);
                ed.set_dpr(rng.pick(&[1.0, 1.25, 1.5, 2.0, 3.0]));
            }
            21 if rng.chance(0.15) => {
                ed.settle();
                doc.save(&ed.ink).expect("save");
                ed.document_saved();
                saves += 1;
                let (back, strokes) = Doc::open(&path).expect("reopen");
                assert_eq!(back.page_count(), doc.page_count(), "{what}: the pages are all in the file");
                assert_eq!(strokes.len(), ed.ink.len(), "{what}: every stroke came back");
            }
            22 => {
                ed.pan_px(rng.range(-100.0, 100.0), rng.range(-400.0, 400.0));
            }
            _ => {
                if rng.chance(0.3) {
                    ed.close_overview();
                }
            }
        }
        follow_pages(&mut ed, &mut doc);
        // (Drawing it and walking every stroke is the dear part: every few rounds, and always when it has just been opened.)
        if round % 6 == 0 || ed.chrome.overview.is_none() == was_open {
            check_overview(&ed, &doc, &what);
        }
    }
    eprintln!("overview opened: {opened}; edits by hand: {edits}; drags and clicks: {carried}; saves: {saves}; pages: {}", ed.layout.len());
    assert!(opened > 100 && edits > 50 && saves > 20, "the storm should have used the overview");
    // Everything that was done can be undone and redone, with the document following.
    ed.close_overview();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        follow_pages(&mut ed, &mut doc);
        undone += 1;
    }
    for _ in 0..undone {
        ed.redo();
        follow_pages(&mut ed, &mut doc);
    }
    check_overview(&ed, &doc, "after undoing and redoing everything");
    doc.save(&ed.ink).expect("save at the end");
    assert_eq!(Doc::open(&path).expect("reopen").0.page_count(), doc.page_count());
    let _ = std::fs::remove_dir_all(dir);
}

// ── hostile outlines and page trees ──────────────────────────────────────────────────────────

/// A PDF with `pages` pages and a hostile outline (and sometimes a hostile page tree): object 1 is the catalog, 2 the page
/// tree, 3.. the pages, then the outline's objects.
fn hostile_pdf(kind: usize, pages: usize) -> Vec<u8> {
    let first_page = 3;
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", first_page + i)).collect();
    let mut objs = vec![String::new(), format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" "))];
    objs.extend((0..pages).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string()));
    let root = objs.len() + 1;
    let item = |title: &str, extra: &str| format!("<< /Title ({title}) {extra} >>");
    let mut catalog = format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R >>");
    match kind {
        // An entry that is its own next, its own child, and its own parent.
        0 => {
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count 1 >>", root + 1, root + 1));
            objs.push(item("loop", &format!("/Parent {root} 0 R /Next {0} 0 R /First {0} 0 R /Prev {0} 0 R /Dest [3 0 R /Fit]", root + 1)));
        }
        // Two siblings that point at each other for ever.
        1 => {
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count 2 >>", root + 1, root + 2));
            objs.push(item("a", &format!("/Parent {root} 0 R /Next {} 0 R /Dest [3 0 R /Fit]", root + 2)));
            objs.push(item("b", &format!("/Parent {root} 0 R /Next {} 0 R /Dest [4 0 R /Fit]", root + 1)));
        }
        // A count that says millions, destinations that are nowhere, not a page, or not a destination at all.
        2 => {
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count 99999999 >>", root + 1, root + 4));
            objs.push(item("nowhere", &format!("/Parent {root} 0 R /Next {} 0 R /Dest [99 0 R /Fit]", root + 2)));
            objs.push(item("not a page", &format!("/Parent {root} 0 R /Next {} 0 R /Dest [{root} 0 R /Fit]", root + 3)));
            objs.push(item("a number", &format!("/Parent {root} 0 R /Next {} 0 R /Dest 7", root + 4)));
            objs.push(item("negative", "/Count -5 /Dest [3 0 R /XYZ -1e30 1e30 null]"));
        }
        // Titles that are control characters, enormous, or not text.
        3 => {
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count 3 >>", root + 1, root + 3));
            objs.push(item(&"x".repeat(6000), &format!("/Parent {root} 0 R /Next {} 0 R /Dest [3 0 R /Fit]", root + 2)));
            objs.push(item("a\\000b\\007c\\r\\n\\t  d", &format!("/Parent {root} 0 R /Next {} 0 R /Dest [3 0 R /Fit]", root + 3)));
            objs.push(format!("<< /Title 42 /Parent {root} 0 R /Dest [3 0 R /Fit] >>"));
        }
        // A chain of entries each inside the one before: far deeper than anything should be listed.
        4 => {
            let depth = 400;
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {depth} >>", root + 1, root + 1));
            for d in 0..depth {
                let me = root + 1 + d;
                let parent = if d == 0 { root } else { me - 1 };
                let kid = if d + 1 < depth { format!("/First {} 0 R /Last {} 0 R /Count 1", me + 1, me + 1) } else { String::new() };
                objs.push(item(&format!("level {d}"), &format!("/Parent {parent} 0 R {kid} /Dest [3 0 R /Fit]")));
            }
        }
        // A very long list of siblings.
        5 => {
            let n = 30_000;
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {n} >>", root + 1, root + n));
            for k in 0..n {
                let me = root + 1 + k;
                let next = if k + 1 < n { format!("/Next {} 0 R", me + 1) } else { String::new() };
                objs.push(item(&format!("entry {k}"), &format!("/Parent {root} 0 R {next} /Dest [{} 0 R /Fit]", first_page + k % pages.max(1))));
            }
        }
        // The outline is not an outline.
        6 => objs.push("[1 2 3]".to_string()),
        // A page tree in which a node holds itself, and a page that holds the tree.
        _ => {
            objs[1] = format!("<< /Type /Pages /Kids [2 0 R {}] /Count 99 >>", kids.join(" "));
            objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count 1 >>", root + 1, root + 1));
            objs.push(item("tree", &format!("/Parent {root} 0 R /Dest [3 0 R /Fit]")));
            catalog = format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R >>");
        }
    }
    objs[0] = catalog;
    crate::testutil::build_pdf(&objs)
}

#[test]
fn stress_hostile_outlines_and_page_trees_open_or_are_refused_and_what_opens_edits_draws_and_saves() {
    let dir = temp_dir("hostile-outline");
    let mut rng = Rng::new(99);
    let (mut opened, mut refused, mut edited) = (0, 0, 0);
    let mut files: Vec<Vec<u8>> = (0..8).flat_map(|kind| [1, 4].map(|pages| hostile_pdf(kind, pages))).collect();
    // And the proper outline with bytes damaged: any of them may be a wrong number, a wrong name, or a cut.
    let good = crate::testutil::pdf_with_outline();
    for _ in 0..120 * scale().min(3) {
        let mut bytes = good.clone();
        for _ in 0..1 + rng.below(12) {
            let at = rng.below(bytes.len());
            bytes[at] = rng.next() as u8;
        }
        if rng.chance(0.2) {
            bytes.truncate(rng.below(bytes.len()));
        }
        files.push(bytes);
    }
    for (i, bytes) in files.iter().enumerate() {
        let path = dir.join(format!("h{i}.pdf"));
        std::fs::write(&path, bytes).expect("write");
        let Ok((mut doc, strokes)) = Doc::open(&path) else {
            refused += 1;
            continue;
        };
        opened += 1;
        let items = doc.take_outline();
        assert!(items.len() <= 20_000, "file {i}: {} outline entries", items.len());
        for entry in &items {
            assert!(entry.depth <= 40, "file {i}: an entry {} deep", entry.depth);
            assert!(entry.title.chars().count() <= 200 && !entry.title.chars().any(char::is_control), "file {i}: a title that is not clean");
            assert!(entry.page.is_none_or(|p| p < doc.page_count()), "file {i}: an entry goes to page {:?} of {}", entry.page, doc.page_count());
        }
        let mut ed = Editor::new([1100.0, 800.0], 1.0, crate::motion::Prefs::REDUCED);
        ed.load(doc.layout(), strokes);
        ed.outline = items;
        ed.links = doc.take_links();
        ed.open_overview();
        for _ in 0..10 {
            let n = ed.layout.len();
            let edit = match rng.below(5) {
                0 => PageEdit::Insert { at: rng.below(n + 1), size: [612.0, 792.0] },
                1 => PageEdit::delete(rng.below(n)),
                2 => PageEdit::Duplicate { pages: vec![rng.below(n)] },
                3 => PageEdit::Move { pages: vec![rng.below(n)], to: rng.below(n) },
                _ => PageEdit::Arrange { order: (0..n).rev().collect() },
            };
            edited += usize::from(ed.edit_pages(edit));
            follow_pages(&mut ed, &mut doc);
            check_overview(&ed, &doc, &format!("file {i}"));
            if rng.chance(0.3) {
                ed.undo();
                follow_pages(&mut ed, &mut doc);
            }
        }
        // Saved and read again, whatever the file was like: an error is fine, a panic is not.
        let _ = doc.save(&ed.ink);
        let _ = Doc::open(&path);
    }
    eprintln!("hostile files: {opened} opened, {refused} refused; page edits made on them: {edited}");
    assert!(opened > 10 && edited > 50, "most of them should open and take edits ({opened} opened, {edited} edits)");
    let _ = crate::pdf_read::outline; // (the reader under test, named so that a rename here is noticed)
    let _ = std::fs::remove_dir_all(dir);
}

/// What a document and its overview have come to: nothing of how long it took to get there.
#[derive(Debug, PartialEq)]
struct Outcome {
    pages: Vec<[u32; 2]>,
    strokes: Vec<(u64, u32, usize)>,
    selection: Vec<u64>,
    tool: Tool,
    can_undo: bool,
    can_redo: bool,
    overview: Option<(bool, Vec<usize>, u32)>,
    prompt: bool,
}

fn outcome(ed: &Editor) -> Outcome {
    Outcome {
        pages: (0..ed.layout.len()).filter_map(|i| ed.layout.page(i)).map(|p| [p.w.to_bits(), p.h.to_bits()]).collect(),
        strokes: ed.ink.strokes().iter().map(|s| (s.id, s.page, s.pts.len())).collect(),
        selection: ed.selection.clone(),
        tool: ed.tool,
        can_undo: ed.ink.can_undo(),
        can_redo: ed.ink.can_redo(),
        overview: ed.chrome.overview.as_ref().map(|o| (o.edit, o.selected.iter().copied().collect(), o.scroll.to_bits())),
        prompt: ed.chrome.prompt.is_some(),
    }
}

/// Let everything that moves arrive (frames of a sixtieth of a second until nothing asks for another).
fn arrive(ed: &mut Editor) {
    for _ in 0..600 {
        if !ed.step_view(1.0 / 60.0) {
            break;
        }
    }
}

/// The same random events, one at a time with everything arriving in between, once with motion reduced and once not: the
/// document and its overview must be in the same place after each.
fn run_with(prefs: crate::motion::Prefs, seed: u64, rounds: usize) -> Vec<Outcome> {
    let mut ed = Editor::new([1280.0, 800.0], 1.25, crate::motion::Prefs::REDUCED);
    ed.load(crate::view::Layout::new(&[[612.0, 792.0]; 12]), vec![]);
    ed.motions.set_prefs(prefs);
    let mut rng = Rng::new(seed);
    let mut seen = Vec::new();
    for round in 0..rounds {
        if ed.layout.len() > 30 {
            let n = ed.layout.len();
            ed.edit_pages(PageEdit::Delete { pages: (15..n).collect(), grave: 0 });
        }
        let size = ed.view.viewport;
        let (layout, open) = (ed.overview_layout(), ed.overview_open());
        let spot = |rng: &mut Rng| -> [f32; 2] {
            if open && rng.chance(0.8) {
                [rng.range(layout.card[0], layout.card[2]), rng.range(layout.card[1], layout.card[3])]
            } else {
                [rng.range(0.0, size[0]), rng.range(0.0, size[1])]
            }
        };
        match rng.below(14) {
            0 | 1 => ed.open_overview(),
            2..=4 => {
                let at = spot(&mut rng);
                mouse_click(&mut ed, at);
            }
            5 => {
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                mouse_drag(&mut ed, a, b, 1 + rng.below(6));
            }
            6 => {
                ed.mouse_moved(spot(&mut rng));
                ed.wheel(Wheel::Lines(0.0, rng.range(-4.0, 4.0)));
            }
            7 => {
                ed.press(rng.pick(&[Key::Escape, Key::Delete, Key::Home(false), Key::End(false), Key::Arrow(Dir::Down, false)]));
            }
            8 => {
                ed.shortcut(rng.pick(&["a", "d", "z", "y"]), false);
            }
            9 => {
                let n = ed.layout.len();
                let edit = match rng.below(4) {
                    0 => PageEdit::Insert { at: rng.below(n + 1), size: [612.0, 792.0] },
                    1 => PageEdit::delete(rng.below(n)),
                    2 => PageEdit::Duplicate { pages: vec![rng.below(n)] },
                    _ => PageEdit::Move { pages: vec![rng.below(n)], to: rng.below(n) },
                };
                ed.edit_pages(edit);
            }
            10 => {
                let at = ed.toolbar().centre_of(crate::toolbar::Item::Tool(rng.pick(&Tool::ALL)));
                mouse_click(&mut ed, at);
            }
            11 => {
                let path: Vec<[f32; 2]> = (0..3 + rng.below(4)).map(|_| spot(&mut rng)).collect();
                pen_path(&mut ed, PenButtons { in_range: true, contact: true, eraser: false, barrel: false }, &path);
            }
            12 => {
                let at = spot(&mut rng);
                tap_pen(&mut ed, at, 1000.0);
            }
            _ => {
                if round % 40 == 0 && !ed.overview_open() {
                    ed.ask_unsaved();
                } else {
                    ed.undo();
                }
            }
        }
        arrive(&mut ed);
        seen.push(outcome(&ed));
    }
    seen
}

#[test]
fn stress_motion_changes_how_long_things_take_and_never_where_they_end_up() {
    for seed in [3, 77, 2024] {
        let rounds = 400 * scale().min(3);
        let still = run_with(crate::motion::Prefs::REDUCED, seed, rounds);
        let moving = run_with(crate::motion::Prefs::FULL, seed, rounds);
        // (It has to have done something: the overview was up and edited, pages changed, ink was drawn.)
        assert!(moving.iter().any(|o| o.overview.as_ref().is_some_and(|v| v.0)), "seed {seed}: edit mode was reached");
        assert!(moving.iter().any(|o| !o.strokes.is_empty()) && moving.windows(2).any(|w| w[0].pages != w[1].pages), "seed {seed}");
        for (round, (a, b)) in still.iter().zip(&moving).enumerate() {
            assert_eq!(a, b, "seed {seed}, round {round}: with motion on, things came to something else");
        }
    }
}
