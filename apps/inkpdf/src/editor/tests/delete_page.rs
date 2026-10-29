//! The delete-page button: it deletes the page being looked at, without asking, and never the last page there is. A deleted
//! page is a step of the history: undoing it brings the page back with its ink.

use crate::pages::PageEdit;

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

/// The edits of the pages the shell has been asked to follow, taken.
fn asked(ed: &mut Editor) -> Vec<PageEdit> {
    ed.requests.take_page_edits()
}

#[test]
fn the_button_deletes_the_page_being_looked_at_with_every_device_and_asks_nobody() {
    let mut ed = pages(4);
    let ids: Vec<u64> = (0..4).map(|p| ink_on(&mut ed, p, 100.0)).collect();
    let button = delete_button(&ed);
    mouse_click_at(&mut ed, button);
    assert_eq!(ed.layout.len(), 3, "the mouse");
    assert!(ed.ink.get(ids[0]).is_none() && ed.ink.get(ids[1]).is_some(), "the page being looked at is the first, and went with its ink");
    assert!(ed.chrome.prompt.is_none(), "no question is asked");
    assert!(matches!(asked(&mut ed).as_slice(), [PageEdit::Delete { pages, .. }] if pages == &[0]), "and the shell is told");
    ed.pan_px(0.0, -1.0e6);
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert_eq!(ed.layout.len(), 2, "the stylus: now the last");
    assert!(ed.ink.get(ids[3]).is_none() && ed.ink.get(ids[2]).is_some());
    assert_eq!(ed.ink.len(), 2, "and the press drew nothing");
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(2, Phase::Down, button, 30.0));
    ed.pointer_event(touch(2, Phase::Up, button, 30.0));
    assert_eq!(ed.layout.len(), 1, "a finger");
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
            assert_eq!(ed.layout.len(), 1, "{mode:?} {tool:?}");
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
    assert_eq!(ed.layout.len(), 1, "nothing was done");
    assert!(asked(&mut ed).is_empty() && !ed.ink.can_undo() && !ed.ink.is_dirty());
    assert!(pages(2).toolbar_state().can_delete, "with another page it is live");
    assert!(!ed.edit_pages(PageEdit::delete(0)), "and no way round it");
    assert!(!pages(3).edit_pages(PageEdit::Delete { pages: vec![0, 1, 2], grave: 0 }), "all of them is not an edit either");
}

#[test]
fn a_deleted_page_takes_its_ink_with_it_and_the_later_pages_come_up_with_theirs() {
    let mut ed = pages(3);
    let (a, b, c) = (ink_on(&mut ed, 0, 100.0), ink_on(&mut ed, 1, 100.0), ink_on(&mut ed, 2, 100.0));
    let c_before = ed.ink.get(c).expect("c").pts.clone();
    let gap = ed.layout.page(2).expect("page").y - ed.layout.page(1).expect("page").y;
    assert!(ed.edit_pages(PageEdit::delete(1)));
    assert_eq!(ed.layout.len(), 2);
    assert!(ed.ink.get(b).is_none(), "the page's own ink is gone");
    assert_eq!(ed.ink.get(a).expect("a").page, 0, "the page before is untouched");
    let moved = ed.ink.get(c).expect("c");
    assert_eq!(moved.page, 1, "the page after is the second now");
    assert!(moved.pts.iter().zip(&c_before).all(|(n, o)| (n[0] - o[0]).abs() < 1e-3 && (n[1] - o[1] + gap).abs() < 1e-3), "and came up by a page");
    let page = ed.layout.page(1).expect("page");
    assert!(moved.bbox()[1] > page.y && moved.bbox()[3] < page.bottom(), "it is on the page it says");
}

#[test]
fn deleting_a_page_lets_go_of_what_was_selected_on_it_and_is_a_step_that_can_be_undone() {
    let mut ed = pages(3);
    let gone = ink_on(&mut ed, 1, 100.0);
    let kept = ink_on(&mut ed, 0, 100.0);
    ed.selection = vec![gone, kept];
    assert!(ed.edit_pages(PageEdit::delete(1)));
    assert_eq!(ed.selection, vec![kept], "only what is still there stays selected");
    assert!(ed.ink.can_undo() && ed.toolbar_state().can_undo, "the deletion is a step, so the undo button is live");
    assert!(ed.ink.is_dirty(), "and the document is changed, so it asks to be saved");
}

