//! The cards, pills and keys that browse and annotate (sticky notes, web links, the back and forward pill, the search card, the soft keyboard, the card that
//! says something went wrong), thrown at the editor in any order with the window changing size under them. After every event: nothing that floats is
//! outside the window or not a number, there is at most one card at the right, and nothing panics.

use super::*;

const SIZES: [[f32; 2]; 8] =
    [[360.0, 480.0], [720.0, 1280.0], [1080.0, 2400.0], [2400.0, 1080.0], [1366.0, 768.0], [1920.0, 1080.0], [3840.0, 2160.0], [800.0, 600.0]];

fn with_everything() -> Editor {
    let exts: Vec<_> = inkpdf_ext::all().into_iter().filter(|e| ["notes", "weblink", "laser"].contains(&e.id)).collect();
    let mut ed = Editor::with_extensions([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED, &exts);
    ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.links = vec![
        Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Page(2) },
        Link { rect: [112.0, 402.0, 312.0, 422.0], target: Target::Url("https://example.com/path".into()) },
        Link { rect: [112.0, 502.0, 312.0, 522.0], target: Target::Page(0) },
    ];
    ed.set_text_files(vec![Some(0), Some(1), Some(2)]);
    for page in 0..3 {
        ed.add_text(page, Some(std::sync::Arc::new(crate::pagetext::FakeText::new(&["hello world", "hello again", "something else", "hello"]))));
    }
    ed
}

/// Everything that floats over the page, as rectangles.
fn floats(ed: &Editor) -> Vec<(&'static str, [f32; 4])> {
    let mut all = vec![("toolbar", ed.toolbar().pills[0]), ("toolbar", ed.toolbar().pills[1]), ("toolbar", ed.toolbar().pills[2])];
    all.extend(ed.context_bar().map(|b| ("bar", b.rect)));
    all.extend(ed.search_layout().map(|c| ("search", c.card)));
    all.extend(ed.item_card_layout().map(|c| ("item card", c.card)));
    all.extend(ed.link_layout().map(|c| ("link card", c.card)));
    all.extend(ed.trail_layout().map(|c| ("trail", c.card)));
    all.extend(ed.error_layout().map(|c| ("error", c.card)));
    all.extend(ed.theme_layout().map(|c| ("theme", c.rect)));
    all
}

fn check_floats(ed: &Editor, what: &str) {
    let [w, h] = ed.view.viewport;
    for (name, r) in floats(ed) {
        assert!(r.iter().all(|v| v.is_finite()), "{what}: {name} is not a number: {r:?}");
        let empty = r[2] <= r[0];
        assert!(empty || (r[0] >= -0.5 && r[1] >= -0.5 && r[2] <= w + 0.5 && r[3] <= h + 0.5), "{what}: {name} {r:?} is outside the window {w}x{h}");
    }
    let rights = [ed.search_layout().is_some(), ed.item_card_layout().is_some(), ed.link_layout().is_some()].iter().filter(|b| **b).count();
    assert!(rights <= 1, "{what}: {rights} cards at the right at once");
}

/// A press of the stylus's tip, or of the left button, at `at`.
fn tap(ed: &mut Editor, rng: &mut Rng, at: [f32; 2]) {
    if rng.chance(0.5) {
        ed.pointer_event(pen_at(Phase::Down, at));
        ed.pointer_event(pen_at(Phase::Up, at));
    } else {
        ed.mouse_moved(at);
        ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
        ed.mouse_button(ButtonState::Released, MouseButton::Left);
    }
}

