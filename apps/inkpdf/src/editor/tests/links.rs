use super::*;
use crate::links::{Link, Target};

/// A link over a band of page 0, 100 to 300 points across and 90 to 110 down.
fn with_links(ed: &mut Editor, target: Target) {
    ed.links = vec![Link { rect: [112.0, 102.0, 312.0, 122.0], target }];
}

fn tap_with_finger(ed: &mut Editor, at: [f32; 2]) {
    ed.pointer_event(touch(1, Phase::Down, at, 30.0));
    ed.pointer_event(touch(1, Phase::Up, at, 30.0));
}

#[test]
fn a_finger_tap_on_a_web_link_asks_to_open_it() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 100.0);
    tap_with_finger(&mut ed, on);
    assert_eq!(ed.requests.url(), Some("https://example.com/"));
}

#[test]
fn a_finger_tap_on_a_page_link_glides_to_that_page() {
    let mut ed = editor();
    with_links(&mut ed, Target::Page(1));
    assert!(ed.view.scroll[1] < 100.0);
    let on = on_page(&ed, 150.0, 100.0);
    tap_with_finger(&mut ed, on);
    settle(&mut ed);
    let page = ed.layout.page(1).expect("page 1");
    // As near the top of the window as the end of the document allows.
    let wanted = (page.y - crate::view::DOC_MARGIN).min(ed.layout.height() - ed.view.viewport[1] / ed.view.zoom);
    assert!(ed.view.scroll[1] > 500.0 && (ed.view.scroll[1] - wanted).abs() < 1.0, "scroll {} wanted {wanted}", ed.view.scroll[1]);
    assert!(ed.requests.url().is_none());
}

#[test]
fn a_tap_off_the_link_does_nothing() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let off = on_page(&ed, 150.0, 300.0);
    tap_with_finger(&mut ed, off);
    assert!(ed.requests.url().is_none());
}

#[test]
fn a_mouse_click_never_follows_a_link() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 100.0);
    mouse_click_at(&mut ed, on);
    ed.mouse_button(ElementState::Pressed, MouseButton::Right);
    ed.mouse_button(ElementState::Released, MouseButton::Right);
    assert!(ed.requests.url().is_none());
    let before = ed.view.scroll;
    with_links(&mut ed, Target::Page(1));
    mouse_click_at(&mut ed, on);
    settle(&mut ed);
    assert_eq!(ed.view.scroll, before, "a click on a page link does not move the page either");
}

#[test]
fn the_pen_never_follows_a_link_it_draws_a_dot_on_it() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 100.0);
    pen_stroke(&mut ed, TIP, &[on, on]);
    assert!(ed.requests.url().is_none());
    assert_eq!(ed.ink.len(), 1, "the pen's job is ink");
}

#[test]
fn a_swipe_or_a_long_press_over_a_link_is_not_a_tap() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 100.0);
    // A drag across the link pans the page.
    ed.pointer_event(touch(1, Phase::Down, on, 30.0));
    ed.pointer_event(touch(1, Phase::Move, [on[0] + 80.0, on[1]], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [on[0] + 80.0, on[1]], 30.0));
    assert!(ed.requests.url().is_none(), "a pan is not a tap");
    // A touch held for a while is not a tap either.
    let mut down = touch(2, Phase::Down, on, 30.0);
    down.samples[0].t_ms -= 800.0;
    ed.pointer_event(down);
    ed.pointer_event(touch(2, Phase::Up, on, 30.0));
    assert!(ed.requests.url().is_none(), "a long press is not a tap");
}

#[test]
fn a_palm_resting_on_a_link_does_not_follow_it() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 100.0);
    ed.pointer_event(touch(1, Phase::Down, on, 300.0));
    ed.pointer_event(touch(1, Phase::Up, on, 300.0));
    assert!(ed.requests.url().is_none());
}

#[test]
fn links_are_forgotten_when_another_document_loads() {
    let mut ed = editor();
    with_links(&mut ed, Target::Page(0));
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(ed.links.is_empty());
}
