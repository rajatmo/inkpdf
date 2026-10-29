use super::*;
use crate::pages::PageEdit;

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
fn pulling_up_past_the_last_page_adds_a_blank_page_once() {
    let mut ed = at_the_end();
    let end = drag(&mut ed, [600.0, 700.0], [0.0, -10.0], 8);
    assert_eq!(ed.layout.len(), 2, "80 pixels is not far enough");
    assert!(ed.pull_progress() > 0.3 && ed.pull_progress() < 1.0);
    drag_on(&mut ed, end, [0.0, -10.0], 10);
    assert_eq!(ed.layout.len(), 3, "140 pixels is");
    assert!(matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Insert { at: 2, .. }]), "at the end, and the shell is told");
    drag_on(&mut ed, [600.0, 500.0], [0.0, -10.0], 20);
    assert_eq!(ed.layout.len(), 3, "one page per pull, however long the finger keeps going");
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
    assert_eq!(ed.layout.len(), 3);
    assert_eq!(ed.pull_progress(), 0.0, "lifting ends the pull");
    settle(&mut ed);
    ed.pan_px(0.0, -1.0e6);
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 16);
    assert_eq!(ed.layout.len(), 4);
}

#[test]
fn scrolling_up_the_page_normally_adds_nothing() {
    let mut ed = editor();
    drag(&mut ed, [600.0, 800.0], [0.0, -10.0], 40);
    assert_eq!(ed.layout.len(), 2);
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
    assert_eq!(ed.layout.len(), 2);
}

#[test]
fn only_a_finger_adds_pages() {
    let mut ed = at_the_end();
    // The wheel scrolls to the end and stops.
    for _ in 0..30 {
        ed.wheel(Wheel::Lines(0.0, -5.0));
    }
    assert_eq!(ed.layout.len(), 2);
    // A pen dragging at the end draws.
    let a = on_page(&ed, 100.0, 100.0);
    pen_stroke(&mut ed, TIP, &[a, [a[0], a[1] - 400.0]]);
    assert_eq!(ed.layout.len(), 2);
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
    assert_eq!(ed.layout.len(), 2);
}

#[test]
fn an_added_page_is_marked_changed_and_shown() {
    let mut ed = at_the_end();
    assert!(!ed.ink.is_dirty());
    assert!(ed.edit_pages(PageEdit::Insert { at: 2, size: [612.0, 792.0] }));
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
fn the_plus_button_adds_a_page_after_the_one_being_looked_at() {
    let mut ed = editor();
    let plus = ed.toolbar().centre_of(Item::AddPage);
    mouse_click_at(&mut ed, plus);
    assert_eq!(ed.layout.len(), 3, "a page more");
    assert!(
        matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Insert { at: 1, .. }]),
        "after the first, which is the one being looked at"
    );
    ed.pan_px(0.0, -1.0e6);
    let looking = ed.view.current_page(&ed.layout);
    pen_stroke(&mut ed, TIP, &[plus, plus]);
    assert_eq!(ed.layout.len(), 4);
    assert!(
        matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Insert { at, .. }] if *at == looking + 1),
        "the stylus: after the page now being looked at"
    );
    assert_eq!(ed.ink.len(), 0);
    // A finger can press it too.
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(2, Phase::Down, plus, 30.0));
    ed.pointer_event(touch(2, Phase::Up, plus, 30.0));
    assert_eq!(ed.layout.len(), 5);
}

#[test]
fn a_page_inserted_among_the_others_carries_the_ink_on_the_later_pages_down_with_them() {
    let (mut ed, first, second) = two_pages_with_ink();
    let (a_before, b_before) = (ed.ink.get(first).expect("first").pts.clone(), ed.ink.get(second).expect("second").pts.clone());
    // A page after the first: 612 x 792 again.
    let new = Layout::new(&[[612.0, 792.0]; 3]);
    let gap = new.page(2).expect("page").y - ed.layout.page(1).expect("page").y;
    assert!(ed.edit_pages(PageEdit::Insert { at: 1, size: [612.0, 792.0] }));
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
    // The insertion is a step of the history: undoing it takes the page away and the ink goes back up; redo puts both back.
    ed.undo();
    assert_eq!(ed.layout.len(), 2);
    let up = ed.ink.get(second).expect("second");
    assert_eq!(up.page, 1);
    assert!(up.pts.iter().zip(&b_before).all(|(n, o)| (n[0] - o[0]).abs() < 1e-2 && (n[1] - o[1]).abs() < 1e-2), "back where it was");
    ed.redo();
    let back = ed.ink.get(second).expect("second again");
    assert_eq!((ed.layout.len(), back.page), (3, 2));
    assert!((back.pts[0][1] - b_before[0][1] - gap).abs() < 1e-2, "and down again");
}

