//! The delete-page button: it deletes the page being looked at, without asking, and never the last page there is.

use super::*;

/// An editor on a document of `n` pages, 612 x 792 each, at 100%.
fn pages(n: usize) -> Editor {
    let mut ed = Editor::new([1200.0, 900.0], 1.0);
    ed.load(Layout::new(&vec![[612.0, 792.0]; n]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed
}

/// A stroke on `page`, 50 points in from its corner and `y` down.
fn ink_on(ed: &mut Editor, page: usize, y: f32) -> u64 {
    let r = ed.layout.page(page).expect("page");
    ed.ink.add(page as u32, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r.x + 50.0, r.y + y], [r.x + 150.0, r.y + y + 30.0]]).expect("stroke")
}

fn delete_button(ed: &Editor) -> [f32; 2] {
    ed.toolbar().centre_of(Item::DeletePage)
}

#[test]
fn the_button_asks_for_the_page_being_looked_at_with_every_device_and_asks_nobody() {
    let mut ed = pages(3);
    let button = delete_button(&ed);
    mouse_click_at(&mut ed, button);
    assert!(ed.requests.has(&Request::DeletePage(0)), "the mouse: the page being looked at is the first");
    assert!(ed.chrome.prompt.is_none(), "no question is asked");
    ed.requests.take(&Request::DeletePage(0));
    ed.pan_px(0.0, -1.0e6);
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert!(ed.requests.has(&Request::DeletePage(2)), "the stylus: now the last");
    assert_eq!(ed.ink.len(), 0, "and the press drew nothing");
    ed.requests.take(&Request::DeletePage(2));
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(2, Phase::Down, button, 30.0));
    ed.pointer_event(touch(2, Phase::Up, button, 30.0));
    assert!(ed.requests.has(&Request::DeletePage(2)), "a finger");
}

#[test]
fn the_button_works_in_both_modes_and_with_every_tool() {
    for mode in [Mode::Desktop, Mode::Tablet] {
        for tool in [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Laser] {
            let mut ed = pages(2);
            ed.set_mode(mode);
            ed.set_tool(tool);
            ed.chrome.panel = None;
            let button = delete_button(&ed);
            mouse_click_at(&mut ed, button);
            assert!(ed.requests.has(&Request::DeletePage(0)), "{mode:?} {tool:?}");
        }
    }
}

#[test]
fn the_last_page_there_is_cannot_be_deleted_and_the_button_says_so() {
    let mut ed = pages(1);
    assert!(!ed.toolbar_state().can_delete, "dim");
    let button = delete_button(&ed);
    mouse_click_at(&mut ed, button);
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert!(!ed.requests.has(&Request::DeletePage(0)), "nothing was asked for");
    assert!(pages(2).toolbar_state().can_delete, "with another page it is live");
}

#[test]
fn a_deleted_page_takes_its_ink_with_it_and_the_later_pages_come_up_with_theirs() {
    let mut ed = pages(3);
    let (a, b, c) = (ink_on(&mut ed, 0, 100.0), ink_on(&mut ed, 1, 100.0), ink_on(&mut ed, 2, 100.0));
    let c_before = ed.ink.get(c).expect("c").pts.clone();
    let gap = ed.layout.page(2).expect("page").y - ed.layout.page(1).expect("page").y;
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 2]), 1);
    assert!(ed.ink.get(b).is_none(), "the page's own ink is gone");
    assert_eq!(ed.ink.get(a).expect("a").page, 0, "the page before is untouched");
    let moved = ed.ink.get(c).expect("c");
    assert_eq!(moved.page, 1, "the page after is the second now");
    assert!(moved.pts.iter().zip(&c_before).all(|(n, o)| (n[0] - o[0]).abs() < 1e-3 && (n[1] - o[1] + gap).abs() < 1e-3), "and came up by a page");
    let page = ed.layout.page(1).expect("page");
    assert!(moved.bbox()[1] > page.y && moved.bbox()[3] < page.bottom(), "it is on the page it says");
}

#[test]
fn deleting_a_page_lets_go_of_what_was_selected_on_it_and_of_the_history() {
    let mut ed = pages(3);
    let gone = ink_on(&mut ed, 1, 100.0);
    let kept = ink_on(&mut ed, 0, 100.0);
    ed.selection = vec![gone, kept];
    assert!(ed.ink.can_undo());
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 2]), 1);
    assert_eq!(ed.selection, vec![kept], "only what is still there stays selected");
    assert!(!ed.ink.can_undo() && !ed.ink.can_redo(), "no step can refer to the page that went");
    assert!(!ed.toolbar_state().can_undo, "so the undo button is dim");
    assert!(ed.ink.is_dirty(), "and the document is changed, so it asks to be saved");
}

#[test]
fn a_stroke_under_way_when_the_page_goes_is_ended_not_left_half_done() {
    let mut ed = pages(3);
    let a = on_page(&ed, 100.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    ed.pointer_event(pen(Phase::Move, TIP, &[a, on_page(&ed, 160.0, 340.0), on_page(&ed, 220.0, 300.0)]));
    assert!(!ed.is_idle(), "the pen is mid-stroke");
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 2]), 1);
    assert!(ed.is_idle(), "nothing is left half done on a layout that is gone");
    // The pen lifting afterwards is for a gesture that no longer exists.
    ed.pointer_event(pen(Phase::Up, TIP, &[on_page(&ed, 220.0, 300.0)]));
    assert!(ed.is_idle());
}
#[test]
fn typing_in_a_text_box_when_the_page_goes_does_not_survive_it() {
    let mut ed = pages(2);
    ed.set_tool(Tool::Text);
    ed.chrome.panel = None;
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    ed.type_text("about to vanish");
    assert!(ed.is_typing());
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 1]), 0);
    assert!(!ed.is_typing(), "the box is closed");
    assert_eq!(ed.ink.len(), 0, "and nothing of it is left on the page that is there now");
}

#[test]
fn the_view_goes_to_the_page_that_took_its_place_or_the_new_last_page() {
    let mut ed = pages(4);
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 3]), 1);
    settle(&mut ed);
    let page = ed.layout.page(1).expect("page");
    assert!((ed.view.scroll[1] - (page.y - crate::view::DOC_MARGIN)).abs() < 1.0, "on the page that moved up into its place");
    let mut last = pages(3);
    last.page_deleted(Layout::new(&[[612.0, 792.0]; 2]), 2);
    settle(&mut last);
    // (The last page cannot be scrolled to the top of the window, so it is the page being looked at that counts.)
    assert_eq!(last.view.current_page(&last.layout), 1, "the last page went: on the new last one");
}

#[test]
fn page_numbers_in_the_dirty_set_follow_the_pages() {
    let mut ed = pages(4);
    ink_on(&mut ed, 0, 100.0);
    ink_on(&mut ed, 3, 100.0);
    ed.ink.mark_clean();
    ed.ink.mark_dirty(3);
    ed.page_deleted(Layout::new(&[[612.0, 792.0]; 3]), 1);
    let dirty: Vec<u32> = ed.ink.dirty_pages().iter().copied().collect();
    assert!(dirty.contains(&2) && !dirty.contains(&3), "the page that was fourth is third: {dirty:?}");
    assert!(dirty.contains(&1), "and the page that took the deleted one's place is marked, so the file is written without it");
}
