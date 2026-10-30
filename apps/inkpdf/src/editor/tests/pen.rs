use super::*;

#[test]
fn the_pen_draws_a_stroke_that_can_be_undone() {
    let mut ed = editor();
    let pts: Vec<[f32; 2]> = (0..20).map(|i| on_page(&ed, 100.0 + i as f32 * 8.0, 200.0 + (i as f32 * 0.5).sin() * 20.0)).collect();
    pen_stroke(&mut ed, TIP, &pts);
    assert_eq!(ed.ink.len(), 1);
    let s = &ed.ink.strokes()[0];
    assert_eq!(s.page, 0);
    assert!(s.pts.len() >= 3);
    let first = ed.view.doc_to_screen(s.pts[0]);
    assert!((first[0] - pts[0][0]).abs() < 0.5 && (first[1] - pts[0][1]).abs() < 0.5);
    assert!(ed.ink.is_dirty());
    assert!(ed.is_idle(), "the gesture is over");
    ed.undo();
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn a_tap_with_the_pen_makes_a_dot() {
    let mut ed = editor();
    let p = on_page(&ed, 300.0, 300.0);
    pen_stroke(&mut ed, TIP, &[p, p]);
    assert_eq!(ed.ink.len(), 1);
    assert_eq!(ed.ink.strokes()[0].pts.len(), 1);
}

/// A slow (250 px/s) horizontal line at page y = 300 with a pixel and a half of tremor in each
/// sample: a careful hand, where tremor shows most.
fn shaky_line(ed: &Editor) -> Vec<[f32; 2]> {
    let mut seed = 0x9E37_79B9_u32;
    (0..160)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let tremor = (seed % 3001) as f32 / 1000.0 - 1.5;
            on_page(ed, 100.0 + i as f32, 300.0 + tremor)
        })
        .collect()
}

/// How far the middle of a stroke strays from the page line y = 300, as a root-mean-square in
/// points. (The very ends are left out: the stroke starts on the raw first sample and ends
/// exactly where the pen did, so they carry the tremor on purpose.)
fn worst_wobble(ed: &Editor) -> f32 {
    let pts = &ed.ink.strokes()[0].pts;
    let middle = &pts[pts.len() / 6..pts.len() - pts.len() / 6];
    (middle.iter().map(|p| (p[1] - (12.0 + 300.0)).powi(2)).sum::<f32>() / middle.len() as f32).sqrt()
}

#[test]
fn a_shaky_hand_comes_out_steadier() {
    let mut steady = editor();
    let mut off = editor();
    off.styles.smoothing = 0.0;
    let path = shaky_line(&steady);
    pen_stroke(&mut steady, TIP, &path);
    pen_stroke(&mut off, TIP, &path);
    let (with, without) = (worst_wobble(&steady), worst_wobble(&off));
    assert!(without > 0.7, "the test input really is shaky: {without}");
    assert!(with < without * 0.5, "smoothing cut the wobble from {without:.2} to {with:.2}");
}

#[test]
fn a_fast_stroke_still_ends_exactly_where_the_pen_did() {
    let mut ed = editor();
    // 12 px every 4 ms: 3000 px/s, the sort of flick a filter would drag behind. It stays
    // on the page, since ink cannot leave it.
    let path: Vec<[f32; 2]> = (0..40).map(|i| on_page(&ed, 100.0 + i as f32 * 12.0, 400.0 + i as f32 * 2.0)).collect();
    pen_stroke(&mut ed, TIP, &path);
    let stroke = &ed.ink.strokes()[0];
    let (first, last) = (stroke.pts[0], *stroke.pts.last().expect("points"));
    let (want_first, want_last) = (ed.view.screen_to_doc(path[0]), ed.view.screen_to_doc(path[path.len() - 1]));
    assert!((first[0] - want_first[0]).abs() < 0.01 && (first[1] - want_first[1]).abs() < 0.01, "start {first:?} vs {want_first:?}");
    assert!((last[0] - want_last[0]).abs() < 0.01 && (last[1] - want_last[1]).abs() < 0.01, "end {last:?} vs {want_last:?}");
}

#[test]
fn smoothing_does_not_round_a_deliberate_corner() {
    let mut ed = editor();
    // Down 150 px, then straight back up: a V whose point must survive.
    let mut path: Vec<[f32; 2]> = (0..=30).map(|i| on_page(&ed, 200.0 + i as f32 * 3.0, 200.0 + i as f32 * 5.0)).collect();
    path.extend((1..=30).map(|i| on_page(&ed, 290.0 + i as f32 * 3.0, 350.0 - i as f32 * 5.0)));
    pen_stroke(&mut ed, TIP, &path);
    let deepest = ed.ink.strokes()[0].pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max) - 12.0;
    // The raw point of the V is at page y = 350; the steadied path may trail it a little.
    assert!(deepest > 340.0, "the point of the V was cut off at {deepest}");
}

