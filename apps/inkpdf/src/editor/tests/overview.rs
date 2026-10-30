//! The pages overview: opened from the toolbar, a card over the document in which every device can go to a page, scroll,
//! and press the buttons of its header and the entries of its table of contents; and in which nothing reaches the page.

use crate::ink::Style;
use crate::input::Mode;
use crate::links::Target;
use crate::outline::OutlineItem;
use crate::pages::PageEdit;
use crate::toolbar::{OverviewControl, OverviewHover};
use crate::tools::Key;

use super::*;
use crate::scene::Capsule;

/// An editor on a document of `n` pages, 612 x 792 each, in a 1200 x 900 window.
fn many(n: usize) -> Editor {
    let mut ed = Editor::reference([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&vec![[612.0, 792.0]; n]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed
}

fn item(title: &str, depth: usize, page: Option<usize>, has_children: bool) -> OutlineItem {
    OutlineItem { title: title.into(), depth, page, open: true, has_children }
}

/// Chapters over pages, one with sections: `Chapter 1 > (1.1, 1.2)`, `Chapter 2`, `Appendix` (goes nowhere).
fn with_outline(mut ed: Editor) -> Editor {
    ed.outline = vec![
        item("Chapter 1", 0, Some(0), true),
        item("1.1 Basics", 1, Some(2), false),
        item("1.2 More", 1, Some(5), false),
        item("Chapter 2", 0, Some(9), false),
        item("Appendix", 0, None, false),
    ];
    ed
}

fn pages_button(ed: &Editor) -> [f32; 2] {
    ed.toolbar().centre_of(Item::Pages)
}

/// Let the overview's pages and table of contents finish gliding to where the wheel or a key sent them.
fn glide_done(ed: &mut Editor) {
    ed.step_view(10.0);
}

fn scroll_of(ed: &Editor) -> f32 {
    ed.chrome.overview.as_ref().map_or(0.0, |o| o.scroll)
}

/// Where page `i`'s cell has its middle.
fn cell_centre(ed: &Editor, i: usize) -> [f32; 2] {
    let c = ed.overview_layout().cell(i, scroll_of(ed));
    [(c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0]
}

fn button_centre(ed: &Editor, control: OverviewControl) -> [f32; 2] {
    let b = ed.overview_layout().button(control);
    [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
}

/// The devices that can press: a tap with each of them at `pos`.
#[derive(Clone, Copy, Debug)]
enum Press {
    Mouse,
    Stylus,
    Finger,
}

const ALL: [Press; 3] = [Press::Mouse, Press::Stylus, Press::Finger];

fn tap(ed: &mut Editor, how: Press, pos: [f32; 2]) {
    match how {
        Press::Mouse => mouse_click_at(ed, pos),
        Press::Stylus => pen_stroke(ed, TIP, &[pos, pos]),
        Press::Finger => {
            ed.palm = crate::input::PalmGuard::default();
            ed.pointer_event(touch(2, Phase::Down, pos, 30.0));
            ed.pointer_event(touch(2, Phase::Up, pos, 30.0));
        }
    }
}

fn open(ed: &mut Editor, how: Press) {
    let at = pages_button(ed);
    tap(ed, how, at);
    assert!(ed.overview_open(), "{how:?} opens the overview");
}

#[test]
fn the_pages_button_opens_the_overview_with_every_device_in_both_modes_and_with_every_tool() {
    for mode in [Mode::Desktop, Mode::Tablet] {
        for tool in [Tool::Pen, Tool::Geometry, Tool::Text, LASER] {
            for how in ALL {
                let mut ed = many(5);
                ed.set_mode(mode);
                ed.set_tool(tool);
                ed.chrome.panel = None;
                open(&mut ed, how);
                assert_eq!(ed.ink.len(), 0, "{mode:?} {tool:?} {how:?}: the press drew nothing");
            }
        }
    }
}

#[test]
fn it_closes_with_its_button_with_escape_and_with_a_tap_on_the_dimmed_document() {
    for how in ALL {
        let mut ed = many(5);
        open(&mut ed, how);
        let close = button_centre(&ed, OverviewControl::Close);
        tap(&mut ed, how, close);
        assert!(!ed.overview_open(), "{how:?}: the close button");
        open(&mut ed, how);
        let layout = ed.overview_layout();
        // A press inside the card on nothing in particular does nothing; one on the document round it puts it away.
        tap(&mut ed, how, [layout.grid[2] - 5.0, layout.grid[3] - 5.0]);
        assert!(ed.overview_open(), "{how:?}: an empty place in the card");
        tap(&mut ed, how, [layout.card[0] - 8.0, 400.0]);
        assert!(!ed.overview_open(), "{how:?}: the document round the card");
    }
    let mut ed = many(5);
    open(&mut ed, Press::Mouse);
    assert!(ed.press(Key::Escape));
    assert!(!ed.overview_open(), "Escape");
    assert!(!ed.press(Key::Escape) || !ed.overview_open());
}

#[test]
fn a_tap_on_a_page_goes_to_it_and_puts_the_overview_away_with_every_device() {
    for how in ALL {
        let mut ed = many(30);
        open(&mut ed, how);
        let layout = ed.overview_layout();
        // A page in the second row, then one far down after scrolling.
        let target = layout.columns() + 1;
        let at = cell_centre(&ed, target);
        tap(&mut ed, how, at);
        assert!(!ed.overview_open(), "{how:?}");
        settle(&mut ed);
        assert_eq!(ed.view.current_page(&ed.layout), target, "{how:?}: the view is on the page that was tapped");
    }
}

#[test]
fn while_it_is_up_nothing_reaches_the_page_not_the_pen_nor_a_finger_nor_the_keyboard() {
    let mut ed = many(30);
    open(&mut ed, Press::Mouse);
    let before = (ed.view.scroll, ed.view.zoom, ed.selection.clone());
    let layout = ed.overview_layout();
    let inside = [layout.grid[0] + 30.0, layout.grid[3] - 10.0];
    // The stylus draws nothing, in any tool.
    for tool in [Tool::Pen, Tool::Geometry, Tool::Text, LASER] {
        ed.set_tool(tool);
        pen_stroke(&mut ed, TIP, &[inside, [inside[0] + 80.0, inside[1] - 5.0], [inside[0] + 160.0, inside[1]]]);
        assert_eq!(ed.ink.len(), 0, "{tool:?}");
        assert!(ed.overview_open(), "{tool:?}: a stroke in the card is not a way out");
    }
    // A pinch and a swipe do not move the document under the card.
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(3, Phase::Down, [layout.card[0] + 50.0, 450.0], 30.0));
    ed.pointer_event(touch(4, Phase::Down, [layout.card[0] + 150.0, 450.0], 30.0));
    ed.pointer_event(touch(3, Phase::Move, [layout.card[0] + 20.0, 450.0], 30.0));
    ed.pointer_event(touch(4, Phase::Move, [layout.card[0] + 250.0, 450.0], 30.0));
    ed.pointer_event(touch(3, Phase::Up, [layout.card[0] + 20.0, 450.0], 30.0));
    ed.pointer_event(touch(4, Phase::Up, [layout.card[0] + 250.0, 450.0], 30.0));
    // The keyboard: the document's shortcuts and keys do nothing (select all, delete, the arrows' nudge, typing).
    assert!(
        !ed.shortcut("c", false) && !ed.shortcut("x", false) && !ed.shortcut("v", false),
        "the page's clipboard shortcuts are not the overview's"
    );
    assert!(ed.shortcut("a", false), "Ctrl+A is the overview's own: it chooses the pages");
    assert!(ed.selection.is_empty(), "and chooses no ink on the page under it");
    assert!(!ed.press(Key::Delete) && !ed.press(Key::Text("x")));
    assert_eq!(ed.view.zoom, before.1);
    assert_eq!(ed.view.scroll, before.0, "the document did not move");
    assert_eq!(ed.selection, before.2);
    assert!(ed.overview_open());
}

#[test]
fn the_wheel_a_drag_and_the_keys_scroll_the_pages_and_never_past_their_ends() {
    for how in [Press::Stylus, Press::Finger, Press::Mouse] {
        let mut ed = many(120);
        open(&mut ed, Press::Mouse);
        let layout = ed.overview_layout();
        let max = layout.max_scroll(120);
        assert!(max > 0.0);
        let start = scroll_of(&ed);
        // A drag upwards on an empty place scrolls down, with any device.
        let from = [layout.grid[2] - 10.0, layout.grid[3] - 40.0];
        let to = [from[0], from[1] - 200.0];
        match how {
            Press::Mouse => {
                ed.mouse_moved(from);
                ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
                ed.mouse_moved([from[0], from[1] - 100.0]);
                ed.mouse_moved(to);
                ed.mouse_button(ButtonState::Released, MouseButton::Left);
            }
            Press::Stylus => pen_stroke(&mut ed, TIP, &[from, [from[0], from[1] - 100.0], to]),
            Press::Finger => {
                ed.palm = crate::input::PalmGuard::default();
                ed.pointer_event(touch(2, Phase::Down, from, 30.0));
                ed.pointer_event(touch(2, Phase::Move, [from[0], from[1] - 100.0], 30.0));
                ed.pointer_event(touch(2, Phase::Move, to, 30.0));
                ed.pointer_event(touch(2, Phase::Up, to, 30.0));
            }
        }
        assert!((scroll_of(&ed) - (start + 200.0)).abs() < 1.0, "{how:?}: scrolled by the drag, from {start} to {}", scroll_of(&ed));
        assert!(ed.overview_open(), "{how:?}: a drag is not a tap on anything");
    }
    let mut ed = many(120);
    open(&mut ed, Press::Mouse);
    let max = ed.overview_layout().max_scroll(120);
    ed.mouse_moved([ed.overview_layout().grid[0] + 200.0, ed.overview_layout().grid[1] + 200.0]);
    ed.wheel(Wheel::Lines(0.0, -3.0));
    glide_done(&mut ed);
    assert!(scroll_of(&ed) > 0.0, "the wheel scrolls down");
    ed.wheel(Wheel::Lines(0.0, 1000.0));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), 0.0, "and not above the first row");
    ed.wheel(Wheel::Lines(0.0, -1.0e6));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), max, "nor below the last");
    ed.wheel(Wheel::Pixels(0.0, 5000.0));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), 0.0, "a touchpad too");
    assert!(ed.press(Key::End(false)));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), max, "End");
    assert!(ed.press(Key::Home(false)));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), 0.0, "Home");
    assert!(ed.press(Key::Arrow(crate::tools::Dir::Down, false)));
    glide_done(&mut ed);
    assert!(scroll_of(&ed) > 0.0, "the arrow");
    assert_eq!(ed.overview_layout().max_scroll(120), max);
}

