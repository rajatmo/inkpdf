//! The text tool: the mouse places, selects and moves text boxes, the keyboard types into the one
//! being edited, the finger pans and zooms, and the stylus does nothing to the page.

use super::*;

fn with_text() -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Text);
    ed.chrome.panel = None;
    ed
}

fn tap(ed: &mut Editor, x: f32, y: f32) {
    let at = on_page(ed, x, y);
    mouse_click_at(ed, at);
}

/// The mouse button pressed at the first point, dragged through the rest and let go.
fn mouse_drag(ed: &mut Editor, path: &[[f32; 2]]) {
    ed.mouse_moved(path[0]);
    ed.mouse_button(ElementState::Pressed, MouseButton::Left);
    for &p in &path[1..] {
        ed.mouse_moved(p);
    }
    ed.mouse_button(ElementState::Released, MouseButton::Left);
}

fn texts(ed: &Editor) -> Vec<String> {
    ed.ink.strokes().iter().filter_map(|s| s.text.as_ref().map(|t| t.text.clone())).collect()
}

/// A box with `words` in it, its top-left corner at (`x`, `y`) on page 0, finished.
fn written(ed: &mut Editor, x: f32, y: f32, words: &str) {
    tap(ed, x, y);
    ed.type_text(words);
    ed.finish_text();
}

#[test]
fn a_tap_on_the_corner_a_box_was_placed_at_or_just_outside_it_edits_it() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    for (dx, dy) in [(0.0, 0.0), (-3.0, -3.0), (-5.0, 2.0)] {
        // (As after reopening a file: nothing is selected, so only the box itself can be hit.)
        ed.selection.clear();
        tap(&mut ed, 100.0 + dx, 200.0 + dy);
        assert!(ed.text_edit().is_some_and(|e| e.target.is_some()), "a tap {dx},{dy} from the corner edits the box");
        ed.finish_text();
    }
    assert_eq!(ed.ink.len(), 1, "no second box was made");
    // Well away from it, a tap still starts a new box.
    ed.selection.clear();
    tap(&mut ed, 100.0 - 20.0, 200.0);
    assert!(ed.text_edit().is_some_and(|e| e.target.is_none()));
}

#[test]
fn a_tap_on_empty_page_starts_a_box_and_the_keyboard_fills_it() {
    let mut ed = with_text();
    assert!(!ed.is_typing());
    tap(&mut ed, 100.0, 200.0);
    assert!(ed.is_typing());
    assert_eq!(ed.ink.len(), 0, "nothing is stored until the typing ends");
    ed.type_text("Hello");
    ed.type_text(", world");
    assert_eq!(ed.text_edit().map(|e| e.text.as_str()), Some("Hello, world"));
    // Another tap on empty page ends it and starts the next.
    tap(&mut ed, 100.0, 400.0);
    assert_eq!(texts(&ed), ["Hello, world"]);
    assert!(ed.is_typing() && ed.text_edit().is_some_and(|e| e.target.is_none() && e.text.is_empty()));
    // The box begins where the pen touched.
    let stroke = &ed.ink.strokes()[0];
    assert_eq!(stroke.pts[0], [112.0, 212.0]);
    assert_eq!(stroke.text.as_ref().map(|t| t.size), Some(ToolStyle::TEXT.width));
    assert_eq!(stroke.style.rgba, PALETTE[ToolStyle::TEXT.colour]);
}

#[test]
fn only_the_text_tool_while_a_box_is_being_edited_hears_the_keyboard() {
    for tool in [Tool::Pen, Tool::Geometry, Tool::Laser] {
        let mut ed = editor();
        ed.set_tool(tool);
        let before = (ed.ink.len(), ed.selection.clone(), ed.ink.is_dirty(), ed.view.scroll);
        ed.type_text("abc\n");
        ed.backspace();
        assert_eq!((ed.ink.len(), ed.selection.clone(), ed.ink.is_dirty(), ed.view.scroll), before, "{tool:?}");
        assert!(ed.text_edit().is_none() && !ed.is_typing(), "{tool:?}");
    }
    // The text tool with no box open: typing starts nothing.
    let mut ed = with_text();
    ed.type_text("abc");
    assert!(ed.text_edit().is_none() && ed.ink.len() == 0);
    // And not while the prompt is up.
    tap(&mut ed, 100.0, 200.0);
    ed.ask_unsaved();
    ed.type_text("abc");
    assert_eq!(ed.text_edit().map(|e| e.text.as_str()), Some(""));
}

