use super::*;
use crate::links::{Link, Target};

/// A link over a band of page 0, 100 to 300 points across and 190 to 210 down (below the bars at the top of the window).
fn with_links(ed: &mut Editor, target: Target) {
    ed.links = vec![Link { rect: [112.0, 202.0, 312.0, 222.0], target }];
}

#[test]
fn a_finger_tap_on_a_web_link_asks_to_open_it() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
    finger_tap(&mut ed, on);
    assert_eq!(ed.requests.url(), Some("https://example.com/"));
}

#[test]
fn a_finger_tap_on_a_page_link_glides_to_that_page() {
    let mut ed = editor();
    with_links(&mut ed, Target::Page(1));
    assert!(ed.view.scroll[1] < 100.0);
    let on = on_page(&ed, 150.0, 200.0);
    finger_tap(&mut ed, on);
    settle(&mut ed);
    let page = ed.layout.page(1).expect("page 1");
    // As near the top of the window as the end of the document allows, with the page's first line below the bars (they float over the page).
    let bars = ed.context_bar().map_or(ed.toolbar().bottom(), |c| c.rect[3]) + 8.0 * ed.dpr;
    let wanted = (page.y - bars / ed.view.zoom).min(ed.layout.height() - ed.view.viewport[1] / ed.view.zoom);
    assert!(ed.view.scroll[1] > 500.0 && (ed.view.scroll[1] - wanted).abs() < 1.0, "scroll {} wanted {wanted}", ed.view.scroll[1]);
    let top_of_page = ed.view.doc_to_screen([page.x, page.y])[1];
    assert!(
        top_of_page >= ed.toolbar().bottom() || wanted != page.y - bars / ed.view.zoom,
        "the page begins at {top_of_page}, under the bars that end at {}",
        ed.toolbar().bottom()
    );
    assert!(ed.requests.url().is_none());
}

#[test]
fn a_tap_off_the_link_does_nothing() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let off = on_page(&ed, 150.0, 300.0);
    finger_tap(&mut ed, off);
    assert!(ed.requests.url().is_none());
}

#[test]
fn a_mouse_click_never_follows_a_link() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
    mouse_click_at(&mut ed, on);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Right);
    ed.mouse_button(ButtonState::Released, MouseButton::Right);
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
    let on = on_page(&ed, 150.0, 200.0);
    pen_stroke(&mut ed, TIP, &[on, on]);
    assert!(ed.requests.url().is_none());
    assert_eq!(ed.ink.len(), 1, "the pen's job is ink");
}

#[test]
fn a_swipe_or_a_long_press_over_a_link_is_not_a_tap() {
    let mut ed = editor();
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
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
    let on = on_page(&ed, 150.0, 200.0);
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

fn selecting(ed: &mut Editor) {
    ed.ws.tool = Tool::Select;
}

fn mouse_press_release(ed: &mut Editor, down: [f32; 2], up: [f32; 2]) {
    ed.mouse_moved(down);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(up);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
}

#[test]
fn with_the_select_tool_a_mouse_click_follows_a_web_link() {
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
    mouse_click_at(&mut ed, on);
    assert_eq!(ed.requests.url(), Some("https://example.com/"));
}

#[test]
fn with_the_select_tool_a_mouse_click_follows_a_page_link() {
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Page(1));
    let on = on_page(&ed, 150.0, 200.0);
    mouse_click_at(&mut ed, on);
    settle(&mut ed);
    assert!(ed.view.scroll[1] > 500.0, "scroll {}", ed.view.scroll[1]);
}

#[test]
fn with_the_select_tool_a_drag_from_a_link_is_not_a_click() {
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
    mouse_press_release(&mut ed, on, [on[0] + 60.0, on[1]]);
    assert!(ed.requests.url().is_none());
}

#[test]
fn with_the_select_tool_a_click_off_the_link_does_nothing() {
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let off = on_page(&ed, 150.0, 300.0);
    mouse_click_at(&mut ed, off);
    assert!(ed.requests.url().is_none());
}

#[test]
fn with_the_select_tool_a_stylus_tap_follows_a_link() {
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    let on = on_page(&ed, 150.0, 200.0);
    pen_stroke(&mut ed, TIP, &[on, on]);
    assert_eq!(ed.requests.url(), Some("https://example.com/"));
    // A stroke across the link is a selection, not a click.
    let mut ed = editor();
    selecting(&mut ed);
    with_links(&mut ed, Target::Url("https://example.com/".into()));
    pen_stroke(&mut ed, TIP, &[on, [on[0] + 40.0, on[1] + 5.0], [on[0] + 80.0, on[1] + 5.0]]);
    assert!(ed.requests.url().is_none());
}

#[test]
fn a_finger_tap_that_wobbles_follows_a_link_on_a_dense_screen_only() {
    // 15 screen pixels is a hair's breadth at 2.6 times the scale, a swipe at 1.
    for (dpr, follows) in [(2.6, true), (1.0, false)] {
        let mut ed = editor();
        ed.set_dpr(dpr);
        // (Lower down the page than the other tests, out of the way of the bars, which grow with the scale.)
        ed.links = vec![Link { rect: [112.0, 502.0, 312.0, 522.0], target: Target::Url("https://example.com/".into()) }];
        let on = on_page(&ed, 150.0, 500.0);
        ed.pointer_event(touch(1, Phase::Down, on, 30.0));
        ed.pointer_event(touch(1, Phase::Move, [on[0] + 15.0, on[1]], 30.0));
        ed.pointer_event(touch(1, Phase::Up, [on[0] + 15.0, on[1]], 30.0));
        assert_eq!(ed.requests.url().is_some(), follows, "dpr {dpr}");
    }
}
