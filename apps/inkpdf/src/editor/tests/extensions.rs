//! Extensions: a tool that is not the program's own, registered from outside, is on the toolbar, takes its input, shows what it paints and
//! keeps the frames coming while it moves, with nothing of the program changed to make room for it; and what every extension must be
//! (named, served by this API, with an icon that draws) is checked for those this build has.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::input::Mode;
use crate::tools::Key;
use inkpdf_api as api;

use super::*;

static DOWNS: AtomicUsize = AtomicUsize::new(0);
static TICKS_LEFT: AtomicUsize = AtomicUsize::new(0);

/// A tool made in this file: it counts presses, paints a line where the pointer is, moves by itself for as long as `TICKS_LEFT` says and
/// listens to the keyboard.
#[derive(Default)]
struct Dummy {
    down: Option<(api::Device, u32)>,
    at: Option<api::P>,
    keys: String,
}

impl api::Tool for Dummy {
    fn policy(&self, desktop: bool) -> api::Policy {
        api::Policy { keyboard: true, ..api::Policy::stylus_tool(desktop) }
    }
    fn pointer(&mut self, cx: &mut dyn api::Context, ev: &api::PointerEvent<'_>) {
        if let Some(last) = ev.samples.last() {
            self.at = Some(cx.to_doc(last.pos));
        }
        match ev.phase {
            api::Phase::Down => {
                DOWNS.fetch_add(1, Ordering::SeqCst);
                self.down = Some((ev.device, ev.id));
            }
            api::Phase::Up | api::Phase::Cancel => self.down = None,
            _ => {}
        }
        cx.request_redraw();
    }
    fn owns(&self, device: api::Device, id: u32) -> bool {
        self.down == Some((device, id))
    }
    fn busy(&self) -> bool {
        self.down.is_some()
    }
    fn end(&mut self, _cx: &mut dyn api::Context) {
        self.down = None;
    }
    fn cancel(&mut self, _cx: &mut dyn api::Context) {
        self.down = None;
    }
    fn key(&mut self, _cx: &mut dyn api::Context, key: api::Key<'_>) -> bool {
        if let api::Key::Text(t) = key {
            self.keys.push_str(t);
        }
        true
    }
    fn tick(&mut self, _now_ms: f64) -> bool {
        TICKS_LEFT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok()
    }
    fn paint(&self, cx: &dyn api::Context, out: &mut dyn api::Painter) {
        if let Some(at) = self.at {
            out.doc_line(at, at, 3.0 / cx.zoom(), cx.theme().accent);
        }
    }
}

const DUMMY_ICON: &str = "<svg viewBox='0 0 56 56'><path d='M10 10L46 46'/><path d='M46 10L10 46'/></svg>";

fn dummy() -> api::Extension {
    api::Extension { api: api::API_VERSION, id: "dummy", label: "Dummy", icon_svg: DUMMY_ICON, create: || Box::<Dummy>::default() }
}