#[test]
fn only_the_mouse_places_and_selects_text_and_the_stylus_and_a_finger_do_nothing() {
    let mut ed = with_text();
    let at = on_page(&ed, 100.0, 200.0);
    // The stylus does nothing to the page with the text tool: not its tip, not its eraser end, not its barrel.
    let away = on_page(&ed, 300.0, 300.0);
    for buttons in [TIP, ERASER, BARREL] {
        pen_stroke(&mut ed, buttons, &[at, at]);
        pen_stroke(&mut ed, buttons, &[at, away]);
    }
    // A finger pans and does not start a box either.
    ed.pointer_event(touch(1, Phase::Down, at, 30.0));
    ed.pointer_event(touch(1, Phase::Up, at, 30.0));
    assert!(ed.text_edit().is_none() && ed.ink.len() == 0 && ed.selection.is_empty() && !ed.ink.is_dirty());
    // The mouse starts a box.
    mouse_click_at(&mut ed, at);
    assert!(ed.is_typing());
    ed.type_text("Hello");
    ed.finish_text();
    // Neither the stylus nor a finger selects or edits it.
    ed.selection.clear();
    let inside = on_page(&ed, 105.0, 205.0);
    for buttons in [TIP, ERASER, BARREL] {
        pen_stroke(&mut ed, buttons, &[inside, inside]);
    }
    ed.pointer_event(touch(2, Phase::Down, inside, 30.0));
    ed.pointer_event(touch(2, Phase::Up, inside, 30.0));
    assert!(ed.selection.is_empty() && ed.text_edit().is_none());
    assert_eq!(texts(&ed), ["Hello"], "and nothing was erased");
    // The mouse does.
    mouse_click_at(&mut ed, inside);
    assert!(ed.text_edit().is_some_and(|e| e.text == "Hello"));
}

#[test]
fn the_stylus_can_still_press_the_buttons_of_the_toolbar_with_the_text_tool() {
    let mut ed = with_text();
    let save = ed.toolbar().centre_of(Item::Save);
    pen_stroke(&mut ed, TIP, &[save, save]);
    assert!(ed.requests.has(&Request::Save));
}
#[test]
fn the_backspace_key_takes_a_character_away_and_enter_starts_a_line() {
    let mut ed = with_text();
    tap(&mut ed, 100.0, 160.0);
    ed.type_text("abc");
    ed.backspace();
    ed.type_text("\n");
    ed.type_text("xy");
    assert_eq!(ed.text_edit().map(|e| e.text.as_str()), Some("ab\nxy"));
    // Backspace past the start is harmless.
    for _ in 0..10 {
        ed.backspace();
    }
    assert_eq!(ed.text_edit().map(|e| e.text.as_str()), Some(""));
    // Control characters are not text; a tab is a space.
    ed.type_text("a\u{7}\u{1b}b\tc");
    assert_eq!(ed.text_edit().map(|e| e.text.as_str()), Some("ab c"));
}

#[test]
fn a_box_is_never_longer_than_the_limit() {
    let mut ed = with_text();
    tap(&mut ed, 100.0, 160.0);
    ed.type_text(&"x".repeat(crate::text::MAX_CHARS - 1));
    ed.type_text("yyyyyyyy");
    assert_eq!(ed.text_edit().map(|e| e.text.chars().count()), Some(crate::text::MAX_CHARS));
    ed.type_text("z");
    assert_eq!(ed.text_edit().map(|e| e.text.chars().count()), Some(crate::text::MAX_CHARS));
}

#[test]
fn a_tap_on_a_box_edits_it_and_the_change_is_one_undo_step() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    let id = ed.ink.strokes()[0].id;
    assert_eq!(ed.selection, [id], "a new box is selected");
    tap(&mut ed, 105.0, 205.0);
    assert!(ed.text_edit().is_some_and(|e| e.target == Some(id)));
    ed.type_text(" there");
    ed.finish_text();
    assert_eq!(texts(&ed), ["Hello there"]);
    assert_eq!(ed.ink.len(), 1, "the same box, changed");
    ed.undo();
    assert_eq!(texts(&ed), ["Hello"]);
    ed.redo();
    assert_eq!(texts(&ed), ["Hello there"]);
    // Tapping it and ending without a change makes no step.
    let steps = ed.ink.can_undo();
    ed.undo();
    ed.redo();
    tap(&mut ed, 105.0, 205.0);
    ed.finish_text();
    assert_eq!((steps, ed.ink.can_redo()), (true, false));
}

