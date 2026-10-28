use super::*;

const WINDOWS: [(f32, f32); 5] = [(480.0, 1.0), (800.0, 1.0), (1100.0, 1.5), (1650.0, 1.5), (3840.0, 2.0)];

#[test]
fn every_item_is_hit_at_its_centre_and_there_are_no_others() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let mut seen = Vec::new();
    for &(item, r) in &tb.items {
        let c = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        assert_eq!(tb.hit(c), Some(item));
        assert!(tb.contains(c));
        seen.push(item);
    }
    assert_eq!(
        seen,
        [
            Item::Close,
            Item::Undo,
            Item::Redo,
            Item::Save,
            Item::DeletePage,
            Item::AddPage,
            Item::Tool(Tool::Pen),
            Item::Tool(Tool::Geometry),
            Item::Tool(Tool::Text),
            Item::Tool(Tool::Laser),
            Item::Fullscreen
        ]
    );
}

#[test]
fn the_main_toolbar_holds_only_the_tools() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let in_middle: Vec<Item> =
        tb.items.iter().filter(|(_, r)| rect_contains(tb.pills[1], [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0])).map(|(i, _)| *i).collect();
    assert_eq!(in_middle, [Item::Tool(Tool::Pen), Item::Tool(Tool::Geometry), Item::Tool(Tool::Text), Item::Tool(Tool::Laser)]);
}

#[test]
fn close_is_in_a_pill_of_its_own_at_the_top_left_and_full_screen_at_the_top_right() {
    for (w, dpr) in WINDOWS {
        let tb = Toolbar::layout([w, 900.0], dpr);
        let [left, middle, right] = tb.pills;
        assert!(left[2] < middle[0] && middle[2] < right[0], "three separate pills at {w}: {:?}", tb.pills);
        assert!(left[0] <= 12.0 * dpr + 0.5 && left[0] >= 0.0, "the left pill hugs the left edge at {w}");
        assert!(right[2] <= w + 0.5 && right[2] >= w - 13.0 * dpr, "the right pill hugs the right edge at {w}");
        let close = tb.button(Item::Close).expect("close");
        assert!(close[0] < left[0] + 12.0 * dpr && close[0] >= left[0], "close is the first thing in the left pill at {w}");
        let full = tb.button(Item::Fullscreen).expect("fullscreen");
        assert!(rect_contains(right, [(full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0]));
    }
}

#[test]
fn the_middle_pill_is_centred_and_everything_fits_any_reasonable_window() {
    for (w, dpr) in WINDOWS {
        let tb = Toolbar::layout([w, 900.0], dpr);
        let middle = tb.pills[1];
        assert!(((middle[0] + middle[2]) / 2.0 - w / 2.0).abs() < 1.0, "not centred at {w}");
        for pill in tb.pills {
            assert!(pill[0] >= 0.0 && pill[2] <= w + 0.5, "{pill:?} off screen at {w}");
        }
    }
}

#[test]
fn outside_the_pills_hits_nothing() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    assert_eq!(tb.hit([5.0, 5.0]), None);
    assert!(!tb.contains([5.0, 5.0]));
    assert!(!tb.contains([800.0, 900.0]));
    let gap = [(tb.pills[0][2] + tb.pills[1][0]) / 2.0, tb.pills[0][1] + 20.0];
    assert!(!tb.contains(gap), "between the pills is the page");
}

#[test]
fn the_gap_inside_the_left_pill_hits_nothing() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let (close, undo) = (tb.button(Item::Close).expect("close"), tb.button(Item::Undo).expect("undo"));
    let mid = [(close[2] + undo[0]) / 2.0, (close[1] + close[3]) / 2.0];
    assert_eq!(tb.hit(mid), None);
    assert!(tb.contains(mid), "still on the pill");
}

#[test]
fn widths_map_to_the_slider_and_back() {
    for w in [MIN_WIDTH, 0.5, 1.2, 2.5, 5.0, 12.0, MAX_WIDTH] {
        let back = width_at(slider_position(w));
        assert!((back / w - 1.0).abs() < 1e-3, "{w} -> {back}");
    }
    assert_eq!(width_at(0.0), MIN_WIDTH);
    assert!((width_at(1.0) - MAX_WIDTH).abs() < 1e-3);
    // Beyond the ends it stops at them; nonsense widths do not produce nonsense positions.
    assert_eq!(width_at(-3.0), MIN_WIDTH);
    assert_eq!(slider_position(1e9), 1.0);
    assert_eq!(slider_position(0.0), 0.0);
    assert_eq!(slider_position(-5.0), 0.0);
    // Equal steps along the slider are equal ratios of width: it is logarithmic.
    let (a, b, c) = (width_at(0.2), width_at(0.4), width_at(0.6));
    assert!((b / a - c / b).abs() < 1e-3);
}