#[test]
fn opening_scrolls_to_the_page_being_looked_at_and_the_page_is_ringed_by_going_back_to_it() {
    let mut ed = many(120);
    ed.ws.scroll_to_page(97);
    settle(&mut ed);
    open(&mut ed, Press::Mouse);
    let layout = ed.overview_layout();
    let cell = layout.cell(97, scroll_of(&ed));
    assert!(cell[1] >= layout.grid[1] && cell[3] <= layout.grid[3], "the page being looked at is in view: {cell:?}");
    let mut ed = many(5);
    open(&mut ed, Press::Mouse);
    assert_eq!(scroll_of(&ed), 0.0, "a short document needs no scrolling");
}

#[test]
fn the_buttons_add_a_page_after_the_one_being_looked_at_and_undo_and_redo_it_without_leaving() {
    for how in ALL {
        let mut ed = many(6);
        open(&mut ed, how);
        let add = button_centre(&ed, OverviewControl::Add);
        tap(&mut ed, how, add);
        assert_eq!(ed.layout.len(), 7, "{how:?}: a page was added");
        assert!(ed.overview_open(), "{how:?}: and the overview is still up");
        let undo = button_centre(&ed, OverviewControl::Undo);
        tap(&mut ed, how, undo);
        assert_eq!(ed.layout.len(), 6, "{how:?}: undone");
        let redo = button_centre(&ed, OverviewControl::Redo);
        tap(&mut ed, how, redo);
        assert_eq!(ed.layout.len(), 7, "{how:?}: redone");
        assert!(ed.overview_open());
    }
}

#[test]
fn the_table_of_contents_goes_to_a_chapters_page_and_folds_and_unfolds_its_sections() {
    for how in ALL {
        let mut ed = with_outline(many(12));
        open(&mut ed, how);
        let rows = ed.toc_rows();
        assert_eq!(rows.len(), 5, "everything is shown to begin with");
        let layout = ed.overview_layout();
        let title_of = |ed: &Editor, k: usize| {
            let r = layout.toc_row(k, ed.chrome.overview.as_ref().map_or(0.0, |o| o.toc_scroll));
            [r[2] - 6.0, (r[1] + r[3]) / 2.0]
        };
        // The arrow of Chapter 1 folds its two sections away, and again unfolds them.
        let arrow = layout.toc_arrow(rows[0], 0, 0.0);
        tap(&mut ed, how, [(arrow[0] + arrow[2]) / 2.0, (arrow[1] + arrow[3]) / 2.0]);
        assert_eq!(ed.toc_rows().len(), 3, "{how:?}: two sections are folded away");
        assert!(ed.overview_open(), "{how:?}: folding does not leave");
        tap(&mut ed, how, [(arrow[0] + arrow[2]) / 2.0, (arrow[1] + arrow[3]) / 2.0]);
        assert_eq!(ed.toc_rows().len(), 5, "{how:?}: and they are back");
        // A section's title goes to its page.
        let at = title_of(&ed, 2);
        tap(&mut ed, how, at);
        assert!(!ed.overview_open(), "{how:?}: going to a section puts the overview away");
        settle(&mut ed);
        assert_eq!(ed.view.current_page(&ed.layout), 5, "{how:?}: 1.2 is on the sixth page");
        // An entry that goes nowhere does nothing.
        open(&mut ed, how);
        let appendix = title_of(&ed, 4);
        tap(&mut ed, how, appendix);
        assert!(ed.overview_open(), "{how:?}: the appendix goes nowhere");
    }
}