#[test]
fn undo_while_typing_throws_the_typing_away_first() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    // Typing a new box.
    tap(&mut ed, 100.0, 400.0);
    ed.type_text("draft");
    ed.undo();
    assert!(ed.text_edit().is_none());
    assert_eq!(texts(&ed), ["Hello"], "the draft went and the finished box stayed");
    // Typing into an old box.
    tap(&mut ed, 105.0, 205.0);
    ed.type_text(" there");
    ed.undo();
    assert!(ed.text_edit().is_none());
    assert_eq!(texts(&ed), ["Hello"]);
    // With nothing typed, undo is an ordinary undo.
    tap(&mut ed, 100.0, 400.0);
    ed.undo();
    assert!(texts(&ed).is_empty(), "the box was undone");
}

#[test]
fn an_empty_box_leaves_nothing_and_emptying_a_box_removes_it() {
    let mut ed = with_text();
    tap(&mut ed, 100.0, 200.0);
    ed.type_text("  \n ");
    ed.finish_text();
    assert_eq!(ed.ink.len(), 0);
    assert!(!ed.ink.is_dirty());
    written(&mut ed, 100.0, 200.0, "Hello");
    tap(&mut ed, 105.0, 205.0);
    for _ in 0..5 {
        ed.backspace();
    }
    ed.finish_text();
    assert_eq!(ed.ink.len(), 0);
    assert!(ed.selection.is_empty());
    ed.undo();
    assert_eq!(texts(&ed), ["Hello"], "undo brings it back");
}

#[test]
fn a_drag_moves_a_box_and_a_tap_does_not() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    let from = ed.ink.strokes()[0].pts[0];
    let path = [on_page(&ed, 105.0, 205.0), on_page(&ed, 125.0, 245.0), on_page(&ed, 205.0, 305.0)];
    mouse_drag(&mut ed, &path);
    let to = ed.ink.strokes()[0].pts[0];
    assert!((to[0] - from[0] - 100.0).abs() < 0.5 && (to[1] - from[1] - 100.0).abs() < 0.5, "moved by the drag: {from:?} to {to:?}");
    assert!(ed.text_edit().is_none(), "a drag does not start typing");
    assert_eq!(texts(&ed), ["Hello"]);
    ed.undo();
    assert_eq!(ed.ink.strokes()[0].pts[0], from, "the move is one undo step");
}

#[test]
fn leaving_the_tool_saving_or_adding_a_page_finishes_the_typing() {
    type Step = fn(&mut Editor);
    let steps: [(&str, Step); 4] = [
        ("tool", |ed| ed.set_tool(Tool::Pen)),
        ("save", |ed| {
            let save = ed.toolbar().centre_of(Item::Save);
            mouse_click_at(ed, save);
        }),
        ("page", |ed| ed.page_inserted(Layout::new(&[[612.0, 792.0], [612.0, 792.0], [612.0, 792.0]]), 2)),
        ("settle", |ed| ed.finish_text()),
    ];
    for (what, step) in steps {
        let mut ed = with_text();
        tap(&mut ed, 100.0, 200.0);
        ed.type_text("kept");
        step(&mut ed);
        assert!(ed.text_edit().is_none(), "{what}");
        assert_eq!(texts(&ed), ["kept"], "{what}");
    }
}

#[test]
fn the_colour_and_size_in_the_panel_style_the_box_being_typed() {
    let mut ed = with_text();
    tap(&mut ed, 100.0, 200.0);
    ed.type_text("big");
    ed.chrome.panel = Some(PanelState::default());
    ed.set_slider(Slider::Width, 60.0);
    ed.palette[2] = [200, 10, 10, 255];
    ed.text_style.colour = 2;
    ed.restyle_tool();
    ed.finish_text();
    let s = &ed.ink.strokes()[0];
    assert_eq!((s.text.as_ref().map(|t| t.size), s.style.rgba), (Some(60.0), [200, 10, 10, 255]));
    // The size stays in the text range, which is not the pen's.
    ed.set_slider(Slider::Width, 1.0e6);
    assert_eq!(ed.text_style.width, crate::tool::TEXT_SIZES.1);
    ed.set_slider(Slider::Width, -4.0);
    assert_eq!(ed.text_style.width, crate::tool::TEXT_SIZES.0);
    assert_eq!(ed.pen, ToolStyle::PEN, "the pen is untouched");
}