fn with_dummy() -> Editor {
    let mut ed = Editor::with_extensions([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED, &[dummy()]);
    ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed
}

const DUMMY: Tool = Tool::named("dummy");

#[test]
fn a_tool_registered_from_outside_is_on_the_toolbar_and_the_others_do_not_have_it() {
    let ed = with_dummy();
    assert_eq!(ed.toolbar().tools().last(), Some(&DUMMY), "after the program's own tools");
    assert_eq!(ed.toolbar().tools().len(), 7);
    assert!(editor().toolbar().tool_index(DUMMY).is_none(), "an editor without it has no button for it");
    assert!(!Tool::all().contains(&DUMMY), "and the program's own list is not touched");
    // The button is drawn (its icon is the extension's own).
    let mut with = crate::toolbar::Ui::default();
    ed.draw_interface(&mut with);
    let mut without = crate::toolbar::Ui::default();
    Editor::with_extensions([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED, &[]).draw_interface(&mut without);
    assert!(with.lines.len() > without.lines.len(), "its icon is drawn");
}

#[test]
fn pressing_its_button_chooses_it_and_the_stylus_and_the_mouse_reach_it_and_a_finger_does_not() {
    let mut ed = with_dummy();
    ed.set_mode(Mode::Desktop);
    let at = ed.toolbar().centre_of(Item::Tool(DUMMY));
    pen_stroke(&mut ed, TIP, &[at, at]);
    assert_eq!(ed.tool, DUMMY);
    let before = DOWNS.load(Ordering::SeqCst);
    let p = on_page(&ed, 200.0, 300.0);
    pen_stroke(&mut ed, TIP, &[p, p]);
    assert_eq!(DOWNS.load(Ordering::SeqCst), before + 1, "the stylus");
    mouse_click_at(&mut ed, p);
    assert_eq!(DOWNS.load(Ordering::SeqCst), before + 2, "the mouse, as a stylus in the desktop mode");
    ed.pointer_event(touch(1, Phase::Down, p, 8.0));
    ed.pointer_event(touch(1, Phase::Up, p, 8.0));
    assert_eq!(DOWNS.load(Ordering::SeqCst), before + 2, "a finger only pans");
    assert!(ed.is_idle());
    assert_eq!(ed.ink.len(), 0, "and it drew no ink");
}

#[test]
fn it_paints_over_the_page_in_document_space_through_the_api_and_keeps_the_frames_coming_while_it_moves() {
    let mut ed = with_dummy();
    ed.set_tool(DUMMY);
    let p = on_page(&ed, 200.0, 300.0);
    pen_stroke(&mut ed, TIP, &[p, p]);
    let mut out = crate::tools::paint::tests::Recorder::default();
    ed.tools.get(DUMMY).paint(&ed, &mut out);
    assert_eq!(out.doc.len(), 1);
    let on = ed.view.doc_to_screen(out.doc[0].0);
    assert!((on[0] - p[0]).abs() < 0.01 && (on[1] - p[1]).abs() < 0.01, "the point it was given, in the document");
    assert_eq!(out.doc[0].3, ed.palette().accent, "in the colour of the look");
    TICKS_LEFT.store(3, Ordering::SeqCst);
    assert!(ed.tick_tools(0.0) && ed.tick_tools(0.0) && ed.tick_tools(0.0), "moving by itself");
    assert!(!ed.tick_tools(0.0), "and then still: no frames are asked for");
}

#[test]
fn the_keyboard_goes_to_it_when_its_policy_says_so_and_a_new_document_makes_it_afresh() {
    let mut ed = with_dummy();
    ed.set_mode(Mode::Desktop);
    ed.set_tool(DUMMY);
    assert!(ed.press(Key::Text("hi")));
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(ed.toolbar().tool_index(DUMMY).is_some(), "still registered");
    assert!(!ed.tools.get(DUMMY).busy());
}

#[test]
fn a_tool_that_is_not_registered_is_the_pens_and_a_bad_extension_is_not_served() {
    let ed = editor();
    assert_eq!(ed.tools.get(DUMMY).busy(), ed.tools.get(Tool::Pen).busy(), "a name left in the settings by an extension that is gone");
    let bad = [
        api::Extension { api: 99, ..dummy() },
        api::Extension { id: "Has Caps", ..dummy() },
        api::Extension { id: "", ..dummy() },
        api::Extension { label: "  ", ..dummy() },
        api::Extension { icon_svg: "nothing", ..dummy() },
    ];
    for extension in bad {
        assert!(extension.problem().is_some(), "{:?}", extension.id);
    }
    assert!(dummy().problem().is_none());
}

#[test]
fn every_extension_this_build_has_is_served_named_once_and_draws_an_icon_inside_its_box() {
    let all = inkpdf_ext::all();
    assert!(!all.is_empty());
    let mut names = std::collections::HashSet::new();
    for e in &all {
        assert_eq!(e.problem(), None, "{}", e.id);
        assert!(names.insert(e.id), "{} is registered twice", e.id);
        assert!(Tool::from_name(e.id).is_some(), "{} is in the registry", e.id);
        assert!(crate::toolbar::icon_is_drawable(e.icon_svg), "{}'s icon draws something inside its box", e.id);
        assert!(!["pen", "geometry", "text", "select"].contains(&e.id), "{} is a name of the program's own", e.id);
    }
}

#[test]
fn an_extension_tool_costs_the_pens_path_next_to_nothing() {
    // The pen's path has about two and a half milliseconds from sample to frame: an extension's tool is given a small part of that.
    let mut ed = with_dummy();
    ed.set_tool(DUMMY);
    let p = on_page(&ed, 200.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[p]));
    let moves: Vec<_> = (0..4000).map(|i| [p[0] + (i % 300) as f32, p[1] + (i % 200) as f32]).collect();
    let started = std::time::Instant::now();
    for chunk in moves.chunks(4) {
        ed.pointer_event(pen(Phase::Move, TIP, chunk));
    }
    let per_event = started.elapsed().as_secs_f64() / 1000.0;
    ed.pointer_event(pen(Phase::Up, TIP, &[p]));
    assert!(per_event < 150e-6, "{:.1} microseconds an event (four samples each)", per_event * 1e6);
}
