use super::*;

/// Windows wide enough for the three pills whole (in pixels, at their scale): a desktop's and a tablet's.
const WINDOWS: [(f32, f32); 4] = [(900.0, 1.0), (1650.0, 1.5), (1800.0, 1.5), (3840.0, 2.0)];

/// Windows from a small phone held upright to a wide screen, narrow ones included, at the scales they come in.
const SWEEP: [(f32, f32); 12] = [
    (320.0, 1.0),
    (360.0, 1.0),
    (480.0, 1.0),
    (800.0, 1.0),
    (900.0, 1.0),
    (1000.0, 2.0),
    (1080.0, 2.625),
    (1200.0, 2.0),
    (1600.0, 2.0),
    (1650.0, 1.5),
    (2560.0, 2.0),
    (3840.0, 2.0),
];

/// The middle of a rectangle.
fn middle(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

#[test]
fn every_item_is_hit_at_its_centre_and_there_are_no_others() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let mut seen = Vec::new();
    for &(item, r) in tb.items() {
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
            Item::Pages,
            Item::Search,
            Item::Tool(Tool::Select),
            Item::Tool(Tool::Pen),
            Item::Tool(Tool::Geometry),
            Item::Tool(Tool::Text),
            Item::Tool(Tool::Lasso),
            Item::Tool(Tool::Eraser),
            Item::Tool(Tool::named("laser")),
            Item::Theme,
            Item::ModeSwitch,
            Item::Finger,
            Item::Present,
            Item::Fullscreen
        ]
    );
}

