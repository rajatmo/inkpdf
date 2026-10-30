use super::*;
use crate::form::{Field, Form, Kind, Spec, Value};
use crate::links::{Link, Target};
use crate::tools::Key;

/// A form on page 0: a text field (the first band), a check box, two radio buttons of one group, and a choice of Red and Green. Each box is a link,
/// as the document makes them.
fn with_form(ed: &mut Editor) {
    let spec = |name: &str, states: &[Option<&str>], options: &[&str]| Spec {
        name: name.into(),
        on_states: states.iter().map(|s| s.map(str::to_string)).collect(),
        options: options.iter().map(|o| (o.to_string(), o.to_string())).collect(),
        ..Spec::default()
    };
    ed.form = Form {
        fields: vec![
            Field::new(Kind::Text, spec("name", &[None], &[])),
            Field::new(Kind::Check, spec("agree", &[Some("Yes")], &[])),
            Field::new(Kind::Radio, spec("size", &[Some("Small"), Some("Large")], &[])),
            Field::new(Kind::Choice, spec("colour", &[None], &["Red", "Green"])),
        ],
    };
    let at = |field: usize, widget: usize, rect: [f32; 4]| Link { rect, target: Target::Field { field, widget } };
    ed.links = vec![
        at(0, 0, [112.0, 202.0, 312.0, 222.0]),
        at(1, 0, [112.0, 252.0, 132.0, 272.0]),
        at(2, 0, [112.0, 302.0, 132.0, 322.0]),
        at(2, 1, [212.0, 302.0, 232.0, 322.0]),
        at(3, 0, [112.0, 352.0, 312.0, 372.0]),
    ];
}

/// The centre of box `n` (in the order above), on the screen.
fn box_at(ed: &Editor, n: usize) -> [f32; 2] {
    let r = ed.links[n].rect;
    ed.view.doc_to_screen([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0])
}

/// A press on the box with each way there is: a finger (with any tool), and the stylus's tip and the left mouse button (with the Select tool).
fn press_with(ed: &mut Editor, how: &str, at: [f32; 2]) {
    match how {
        "finger" => {
            ed.pointer_event(touch(1, Phase::Down, at, 30.0));
            ed.pointer_event(touch(1, Phase::Up, at, 30.0));
        }
        "stylus" => pen_stroke(ed, TIP, &[at, at]),
        _ => mouse_click_at(ed, at),
    }
}

fn press_box(ed: &mut Editor, how: &str, n: usize) {
    let at = box_at(ed, n);
    press_with(ed, how, at);
}

/// A press on bare page at (`x`, `y`) of page 0.
fn press_at(ed: &mut Editor, how: &str, page_xy: [f32; 2]) {
    let at = on_page(ed, page_xy[0], page_xy[1]);
    press_with(ed, how, at);
}

const WAYS: [&str; 3] = ["finger", "stylus", "mouse"];