#[test]
fn a_document_with_no_outline_has_no_table_and_the_table_scrolls_on_its_own() {
    let mut ed = many(5);
    open(&mut ed, Press::Mouse);
    assert!(ed.overview_layout().toc.is_none());
    let mut ed = many(5);
    ed.outline = (0..80).map(|i| item(&format!("Entry {i}"), 0, Some(i % 5), false)).collect();
    open(&mut ed, Press::Mouse);
    let layout = ed.overview_layout();
    let toc = layout.toc.expect("a table");
    ed.mouse_moved([(toc[0] + toc[2]) / 2.0, (toc[1] + toc[3]) / 2.0]);
    ed.wheel(Wheel::Lines(0.0, -4.0));
    glide_done(&mut ed);
    let moved = ed.chrome.overview.as_ref().map_or(0.0, |o| o.toc_scroll);
    assert!(moved > 0.0, "the wheel over the table scrolls it");
    assert_eq!(scroll_of(&ed), 0.0, "and not the pages");
    ed.wheel(Wheel::Lines(0.0, -1.0e6));
    glide_done(&mut ed);
    let end = ed.chrome.overview.as_ref().map_or(0.0, |o| o.toc_scroll);
    assert_eq!(end, layout.toc_max_scroll(80), "as far as it goes");
}

#[test]
fn opening_it_shows_the_section_being_read_and_scrolls_the_table_to_it() {
    let mut ed = many(100);
    ed.outline = (0..60).map(|i| item(&format!("Chapter {i}"), 0, Some(i), false)).collect();
    ed.ws.scroll_to_page(55);
    settle(&mut ed);
    open(&mut ed, Press::Mouse);
    let toc_scroll = ed.chrome.overview.as_ref().map_or(0.0, |o| o.toc_scroll);
    let layout = ed.overview_layout();
    let row = layout.toc_row(55, toc_scroll);
    assert!(row[1] >= layout.toc_rows[1] && row[3] <= layout.toc_rows[3], "the entry for the page being read is in view: {row:?}");
}

#[test]
fn what_was_under_way_when_it_opens_is_ended_and_what_was_typed_is_kept() {
    let mut ed = many(4);
    ed.set_tool(Tool::Text);
    ed.chrome.panel = None;
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    ed.type_text("kept words");
    assert!(ed.is_typing());
    ed.open_overview();
    assert!(ed.overview_open() && !ed.is_typing(), "the box is closed");
    assert_eq!(ed.ink.len(), 1, "and what was typed is part of the document");
    let mut ed = many(4);
    let a = on_page(&ed, 100.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    ed.pointer_event(pen(Phase::Move, TIP, &[a, on_page(&ed, 160.0, 340.0)]));
    assert!(!ed.is_idle());
    ed.open_overview();
    assert!(ed.is_idle(), "a stroke under way is ended, not left half done under the card");
}

#[test]
fn the_overview_follows_edits_of_the_pages_made_while_it_is_up() {
    let mut ed = many(60);
    open(&mut ed, Press::Mouse);
    ed.press(Key::End(false));
    glide_done(&mut ed);
    let before = scroll_of(&ed);
    assert!(before > 0.0);
    // Most of the pages go: what is left fits, and the scroll is back in range.
    assert!(ed.edit_pages(PageEdit::Delete { pages: (3..60).collect(), grave: 0 }));
    assert_eq!(ed.layout.len(), 3);
    assert_eq!(scroll_of(&ed), 0.0, "there is nothing to scroll to any more");
    assert!(ed.overview_open());
    ed.undo();
    assert_eq!(ed.layout.len(), 60);
    assert!(scroll_of(&ed) <= ed.overview_layout().max_scroll(60));
}

#[test]
fn the_prompt_and_a_new_document_put_the_overview_away() {
    let mut ed = many(5);
    open(&mut ed, Press::Mouse);
    ed.ask_unsaved();
    assert!(!ed.overview_open() && ed.chrome.prompt.is_some(), "the prompt is the window's now");
    let mut ed = many(5);
    open(&mut ed, Press::Mouse);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.overview_open());
}

#[test]
fn links_and_the_outline_of_the_document_are_the_same_whether_the_overview_was_up_or_not() {
    let mut ed = with_outline(many(12));
    ed.links = vec![crate::links::Link { rect: [10.0, 10.0, 60.0, 30.0], target: Target::Page(3) }];
    let (links, outline) = (ed.links.clone(), ed.outline.clone());
    open(&mut ed, Press::Mouse);
    let close = button_centre(&ed, OverviewControl::Close);
    tap(&mut ed, Press::Mouse, close);
    assert_eq!((ed.links, ed.outline), (links, outline));
}

#[test]
fn hovering_lights_what_is_under_the_pointer_and_a_press_that_slides_off_does_nothing() {
    let mut ed = many(30);
    open(&mut ed, Press::Mouse);
    let at = cell_centre(&ed, 2);
    ed.mouse_moved(at);
    assert_eq!(ed.chrome.overview.as_ref().and_then(|o| o.hover_for_tests()), Some(OverviewHover::Page(2)));
    // Pressed on one page and let go on another: nothing happens.
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    let other = cell_centre(&ed, 3);
    ed.mouse_moved(other);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert!(ed.overview_open(), "a press that slid onto another page is not a tap on either");
    assert_eq!(ed.view.current_page(&ed.layout), 0);
}

// â”€â”€ edit mode â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// A document of `n` pages with a stroke on each, whose ids say which page is which.
fn with_ink(n: usize) -> (Editor, Vec<u64>) {
    let mut ed = many(n);
    let ids = (0..n)
        .map(|p| {
            let r = ed.layout.page(p).expect("page");
            ed.ink
                .add(p as u32, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r.x + 40.0, r.y + 50.0], [r.x + 90.0, r.y + 70.0]])
                .expect("stroke")
        })
        .collect();
    (ed, ids)
}