#[test]
fn the_main_toolbar_holds_only_the_tools() {
    let tb = Toolbar::layout([1650.0, 1350.0], 1.5);
    let in_middle: Vec<Item> =
        tb.items().iter().filter(|(_, r)| rect_contains(tb.pills[1], [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0])).map(|(i, _)| *i).collect();
    assert_eq!(
        in_middle,
        [
            Item::Tool(Tool::Select),
            Item::Tool(Tool::Pen),
            Item::Tool(Tool::Geometry),
            Item::Tool(Tool::Text),
            Item::Tool(Tool::Lasso),
            Item::Tool(Tool::Eraser),
            Item::Tool(Tool::named("laser"))
        ]
    );
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
    for &(_, r) in tb.items() {
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
    tb.draw(&State { tool: Tool::named("laser"), ..state }, &mut other);
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

#[test]
fn the_mode_button_and_the_finger_button_are_left_of_the_fullscreen_button_in_the_right_pill() {
    for (w, dpr) in WINDOWS {
        let tb = Toolbar::layout([w, 900.0], dpr);
        let (mode, finger, present, full) = (
            tb.button(Item::ModeSwitch).expect("mode"),
            tb.button(Item::Finger).expect("finger"),
            tb.button(Item::Present).expect("present"),
            tb.button(Item::Fullscreen).expect("fullscreen"),
        );
        assert!((finger[0] - mode[2]).abs() < 1.0, "the finger button is right beside the mode button, on its right, at {w}");
        assert!((present[0] - finger[2]).abs() < 1.0, "the button for the slides is right beside the finger button, at {w}");
        assert!((full[0] - present[2]).abs() < 1.0, "fullscreen is right beside the button for the slides, on its right, at {w}");
        for button in [mode, finger, present, full] {
            assert!(rect_contains(tb.pills[2], [(button[0] + button[2]) / 2.0, (button[1] + button[3]) / 2.0]), "in the right pill at {w}");
        }
        assert!(tb.pills[1][2] < tb.pills[2][0], "and the right pill does not touch the middle one at {w}");
        assert!(full[2] <= w + 0.5 && tb.pills[2][2] >= w - 13.0 * dpr, "the right pill still hugs the right edge at {w}");
    }
}

#[test]
fn the_mode_button_shows_which_mode_it_is_by_its_icon_and_the_tablet_in_the_accent_colour() {
    let lines = |mode: Mode| {
        let mut ui = Ui::default();
        Toolbar::layout([1650.0, 1350.0], 1.5).draw(&State { mode, ..State::default() }, &mut ui);
        ui.lines.iter().map(|l| format!("{l:?}")).collect::<Vec<_>>()
    };
    let (desk, slate) = (lines(Mode::Desktop), lines(Mode::Tablet));
    assert_ne!(desk, slate, "the two modes look different");
    // Nothing else on the toolbars changed: only the one button's lines differ.
    let differing = desk.iter().zip(&slate).filter(|(a, b)| a != b).count();
    assert!(differing > 0 && differing <= desk.len().max(slate.len()) / 4, "only the mode button is redrawn: {differing} of {}", desk.len());
    let accent = |lines: &[String]| lines.iter().filter(|l| l.contains(&format!("{:?}", crate::palette::LIGHT.accent))).count();
    assert!(accent(&slate) > accent(&desk), "the tablet is drawn in the accent colour");
}

#[test]
fn with_a_picker_the_left_pill_has_a_button_to_open_a_pdf_and_everything_still_fits() {
    let reg = crate::tool::reference_registry().with_open_button(true);
    // (Narrower than that the left pill folds into its round button, and has the button inside it when it is open: see the sweep below.)
    for (w, dpr) in [(1650.0, 1.5), (1100.0, 1.0), (900.0, 1.0)] {
        let tb = Toolbar::layout_for(&reg, [w, 900.0], dpr, None);
        let open = tb.button(Item::Open).expect("an open button");
        assert!(rect_contains(tb.pills[0], [(open[0] + open[2]) / 2.0, (open[1] + open[3]) / 2.0]), "in the left pill at {w}");
        assert!(tb.pills[0][2] < tb.pills[1][0] && tb.pills[1][2] < tb.pills[2][0], "three separate pills at {w}: {:?}", tb.pills);
        assert_eq!(tb.hit([(open[0] + open[2]) / 2.0, (open[1] + open[3]) / 2.0]), Some(Item::Open));
        // It is right beside the save button, on its left.
        let (redo, save) = (tb.button(Item::Redo).expect("redo"), tb.button(Item::Save).expect("save"));
        assert!(redo[2] <= open[0] + 0.5 && (open[2] - save[0]).abs() < 0.5, "{w}");
    }
    assert!(Toolbar::layout([1100.0, 900.0], 1.0).button(Item::Open).is_none(), "and without one there is none");
}

#[test]
fn a_window_too_narrow_for_three_pills_folds_them_into_round_buttons_the_left_one_first_and_the_tools_last() {
    let folded = |w: f32, dpr: f32| {
        let tb = Toolbar::layout([w, 900.0], dpr);
        Pill::ALL.map(|p| tb.folds(p))
    };
    assert_eq!(folded(1650.0, 1.5), [false; 3], "a tablet's width: all whole");
    assert_eq!(folded(800.0, 1.0), [true, false, false], "a little narrower: the document's pill folds first");
    assert_eq!(folded(1080.0, 2.625), [true, false, true], "a phone upright (411 points): the tools stay whole");
    assert_eq!(folded(320.0, 1.0), [true; 3], "a very small one: all three");
}

#[test]
fn every_pill_is_inside_the_window_none_overlap_and_every_button_can_be_pressed_at_every_width() {
    let reg = crate::tool::reference_registry().with_open_button(true);
    for (w, dpr) in SWEEP {
        for open in [None, Some(Pill::Document), Some(Pill::Tools), Some(Pill::View)] {
            let tb = Toolbar::layout_for(&reg, [w, 900.0], dpr, open);
            let shown: Vec<[f32; 4]> = tb.pills.iter().copied().filter(|p| p[2] > p[0]).collect();
            assert!(!shown.is_empty(), "something to press at {w}");
            for (i, pill) in shown.iter().enumerate() {
                assert!(pill[0] >= -0.5 && pill[2] <= w + 0.5, "{pill:?} off screen at {w} with {open:?}");
                for other in &shown[i + 1..] {
                    assert!(pill[2] <= other[0] + 0.5 || other[2] <= pill[0] + 0.5, "{pill:?} meets {other:?} at {w} with {open:?}");
                }
            }
            for &(item, r) in tb.items() {
                assert!(r[2] - r[0] >= 19.0 * dpr && r[3] - r[1] >= 19.0 * dpr, "{item:?} is too small to press at {w}");
                assert!(shown.iter().any(|p| rect_contains(*p, middle(r))), "{item:?} is on a pill at {w} with {open:?}");
                assert_eq!(tb.hit(middle(r)), Some(item), "{item:?} at {w} with {open:?}");
            }
            // With nothing open, the three are there, as three pills or as round buttons: the tools are always one of them.
            if open.is_none() {
                assert_eq!(shown.len(), 3, "all three at {w}");
                assert!(tb.items().iter().any(|(i, _)| matches!(i, Item::Tool(_) | Item::Fold(Pill::Tools))), "the tools at {w}");
            }
        }
    }
}

#[test]
fn a_folded_pill_is_a_round_button_and_the_button_of_the_pill_is_a_fold() {
    let tb = Toolbar::layout([1080.0, 2400.0], 2.625);
    for pill in [Pill::Document, Pill::View] {
        let round = tb.pills[pill.index()];
        assert!((round[2] - round[0] - (round[3] - round[1])).abs() < 0.5, "{pill:?} is round: {round:?}");
        assert_eq!(tb.hit(middle(round)), Some(Item::Fold(pill)));
        assert_eq!(tb.round_of(pill), round);
    }
    assert!(tb.button(Item::Close).is_none() && tb.button(Item::Fullscreen).is_none(), "what is in a folded pill is not there to press");
    assert!(tb.button(Item::Tool(Tool::Pen)).is_some(), "the tools are whole");
    assert_eq!(tb.open(), None);
}

#[test]
fn an_open_pill_is_whole_over_the_others_which_step_aside_where_it_reaches_them() {
    let reg = crate::tool::reference_registry().with_open_button(true);
    let tb = Toolbar::layout_for(&reg, [1080.0, 2400.0], 2.625, Some(Pill::Document));
    assert_eq!(tb.open(), Some(Pill::Document));
    let whole = tb.pills[0];
    assert!(whole[2] - whole[0] > 5.0 * (whole[3] - whole[1]), "the document's pill is whole: {whole:?}");
    for item in [Item::Close, Item::Undo, Item::Save, Item::Open, Item::Pages, Item::Search] {
        assert!(tb.button(item).is_some(), "{item:?} is there to press");
    }
    assert!(tb.button(Item::Fold(Pill::Document)).is_none(), "the round button is the pill now");
    // The tools' pill is under it (it is in the middle of the window): it is not there to press, and nothing hits it.
    assert_eq!(tb.pills[1], [0.0; 4]);
    assert!(tb.button(Item::Tool(Tool::Pen)).is_none());
    assert!(!matches!(tb.hit(middle(tb.round_of(Pill::Tools))), Some(Item::Tool(_))), "no tool under the open pill");
    // An open pill that does not fold (the window is wide) is no different from a closed one.
    let wide = Toolbar::layout_for(&reg, [1650.0, 900.0], 1.5, Some(Pill::View));
    assert_eq!(wide.open(), None);
    assert!(wide.button(Item::Fullscreen).is_some() && wide.button(Item::Close).is_some());
}

#[test]
fn a_pill_grows_from_its_round_button_as_it_unfolds_and_what_is_in_it_comes_out_of_the_button() {
    let reg = crate::tool::reference_registry();
    let state = State::default();
    let draw = |open: Option<Pill>, amount: f32| {
        let tb = Toolbar::layout_for(&reg, [1080.0, 2400.0], 2.625, open);
        let mut look = Look::resting(&state, &tb);
        look.unfold[Pill::Document.index()] = amount;
        let mut ui = Ui::default();
        tb.draw_with(&state, &look, &mut ui);
        (tb, ui)
    };
    let (_, closed) = draw(None, 0.0);
    let (tb, opening) = draw(Some(Pill::Document), 0.5);
    let (_, open) = draw(Some(Pill::Document), 1.0);
    assert_ne!(format!("{:?}", closed.panels), format!("{:?}", opening.panels));
    assert_ne!(format!("{:?}", opening.panels), format!("{:?}", open.panels), "half way is neither");
    assert!(opening.lines.iter().chain(&open.lines).all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
    // Half way, the buttons that are coming out are nearer the round button than they will be.
    let round = middle(tb.round_of(Pill::Document));
    let reach = |ui: &Ui| ui.lines.iter().filter(|c| c.a[0] < tb.pills[0][2]).map(|c| (c.a[0] - round[0]).abs()).fold(0.0, f32::max);
    assert!(reach(&opening) < reach(&open), "{} {}", reach(&opening), reach(&open));
}
