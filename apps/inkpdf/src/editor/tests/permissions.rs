//! A PDF whose owner has not allowed adding to it is opened to be read: the card says so, the tools that add to the document are not available (the reader and
//! the pointer are), nor are the marks over selected text or the edits of the pages; a form that may not be filled is left alone. Everything is as it was for a
//! PDF with every permission.

use std::sync::Arc;

use super::*;
use crate::pagetext::FakeText;
use crate::permissions::Permissions;
use crate::tool::ToolAction;
use crate::toolbar::ContextItem;

const READ_ONLY: Permissions = Permissions { annotate: false, modify: false, assemble: false, fill_forms: false };

/// A pen in an editor of two pages, the second with text on the first, then the permissions of a restricted file.
fn restricted() -> Editor {
    let mut ed = editor();
    ed.set_text_files(vec![Some(0), Some(1)]);
    ed.add_text(0, Some(Arc::new(FakeText::new(&["", "", "", "", "", "", "", "", "Hello world", "The quick brown fox"]))));
    ed.add_text(1, None);
    ed.set_permissions(READ_ONLY);
    ed
}

#[test]
fn a_restricted_pdf_says_so_and_the_tool_in_use_that_adds_to_it_is_put_away() {
    let mut ed = editor();
    assert_eq!(ed.tool, Tool::Pen);
    ed.set_permissions(READ_ONLY);
    assert!(ed.read_only() && ed.error_open(), "the card is up");
    assert_eq!(ed.tool, Tool::Select, "the reader is in use");
    // With every permission nothing is said, and the tool is kept.
    let mut free = editor();
    free.set_permissions(Permissions::default());
    assert!(!free.read_only() && !free.error_open() && free.tool == Tool::Pen);
}

#[test]
fn the_tools_that_add_to_the_document_cannot_be_chosen_and_the_reader_and_the_pointer_can_with_every_device() {
    for how in 0..3 {
        let mut ed = restricted();
        ed.close_error();
        for tool in [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Lasso, Tool::Eraser] {
            let at = ed.toolbar().centre_of(Item::Tool(tool));
            match how {
                0 => mouse_click_at(&mut ed, at),
                1 => pen_tap(&mut ed, at),
                _ => finger_tap(&mut ed, at),
            }
            assert_eq!(ed.tool, Tool::Select, "device {how}: {tool:?} is not for a PDF that cannot be added to");
            assert!(ed.error_open(), "device {how}: the card says why");
            ed.close_error();
        }
        let laser = ed.toolbar().centre_of(Item::Tool(LASER));
        match how {
            0 => mouse_click_at(&mut ed, laser),
            1 => pen_tap(&mut ed, laser),
            _ => finger_tap(&mut ed, laser),
        }
        assert_eq!(ed.tool, LASER, "device {how}: the pointer only shows something");
        assert!(!ed.error_open());
        ed.set_tool(Tool::Select);
        assert_eq!(ed.tool, Tool::Select);
    }
}

#[test]
fn nothing_is_drawn_on_it_with_any_device_and_the_pages_and_their_flags_stay_as_they_are() {
    let mut ed = restricted();
    let path = [on_page(&ed, 100.0, 300.0), on_page(&ed, 160.0, 340.0), on_page(&ed, 220.0, 300.0)];
    pen_stroke(&mut ed, TIP, &path);
    mouse_drag(&mut ed, &path);
    assert_eq!(ed.ink.len(), 0);
    assert!(!ed.insert_page_after(0) && !ed.delete_current_page() && !ed.add_page(), "the pages cannot be edited");
    assert!(!ed.toggle_flag(0) && ed.flags.is_empty());
    assert_eq!(ed.layout.len(), 2);
    assert!(!ed.ink.is_dirty(), "and there is nothing to save");
}

#[test]
fn the_text_can_be_selected_and_copied_but_the_marks_are_dimmed() {
    let mut ed = restricted();
    let (a, b) = (on_page(&ed, 5.0, 8.0 + 160.0), on_page(&ed, 45.0, 8.0 + 160.0));
    pen_stroke(&mut ed, TIP, &[a, [(a[0] + b[0]) / 2.0, a[1]], b]);
    assert_eq!(ed.tools.select.text(&ed.ws).as_deref(), Some("Hello"), "the text is selected");
    let state = ed.context_view().expect("the bar");
    assert!(state.selection);
    assert!(state.actions.iter().flatten().all(|(_, on)| !*on), "the marks are dim");
    let at = ed.context_bar().expect("bar").centre_of(ContextItem::Action(ToolAction::Mark(crate::ink::MarkupKind::Highlight)));
    pen_stroke(&mut ed, TIP, &[at, at]);
    assert_eq!(ed.ink.len(), 0, "a press on a dim mark makes none");
    let copy = ed.context_bar().expect("bar").centre_of(ContextItem::Copy);
    pen_stroke(&mut ed, TIP, &[copy, copy]);
    assert_eq!(ed.clip.text.get().as_deref(), Some("Hello"), "copy works");
}

#[test]
fn a_form_that_may_not_be_filled_is_left_alone_and_opening_another_document_takes_the_permissions_of_that_one() {
    use crate::form::{Field, Form, Kind, Spec};
    let mut ed = editor();
    let spec = Spec { name: "agree".into(), on_states: vec![Some("Yes".into())], ..Spec::default() };
    ed.form = Form { fields: vec![Field::new(Kind::Check, spec)] };
    ed.set_permissions(Permissions { fill_forms: false, ..Permissions::default() });
    ed.press_field(0, 0, [10.0, 10.0, 30.0, 30.0]);
    assert!(!ed.form.fields[0].changed(), "the box was not ticked");
    assert!(ed.error_open());
    ed.close_error();
    ed.set_permissions(Permissions::default());
    ed.press_field(0, 0, [10.0, 10.0, 30.0, 30.0]);
    assert!(ed.form.fields[0].changed(), "with the permission it is");
    // A document loaded afterwards has its own permissions: this one's are forgotten.
    ed.set_permissions(READ_ONLY);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.read_only());
}