#[test]
fn inserting_a_page_stops_what_the_pen_was_doing_and_clears_the_laser_and_shows_the_new_page() {
    let (mut ed, _, _) = two_pages_with_ink();
    ed.set_tool(Tool::Laser);
    let path = [on_page(&ed, 100.0, 300.0), on_page(&ed, 200.0, 320.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert!(ed.laser().is_fading());
    assert!(ed.edit_pages(PageEdit::Insert { at: 1, size: [612.0, 792.0] }));
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
    assert!(ed.edit_pages(PageEdit::Insert { at: 2, size: [612.0, 792.0] }));
    assert_eq!((ed.ink.get(first).expect("a").pts.clone(), ed.ink.get(second).expect("b").pts.clone()), (a, b));
    assert_eq!(ed.ink.get(second).expect("b").page, 1);
}

#[test]
fn moved_pages_carry_their_ink_links_and_selection_and_the_move_is_one_step_that_undoes() {
    use crate::links::{Link, Target};
    let (mut ed, first, second) = two_pages_with_ink();
    let (a, b) = (ed.ink.get(first).expect("first").pts.clone(), ed.ink.get(second).expect("second").pts.clone());
    let (p0, p1) = (ed.layout.page(0).expect("page"), ed.layout.page(1).expect("page"));
    // A link on the first page to the second, and a selection of the first page's ink.
    ed.links = vec![Link { rect: [p0.x + 10.0, p0.y + 10.0, p0.x + 60.0, p0.y + 30.0], target: Target::Page(1) }];
    ed.selection = vec![first];
    ed.ink.mark_clean();
    assert!(ed.edit_pages(PageEdit::Move { pages: vec![1], to: 0 }));
    assert_eq!((ed.ink.get(first).expect("first").page, ed.ink.get(second).expect("second").page), (1, 0), "the ink went with its page");
    let (n0, n1) = (ed.layout.page(0).expect("page"), ed.layout.page(1).expect("page"));
    let (moved_a, moved_b) = (&ed.ink.get(first).expect("first").pts, &ed.ink.get(second).expect("second").pts);
    assert!(
        moved_a.iter().zip(&a).all(|(n, o)| (n[1] - o[1] - (n1.y - p0.y)).abs() < 1e-2 && (n[0] - o[0]).abs() < 1e-2),
        "in the same place on its page"
    );
    assert!(moved_b.iter().zip(&b).all(|(n, o)| (n[1] - o[1] - (n0.y - p1.y)).abs() < 1e-2), "and the other");
    assert_eq!(ed.links[0].target, Target::Page(0), "the link goes to the page it went to, whatever it is called now");
    assert!((ed.links[0].rect[1] - (n1.y + 10.0)).abs() < 1e-2, "and is on the page it was on");
    assert_eq!(ed.selection, vec![first], "the selection is still the same ink");
    assert!(ed.ink.is_dirty(), "a change to write");
    assert!(matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Move { pages, to: 0 }] if pages == &[1]));
    ed.undo();
    assert_eq!((ed.ink.get(first).expect("first").page, ed.ink.get(second).expect("second").page), (0, 1));
    assert_eq!(ed.ink.get(first).expect("first").pts, a, "exactly where it was");
    assert_eq!(ed.links[0].target, Target::Page(1));
    assert!((ed.links[0].rect[1] - (p0.y + 10.0)).abs() < 1e-2);
    assert!(matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Arrange { .. }]), "the shell is told the order to go back to");
    ed.redo();
    assert_eq!(ed.ink.get(second).expect("second").page, 0);
    assert!(!ed.edit_pages(PageEdit::Move { pages: vec![1], to: 1 }), "putting it where it is is not an edit");
    assert!(!ed.edit_pages(PageEdit::Move { pages: vec![7], to: 0 }), "nor is a page that is not there");
}

#[test]
fn a_rearrangement_of_many_pages_is_undone_exactly_by_undo() {
    let mut ed = Editor::new([1200.0, 900.0], 1.0);
    ed.load(Layout::new(&[[600.0, 800.0], [300.0, 400.0], [500.0, 700.0], [200.0, 300.0], [450.0, 500.0]]), vec![]);
    let ids: Vec<u64> = (0..5)
        .map(|p| {
            let r = ed.layout.page(p).expect("page");
            ed.ink
                .add(p as u32, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r.x + 20.0, r.y + 30.0], [r.x + 80.0, r.y + 60.0]])
                .expect("stroke")
        })
        .collect();
    let before: Vec<_> = ids.iter().map(|id| ed.ink.get(*id).expect("stroke").pts.clone()).collect();
    for edit in
        [PageEdit::Move { pages: vec![4, 0], to: 2 }, PageEdit::Arrange { order: vec![3, 2, 1, 0, 4] }, PageEdit::Move { pages: vec![2], to: 4 }]
    {
        assert!(ed.edit_pages(edit));
    }
    for _ in 0..3 {
        ed.undo();
    }
    for (id, pts) in ids.iter().zip(&before) {
        let s = ed.ink.get(*id).expect("stroke");
        assert!(s.pts.iter().zip(pts).all(|(n, o)| (n[0] - o[0]).abs() < 1e-2 && (n[1] - o[1]).abs() < 1e-2), "stroke {id} is exactly where it was");
    }
    assert_eq!(ids.iter().map(|id| ed.ink.get(*id).expect("stroke").page).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
}