#[test]
fn typed_text_wraps_at_the_pages_right_edge_and_never_leaves_the_page() {
    let mut ed = with_text();
    let page = ed.page_rect(0).expect("page");
    // A long text from the middle: broken into lines that end at the page's edge.
    tap(&mut ed, 300.0, 140.0);
    ed.type_text(&"word ".repeat(40));
    let edit = ed.text_edit().cloned().expect("typing");
    assert!(edit.text.lines().count() > 3 && edit.text.replace('\n', "") == "word ".repeat(40), "{:?}", edit.text);
    ed.finish_text();
    let b = ed.ink.strokes()[0].bbox();
    assert!(b[0] >= page[0] - 0.01 && b[2] <= page[2] + 0.5, "{b:?} within {page:?}");
    // One long word is broken inside.
    tap(&mut ed, 300.0, 400.0);
    ed.type_text(&"W".repeat(60));
    ed.finish_text();
    let b = ed.ink.strokes()[1].bbox();
    assert!(b[2] <= page[2] + 0.01, "{b:?}");
    // A box begun right at the edge is moved left, so that there is room to type, and stays there.
    tap(&mut ed, 600.0, 600.0);
    ed.type_text("close to the edge");
    let edit = ed.text_edit().cloned().expect("typing");
    assert!(edit.top_left[0] < 500.0, "moved left: {:?}", edit.top_left);
    ed.finish_text();
    let b = ed.ink.strokes()[2].bbox();
    assert!(b[0] >= page[0] && b[2] <= page[2] + 0.5, "{b:?} within {page:?}");
    // Past the bottom edge the box is moved up.
    tap(&mut ed, 100.0, 780.0);
    ed.type_text("low\nlower\nlowest");
    ed.finish_text();
    let b = ed.ink.strokes()[3].bbox();
    assert!(b[3] <= page[3] + 0.01, "{b:?} within {page:?}");
    // Backspace takes a line break away whole, giving the words back as they were.
    tap(&mut ed, 100.0, 700.0);
    ed.type_text(&"ab ".repeat(100));
    let broken = ed.text_edit().map(|e| e.text.clone()).unwrap_or_default();
    assert!(broken.contains('\n'));
    // Making the text bigger breaks it again.
    let lines = broken.lines().count();
    ed.text_style.width = 90.0;
    ed.restyle_tool();
    assert!(ed.text_edit().is_some_and(|e| e.text.lines().count() > lines));
    ed.finish_text();
}
#[test]
fn text_is_erased_and_lassoed_by_the_text_tool_alone_and_moved_by_the_mouse() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    let across = [on_page(&ed, 100.0, 195.0), on_page(&ed, 110.0, 210.0), on_page(&ed, 120.0, 230.0)];
    let around =
        [on_page(&ed, 80.0, 180.0), on_page(&ed, 300.0, 180.0), on_page(&ed, 300.0, 260.0), on_page(&ed, 80.0, 260.0), on_page(&ed, 80.0, 180.0)];
    // The pen tool works on handwriting: its eraser end and its barrel button pass the text by.
    ed.set_tool(Tool::Pen);
    pen_stroke(&mut ed, ERASER, &across);
    pen_stroke(&mut ed, BARREL, &around);
    assert_eq!((ed.ink.len(), ed.selection.len()), (1, 0));
    // As a tablet the text tool takes the stylus: its eraser end erases the box, its barrel button lassos it.
    ed.set_tool(Tool::Text);
    ed.set_mode(Mode::Tablet);
    pen_stroke(&mut ed, ERASER, &across);
    assert_eq!(ed.ink.len(), 0);
    ed.undo();
    assert_eq!(texts(&ed), ["Hello"]);
    ed.selection.clear();
    pen_stroke(&mut ed, BARREL, &around);
    assert_eq!(ed.selection.len(), 1);
    assert!(ed.text_edit().is_none());
    ed.set_mode(Mode::Desktop);
    // With the text tool the mouse takes hold of a selection and drags it.
    ed.set_tool(Tool::Text);
    ed.selection = vec![ed.ink.strokes()[0].id];
    let from = ed.ink.strokes()[0].pts[0];
    let path = [on_page(&ed, 105.0, 205.0), on_page(&ed, 150.0, 250.0), on_page(&ed, 205.0, 305.0)];
    mouse_drag(&mut ed, &path);
    let to = ed.ink.strokes()[0].pts[0];
    assert!((to[0] - from[0] - 100.0).abs() < 0.5 && (to[1] - from[1] - 100.0).abs() < 0.5);
}
#[test]
fn text_survives_a_page_inserted_before_it() {
    let mut ed = with_text();
    written(&mut ed, 100.0, 200.0, "Hello");
    let before = ed.ink.strokes()[0].pts[0];
    let layout = Layout::new(&[[612.0, 792.0], [612.0, 792.0], [612.0, 792.0]]);
    let shift = layout.page(2).map(|p| p.y).unwrap_or(0.0) - ed.layout.page(1).map(|p| p.y).unwrap_or(0.0);
    ed.page_inserted(layout, 0);
    let after = &ed.ink.strokes()[0];
    assert_eq!(after.page, 1);
    assert!((after.pts[0][1] - before[1] - shift).abs() < 0.01, "{before:?} {:?}", after.pts[0]);
    assert_eq!(texts(&ed), ["Hello"]);
}