#[test]
fn undoing_a_deletion_brings_the_page_back_with_its_ink_and_redo_deletes_it_again() {
    let mut ed = pages(3);
    let (a, b, c) = (ink_on(&mut ed, 0, 100.0), ink_on(&mut ed, 1, 100.0), ink_on(&mut ed, 2, 100.0));
    let (b_pts, c_pts) = (ed.ink.get(b).expect("b").pts.clone(), ed.ink.get(c).expect("c").pts.clone());
    assert!(ed.edit_pages(PageEdit::delete(1)));
    let told = asked(&mut ed);
    let [PageEdit::Delete { grave, .. }] = told.as_slice() else { panic!("the shell was told {told:?}") };
    assert_ne!(*grave, 0, "the deleted page is kept under a name of its own");
    ed.undo();
    assert_eq!(ed.layout.len(), 3, "the page is back");
    assert!(ed.ink.get(b).is_some() && ed.ink.get(a).is_some(), "with its ink");
    assert!(ed.ink.get(b).expect("b").pts == b_pts, "where it was");
    assert!(ed.ink.get(c).expect("c").pts.iter().zip(&c_pts).all(|(n, o)| (n[1] - o[1]).abs() < 1e-2), "and the later page is back down");
    assert_eq!(ed.ink.get(c).expect("c").page, 2);
    assert_eq!(asked(&mut ed), vec![PageEdit::Restore { grave: *grave, pages: vec![(1, [612.0, 792.0])] }], "the shell is told to put it back");
    ed.redo();
    assert_eq!(ed.layout.len(), 2);
    assert!(ed.ink.get(b).is_none() && ed.ink.get(c).expect("c").page == 1);
    assert_eq!(asked(&mut ed), vec![PageEdit::Delete { pages: vec![1], grave: *grave }]);
}

#[test]
fn undo_goes_back_through_pages_and_ink_in_the_order_they_were_done() {
    let mut ed = pages(2);
    let a = ink_on(&mut ed, 1, 100.0);
    assert!(ed.insert_page_after(0));
    let b = ink_on(&mut ed, 2, 300.0);
    assert!(ed.edit_pages(PageEdit::delete(0)));
    assert_eq!(ed.layout.len(), 2);
    for _ in 0..4 {
        ed.undo();
    }
    assert_eq!((ed.layout.len(), ed.ink.len()), (2, 0), "everything is undone");
    assert!(!ed.ink.can_undo());
    for _ in 0..4 {
        ed.redo();
    }
    assert_eq!(ed.layout.len(), 2);
    assert_eq!(ed.ink.get(a).expect("a").page, 1, "the first page had no ink: the ink is on the page that was the last, now the second");
    assert_eq!(ed.ink.get(b).expect("b").page, 1);
}

#[test]
fn a_stroke_under_way_when_the_page_goes_is_ended_not_left_half_done() {
    let mut ed = pages(3);
    let a = on_page(&ed, 100.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    ed.pointer_event(pen(Phase::Move, TIP, &[a, on_page(&ed, 160.0, 340.0), on_page(&ed, 220.0, 300.0)]));
    assert!(!ed.is_idle(), "the pen is mid-stroke");
    assert!(ed.edit_pages(PageEdit::delete(1)));
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
    assert!(ed.edit_pages(PageEdit::delete(0)));
    assert!(!ed.is_typing(), "the box is closed");
    assert_eq!(ed.ink.len(), 0, "and nothing of it is left on the page that is there now");
}

#[test]
fn the_view_goes_to_the_page_that_took_its_place_or_the_new_last_page() {
    let mut ed = pages(4);
    assert!(ed.edit_pages(PageEdit::delete(1)));
    settle(&mut ed);
    let page = ed.layout.page(1).expect("page");
    assert!((ed.view.scroll[1] - (page.y - crate::view::DOC_MARGIN)).abs() < 1.0, "on the page that moved up into its place");
    let mut last = pages(3);
    assert!(last.edit_pages(PageEdit::delete(2)));
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
    assert!(ed.edit_pages(PageEdit::delete(1)));
    let dirty: Vec<u32> = ed.ink.dirty_pages().iter().copied().collect();
    assert!(dirty.contains(&2) && !dirty.contains(&3), "the page that was fourth is third: {dirty:?}");
    assert!(dirty.contains(&1), "and the page that took the deleted one's place is marked, so the file is written without it");
}

#[test]
fn saving_forgets_the_deletions_that_can_no_longer_be_undone_but_not_the_ink_after_them() {
    let mut ed = pages(3);
    ink_on(&mut ed, 0, 100.0);
    assert!(ed.edit_pages(PageEdit::delete(1)));
    let after = ink_on(&mut ed, 1, 200.0);
    ed.document_saved();
    ed.undo();
    assert!(ed.ink.get(after).is_none(), "what was drawn after the deletion can still be undone");
    assert!(!ed.ink.can_undo(), "the deletion cannot: the file has been written without the page");
    assert_eq!(ed.layout.len(), 2);
}