#[test]
fn items_are_big_enough_to_tap() {
    let tb = Toolbar::layout([1100.0, 900.0], 1.0);
    for &(_, r) in &tb.items {
        assert!(r[2] - r[0] >= 24.0 && r[3] - r[1] >= 24.0);
    }
}

#[test]
fn the_pen_button_wears_the_colour_the_pen_writes_in() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let draw = |colour| {
        let mut ui = Ui::default();
        tb.draw(&State { pen_colour: colour, ..State::default() }, &mut ui);
        ui.lines
    };
    let magenta = [255, 0, 255, 255];
    assert!(draw(magenta).iter().any(|c| c.color == magenta));
    assert!(!draw(PALETTE[1]).iter().any(|c| c.color == magenta));
}

#[test]
fn drawing_stays_inside_the_pills_and_marks_the_tool_in_use() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let state = State { can_undo: true, unsaved: true, ..State::default() };
    let mut ui = Ui::default();
    tb.draw(&state, &mut ui);
    assert!(ui.lines.len() > 40, "icons are made of many segments: {}", ui.lines.len());
    for c in &ui.lines {
        for p in [c.a, c.b] {
            assert!(
                tb.pills.iter().any(|r| p[0] >= r[0] - 1.0 && p[0] <= r[2] + 1.0 && p[1] >= r[1] - 1.0 && p[1] <= r[3] + 1.0),
                "{p:?} outside every pill"
            );
        }
    }
    // The other tool in use draws differently, so the choice is visible.
    let mut other = Ui::default();
    tb.draw(&State { tool: Tool::Laser, ..state }, &mut other);
    assert_ne!(ui.lines, other.lines);
    assert_eq!(ui.panels.len(), other.panels.len(), "one wash either way");
}

#[test]
fn the_fullscreen_button_changes_its_icon() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let (mut windowed, mut full) = (Ui::default(), Ui::default());
    tb.draw(&State::default(), &mut windowed);
    tb.draw(&State { fullscreen: true, ..State::default() }, &mut full);
    assert_ne!(windowed.lines, full.lines, "corners out versus corners in");
    assert_eq!(windowed.lines.len(), full.lines.len(), "the same icon weight either way, give or take");
}

#[test]
fn a_hovered_button_gets_a_wash_unless_it_is_disabled() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let panels = |state: State| {
        let mut ui = Ui::default();
        tb.draw(&state, &mut ui);
        ui.panels.len()
    };
    let plain = panels(State { can_undo: true, ..State::default() });
    assert_eq!(panels(State { can_undo: true, hover: Some(Item::Undo), ..State::default() }), plain + 1);
    assert_eq!(
        panels(State { can_undo: false, hover: Some(Item::Undo), ..State::default() }),
        plain,
        "an undo with nothing to undo does not light up"
    );
}

#[test]
fn the_delete_button_is_left_of_the_add_button_and_both_end_the_left_pill_with_a_gap_before_them() {
    for (w, dpr) in WINDOWS {
        let tb = Toolbar::layout([w, 900.0], dpr);
        let (save, delete, plus) =
            (tb.button(Item::Save).expect("save"), tb.button(Item::DeletePage).expect("delete"), tb.button(Item::AddPage).expect("add"));
        assert!(delete[0] > save[2] + 4.0 * dpr, "a gap between save and delete at {w}");
        assert!((plus[0] - delete[2]).abs() < 1.0, "delete is right beside add, on its left, at {w}");
        for button in [delete, plus] {
            assert!(rect_contains(tb.pills[0], [(button[0] + button[2]) / 2.0, (button[1] + button[3]) / 2.0]), "in the left pill at {w}");
        }
        assert!(plus[2] < tb.pills[0][2] && tb.pills[0][2] < tb.pills[1][0], "and the pills do not touch at {w}");
    }
}

#[test]
fn the_delete_button_is_dim_and_does_not_light_up_when_there_is_only_one_page() {
    let panels = |state: State| {
        let mut ui = Ui::default();
        Toolbar::layout([1650.0, 1350.0], 1.5).draw(&state, &mut ui);
        ui.panels.len()
    };
    let one_page = panels(State { can_delete: false, ..State::default() });
    let many = panels(State { can_delete: true, ..State::default() });
    assert_eq!(many, one_page, "no wash either way until the mouse is over it");
    assert_eq!(panels(State { can_delete: true, hover: Some(Item::DeletePage), ..State::default() }), many + 1, "it lights up under the mouse");
    assert_eq!(panels(State { can_delete: false, hover: Some(Item::DeletePage), ..State::default() }), one_page, "but not with nothing to delete");
    let lines = |state: State| {
        let mut ui = Ui::default();
        Toolbar::layout([1650.0, 1350.0], 1.5).draw(&state, &mut ui);
        ui.lines.iter().map(|l| format!("{l:?}")).collect::<Vec<_>>()
    };
    assert_ne!(lines(State { can_delete: true, ..State::default() }), lines(State { can_delete: false, ..State::default() }), "drawn dimmer");
}