#[test]
fn a_stroke_that_crosses_into_the_next_page_becomes_two_strokes() {
    let mut ed = editor();
    // Straight down from near the bottom of page 0, across the gap, onto page 1.
    let pts: Vec<[f32; 2]> = (0..30).map(|i| [600.0, 770.0 + i as f32 * 4.0]).collect();
    pen_stroke(&mut ed, TIP, &pts);
    let pages: Vec<u32> = ed.ink.strokes().iter().map(|s| s.page).collect();
    assert_eq!(pages, vec![0, 1], "one stroke on each page, none in the gap");
}

#[test]
fn the_pen_does_not_draw_off_the_page() {
    let mut ed = editor();
    // Left of the page, in the grey margin.
    pen_stroke(&mut ed, TIP, &[[50.0, 300.0], [60.0, 320.0], [70.0, 340.0]]);
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn the_eraser_end_removes_what_it_touches_as_one_step() {
    let mut ed = editor();
    // Below the toolbar, which a pen pressing on it would use instead.
    add_stroke(&mut ed, &[[100.0, 200.0], [300.0, 200.0]]);
    add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
    let keep = add_stroke(&mut ed, &[[100.0, 600.0], [300.0, 600.0]]);
    let down = on_page(&ed, 200.0, 130.0);
    ed.pointer_event(pen(Phase::Down, ERASER, &[down]));
    let sweep: Vec<[f32; 2]> = (0..30).map(|i| on_page(&ed, 200.0, 130.0 + i as f32 * 8.0)).collect();
    ed.pointer_event(pen(Phase::Move, ERASER, &sweep));
    assert_eq!(ed.erasing.len(), 2, "touched strokes are hidden while the eraser is down");
    assert_eq!(ed.ink.len(), 3, "but not removed until it lifts");
    ed.pointer_event(pen(Phase::Up, ERASER, &[on_page(&ed, 200.0, 370.0)]));
    assert!(ed.erasing.is_empty());
    assert_eq!(ed.ink.strokes().iter().map(|s| s.id).collect::<Vec<_>>(), vec![keep]);
    ed.undo();
    assert_eq!(ed.ink.len(), 3, "one undo brings back everything the eraser removed");
}

#[test]
fn selecting_everything_while_the_eraser_is_down_does_not_keep_what_it_then_erases() {
    let mut ed = editor();
    let doomed = add_stroke(&mut ed, &[[100.0, 200.0], [300.0, 200.0]]);
    let keep = add_stroke(&mut ed, &[[100.0, 600.0], [300.0, 600.0]]);
    ed.pointer_event(pen(Phase::Down, ERASER, &[on_page(&ed, 200.0, 130.0)]));
    let sweep: Vec<[f32; 2]> = (0..12).map(|i| on_page(&ed, 200.0, 130.0 + i as f32 * 8.0)).collect();
    ed.pointer_event(pen(Phase::Move, ERASER, &sweep));
    assert!(ed.erasing.contains(&doomed));
    // Everything selected, in the middle of the sweep.
    ed.select_all_on_page();
    ed.pointer_event(pen(Phase::Up, ERASER, &[on_page(&ed, 200.0, 230.0)]));
    assert!(ed.ink.get(doomed).is_none());
    assert_eq!(ed.selection, vec![keep], "only what is still there stays selected");
}

#[test]
fn the_eraser_never_draws() {
    let mut ed = editor();
    let path = [on_page(&ed, 100.0, 100.0), on_page(&ed, 200.0, 120.0), on_page(&ed, 300.0, 100.0)];
    pen_stroke(&mut ed, ERASER, &path);
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn the_barrel_button_draws_a_lasso_that_selects() {
    let mut ed = editor();
    let inside = add_stroke(&mut ed, &[[150.0, 150.0], [200.0, 170.0], [250.0, 150.0]]);
    let _outside = add_stroke(&mut ed, &[[450.0, 500.0], [500.0, 520.0]]);
    let ring: Vec<[f32; 2]> =
        [[100.0, 250.0], [100.0, 100.0], [300.0, 100.0], [300.0, 250.0], [100.0, 250.0]].iter().map(|p| on_page(&ed, p[0], p[1])).collect();
    pen_stroke(&mut ed, BARREL, &ring);
    assert_eq!(ed.selection, vec![inside]);
    assert_eq!(ed.ink.len(), 2, "a lasso adds no ink");
}

#[test]
fn the_pen_drags_a_selection_and_a_touch_elsewhere_only_lets_go() {
    let mut ed = editor();
    let id = add_stroke(&mut ed, &[[150.0, 150.0], [250.0, 150.0]]);
    ed.selection = vec![id];
    let before = ed.ink.strokes()[0].pts[0];
    // Press on the selection and drag it 60 right, 40 down.
    let (from, to) = (on_page(&ed, 200.0, 150.0), on_page(&ed, 260.0, 190.0));
    ed.pointer_event(pen(Phase::Down, TIP, &[from]));
    ed.pointer_event(pen(Phase::Move, TIP, &[on_page(&ed, 230.0, 170.0), to]));
    ed.pointer_event(pen(Phase::Up, TIP, &[to]));
    assert_eq!(ed.ink.len(), 1, "dragging a selection draws nothing");
    let after = ed.ink.strokes()[0].pts[0];
    assert!((after[0] - before[0] - 60.0).abs() < 0.5 && (after[1] - before[1] - 40.0).abs() < 0.5, "{before:?} -> {after:?}");
    // Pressing away from the selection lets go of it and does not start a stroke.
    let path = [on_page(&ed, 500.0, 500.0), on_page(&ed, 520.0, 520.0), on_page(&ed, 540.0, 500.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert!(ed.selection.is_empty());
    assert_eq!(ed.ink.len(), 1, "that press only dismissed the selection");
}

#[test]
fn the_pen_presses_toolbar_buttons_instead_of_drawing() {
    let mut ed = editor();
    let bar = ed.toolbar();
    // The pen picks the colour in its panel and the panel goes away with a press on the page.
    open_settings(&mut ed);
    let red = ed.toolbar().tool_panel(Tool::Pen, ed.view.viewport[0], false).expect("panel").swatch_centre(2);
    pen_stroke(&mut ed, TIP, &[red, red]);
    assert_eq!(ed.styles.pen.colour, 2);
    assert_eq!(ed.ink.len(), 0);
    // New strokes use the chosen colour.
    let path = [on_page(&ed, 10.0, 300.0), on_page(&ed, 40.0, 320.0), on_page(&ed, 70.0, 300.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.ink.len(), 0, "the first press on the page only put the panel away");
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.ink.strokes()[0].style.rgba, PALETTE[2]);
    // Pressing a button but lifting somewhere else does not press it.
    let laser = bar.centre_of(Item::Tool(LASER));
    ed.pointer_event(pen(Phase::Down, TIP, &[laser]));
    ed.pointer_event(pen(Phase::Up, TIP, &[on_page(&ed, 400.0, 400.0)]));
    assert_eq!(ed.tool, Tool::Pen);
}

#[test]
fn the_stroke_being_drawn_looks_exactly_as_if_all_of_it_were_smoothed_afresh_every_frame() {
    use crate::input::predict;
    use crate::smooth::curve_through;
    use crate::tools::Tool as _;
    use crate::tools::paint::{CURVE_STEP, tests::Recorder};
    let mut ed = editor();
    let at = |ed: &mut Editor, phase, pts: &[[f32; 2]]| ed.pointer_event(pen(phase, TIP, pts));
    let wave = |i: usize| on_page(&editor(), 20.0 + i as f32 * 2.5, 300.0 + (i as f32 * 0.35).sin() * 60.0 + (i % 7) as f32);
    at(&mut ed, Phase::Down, &[wave(0)]);
    for i in 1..160 {
        at(&mut ed, Phase::Move, &[wave(i)]);
        let (pts, recent) = ed.tools.pen.live().expect("drawing");
        let mut line = pts.to_vec();
        if let Some(ahead) = predict(recent, 12.0) {
            line.push(ed.view.screen_to_doc(ahead));
        }
        let want = curve_through(&line, CURVE_STEP);
        let mut out = Recorder::default();
        ed.tools.pen.paint(&ed, &mut out);
        let mut got: Vec<P> = out.doc.iter().flat_map(|(a, b, ..)| [*a, *b]).collect();
        got.dedup();
        let mut want = want;
        want.dedup();
        assert_eq!(got, want, "after {i} samples");
    }
}

#[test]
fn erasing_empty_page_leaves_the_document_unchanged() {
    for tool in [Tool::Pen, Tool::Geometry] {
        let mut ed = editor();
        ed.tool = tool;
        let path = [on_page(&ed, 100.0, 100.0), on_page(&ed, 300.0, 140.0), on_page(&ed, 500.0, 200.0)];
        pen_stroke(&mut ed, ERASER, &path);
        assert!(!ed.ink.is_dirty(), "{tool:?}: nothing was there to erase");
        assert!(!ed.ink.can_undo(), "{tool:?}: and there is nothing to undo");
    }
}