fn middle(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

#[test]
fn stress_the_cards_pills_and_keys_that_browse_and_annotate_in_any_order_in_any_size_of_window() {
    let mut seen = std::collections::BTreeMap::<&'static str, usize>::new();
    for seed in 1..=4u64 {
        let mut rng = Rng::new(seed * 104_729);
        let mut ed = with_everything();
        let notes = Tool::from_name("notes");
        for i in 0..(8_000 * scale()) {
            let what = match rng.below(29) {
                0 => {
                    let size = rng.pick(&SIZES);
                    ed.set_viewport(size);
                    ed.step_view(0.016);
                    "resize"
                }
                1 | 2 => {
                    // A link, followed with a finger or a click.
                    let at = ed.view.doc_to_screen([112.0 + rng.range(0.0, 200.0), 300.0 + 100.0 * rng.below(3) as f32 + 10.0]);
                    if rng.chance(0.5) {
                        ed.pointer_event(PointerEvent {
                            device: Device::Touch,
                            id: 1,
                            phase: Phase::Down,
                            samples: vec![Sample { pos: at, pressure: 1.0, t_ms: 1.0 }],
                            pen: PenButtons::default(),
                            contact_px: 20.0,
                        });
                        ed.pointer_event(PointerEvent {
                            device: Device::Touch,
                            id: 1,
                            phase: Phase::Up,
                            samples: vec![Sample { pos: at, pressure: 1.0, t_ms: 2.0 }],
                            pen: PenButtons::default(),
                            contact_px: 20.0,
                        });
                    } else {
                        tap(&mut ed, &mut rng, at);
                    }
                    "link"
                }
                3 | 4 => {
                    if let Some(p) = ed.trail_layout() {
                        let item = rng.pick(&[crate::toolbar::TrailItem::Back, crate::toolbar::TrailItem::Forward]);
                        tap(&mut ed, &mut rng, p.centre_of(item));
                    }
                    "trail button"
                }
                5 => {
                    ed.press(Key::Trail(rng.chance(0.5)));
                    "trail key"
                }
                6 | 7 => {
                    if let Some(l) = ed.link_layout() {
                        let parts = l.named_parts();
                        let at = middle(parts[rng.below(parts.len())].1);
                        tap(&mut ed, &mut rng, at);
                    }
                    "link card"
                }
                8 | 9 => {
                    if let Some(l) = ed.item_card_layout() {
                        let parts = l.named_parts();
                        let at = middle(parts[rng.below(parts.len())].1);
                        tap(&mut ed, &mut rng, at);
                    }
                    "item card"
                }
                10 => {
                    if let Some(note) = notes {
                        ed.set_tool(note);
                    }
                    let at = ed.view.doc_to_screen([rng.range(0.0, 600.0), rng.range(0.0, 2300.0)]);
                    tap(&mut ed, &mut rng, at);
                    "note tool"
                }
                11 => {
                    ed.toggle_search();
                    "search"
                }
                12 => {
                    let key = match rng.below(6) {
                        0 => Key::Text("some words "),
                        1 => Key::Text("\n"),
                        2 => Key::Escape,
                        3 => Key::Delete,
                        4 => Key::Backspace,
                        _ => Key::Arrow(rng.pick(&[Dir::Up, Dir::Down, Dir::Left, Dir::Right]), false),
                    };
                    ed.press(key);
                    "key"
                }
                13 => {
                    ed.show_error("Could not save:\nThe disk is full and so on and on and on");
                    "error"
                }
                14 => {
                    if let Some(l) = ed.error_layout() {
                        tap(&mut ed, &mut rng, l.close_middle());
                    }
                    "error close"
                }
                15 => {
                    let covered = rng.pick(&[0.0, 300.0, 700.0, 1.0e6, f32::NAN]);
                    ed.set_keyboard_covered(covered);
                    ed.step_view(0.016);
                    "keyboard"
                }
                16 => {
                    ed.insert_page_after(rng.below(3));
                    "add page"
                }
                17 => {
                    ed.undo();
                    "undo"
                }
                18 => {
                    ed.redo();
                    "redo"
                }
                19 => {
                    let at = ed.view.doc_to_screen([rng.range(0.0, 600.0), rng.range(0.0, 2300.0)]);
                    ed.pan_px(rng.range(-300.0, 300.0), rng.range(-600.0, 600.0));
                    let _ = at;
                    "pan"
                }
                20 => {
                    if let Some(note) = notes {
                        ed.set_tool(note);
                    }
                    ed.set_tool(rng.pick(&[Tool::Select, Tool::Pen, Tool::Lasso, Tool::Eraser, Tool::Text]));
                    "tool"
                }
                21 => {
                    ed.chrome.theme_menu = rng.chance(0.5);
                    ed.chrome.panel = rng.chance(0.3).then(crate::toolbar::PanelState::default);
                    "menus"
                }
                26 => {
                    // The slides begin (with the button or the shortcut) or end, whatever else is up.
                    if rng.chance(0.5) {
                        ed.shortcut("l", false);
                    } else {
                        let at = ed.toolbar().centre_of(crate::toolbar::Item::Present);
                        tap(&mut ed, &mut rng, at);
                    }
                    "slides"
                }
                27 => {
                    let key = rng.pick(&[
                        Key::Page(true),
                        Key::Page(false),
                        Key::Home(false),
                        Key::End(false),
                        Key::Arrow(Dir::Left, false),
                        Key::Arrow(Dir::Right, false),
                        Key::Escape,
                    ]);
                    ed.press(key);
                    ed.wheel(Wheel::Lines(0.0, rng.range(-3.0, 3.0)));
                    "slide keys"
                }
                _ => {
                    ed.step_view(0.016);
                    "frame"
                }
            };
            *seen.entry(what).or_default() += 1;
            let _ = ed.take_redraw();
            check_floats(&ed, &format!("seed {seed}, event {i} ({what})"));
            if let Some(v) = ed.presentation_view() {
                let r = v.page_rect.map(f32::from_bits);
                let [w, h] = ed.view.viewport;
                assert!(v.page < ed.layout.len(), "seed {seed}, event {i}: slide {} of {}", v.page, ed.layout.len());
                assert!(
                    r.iter().all(|x| x.is_finite()) && r[0] >= -1.0 && r[1] >= -1.0 && r[2] <= w + 1.0 && r[3] <= h + 1.0,
                    "seed {seed}, event {i}: the page {r:?} in {w}x{h}"
                );
            }
            if i % 200 == 0 {
                check_editor(&ed, &format!("seed {seed}, event {i} ({what})"));
            }
        }
    }
    for needed in
        ["link", "trail button", "trail key", "link card", "item card", "note tool", "search", "error", "keyboard", "resize", "slides", "slide keys"]
    {
        assert!(seen.get(needed).copied().unwrap_or(0) > 100, "the storm did {needed} only {:?} times: {seen:?}", seen.get(needed));
    }
}
