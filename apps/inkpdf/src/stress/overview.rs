//! The pages overview under hostile input, and motion on against motion off.

use super::*;

#[test]
fn stress_the_pages_overview_under_hostile_input_keeps_the_pages_the_selection_and_the_file_consistent() {
    let dir = temp_dir("overview");
    let path = dir.join("outline.pdf");
    std::fs::write(&path, crate::testutil::pdf_with_outline()).expect("write");
    let (mut doc, contents) = Doc::open_contents(&path).expect("open");
    let mut ed = Editor::reference([1280.0, 800.0], 1.25, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    ed.outline = contents.outline;
    ed.links = contents.links;
    let mut rng = Rng::new(808);
    // (With motion on, and frames of any length in between: what moves must stay a number and within its ends.)
    ed.motions.set_prefs(crate::motion::Prefs::FULL);
    let (mut opened, mut edits, mut saves, mut carried) = (0, 0, 0, 0);
    // (More rounds than this find nothing new: a document of this size saves slower and slower, not differently.)
    for round in 0..(8_000 * scale().min(3)) {
        let what = format!("round {round}");
        ed.step_view(rng.pick(&[0.0, 0.004, 0.016, 0.016, 0.033, 0.5, 5.0, -1.0, f32::NAN, f32::INFINITY]));
        check_motion(&ed, &what);
        let was_open = ed.overview_open();
        // (A real document does not grow without end; a long storm would otherwise spend its time on thousands of pages.)
        if ed.layout.len() > 60 {
            let n = ed.layout.len();
            ed.edit_pages(PageEdit::Delete { pages: (30..n).collect(), grave: 0 });
            follow_pages(&mut ed, &mut doc);
        }
        let size = ed.view.viewport;
        let layout = ed.overview_layout();
        // Where a pointer might be: mostly in the card, sometimes anywhere, sometimes nowhere a pointer can be.
        let spot = |rng: &mut Rng| -> [f32; 2] {
            if rng.chance(0.8) {
                [rng.range(layout.card[0], layout.card[2]), rng.range(layout.card[1], layout.card[3])]
            } else {
                [rng.finite(size[0]), rng.finite(size[1])]
            }
        };
        match rng.below(26) {
            0 | 1 => {
                if !ed.overview_open() {
                    ed.open_overview();
                    opened += 1;
                }
            }
            2..=5 => {
                let at = spot(&mut rng);
                mouse_click(&mut ed, at);
            }
            6 | 7 => {
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                mouse_drag(&mut ed, a, b, 1 + rng.below(8));
                carried += 1;
            }
            8 | 9 => {
                let at = spot(&mut rng);
                tap_pen(&mut ed, at, 1000.0);
            }
            10 => {
                let path: Vec<[f32; 2]> = (0..2 + rng.below(6)).map(|_| spot(&mut rng)).collect();
                pen_path(&mut ed, PenButtons { in_range: true, contact: true, eraser: rng.chance(0.2), barrel: rng.chance(0.2) }, &path);
            }
            11 => {
                let (a, b) = (spot(&mut rng), spot(&mut rng));
                ed.palm = crate::input::PalmGuard::default();
                let touch = |phase, pos| PointerEvent {
                    device: Device::Touch,
                    id: 5,
                    phase,
                    samples: vec![Sample { pos, pressure: 1.0, t_ms: 1000.0 }],
                    pen: PenButtons::default(),
                    contact_px: 20.0,
                };
                ed.pointer_event(touch(Phase::Down, a));
                ed.pointer_event(touch(Phase::Move, b));
                ed.pointer_event(touch(Phase::Up, b));
            }
            12 => {
                let (x, y) = (rng.range(-3.0, 3.0), rng.range(-6.0, 6.0));
                ed.mouse_moved(spot(&mut rng));
                ed.wheel(Wheel::Lines(x, y));
            }
            13 => {
                let key = match rng.below(8) {
                    0 => Key::Escape,
                    1 => Key::Delete,
                    2 => Key::Home(false),
                    3 => Key::End(false),
                    4 => Key::Arrow(Dir::Down, false),
                    5 => Key::Arrow(Dir::Up, false),
                    6 => Key::Backspace,
                    _ => Key::Text("x"),
                };
                ed.press(key);
            }
            14 | 15 => {
                let letter = rng.pick(&["a", "d", "z", "y", "c", "x", "v"]);
                ed.shortcut(letter, rng.chance(0.2));
            }
            16 => {
                // A page is added, deleted, copied or moved by something other than the overview's own controls.
                let n = ed.layout.len();
                let edit = match rng.below(4) {
                    0 => PageEdit::Insert { at: rng.below(n + 1), size: [612.0, 792.0] },
                    1 => PageEdit::delete(rng.below(n)),
                    2 => PageEdit::Duplicate { pages: vec![rng.below(n), rng.below(n)] },
                    _ => PageEdit::Move { pages: vec![rng.below(n)], to: rng.below(n) },
                };
                if n < 40 || matches!(edit, PageEdit::Delete { .. }) {
                    edits += usize::from(ed.edit_pages(edit));
                }
            }
            17 | 18 => {
                if rng.chance(0.5) {
                    ed.undo()
                } else {
                    ed.redo()
                }
            }
            19 if ed.ink.len() < 300 => {
                // Ink on a page, as when the overview is not up.
                let page = rng.below(ed.layout.len());
                let r = ed.layout.page(page).expect("page");
                ed.ink.add(page as u32, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[r.x + 30.0, r.y + 40.0], [r.x + 90.0, r.y + 60.0]]);
            }
            20 => {
                // The window changes size or scale.
                ed.set_viewport([rng.range(300.0, 2400.0), rng.range(260.0, 1500.0)]);
                ed.set_dpr(rng.pick(&[1.0, 1.25, 1.5, 2.0, 3.0]));
            }
            21 if rng.chance(0.15) => {
                ed.settle();
                doc.save(&ed.ink).expect("save");
                ed.document_saved();
                saves += 1;
                let (back, strokes) = Doc::open(&path).expect("reopen");
                assert_eq!(back.page_count(), doc.page_count(), "{what}: the pages are all in the file");
                assert_eq!(strokes.len(), ed.ink.len(), "{what}: every stroke came back");
            }
            22 => {
                ed.pan_px(rng.range(-100.0, 100.0), rng.range(-400.0, 400.0));
            }
            _ => {
                if rng.chance(0.3) {
                    ed.close_overview();
                }
            }
        }
        follow_pages(&mut ed, &mut doc);
        // (Drawing it and walking every stroke is the dear part: every few rounds, and always when it has just been opened.)
        if round % 6 == 0 || ed.chrome.overview.is_none() == was_open {
            check_overview(&ed, &doc, &what);
        }
    }
    eprintln!("overview opened: {opened}; edits by hand: {edits}; drags and clicks: {carried}; saves: {saves}; pages: {}", ed.layout.len());
    assert!(opened > 100 && edits > 50 && saves > 20, "the storm should have used the overview");
    // Everything that was done can be undone and redone, with the document following.
    ed.close_overview();
    let mut undone = 0;
    while ed.ink.can_undo() {
        ed.undo();
        follow_pages(&mut ed, &mut doc);
        undone += 1;
    }
    for _ in 0..undone {
        ed.redo();
        follow_pages(&mut ed, &mut doc);
    }
    check_overview(&ed, &doc, "after undoing and redoing everything");
    doc.save(&ed.ink).expect("save at the end");
    assert_eq!(Doc::open(&path).expect("reopen").0.page_count(), doc.page_count());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_motion_changes_how_long_things_take_and_never_where_they_end_up() {
    for seed in [3, 77, 2024] {
        let rounds = 400 * scale().min(3);
        let still = run_with(crate::motion::Prefs::REDUCED, seed, rounds);
        let moving = run_with(crate::motion::Prefs::FULL, seed, rounds);
        // (It has to have done something: the overview was up and edited, pages changed, ink was drawn.)
        assert!(moving.iter().any(|o| o.overview.as_ref().is_some_and(|v| v.0)), "seed {seed}: edit mode was reached");
        assert!(moving.iter().any(|o| !o.strokes.is_empty()) && moving.windows(2).any(|w| w[0].pages != w[1].pages), "seed {seed}");
        for (round, (a, b)) in still.iter().zip(&moving).enumerate() {
            assert_eq!(a, b, "seed {seed}, round {round}: with motion on, things came to something else");
        }
    }
}
