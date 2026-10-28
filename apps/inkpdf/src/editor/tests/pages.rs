use super::*;

/// The view scrolled to the bottom of the two pages.
fn at_the_end() -> Editor {
    let mut ed = editor();
    ed.pan_px(0.0, -1.0e6);
    assert!(ed.view.at_end(&ed.layout));
    ed
}

/// One finger drags from `from` by `step` pixels, `n` times, without lifting.
fn drag(ed: &mut Editor, from: [f32; 2], step: [f32; 2], n: usize) -> [f32; 2] {
    let mut at = from;
    ed.pointer_event(touch(1, Phase::Down, at, 30.0));
    for _ in 0..n {
        at = [at[0] + step[0], at[1] + step[1]];
        ed.pointer_event(touch(1, Phase::Move, at, 30.0));
    }
    at
}

#[test]
fn pulling_up_past_the_last_page_asks_for_a_blank_page_once() {
    let mut ed = at_the_end();
    let end = drag(&mut ed, [600.0, 700.0], [0.0, -10.0], 8);
    assert!(!ed.requests.has(&Request::AddPage), "80 pixels is not far enough");
    assert!(ed.pull_progress() > 0.3 && ed.pull_progress() < 1.0);
    drag_on(&mut ed, end, [0.0, -10.0], 10);
    assert!(ed.requests.has(&Request::AddPage), "140 pixels is");
    ed.requests.take(&Request::AddPage);
    drag_on(&mut ed, [600.0, 500.0], [0.0, -10.0], 20);
    assert!(!ed.requests.has(&Request::AddPage), "one page per pull, however long the finger keeps going");
}

/// Carry on dragging the finger already down.
fn drag_on(ed: &mut Editor, from: [f32; 2], step: [f32; 2], n: usize) {
    let mut at = from;
    for _ in 0..n {
        at = [at[0] + step[0], at[1] + step[1]];
        ed.pointer_event(touch(1, Phase::Move, at, 30.0));
    }
}

#[test]
fn a_new_pull_after_lifting_can_add_another_page() {
    let mut ed = at_the_end();
    let end = drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 16);
    ed.pointer_event(touch(1, Phase::Up, end, 30.0));
    assert!(ed.requests.has(&Request::AddPage));
    ed.requests.take(&Request::AddPage);
    assert_eq!(ed.pull_progress(), 0.0, "lifting ends the pull");
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 16);
    assert!(ed.requests.has(&Request::AddPage));
}

#[test]
fn scrolling_up_the_page_normally_adds_nothing() {
    let mut ed = editor();
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 40);
    assert!(!ed.requests.has(&Request::AddPage));
    assert_eq!(ed.pull_progress(), 0.0);
    assert!(ed.view.scroll[1] > 100.0, "the page did scroll");
}

#[test]
fn easing_back_down_gives_the_pull_up() {
    let mut ed = at_the_end();
    let end = drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 10);
    let high = ed.pull_progress();
    drag_on(&mut ed, end, [0.0, 10.0], 6);
    assert!(ed.pull_progress() < high);
    drag_on(&mut ed, end, [0.0, 10.0], 20);
    assert_eq!(ed.pull_progress(), 0.0, "and coming back far enough starts over");
    assert!(!ed.requests.has(&Request::AddPage));
}

#[test]
fn only_a_finger_adds_pages() {
    let mut ed = at_the_end();
    // The wheel scrolls to the end and stops.
    for _ in 0..30 {
        ed.wheel(MouseScrollDelta::LineDelta(0.0, -5.0));
    }
    assert!(!ed.requests.has(&Request::AddPage));
    // A pen dragging at the end draws.
    let a = on_page(&ed, 100.0, 100.0);
    pen_stroke(&mut ed, TIP, &[a, [a[0], a[1] - 400.0]]);
    assert!(!ed.requests.has(&Request::AddPage));
}

#[test]
fn a_second_finger_turns_the_pull_into_a_pinch() {
    let mut ed = at_the_end();
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 8);
    assert!(ed.pull_progress() > 0.0);
    ed.pointer_event(touch(2, Phase::Down, [700.0, 700.0], 30.0));
    ed.pointer_event(touch(2, Phase::Move, [720.0, 700.0], 30.0));
    assert_eq!(ed.pull_progress(), 0.0);
}

#[test]
fn a_document_that_fits_the_window_can_still_be_pulled() {
    let mut ed = Editor::new([1200.0, 1500.0], 1.0);
    ed.load(Layout::new(&[[300.0, 300.0]]), vec![]);
    assert!(ed.view.at_end(&ed.layout), "it all fits, so it is always at its end");
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 16);
    assert!(ed.requests.has(&Request::AddPage));
}

#[test]
fn an_added_page_is_marked_changed_and_shown() {
    let mut ed = at_the_end();
    assert!(!ed.ink.is_dirty());
    ed.page_inserted(Layout::new(&[[612.0, 792.0]; 3]), 2);
    assert!(ed.ink.is_dirty(), "the new page has to be written when saving");
    assert_eq!(ed.ink.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![2]);
    settle(&mut ed);
    let page = ed.layout.page(2).expect("page");
    assert!((ed.view.scroll[1] - (page.y - crate::view::DOC_MARGIN)).abs() < 1.0 || ed.view.at_end(&ed.layout));
}