fn value(ed: &Editor, name: &str) -> Option<Value> {
    ed.form.changes().into_iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

#[test]
fn every_device_ticks_a_check_box_chooses_a_radio_button_and_goes_through_a_choice_and_it_counts_as_a_change_to_save() {
    for how in WAYS {
        let mut ed = editor();
        with_form(&mut ed);
        ed.ws.tool = Tool::Select;
        ed.ink.mark_clean();
        assert!(!ed.ink.is_dirty());
        press_box(&mut ed, how, 1);
        assert_eq!(value(&ed, "agree"), Some(Value::Check(true)), "{how}");
        assert!(ed.ink.is_dirty(), "{how}: there is something to save");
        press_box(&mut ed, how, 1);
        assert_eq!(value(&ed, "agree"), Some(Value::Check(false)), "{how}: pressed again, it is off");
        press_box(&mut ed, how, 3);
        assert_eq!(value(&ed, "size"), Some(Value::Radio(Some("Large".into()))), "{how}");
        press_box(&mut ed, how, 2);
        assert_eq!(value(&ed, "size"), Some(Value::Radio(Some("Small".into()))), "{how}: the other button of the group");
        for want in [vec!["Red"], vec!["Green"], vec![]] {
            press_box(&mut ed, how, 4);
            assert_eq!(value(&ed, "colour"), Some(Value::Choice(want.iter().map(ToString::to_string).collect())), "{how}");
        }
    }
}

#[test]
fn a_finger_fills_a_field_with_any_tool_and_the_pen_and_mouse_fill_it_only_with_the_select_tool() {
    for tool in [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Eraser, Tool::Select] {
        let mut ed = editor();
        with_form(&mut ed);
        ed.ws.tool = tool;
        press_box(&mut ed, "finger", 1);
        assert_eq!(value(&ed, "agree"), Some(Value::Check(true)), "{tool:?}: a finger");
    }
    for tool in [Tool::Pen, Tool::Geometry, Tool::Eraser] {
        let mut ed = editor();
        with_form(&mut ed);
        ed.ws.tool = tool;
        press_box(&mut ed, "stylus", 1);
        assert!(value(&ed, "agree").is_none(), "{tool:?}: the stylus draws with it, it does not fill");
    }
}

#[test]
fn a_text_field_is_typed_into_until_enter_escape_or_a_press_elsewhere() {
    for how in WAYS {
        let mut ed = editor();
        with_form(&mut ed);
        ed.ws.tool = Tool::Select;
        assert!(!ed.is_typing());
        press_box(&mut ed, how, 0);
        assert!(ed.is_typing() && ed.form_typing() == Some(0), "{how}: the field takes the keyboard");
        ed.type_text("Ada");
        assert_eq!(ed.form.fields[0].shown(), "Ada", "{how}");
        ed.backspace();
        ed.type_text("a Lovelace");
        assert_eq!(value(&ed, "name"), Some(Value::Text("Ada Lovelace".into())), "{how}");
        assert!(ed.ink.is_dirty());
        // Enter ends it in a field of one line, and so does Escape; what was typed stays.
        ed.type_text("\n");
        assert!(!ed.is_typing() && ed.form.fields[0].shown() == "Ada Lovelace", "{how}: Enter ends the field of one line");
        press_box(&mut ed, how, 0);
        assert!(ed.press(Key::Escape) && !ed.is_typing(), "{how}: Escape");
        // A press on another box, and a press on bare page, let go of it.
        press_box(&mut ed, how, 0);
        press_box(&mut ed, how, 1);
        assert!(!ed.is_typing() && value(&ed, "agree") == Some(Value::Check(true)), "{how}: a press on another box");
        press_box(&mut ed, how, 0);
        press_at(&mut ed, how, [400.0, 600.0]);
        assert!(!ed.is_typing(), "{how}: a press elsewhere");
        assert_eq!(ed.form.fields[0].shown(), "Ada Lovelace");
    }
}

#[test]
fn what_is_typed_in_a_field_is_not_taken_for_a_shortcut_nor_scrolls_the_page_and_ends_with_the_document() {
    let mut ed = editor();
    with_form(&mut ed);
    ed.ws.tool = Tool::Select;
    press_box(&mut ed, "mouse", 0);
    let before = ed.view.scroll;
    ed.type_text(" ");
    ed.press(Key::Page(true));
    assert_eq!(ed.view.scroll, before, "a space is typed, not a page down: {:?}", ed.form.fields[0].shown());
    assert_eq!(ed.form.fields[0].shown(), " ");
    ed.settle();
    assert!(!ed.is_typing(), "settling (before a save or a new tool) ends it");
    press_box(&mut ed, "mouse", 0);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.is_typing() && ed.form.fields.is_empty(), "another document has none of it");
}

#[test]
fn a_field_follows_its_page_when_pages_are_added_before_it_and_is_gone_with_its_page_and_back_with_it() {
    let mut ed = editor();
    with_form(&mut ed);
    ed.ws.tool = Tool::Select;
    let was = ed.links[1].rect;
    assert!(ed.edit_pages(crate::pages::PageEdit::Insert { at: 0, size: [612.0, 792.0] }));
    let now = ed.links[1].rect;
    assert!((now[1] - was[1] - 804.0).abs() < 0.5, "the box moved down a page: {was:?} {now:?}");
    press_box(&mut ed, "mouse", 1);
    assert_eq!(value(&ed, "agree"), Some(Value::Check(true)), "and is pressed where it is now");
    assert!(ed.edit_pages(crate::pages::PageEdit::delete(1)));
    assert!(ed.field_places().next().is_none(), "the boxes of a page that is deleted are gone");
    ed.undo();
    assert_eq!(ed.field_places().count(), 5, "and come back with it");
    assert_eq!(value(&ed, "agree"), Some(Value::Check(true)), "with what was put in them");
}