/// Which page each stroke is on now.
fn pages_of(ed: &Editor, ids: &[u64]) -> Vec<Option<u32>> {
    ids.iter().map(|id| ed.ink.get(*id).map(|s| s.page)).collect()
}

fn action_centre(ed: &Editor, action: crate::toolbar::OverviewAction) -> [f32; 2] {
    let b = ed.overview_layout().action(action);
    [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
}

fn edit_mode(ed: &mut Editor, how: Press) {
    let at = button_centre(ed, OverviewControl::Edit);
    tap(ed, how, at);
    assert!(ed.chrome.overview.as_ref().is_some_and(|o| o.edit), "{how:?}: edit mode is on");
}

fn chosen(ed: &Editor) -> Vec<usize> {
    ed.chrome.overview.as_ref().map(|o| o.selected.iter().copied().collect()).unwrap_or_default()
}

/// A pen or the mouse carries the page under `from` to `to` and lets go.
fn carry(ed: &mut Editor, how: Press, from: [f32; 2], to: [f32; 2]) {
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    match how {
        Press::Mouse => {
            ed.mouse_moved(from);
            ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
            ed.mouse_moved(mid);
            ed.mouse_moved(to);
            ed.mouse_button(ButtonState::Released, MouseButton::Left);
        }
        Press::Stylus => pen_stroke(ed, TIP, &[from, mid, to]),
        Press::Finger => unreachable!("a finger scrolls the pages; it does not carry one"),
    }
}

#[test]
fn edit_mode_chooses_pages_with_a_tap_and_shows_the_bar_of_what_to_do_with_every_device() {
    for how in ALL {
        let (mut ed, ids) = with_ink(8);
        open(&mut ed, how);
        assert!(ed.overview_layout().bar.is_none(), "{how:?}: no bar until edit mode");
        edit_mode(&mut ed, how);
        assert!(ed.overview_layout().bar.is_some(), "{how:?}: the bar is there");
        for page in [2, 5] {
            let at = cell_centre(&ed, page);
            tap(&mut ed, how, at);
        }
        assert_eq!(chosen(&ed), vec![2, 5], "{how:?}: a tap chooses a page, and the overview stays up");
        assert!(ed.overview_open());
        let at = cell_centre(&ed, 2);
        tap(&mut ed, how, at);
        assert_eq!(chosen(&ed), vec![5], "{how:?}: a second tap lets go of it");
        // Leaving edit mode lets go of everything.
        edit_mode_off(&mut ed, how);
        assert!(chosen(&ed).is_empty() && ed.overview_layout().bar.is_none(), "{how:?}");
        assert_eq!(ed.layout.len(), 8);
        assert_eq!(pages_of(&ed, &ids), (0..8).map(Some).collect::<Vec<_>>(), "{how:?}: nothing was done to the pages");
    }
}

fn edit_mode_off(ed: &mut Editor, how: Press) {
    let at = button_centre(ed, OverviewControl::Edit);
    tap(ed, how, at);
    assert!(!ed.chrome.overview.as_ref().is_some_and(|o| o.edit));
}

#[test]
fn the_bar_deletes_duplicates_and_moves_the_chosen_pages_with_every_device_and_undo_takes_each_back() {
    use crate::toolbar::OverviewAction as A;
    for how in ALL {
        let (mut ed, ids) = with_ink(6);
        open(&mut ed, how);
        edit_mode(&mut ed, how);
        for page in [1, 3] {
            let at = cell_centre(&ed, page);
            tap(&mut ed, how, at);
        }
        // Later: the two come together one place on.
        let at = action_centre(&ed, A::Later);
        tap(&mut ed, how, at);
        assert_eq!(pages_of(&ed, &ids), [0u32, 2, 1, 3, 4, 5].map(Some).to_vec(), "{how:?}: the two come together one place later");
        assert_eq!(chosen(&ed), vec![2, 3], "{how:?}: the same pages are still the chosen ones");
        let at = action_centre(&ed, A::Earlier);
        tap(&mut ed, how, at);
        assert_eq!(chosen(&ed), vec![1, 2], "{how:?}: and one place back");
        // Duplicate: the copies are what is chosen now.
        let at = action_centre(&ed, A::Duplicate);
        tap(&mut ed, how, at);
        assert_eq!(ed.layout.len(), 8, "{how:?}: two pages more");
        assert_eq!(chosen(&ed), vec![3, 4], "{how:?}: the copies, after the last page copied");
        assert_eq!(ed.ink.len(), 8, "{how:?}: with their ink");
        // Delete the copies.
        let at = action_centre(&ed, A::Delete);
        tap(&mut ed, how, at);
        assert_eq!(ed.layout.len(), 6, "{how:?}: the copies are deleted");
        assert!(chosen(&ed).is_empty(), "{how:?}");
        assert!(ed.overview_open());
        // Undo, from the header: the deletion, the duplicate, then the two moves.
        for step in 0..4 {
            let at = button_centre(&ed, OverviewControl::Undo);
            tap(&mut ed, how, at);
            assert!(ed.overview_open(), "{how:?}: undo step {step}");
        }
        assert_eq!(ed.layout.len(), 6);
        assert_eq!(pages_of(&ed, &ids), (0..6).map(Some).collect::<Vec<_>>(), "{how:?}: every page is where it was");
    }
}

#[test]
fn select_all_chooses_every_page_or_none_and_the_last_page_cannot_be_deleted() {
    use crate::toolbar::OverviewAction as A;
    for how in ALL {
        let (mut ed, _) = with_ink(5);
        open(&mut ed, how);
        edit_mode(&mut ed, how);
        let at = action_centre(&ed, A::SelectAll);
        tap(&mut ed, how, at);
        assert_eq!(chosen(&ed), vec![0, 1, 2, 3, 4], "{how:?}");
        assert!(!ed.overview_enabled()[4], "{how:?}: deleting all of them is not an edit, so the button is dim");
        let at = action_centre(&ed, A::Delete);
        tap(&mut ed, how, at);
        assert_eq!(ed.layout.len(), 5, "{how:?}: a document keeps a page");
        let at = action_centre(&ed, A::SelectAll);
        tap(&mut ed, how, at);
        assert!(chosen(&ed).is_empty(), "{how:?}: again: none");
        // With nothing chosen only select-all is live.
        assert_eq!(ed.overview_enabled(), [true, false, false, false, false]);
    }
}

#[test]
fn the_bar_buttons_are_dim_where_they_cannot_do_anything() {
    let (mut ed, _) = with_ink(5);
    open(&mut ed, Press::Mouse);
    edit_mode(&mut ed, Press::Mouse);
    let at = cell_centre(&ed, 0);
    tap(&mut ed, Press::Mouse, at);
    assert_eq!(ed.overview_enabled(), [true, true, false, true, true], "the first page cannot go earlier");
    let at = cell_centre(&ed, 0);
    tap(&mut ed, Press::Mouse, at);
    let at = cell_centre(&ed, 4);
    tap(&mut ed, Press::Mouse, at);
    assert_eq!(ed.overview_enabled(), [true, true, true, false, true], "and the last cannot go later");
    // A press on a dim button does nothing.
    let before = ed.layout.len();
    let at = action_centre(&ed, crate::toolbar::OverviewAction::Later);
    tap(&mut ed, Press::Stylus, at);
    assert_eq!(ed.layout.len(), before);
    assert_eq!(chosen(&ed), vec![4]);
}

#[test]
fn a_page_dragged_with_the_pen_or_the_mouse_is_dropped_before_the_page_it_is_carried_to() {
    for how in [Press::Mouse, Press::Stylus] {
        let (mut ed, ids) = with_ink(8);
        open(&mut ed, how_to_open(how));
        let (from, to) = (cell_centre(&ed, 1), cell_centre(&ed, 5));
        // Onto the left half of page 5: before it.
        let layout = ed.overview_layout();
        let c5 = layout.cell(5, scroll_of(&ed));
        carry(&mut ed, how, from, [c5[0] + 4.0, to[1]]);
        assert_eq!(pages_of(&ed, &ids), [0u32, 4, 1, 2, 3, 5, 6, 7].map(Some), "{how:?}: page 1 went in before page 5, which is the fifth place now");
        assert!(ed.overview_open(), "{how:?}: a drag is not a tap");
        assert!(matches!(ed.requests.take_page_edits().as_slice(), [PageEdit::Move { pages, .. }] if pages == &[1]), "{how:?}: the shell is told");
        // Undo puts it back.
        ed.undo();
        assert_eq!(pages_of(&ed, &ids), (0..8).map(Some).collect::<Vec<_>>(), "{how:?}");
        // To the very end: past the last page.
        let layout = ed.overview_layout();
        let last = layout.cell(7, scroll_of(&ed));
        let from = cell_centre(&ed, 0);
        carry(&mut ed, how, from, [last[2] - 3.0, (last[1] + last[3]) / 2.0]);
        assert_eq!(pages_of(&ed, &ids)[0], Some(7), "{how:?}: page 0 is the last now");
        // Dropped where it was: nothing happens.
        let before = ed.layout.len();
        let (a, b) = (cell_centre(&ed, 3), cell_centre(&ed, 3));
        ed.requests.take_page_edits();
        carry(&mut ed, how, a, [b[0] + 20.0, b[1]]);
        assert_eq!(ed.layout.len(), before);
        assert!(ed.requests.take_page_edits().is_empty(), "{how:?}: a page put back where it was is no edit");
    }
}

fn how_to_open(how: Press) -> Press {
    how
}

#[test]
fn dragging_a_chosen_page_in_edit_mode_carries_all_the_chosen_ones_together() {
    for how in [Press::Mouse, Press::Stylus] {
        let (mut ed, ids) = with_ink(10);
        open(&mut ed, how);
        edit_mode(&mut ed, how);
        for page in [1, 4, 6] {
            let at = cell_centre(&ed, page);
            tap(&mut ed, how, at);
        }
        let layout = ed.overview_layout();
        let c8 = layout.cell(8, scroll_of(&ed));
        let from = cell_centre(&ed, 4);
        carry(&mut ed, how, from, [c8[0] + 3.0, (c8[1] + c8[3]) / 2.0]);
        // The three come together, in their order, before page 8.
        assert_eq!(
            pages_of(&ed, &ids),
            [0u32, 5, 1, 2, 6, 3, 7, 4, 8, 9].map(Some).to_vec(),
            "{how:?}: pages 1, 4 and 6 are together, in their order, before page 8"
        );
        assert_eq!(chosen(&ed), vec![5, 6, 7], "{how:?}: and they are still the chosen ones, together");
        ed.undo();
        assert_eq!(pages_of(&ed, &ids), (0..10).map(Some).collect::<Vec<_>>(), "{how:?}");
    }
}

#[test]
fn a_page_not_chosen_is_carried_alone_even_in_edit_mode() {
    let (mut ed, ids) = with_ink(8);
    open(&mut ed, Press::Stylus);
    edit_mode(&mut ed, Press::Stylus);
    let at = cell_centre(&ed, 2);
    tap(&mut ed, Press::Stylus, at);
    let layout = ed.overview_layout();
    let c6 = layout.cell(6, scroll_of(&ed));
    let from = cell_centre(&ed, 0);
    carry(&mut ed, Press::Stylus, from, [c6[0] + 3.0, (c6[1] + c6[3]) / 2.0]);
    assert_eq!(pages_of(&ed, &ids)[0], Some(5), "page 0 went in before page 6");
    assert_eq!(chosen(&ed), vec![1], "and the chosen page is the same one, one place earlier");
}

#[test]
fn a_finger_drag_scrolls_the_pages_and_never_carries_one_and_escape_gives_up_a_drag() {
    let (mut ed, ids) = with_ink(90);
    open(&mut ed, Press::Mouse);
    ed.palm = crate::input::PalmGuard::default();
    let from = cell_centre(&ed, 1);
    ed.pointer_event(touch(2, Phase::Down, from, 30.0));
    ed.pointer_event(touch(2, Phase::Move, [from[0], from[1] - 150.0], 30.0));
    ed.pointer_event(touch(2, Phase::Up, [from[0], from[1] - 150.0], 30.0));
    assert!(scroll_of(&ed) > 100.0, "the finger pulled the pages past it");
    assert_eq!(pages_of(&ed, &ids), (0..90).map(Some).collect::<Vec<_>>(), "and no page was moved");
    // A drag under way, given up with Escape: nothing moves, and the overview is still up.
    let shown = ed.overview_layout().visible_pages(90, scroll_of(&ed));
    // (A page in the second visible row: the first may be partly scrolled off the top.)
    let a = cell_centre(&ed, shown.start + ed.overview_layout().columns() + 1);
    ed.mouse_moved(a);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved([a[0] + 40.0, a[1] + 40.0]);
    ed.mouse_moved([a[0] + 200.0, a[1] + 40.0]);
    assert!(ed.chrome.overview.as_ref().is_some_and(|o| o.dragging_for_tests()), "a page is being carried");
    assert!(ed.press(Key::Escape));
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(pages_of(&ed, &ids), (0..90).map(Some).collect::<Vec<_>>(), "nothing moved");
    assert!(ed.overview_open());
}

#[test]
fn a_page_held_near_the_edge_while_it_is_carried_scrolls_the_pages() {
    let (mut ed, _) = with_ink(150);
    open(&mut ed, Press::Mouse);
    let layout = ed.overview_layout();
    let a = cell_centre(&ed, 0);
    ed.mouse_moved(a);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved([a[0] + 30.0, a[1] + 60.0]);
    let before = scroll_of(&ed);
    for _ in 0..30 {
        ed.mouse_moved([a[0] + 30.0, layout.grid[3] - 5.0]);
    }
    assert!(scroll_of(&ed) > before + 100.0, "held at the bottom edge the pages scroll up under it");
    ed.press(Key::Escape);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
}

#[test]
fn the_keyboard_chooses_duplicates_and_deletes_and_escape_steps_out_a_level_at_a_time() {
    let (mut ed, ids) = with_ink(6);
    open(&mut ed, Press::Mouse);
    // Ctrl+A chooses every page (and is edit mode); Escape lets go of them, then leaves edit mode, then the overview.
    assert!(ed.shortcut("a", false));
    assert_eq!(chosen(&ed), vec![0, 1, 2, 3, 4, 5]);
    assert!(ed.selection.is_empty(), "the document's own selection is not touched");
    assert!(ed.press(Key::Escape));
    assert!(chosen(&ed).is_empty() && ed.chrome.overview.as_ref().is_some_and(|o| o.edit), "the pages are let go of, edit mode stays");
    assert!(ed.press(Key::Escape));
    assert!(ed.overview_open() && !ed.chrome.overview.as_ref().is_some_and(|o| o.edit), "then edit mode goes");
    assert!(ed.press(Key::Escape));
    assert!(!ed.overview_open(), "then the overview");
    // Choose two pages, duplicate them with Ctrl+D, delete the copies with Delete.
    open(&mut ed, Press::Mouse);
    edit_mode(&mut ed, Press::Mouse);
    for page in [1, 2] {
        let at = cell_centre(&ed, page);
        tap(&mut ed, Press::Mouse, at);
    }
    assert!(ed.shortcut("d", false));
    assert_eq!((ed.layout.len(), chosen(&ed)), (8, vec![3, 4]));
    assert!(ed.press(Key::Delete));
    assert_eq!(ed.layout.len(), 6);
    assert_eq!(pages_of(&ed, &ids), (0..6).map(Some).collect::<Vec<_>>());
    assert!(!ed.press(Key::Delete), "with nothing chosen Delete does nothing");
    // In the tablet mode there are no shortcuts to count on.
    ed.set_mode(Mode::Tablet);
    assert!(!ed.shortcut("a", false));
}

#[test]
fn the_overview_in_edit_mode_follows_undo_and_the_pages_chosen_follow_their_pages() {
    let (mut ed, ids) = with_ink(7);
    open(&mut ed, Press::Mouse);
    edit_mode(&mut ed, Press::Mouse);
    let at = cell_centre(&ed, 5);
    tap(&mut ed, Press::Mouse, at);
    assert_eq!(chosen(&ed), vec![5]);
    // Pages added before it: it is still the same page.
    assert!(ed.edit_pages(PageEdit::Insert { at: 0, size: [612.0, 792.0] }));
    assert_eq!(chosen(&ed), vec![6], "the chosen page is one place later");
    ed.undo();
    assert_eq!(chosen(&ed), vec![5]);
    // The chosen page is deleted by something else: it is no longer chosen.
    assert!(ed.edit_pages(PageEdit::delete(5)));
    assert!(chosen(&ed).is_empty());
    assert_eq!(pages_of(&ed, &ids)[5], None);
}

#[test]
fn the_title_says_how_many_are_chosen_and_the_bar_takes_room_from_the_pages_only_in_edit_mode() {
    let (mut ed, _) = with_ink(4);
    open(&mut ed, Press::Mouse);
    let plain = ed.overview_layout();
    edit_mode(&mut ed, Press::Mouse);
    let editing = ed.overview_layout();
    assert!(editing.grid[3] < plain.grid[3], "the bar is under the pages");
    assert_eq!(editing.bar.map(|b| b[3]), Some(editing.card[3]));
    for (action, r) in editing.actions {
        assert!(r[1] >= editing.bar.expect("bar")[1] - 0.01 && r[3] <= editing.card[3] + 0.01, "{action:?} is in the bar");
    }
    let at = cell_centre(&ed, 1);
    tap(&mut ed, Press::Mouse, at);
    let mut ui = crate::toolbar::Ui::default();
    ed.draw_overview(&mut ui);
    assert!(ui.labels.iter().any(|l| l.text == "1 selected"), "{:?}", ui.labels.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert!(ui.labels.iter().any(|l| l.text == "Select all"));
}

// â”€â”€ the menu of a page â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn number_centre(ed: &Editor, page: usize) -> [f32; 2] {
    let r = ed.overview_layout().number_row(page, scroll_of(ed));
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

fn menu_item_centre(ed: &Editor, page: usize, item: crate::toolbar::OverviewMenuItem) -> [f32; 2] {
    let (_, items) = ed.overview_layout().menu(page, scroll_of(ed));
    let r = items.iter().find(|(i, _)| *i == item).map(|(_, r)| *r).expect("item");
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

fn menu_open_for(ed: &Editor) -> Option<usize> {
    ed.chrome.overview.as_ref().and_then(|o| o.menu_for_tests())
}

#[test]
fn a_tap_on_a_pages_number_opens_its_menu_and_a_tap_anywhere_else_only_puts_it_away() {
    use crate::toolbar::OverviewMenuItem as M;
    for how in ALL {
        let (mut ed, ids) = with_ink(6);
        open(&mut ed, how);
        let at = number_centre(&ed, 2);
        tap(&mut ed, how, at);
        assert_eq!(menu_open_for(&ed), Some(2), "{how:?}: the menu of page 3 is open");
        // A tap on another page does not go to it: it puts the menu away, and that is all.
        let other = cell_centre(&ed, 4);
        tap(&mut ed, how, other);
        assert_eq!(menu_open_for(&ed), None, "{how:?}");
        assert!(ed.overview_open(), "{how:?}: and the overview is still up");
        assert_eq!(ed.view.current_page(&ed.layout), 0, "{how:?}: the tap did not go to the page");
        // Tapping the number again puts it away too; the picture itself still goes to the page.
        let at = number_centre(&ed, 2);
        tap(&mut ed, how, at);
        tap(&mut ed, how, at);
        assert_eq!(menu_open_for(&ed), None, "{how:?}: a second tap on the number");
        let (menu, _) = ed.overview_layout().menu(2, 0.0);
        let at = number_centre(&ed, 2);
        tap(&mut ed, how, at);
        tap(&mut ed, how, [menu[0] - 6.0, menu[1] - 6.0]);
        assert_eq!(menu_open_for(&ed), None, "{how:?}: outside the menu");
        let _ = (ids, M::Before);
    }
    let (mut ed, _) = with_ink(6);
    open(&mut ed, Press::Mouse);
    let at = number_centre(&ed, 1);
    tap(&mut ed, Press::Mouse, at);
    assert!(ed.press(Key::Escape));
    assert!(menu_open_for(&ed).is_none() && ed.overview_open(), "Escape puts the menu away first");
    tap(&mut ed, Press::Mouse, at);
    ed.wheel(Wheel::Lines(0.0, -1.0));
    glide_done(&mut ed);
    assert!(menu_open_for(&ed).is_none(), "scrolling does too");
}

#[test]
fn the_items_of_the_menu_add_pages_before_and_after_duplicate_and_delete_the_page_and_each_undoes() {
    use crate::toolbar::OverviewMenuItem as M;
    for how in ALL {
        let (mut ed, ids) = with_ink(5);
        open(&mut ed, how);
        let item = |ed: &mut Editor, page: usize, which: M| {
            let at = number_centre(ed, page);
            tap(ed, how, at);
            let at = menu_item_centre(ed, page, which);
            tap(ed, how, at);
            assert!(menu_open_for(ed).is_none(), "{how:?}: the menu goes when an item is done");
            assert!(ed.overview_open(), "{how:?}");
        };
        item(&mut ed, 2, M::Before);
        assert_eq!((ed.layout.len(), pages_of(&ed, &ids)[2]), (6, Some(3)), "{how:?}: a blank page before page 3, which is the fourth now");
        item(&mut ed, 0, M::After);
        assert_eq!((ed.layout.len(), pages_of(&ed, &ids)[1]), (7, Some(2)), "{how:?}: and one after the first");
        item(&mut ed, 1, M::Duplicate);
        assert_eq!((ed.layout.len(), ed.ink.len()), (8, 5), "{how:?}: the blank page after the first is copied, and it has no ink to copy");
        item(&mut ed, 4, M::Delete);
        assert_eq!(ed.layout.len(), 7, "{how:?}: a page deleted");
        for _ in 0..4 {
            let at = button_centre(&ed, OverviewControl::Undo);
            tap(&mut ed, how, at);
        }
        assert_eq!(ed.layout.len(), 5, "{how:?}: all four undone");
        assert_eq!(pages_of(&ed, &ids), (0..5).map(Some).collect::<Vec<_>>());
    }
}

#[test]
fn a_page_with_ink_is_duplicated_with_it_from_the_menu_and_the_last_page_cannot_be_deleted_there() {
    use crate::toolbar::OverviewMenuItem as M;
    let (mut ed, _) = with_ink(1);
    open(&mut ed, Press::Mouse);
    let at = number_centre(&ed, 0);
    tap(&mut ed, Press::Mouse, at);
    let at = menu_item_centre(&ed, 0, M::Delete);
    tap(&mut ed, Press::Mouse, at);
    assert_eq!(ed.layout.len(), 1, "a document keeps a page");
    let at = number_centre(&ed, 0);
    tap(&mut ed, Press::Stylus, at);
    let at = menu_item_centre(&ed, 0, M::Duplicate);
    tap(&mut ed, Press::Stylus, at);
    assert_eq!((ed.layout.len(), ed.ink.len()), (2, 2), "the page and its ink are copied");
    let mut ui = crate::toolbar::Ui::default();
    let at = number_centre(&ed, 1);
    tap(&mut ed, Press::Mouse, at);
    ed.draw_overview(&mut ui);
    for text in ["Add page before", "Add page after", "Duplicate", "Delete"] {
        assert!(ui.labels.iter().any(|l| l.text == text), "{text} is shown");
    }
}

#[test]
fn the_wheel_sends_the_pages_gliding_and_they_arrive_without_overshooting_and_a_finger_moves_them_at_once() {
    let mut ed = many(120);
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    open(&mut ed, Press::Mouse);
    let layout = ed.overview_layout();
    ed.mouse_moved([layout.grid[0] + 200.0, layout.grid[1] + 200.0]);
    ed.wheel(Wheel::Lines(0.0, -3.0));
    assert_eq!(scroll_of(&ed), 0.0, "the wheel has not moved them yet: they glide");
    let mut last = 0.0;
    let mut frames = 0;
    while ed.step_view(1.0 / 60.0) && frames < 120 {
        let now = scroll_of(&ed);
        assert!(now >= last, "never backwards");
        last = now;
        frames += 1;
    }
    let arrived = scroll_of(&ed);
    assert!(arrived > 0.0 && frames > 3 && frames < 60, "a short glide, not a jump and not a crawl: {frames} frames");
    // Two turns in a row go the sum of both, wherever the first had got to.
    ed.wheel(Wheel::Lines(0.0, -1.0));
    ed.step_view(1.0 / 60.0);
    ed.wheel(Wheel::Lines(0.0, -1.0));
    glide_done(&mut ed);
    let unit = ed.overview_layout().unit() * 2.4;
    assert!((scroll_of(&ed) - arrived - 2.0 * unit).abs() < 1.0, "{} from {arrived}", scroll_of(&ed));
    // A turn the other way before it arrived turns it round from where it was heading, never past the ends.
    ed.wheel(Wheel::Lines(0.0, 1.0e6));
    glide_done(&mut ed);
    assert_eq!(scroll_of(&ed), 0.0);
}

#[test]
fn the_overview_comes_into_view_over_a_moment_and_is_there_for_good_after() {
    let mut ed = many(4);
    open(&mut ed, Press::Mouse);
    assert!(ed.overview_open());
    // (Under test the overview is up at once; a real one starts clear and is faded in by the frames.)
    assert_eq!(ed.overview_appear(), 1.0);
    let mut ui = crate::toolbar::Ui::default();
    ed.draw_overview(&mut ui);
    let faded = {
        let mut half = crate::toolbar::Ui::default();
        ed.draw_overview(&mut half);
        half.fade(0.5);
        half
    };
    assert_eq!(ui.panels.len(), faded.panels.len(), "fading changes no shapes");
    assert!(faded.panels.iter().zip(&ui.panels).all(|(f, p)| f.color[3] <= p.color[3]));
    assert!(faded.panels.iter().zip(&ui.panels).any(|(f, p)| f.color[3] < p.color[3]), "and does make them clearer");
    assert!(faded.images.iter().zip(&ui.images).all(|(f, p)| (f.alpha - p.alpha * 0.5).abs() < 1e-6));
}

#[test]
fn in_edit_mode_a_page_not_chosen_has_a_visible_ring_to_tick() {
    let mut ed = many(4);
    open(&mut ed, Press::Mouse);
    let mut plain = crate::toolbar::Ui::default();
    ed.draw_overview(&mut plain);
    ed.chrome.overview.as_mut().expect("open").edit = true;
    let mut editing = crate::toolbar::Ui::default();
    ed.draw_overview(&mut editing);
    // A ring that can be seen on white paper is made of grey lines (and the page shows through it: no disc under it).
    let (grey, solid) = (|c: &Capsule| c.color[0] < 200 && c.color[3] == 255, |c: &Capsule| c.a == c.b);
    let (before, after) = (plain.lines.iter().filter(|c| grey(c)).count(), editing.lines.iter().filter(|c| grey(c)).count());
    assert!(after >= before + 4 * 12, "{after} against {before}: a ring for each of the pages whole in view");
    let covering = |ui: &crate::toolbar::Ui| ui.lines.iter().filter(|c| solid(c) && c.color[0] > 250 && c.color[3] > 0).count();
    assert_eq!(covering(&editing), covering(&plain), "no white disc is laid over the page");
}

fn page_motion(ed: &Editor, page: usize) -> crate::toolbar::PageMotion {
    let state = ed.chrome.overview.as_ref().expect("open");
    crate::toolbar::PageMotion { shift: state.slides_for_tests(page), grow: state.grows_for_tests(page), tick: state.ticks_for_tests(page) }
}

#[test]
fn the_overview_fades_in_from_clear_and_is_whole_when_it_has_come() {
    let mut ed = many(4);
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    open(&mut ed, Press::Mouse);
    assert_eq!(ed.overview_appear(), 0.0, "it starts clear");
    ed.step_view(0.016);
    let early = ed.overview_appear();
    assert!(early > 0.0 && early < 1.0, "{early}");
    let mut last = early;
    let mut frames = 0;
    while ed.step_view(0.016) && frames < 100 {
        let now = ed.overview_appear();
        assert!(now >= last);
        last = now;
        frames += 1;
    }
    assert_eq!(ed.overview_appear(), 1.0);
    assert!(frames < 40, "a short moment: {frames} frames");
    // Faded, what it draws is as clear as it is.
    let mut ui = crate::toolbar::Ui::default();
    ed.draw_overview(&mut ui);
    assert!(ui.labels.iter().all(|l| l.alpha == 1.0) && ui.images.iter().all(|i| i.alpha == 1.0));
}

#[test]
fn what_is_put_away_goes_out_over_a_moment_and_is_then_gone() {
    let mut ed = many(4);
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    open(&mut ed, Press::Mouse);
    glide_done(&mut ed);
    assert_eq!(ed.overview_leaving(), 0.0, "up: none of it is going out");
    ed.close_overview();
    assert_eq!(ed.overview_leaving(), 1.0, "as it was");
    ed.step_view(0.016);
    let going = ed.overview_leaving();
    assert!(going > 0.0 && going < 1.0, "{going}");
    for _ in 0..40 {
        ed.step_view(0.016);
    }
    assert_eq!(ed.overview_leaving(), 0.0);
    assert!(!ed.step_view(0.016), "idle");
    // Opened again while going out: none of it is going out.
    ed.close_overview();
    open(&mut ed, Press::Mouse);
    assert_eq!(ed.overview_leaving(), 0.0);
    // With motion reduced it is gone at once.
    let mut still = many(4);
    open(&mut still, Press::Mouse);
    still.close_overview();
    assert_eq!(still.overview_leaving(), 0.0);
}

#[test]
fn a_page_that_moves_slides_from_where_it_was_and_a_new_one_grows_in() {
    let mut ed = many(10);
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    open(&mut ed, Press::Mouse);
    glide_done(&mut ed);
    // The first page goes to the end: the others each move a place up, and the page itself goes across the grid.
    assert!(ed.edit_pages(PageEdit::Move { pages: vec![0], to: 9 }));
    let layout = ed.overview_layout();
    let before = layout.cell(1, 0.0);
    let after = layout.cell(0, 0.0);
    // What was page 1 is page 0 now, and starts where page 1 was.
    let slid = page_motion(&ed, 0).shift;
    assert!((slid[0] - (before[0] - after[0])).abs() < 1.0 && (slid[1] - (before[1] - after[1])).abs() < 1.0, "{slid:?}");
    let moved = page_motion(&ed, 9).shift;
    assert!(moved[0].abs() > 1.0 || moved[1].abs() > 1.0, "the page that was first comes from where it was: {moved:?}");
    for _ in 0..40 {
        ed.step_view(0.016);
    }
    assert!((0..10).all(|p| page_motion(&ed, p) == crate::toolbar::PageMotion::default()), "all arrived");
    // A page added grows in; the others do not.
    assert!(ed.edit_pages(PageEdit::Insert { at: 3, size: [612.0, 792.0] }));
    assert!(page_motion(&ed, 3).grow > 0.9, "new: {:?}", page_motion(&ed, 3));
    assert_eq!(page_motion(&ed, 2).grow, 0.0);
    ed.step_view(0.016);
    let (early, _) = (page_motion(&ed, 3).grow, ());
    assert!(early > 0.0 && early < 1.0);
    for _ in 0..40 {
        ed.step_view(0.016);
    }
    assert_eq!(page_motion(&ed, 3).grow, 0.0);
    // A copy grows in too.
    assert!(ed.edit_pages(PageEdit::Duplicate { pages: vec![5] }));
    assert!(page_motion(&ed, 6).grow > 0.9, "{:?}", page_motion(&ed, 6));
}

#[test]
fn a_page_chosen_has_its_tick_pop_in_and_with_motion_reduced_nothing_moves() {
    let mut ed = many(6);
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    open(&mut ed, Press::Mouse);
    ed.chrome.overview.as_mut().expect("open").edit = true;
    let at = cell_centre(&ed, 2);
    tap(&mut ed, Press::Mouse, at);
    ed.step_view(0.016);
    let early = page_motion(&ed, 2).tick;
    assert!(early > 0.0 && early < 1.0, "{early}");
    assert_eq!(page_motion(&ed, 1).tick, 0.0, "only the page chosen");
    for _ in 0..30 {
        ed.step_view(0.016);
    }
    assert_eq!(page_motion(&ed, 2).tick, 0.0);
    // The same with motion reduced.
    let mut still = many(6);
    open(&mut still, Press::Mouse);
    still.chrome.overview.as_mut().expect("open").edit = true;
    let at = cell_centre(&still, 2);
    tap(&mut still, Press::Mouse, at);
    still.step_view(0.016);
    assert_eq!(page_motion(&still, 2), crate::toolbar::PageMotion::default());
    assert!(still.edit_pages(PageEdit::Move { pages: vec![0], to: 5 }));
    assert!((0..6).all(|p| page_motion(&still, p) == crate::toolbar::PageMotion::default()));
}
