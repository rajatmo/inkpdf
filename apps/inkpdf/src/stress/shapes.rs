//! Shapes drawn by the pen, and settings text of any kind.

use super::*;

#[test]
fn stress_shapes_drawn_by_the_pen_stay_on_their_page_undo_redo_exactly_and_survive_save_and_reopen() {
    let dir = temp_dir("shapes");
    let path = dir.join("shapes.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::reference([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    ed.tool = Tool::Geometry;
    let mut rng = Rng::new(404);
    let (mut made, mut edits) = (0usize, 0usize);
    for round in 0..(4_000 * scale()) {
        // (Nor is it about the slides, which a press on the button for them begins.)
        ed.stop_presentation();
        let size = ed.view.viewport;
        // (A random press can land on a toolbar button; the storm is about the shape tool. Back to it the way the
        // toolbar changes tools, which ends what the other was doing: the mode button can make the stylus place text.)
        if ed.tool != Tool::Geometry {
            use_tool(&mut ed, Tool::Geometry);
        }
        // (Nor is the bar under the tools to leave the tip choosing to select or erase.)
        ed.styles.shape = rng.pick(&crate::geometry::SHAPES);
        ed.styles.shape_style = crate::tool::ToolStyle { colour: rng.below(5), width: rng.range(MIN_WIDTH, MAX_WIDTH) };
        // Anywhere in the window, off the page, or far beyond it.
        let spot = |rng: &mut Rng| {
            if rng.chance(0.1) { [rng.coord(size[0]), rng.coord(size[1])] } else { [rng.range(0.0, size[0]), rng.range(0.0, size[1])] }
        };
        let (a, b) = (spot(&mut rng), spot(&mut rng));
        let before = ed.ink.len();
        pen_drag(&mut ed, a, b, 1 + rng.below(12));
        made += ed.ink.len().saturating_sub(before);
        // Edit what was just drawn by its anchors: drag them about, double tap them, add and delete.
        for _ in 0..rng.below(5) {
            if !anchor_poke(&mut ed, &mut rng).is_empty() {
                edits += 1;
            }
        }
        // Now and then: undo, redo, or an eraser sweep through everything.
        match rng.below(12) {
            0 => ed.undo(),
            1 => ed.redo(),
            2 => {
                let from = spot(&mut rng);
                ed.pointer_event(PointerEvent {
                    device: Device::Pen,
                    id: 1,
                    phase: Phase::Down,
                    samples: vec![Sample { pos: from, pressure: 1.0, t_ms: 1000.0 }],
                    pen: PenButtons { in_range: true, contact: true, eraser: true, barrel: false },
                    contact_px: 0.0,
                });
                ed.pointer_event(PointerEvent {
                    device: Device::Pen,
                    id: 1,
                    phase: Phase::Up,
                    samples: vec![Sample { pos: spot(&mut rng), pressure: 1.0, t_ms: 1010.0 }],
                    pen: PenButtons { in_range: true, contact: false, eraser: true, barrel: false },
                    contact_px: 0.0,
                });
            }
            3 => {
                let dy = rng.range(-300.0, 300.0);
                ed.pan_px(0.0, dy);
            }
            _ => {}
        }
        // (A random press can also add or delete a page, or undo one: the document follows, as the shell makes it.)
        follow_pages(&mut ed, &mut doc);
        if round % 100 == 0 {
            check_editor(&ed, &format!("round {round}"));
            for s in ed.ink.strokes() {
                assert!(s.pts.len() >= 2 && s.pts.len() <= crate::figure::MAX_ANCHORS * 50, "round {round}: a shape of {} points", s.pts.len());
                assert!(s.figure.is_some(), "round {round}: only shapes were drawn");
            }
        }
    }
    eprintln!("shapes made: {made} ({edits} anchor edits); alive at the end: {}", ed.ink.len());
    // (A press on the outline of the shape that is selected grabs it instead, and many of these are huge.)
    assert!(made > 1000 * scale(), "the storm should have made plenty of shapes ({made})");
    assert!(edits > 1000, "and edited plenty of anchors ({edits})");

    // Save, reopen: every shape comes back with the points it had.
    doc.save(&ed.ink).expect("save");
    let (_, back) = Doc::open(&path).expect("reopen");
    assert_eq!(back.len(), ed.ink.len(), "every shape survives a save and reopen");
    // (The file keeps the ink page by page, each page's strokes in the order they were drawn.)
    let mut by_page: Vec<_> = ed.ink.strokes().iter().collect();
    by_page.sort_by_key(|s| s.page);
    for (stroke, (page, style, pts, figure, _, _, _)) in by_page.into_iter().zip(&back) {
        assert_eq!((stroke.page, stroke.style.width.round()), (*page, style.width.round()));
        assert_eq!(stroke.pts.len(), pts.len(), "a shape keeps all its points");
        // And its anchors: it can still be edited after reopening.
        let figure = figure.as_ref().expect("the shape came back as a shape");
        let before = stroke.figure.as_ref().expect("a shape");
        assert_eq!(before.anchor_count(), figure.anchor_count());
        assert!(
            (0..before.anchor_count()).all(|i| {
                let (a, b) = (before.anchor_pos(i).expect("anchor"), figure.anchor_pos(i).expect("anchor"));
                (a[0] - b[0]).abs() < 0.02 && (a[1] - b[1]).abs() < 0.02
            }),
            "the anchors moved"
        );
        assert!(stroke.pts.iter().zip(pts).all(|(a, b)| (a[0] - b[0]).abs() < 0.02 && (a[1] - b[1]).abs() < 0.02), "the points moved");
    }

    // Undo everything the history still holds, then redo it all: the same ink.
    let end: Vec<_> = ed.ink.strokes().iter().map(|s| (s.id, s.pts.len())).collect();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        undone += 1;
    }
    for _ in 0..undone {
        ed.redo();
    }
    let again: Vec<_> = ed.ink.strokes().iter().map(|s| (s.id, s.pts.len())).collect();
    assert_eq!(again, end, "redoing everything undone restores the ink exactly");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_settings_text_of_any_kind_never_stops_the_app_and_real_settings_come_back_exactly() {
    let mut rng = Rng::new(808);
    let words = [
        "tool",
        "pen",
        "geometry",
        "laser",
        "shape",
        "shape-slot",
        "shape-width",
        "text-slot",
        "text-size",
        "text",
        "slot",
        "width",
        "smoothing",
        "color",
        "0",
        "1",
        "4",
        "5",
        "-1",
        "2.5",
        "NaN",
        "inf",
        "-inf",
        "1e40",
        "ff00ff",
        "#00ff00",
        "zz",
        "é",
        "line",
        "axes3d",
        "ellipse",
        "curve",
        "",
        "999999999999999999999",
    ];
    for _ in 0..(20_000 * scale()) {
        let lines = rng.below(10);
        let text: String =
            (0..lines).map(|_| (0..rng.below(5)).map(|_| rng.pick(&words)).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n");
        let s = crate::settings::Settings::parse(&text);
        for style in [s.pen, s.shape_style] {
            assert!(
                style.colour < s.palette.len() && style.width.is_finite() && (MIN_WIDTH..=MAX_WIDTH).contains(&style.width),
                "{text:?} gave {style:?}"
            );
        }
        let t = s.text_style;
        assert!(
            t.colour < s.palette.len() && t.width.is_finite() && (crate::tool::TEXT_SIZES.0..=crate::tool::TEXT_SIZES.1).contains(&t.width),
            "{text:?} gave {t:?}"
        );
        assert!((0.0..=1.0).contains(&s.smoothing), "{text:?}");
        // Whatever it read, writing it out and reading it back is stable.
        let again = crate::settings::Settings::parse(&s.to_text());
        assert_eq!(
            (again.tool, again.shape, again.pen.colour, again.shape_style.colour, again.palette),
            (s.tool, s.shape, s.pen.colour, s.shape_style.colour, s.palette),
            "{text:?}"
        );
    }
    // Settings made the way the app makes them come back exactly (widths to two decimals).
    for _ in 0..(2_000 * scale()) {
        let hundredths = |v: f32| (v * 100.0).round() / 100.0;
        let mut s = crate::settings::Settings {
            tool: rng.pick(&[Tool::Pen, Tool::Geometry, Tool::Text, Tool::Select, LASER]),
            shape: rng.pick(&crate::geometry::SHAPES),
            pen: crate::tool::ToolStyle { colour: rng.below(5), width: hundredths(rng.range(MIN_WIDTH, MAX_WIDTH)) },
            shape_style: crate::tool::ToolStyle { colour: rng.below(5), width: hundredths(rng.range(MIN_WIDTH, MAX_WIDTH)) },
            text_style: crate::tool::ToolStyle {
                colour: rng.below(5),
                width: hundredths(rng.range(crate::tool::TEXT_SIZES.0, crate::tool::TEXT_SIZES.1)),
            },
            smoothing: hundredths(rng.unit()),
            ..crate::settings::Settings::default()
        };
        for c in &mut s.palette {
            *c = [rng.below(256) as u8, rng.below(256) as u8, rng.below(256) as u8, 255];
        }
        let back = crate::settings::Settings::parse(&s.to_text());
        assert_eq!(
            (back.tool, back.shape, back.pen.colour, back.shape_style.colour, back.text_style.colour, back.palette),
            (s.tool, s.shape, s.pen.colour, s.shape_style.colour, s.text_style.colour, s.palette)
        );
        assert!(
            (back.pen.width - s.pen.width).abs() < 0.006
                && (back.shape_style.width - s.shape_style.width).abs() < 0.006
                && (back.text_style.width - s.text_style.width).abs() < 0.006
                && (back.smoothing - s.smoothing).abs() < 0.006,
            "{s:?} vs {back:?}"
        );
    }
}