#[test]
fn the_close_button_asks_to_close_and_that_is_the_only_way() {
    let mut ed = editor();
    let c = ed.toolbar().centre_of(Item::Close);
    mouse_click_at(&mut ed, c);
    assert!(ed.requests.has(&Request::Close), "the toolbar button");
    ed.requests.take(&Request::Close);
    pen_stroke(&mut ed, TIP, &[c, c]);
    assert!(ed.requests.has(&Request::Close), "pressed with the pen");
    assert_eq!(ed.ink.len(), 0);
}
/// An editor with two 612 x 792 pages and a stroke on each.
fn two_pages_with_ink() -> (Editor, u64, u64) {
    let mut ed = editor();
    let first = add_stroke(&mut ed, &[[100.0, 100.0], [200.0, 140.0]]);
    let second_page = ed.layout.page(1).expect("page");
    let second = ed
        .ink
        .add(
            1,
            Style { rgba: [0, 0, 0, 255], width: 2.0 },
            vec![[second_page.x + 50.0, second_page.y + 60.0], [second_page.x + 150.0, second_page.y + 90.0]],
        )
        .expect("stroke");
    (ed, first, second)
}

#[test]
fn the_plus_button_asks_for_a_page_after_the_one_being_looked_at() {
    let mut ed = editor();
    let plus = ed.toolbar().centre_of(Item::AddPage);
    mouse_click_at(&mut ed, plus);
    assert_eq!(ed.requests.insert_page(), Some(0), "looking at the first page");
    assert!(!ed.requests.has(&Request::AddPage), "it is not the pull at the end");
    ed.requests.take_insert_page();
    ed.pan_px(0.0, -1.0e6);
    pen_stroke(&mut ed, TIP, &[plus, plus]);
    assert_eq!(ed.requests.insert_page(), Some(1), "now the second");
    assert_eq!(ed.ink.len(), 0);
    // A finger can press it too.
    ed.requests.take_insert_page();
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(2, Phase::Down, plus, 30.0));
    ed.pointer_event(touch(2, Phase::Up, plus, 30.0));
    assert_eq!(ed.requests.insert_page(), Some(1));
}

#[test]
fn a_page_inserted_among_the_others_carries_the_ink_on_the_later_pages_down_with_them() {
    let (mut ed, first, second) = two_pages_with_ink();
    let (a_before, b_before) = (ed.ink.get(first).expect("first").pts.clone(), ed.ink.get(second).expect("second").pts.clone());
    // A page after the first: 612 x 792 again.
    let new = Layout::new(&[[612.0, 792.0]; 3]);
    let gap = new.page(2).expect("page").y - ed.layout.page(1).expect("page").y;
    ed.page_inserted(new, 1);
    assert_eq!(ed.ink.get(first).expect("first").page, 0);
    assert_eq!(ed.ink.get(first).expect("first").pts, a_before, "the first page's ink did not move");
    let moved = ed.ink.get(second).expect("second");
    assert_eq!(moved.page, 2, "the second page's ink is on page 3 now");
    assert!(
        moved.pts.iter().zip(&b_before).all(|(n, o)| (n[0] - o[0]).abs() < 1e-3 && (n[1] - o[1] - gap).abs() < 1e-3),
        "and kept its place on that page"
    );
    let page = ed.layout.page(2).expect("page");
    assert!(moved.bbox()[1] > page.y && moved.bbox()[3] < page.bottom(), "it is on the page it says");
    assert_eq!(
        ed.ink.dirty_pages().iter().copied().collect::<Vec<_>>(),
        vec![0, 1, 2],
        "the page that had ink, the page it moved to, and the new page are all unsaved"
    );
    // Undo and redo still work on the moved ink: drawing on the old page-2 ink, then undoing.
    ed.undo();
    assert!(ed.ink.get(second).is_none(), "undo took away the last thing drawn, wherever it now is");
    ed.redo();
    let back = ed.ink.get(second).expect("second again");
    assert_eq!(back.page, 2);
    assert!((back.pts[0][1] - b_before[0][1] - gap).abs() < 1e-3, "redo brought it back where it is now, not where it was");
}

#[test]
fn inserting_a_page_stops_what_the_pen_was_doing_and_clears_the_laser_and_shows_the_new_page() {
    let (mut ed, _, _) = two_pages_with_ink();
    ed.set_tool(Tool::Laser);
    let path = [on_page(&ed, 100.0, 300.0), on_page(&ed, 200.0, 320.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert!(ed.laser().is_fading());
    ed.page_inserted(Layout::new(&[[612.0, 792.0]; 3]), 1);
    assert!(!ed.laser().is_fading());
    assert!(ed.is_idle());
    settle(&mut ed);
    let page = ed.layout.page(1).expect("page");
    assert!((ed.view.scroll[1] - (page.y - crate::view::DOC_MARGIN)).abs() < 1.0, "scrolled to the new page");
}

#[test]
fn a_page_added_at_the_end_moves_nothing() {
    let (mut ed, first, second) = two_pages_with_ink();
    let (a, b) = (ed.ink.get(first).expect("a").pts.clone(), ed.ink.get(second).expect("b").pts.clone());
    ed.page_inserted(Layout::new(&[[612.0, 792.0]; 3]), 2);
    assert_eq!((ed.ink.get(first).expect("a").pts.clone(), ed.ink.get(second).expect("b").pts.clone()), (a, b));
    assert_eq!(ed.ink.get(second).expect("b").page, 1);
}