#[test]
fn undoing_a_deletion_gives_back_the_links_and_outline_entries_that_went_with_the_pages() {
    use crate::links::{Link, Target};
    use crate::outline::OutlineItem;
    let mut ed = Editor::new([1200.0, 900.0], 1.0);
    ed.load(Layout::new(&[[612.0, 792.0]; 4]), vec![]);
    let at = |ed: &Editor, page: usize, target: Target| {
        let r = ed.layout.page(page).expect("page");
        Link { rect: [r.x + 10.0, r.y + 10.0, r.x + 50.0, r.y + 30.0], target }
    };
    ed.links = vec![
        at(&ed, 0, Target::Page(1)),
        at(&ed, 1, Target::Page(3)),
        at(&ed, 2, Target::Page(0)),
        at(&ed, 3, Target::Url("https://example.com".into())),
    ];
    let item = |title: &str, depth, page| OutlineItem { title: title.into(), depth, page, open: true, has_children: false };
    ed.outline = vec![item("one", 0, Some(0)), item("two", 0, Some(1)), item("three", 1, Some(2)), item("none", 0, None)];
    let (links, outline) = (ed.links.clone(), ed.outline.clone());
    assert!(ed.edit_pages(PageEdit::delete(1)));
    // The link on page 1 and the one to it are gone; the others are on pages that moved up, and go to numbers that moved too.
    assert_eq!(ed.links.len(), 2, "{:?}", ed.links);
    assert_eq!(ed.links[0].target, Target::Page(0), "the link on the third page goes to the first, which did not move");
    assert_eq!(
        ed.outline.iter().map(|i| i.page).collect::<Vec<_>>(),
        [Some(0), None, Some(1), None],
        "the entry that went to the deleted page goes nowhere"
    );
    ed.undo();
    assert_eq!(ed.links, links, "every link is back, in its place in the list, where it was and going where it went");
    assert_eq!(ed.outline, outline, "and every entry goes to the page it went to");
    ed.redo();
    assert_eq!(ed.links.len(), 2);
    ed.undo();
    assert_eq!(ed.links, links, "as often as it is done and undone");
    // Once the file is written without the pages, what they took is forgotten along with them.
    assert!(ed.edit_pages(PageEdit::delete(2)));
    assert_eq!(ed.links.len(), 3, "the link on the third page went");
    ed.document_saved();
    assert!(ed.graves.is_empty());
}

#[test]
fn duplicated_pages_copy_their_ink_and_links_and_the_outline_stays_with_the_originals_and_all_of_it_undoes() {
    use crate::links::{Link, Target};
    use crate::outline::OutlineItem;
    let mut ed = Editor::new([1200.0, 900.0], 1.0);
    ed.load(Layout::new(&[[612.0, 792.0]; 4]), vec![]);
    let r1 = ed.layout.page(1).expect("page");
    let ink =
        ed.ink.add(1, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r1.x + 40.0, r1.y + 50.0], [r1.x + 90.0, r1.y + 70.0]]).expect("stroke");
    ed.links = vec![
        Link { rect: [r1.x + 10.0, r1.y + 10.0, r1.x + 50.0, r1.y + 30.0], target: Target::Page(3) },
        Link { rect: [r1.x + 60.0, r1.y + 10.0, r1.x + 90.0, r1.y + 30.0], target: Target::Url("https://example.com".into()) },
    ];
    ed.outline = vec![OutlineItem { title: "Two".into(), depth: 0, page: Some(1), open: false, has_children: false }];
    ed.selection = vec![ink];
    assert!(ed.edit_pages(PageEdit::Duplicate { pages: vec![1] }));
    assert_eq!(ed.layout.len(), 5);
    assert_eq!(ed.ink.len(), 2, "the stroke is copied onto the copy");
    assert_eq!(ed.selection, vec![ink], "the selection is the original, still");
    assert_eq!(ed.links.len(), 4, "and so are the links");
    let copy_page = ed.layout.page(2).expect("page");
    let on_copy: Vec<&Link> = ed.links.iter().filter(|l| l.rect[1] >= copy_page.y && l.rect[1] < copy_page.bottom()).collect();
    assert_eq!(on_copy.len(), 2, "two links are on the copy");
    assert!(on_copy.iter().any(|l| l.target == Target::Page(4)), "a link to the fourth page goes to where it is now, the fifth");
    assert!(
        on_copy.iter().all(|l| (l.rect[0] - (copy_page.x + 10.0)).abs() < 0.5 || (l.rect[0] - (copy_page.x + 60.0)).abs() < 0.5),
        "in the same places on it"
    );
    assert_eq!(ed.outline[0].page, Some(1), "the outline still goes to the page it went to");
    assert!(matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Duplicate { pages }] if pages == &[1]), "the shell is told");
    ed.undo();
    assert_eq!((ed.layout.len(), ed.ink.len(), ed.links.len()), (4, 1, 2), "everything of the copy goes");
    assert_eq!(ed.links[0].target, Target::Page(3));
    ed.redo();
    assert_eq!((ed.layout.len(), ed.ink.len(), ed.links.len()), (5, 2, 4));
}
