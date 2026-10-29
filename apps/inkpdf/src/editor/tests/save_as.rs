//! The Save As dialog through the editor, with every device: whatever the mode guarantees (the left button and the keyboard
//! at a desk, the stylus's tip on a slate) is enough to choose a folder, name the file, save it and put the dialog away.

use std::path::{Path, PathBuf};

use super::*;
use crate::browse::{Entry, MAX_ENTRIES, Place};
use crate::store::DocId;
use crate::stress::Rng;
use crate::toolbar::{SaveAsHit, SaveAsLayout, Ui};
use crate::tools::{Dir, Key};

const DOCUMENTS: &str = "/home/me/Documents";

fn places() -> Vec<Place> {
    vec![Place { label: "Documents".into(), path: DOCUMENTS.into() }, Place { label: "Home".into(), path: "/home/me".into() }]
}

fn entries() -> Vec<Entry> {
    vec![
        Entry { name: "Invoices".into(), folder: true },
        Entry { name: "Taxes".into(), folder: true },
        Entry { name: "old.pdf".into(), folder: false },
    ]
}

/// The dialog up in Documents, the folder read, and what was asked of the shell taken away.
fn asking(mode: Mode) -> Editor {
    let mut ed = editor();
    ed.set_mode(mode);
    ed.ask_save_as(DOCUMENTS.into(), "Untitled.pdf", places());
    assert!(ed.requests.take(&Request::ListFolder(DOCUMENTS.into())), "it asks for the folder it opens on");
    ed.folder_listed(Path::new(DOCUMENTS), Ok(entries()));
    ed
}

fn layout(ed: &Editor) -> SaveAsLayout {
    SaveAsLayout::new(ed.view.viewport, ed.dpr, places().len())
}

fn middle(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

/// Where a part of the dialog is, as a point to press.
fn at(ed: &Editor, hit: SaveAsHit) -> [f32; 2] {
    let l = layout(ed);
    middle(match hit {
        SaveAsHit::Save => l.save,
        SaveAsHit::Cancel => l.cancel,
        SaveAsHit::Up => l.up,
        SaveAsHit::Name => l.name,
        SaveAsHit::Place(i) => l.chips.iter().find(|(n, _)| *n == i).expect("place").1,
        SaveAsHit::Row(i) => l.row_rect(i, 0.0),
        SaveAsHit::List => l.list,
    })
}

#[derive(Clone, Copy, Debug)]
enum By {
    Mouse,
    Stylus,
    Finger,
}

/// Press and let go at `pos` with `by`.
fn tap_at(ed: &mut Editor, by: By, pos: [f32; 2]) {
    match by {
        By::Mouse => mouse_click_at(ed, pos),
        By::Stylus => {
            ed.pointer_event(pen(Phase::Down, TIP, &[pos]));
            ed.pointer_event(pen(Phase::Up, TIP, &[pos]));
        }
        By::Finger => {
            ed.pointer_event(touch(1, Phase::Down, pos, 30.0));
            ed.pointer_event(touch(1, Phase::Up, pos, 30.0));
        }
    }
}

fn tap(ed: &mut Editor, by: By, hit: SaveAsHit) {
    let pos = at(ed, hit);
    tap_at(ed, by, pos);
}

/// The ways a person has, each with the mode that guarantees it.
const WAYS: [(By, Mode); 4] = [(By::Mouse, Mode::Desktop), (By::Stylus, Mode::Desktop), (By::Stylus, Mode::Tablet), (By::Finger, Mode::Tablet)];

#[test]
fn it_opens_on_the_folder_asks_for_it_takes_the_window_and_wants_the_keyboard() {
    let mut ed = editor();
    assert!(!ed.save_as_open() && !ed.is_typing());
    ed.ask_save_as(DOCUMENTS.into(), "Untitled.pdf", places());
    assert!(ed.save_as_open());
    assert!(ed.requests.has(&Request::ListFolder(DOCUMENTS.into())));
    assert!(ed.is_typing(), "the shell hands it the keys that are typed");
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    assert!(
        ui.labels.iter().any(|l| l.text == "Save as") && ui.labels.iter().any(|l| l.text == "Untitled.pdf"),
        "it is drawn, with the name offered"
    );
    assert!(ui.labels.iter().any(|l| l.text == "Documents"), "and the places");
    let mut nothing = Ui::default();
    editor().draw_save_as(&mut nothing);
    assert!(nothing.panels.is_empty() && nothing.labels.is_empty(), "nothing is drawn while it is not up");
}

#[test]
fn the_name_offered_can_be_saved_as_it_is_with_every_device_so_a_slate_needs_only_the_tip() {
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        tap(&mut ed, by, SaveAsHit::Save);
        assert!(ed.requests.take(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("Untitled.pdf")))), "{by:?} in {mode:?}");
        assert!(ed.save_as_open(), "it stays until the shell says how the write went");
    }
}

#[test]
fn the_keyboard_alone_names_the_file_and_enter_saves_it() {
    let mut ed = asking(Mode::Desktop);
    assert!(ed.press(Key::Text("My report")), "the first thing typed replaces the stem that was offered");
    assert!(ed.press(Key::Text("\n")));
    assert!(ed.requests.take(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("My report.pdf")))));
    // A wrong name is said, and nothing is asked.
    let mut ed = asking(Mode::Desktop);
    ed.press(Key::Text("a/b"));
    ed.press(Key::Text("\n"));
    assert!(!ed.requests.has(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("a/b.pdf")))));
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    assert!(ui.labels.iter().any(|l| l.text.contains('/') && l.text.contains("cannot")), "the reason is on the card");
}

#[test]
fn a_tap_on_a_folder_goes_into_it_and_up_comes_back_with_every_device() {
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        tap(&mut ed, by, SaveAsHit::Row(1));
        assert!(ed.requests.take(&Request::ListFolder(PathBuf::from(DOCUMENTS).join("Taxes"))), "{by:?} in {mode:?}");
        tap(&mut ed, by, SaveAsHit::Up);
        assert!(ed.requests.take(&Request::ListFolder(DOCUMENTS.into())), "up: {by:?} in {mode:?}");
        tap(&mut ed, by, SaveAsHit::Place(1));
        assert!(ed.requests.take(&Request::ListFolder("/home/me".into())), "a place: {by:?} in {mode:?}");
    }
}

#[test]
fn a_tap_on_a_pdf_takes_its_name_and_saving_over_it_is_asked_twice() {
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        tap(&mut ed, by, SaveAsHit::Row(2));
        tap(&mut ed, by, SaveAsHit::Save);
        assert!(!ed.requests.has(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("old.pdf")))), "not at once: {by:?}");
        let mut ui = Ui::default();
        ed.draw_save_as(&mut ui);
        assert!(ui.labels.iter().any(|l| l.text == "Replace"), "the button says what it will do");
        tap(&mut ed, by, SaveAsHit::Save);
        assert!(ed.requests.take(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("old.pdf")))), "the second press: {by:?}");
    }
}

#[test]
fn cancel_and_escape_put_it_away_and_say_so() {
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        tap(&mut ed, by, SaveAsHit::Cancel);
        assert!(!ed.save_as_open() && ed.requests.take(&Request::SaveAsCancelled), "{by:?} in {mode:?}");
    }
    let mut ed = asking(Mode::Desktop);
    ed.press(Key::Escape);
    assert!(!ed.save_as_open() && ed.requests.take(&Request::SaveAsCancelled));
    // Put away by the shell (the window is closing): it is told as well.
    let mut ed = asking(Mode::Desktop);
    ed.cancel_save_as();
    assert!(ed.requests.take(&Request::SaveAsCancelled));
    ed.cancel_save_as();
    assert!(!ed.requests.has(&Request::SaveAsCancelled), "only once");
}

#[test]
fn a_press_on_the_card_that_is_not_on_a_button_and_a_press_outside_it_do_nothing() {
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        tap_at(&mut ed, by, [5.0, 5.0]);
        let l = layout(&ed);
        tap_at(&mut ed, by, [l.card[0] + 1.0, l.card[1] + 1.0]);
        tap(&mut ed, by, SaveAsHit::Name);
        assert!(ed.save_as_open(), "{by:?}");
        ed.requests.take(&Request::ListFolder(DOCUMENTS.into()));
        assert!(!ed.requests.has(&Request::SaveAsCancelled));
    }
}

#[test]
fn while_it_is_up_nothing_reaches_the_page() {
    let mut ed = asking(Mode::Desktop);
    let page = on_page(&ed, 100.0, 300.0);
    let before = ed.view.scroll;
    // The mouse, as a stylus, draws nothing; the pen draws nothing; a finger does not pan; the wheel does not scroll the page.
    mouse_click_at(&mut ed, page);
    pen_stroke(&mut ed, TIP, &[page, [page[0] + 40.0, page[1] + 10.0], [page[0] + 80.0, page[1]]]);
    ed.pointer_event(touch(1, Phase::Down, page, 30.0));
    ed.pointer_event(touch(1, Phase::Move, [page[0], page[1] + 80.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [page[0], page[1] + 80.0], 30.0));
    ed.wheel(Wheel::Lines(0.0, -3.0));
    assert_eq!((ed.ink.len(), ed.view.scroll), (0, before), "the document is as it was");
    // The shortcuts of the page do not fire, and the keys go to the name.
    assert!(!ed.shortcut("s", false) && !ed.shortcut("z", false) && !ed.shortcut("a", false));
    assert!(!ed.requests.has(&Request::Save));
}

#[test]
fn a_drag_of_any_device_and_the_wheel_scroll_the_list() {
    let many: Vec<Entry> = (0..60).map(|i| Entry { name: format!("folder {i:02}"), folder: true }).collect();
    for (by, mode) in WAYS {
        let mut ed = asking(mode);
        ed.folder_listed(Path::new(DOCUMENTS), Ok(many.clone()));
        let l = layout(&ed);
        let (from, to) = (middle(l.row_rect(3, 0.0)), middle(l.row_rect(1, 0.0)));
        match by {
            By::Mouse => {
                ed.mouse_moved(from);
                ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
                ed.mouse_moved(to);
                ed.mouse_button(ButtonState::Released, MouseButton::Left);
            }
            By::Stylus => {
                ed.pointer_event(pen(Phase::Down, TIP, &[from]));
                ed.pointer_event(pen(Phase::Move, TIP, &[to]));
                ed.pointer_event(pen(Phase::Up, TIP, &[to]));
            }
            By::Finger => {
                ed.pointer_event(touch(1, Phase::Down, from, 30.0));
                ed.pointer_event(touch(1, Phase::Move, to, 30.0));
                ed.pointer_event(touch(1, Phase::Up, to, 30.0));
            }
        }
        // Dragged up by two rows, so the list moved: and the drag was not a tap on a folder.
        assert!(!ed.requests.has(&Request::ListFolder(PathBuf::from(DOCUMENTS).join("folder 01"))), "{by:?} in {mode:?}");
        let mut ui = Ui::default();
        ed.draw_save_as(&mut ui);
        assert!(!ui.labels.iter().any(|t| t.text == "folder 00" && t.alpha > 0.99), "the first row has gone: {by:?}");
    }
    let mut ed = asking(Mode::Desktop);
    ed.folder_listed(Path::new(DOCUMENTS), Ok(many));
    ed.wheel(Wheel::Lines(0.0, -2.0));
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    assert!(!ui.labels.iter().any(|t| t.text == "folder 00" && t.alpha > 0.99), "the wheel scrolled the list");
}

#[test]
fn the_answers_of_the_shell_are_taken_and_a_failed_write_leaves_the_dialog_for_another_try() {
    let mut ed = asking(Mode::Desktop);
    // The answer for a folder that was left is ignored.
    ed.folder_listed(Path::new("/elsewhere"), Ok(vec![Entry { name: "x".into(), folder: true }]));
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    assert!(!ui.labels.iter().any(|l| l.text == "x"));
    tap(&mut ed, By::Mouse, SaveAsHit::Save);
    ed.requests.take(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("Untitled.pdf"))));
    ed.save_as_done(Err("Could not save: disk full".into()));
    assert!(ed.save_as_open(), "still there");
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    assert!(ui.labels.iter().any(|l| l.text.contains("disk full")), "with the reason");
    tap(&mut ed, By::Mouse, SaveAsHit::Save);
    assert!(ed.requests.take(&Request::SaveTo(DocId::Path(PathBuf::from(DOCUMENTS).join("Untitled.pdf")))), "and can be tried again");
    ed.save_as_done(Ok(()));
    assert!(!ed.save_as_open(), "written: it goes");
    ed.save_as_done(Ok(()));
    ed.folder_listed(Path::new(DOCUMENTS), Ok(vec![]));
}

#[test]
fn ctrl_shift_s_asks_for_save_as_and_ctrl_s_for_save_and_neither_while_it_is_up() {
    let mut ed = editor();
    assert!(ed.shortcut("s", true));
    assert!(ed.requests.take(&Request::SaveAs) && !ed.requests.has(&Request::Save));
    assert!(ed.shortcut("s", false));
    assert!(ed.requests.take(&Request::Save));
    // A slate has no shortcuts.
    ed.set_mode(Mode::Tablet);
    assert!(!ed.shortcut("s", true));
    assert!(!ed.requests.has(&Request::SaveAs));
}

#[test]
fn loading_another_document_puts_it_away() {
    let mut ed = asking(Mode::Tablet);
    ed.load(crate::view::Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.save_as_open());
}

#[test]
fn a_storm_of_hostile_events_leaves_the_dialog_sound() {
    // (Bigger with `INKPDF_STRESS`, as the other storms are.)
    let bigger: usize = std::env::var("INKPDF_STRESS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let rounds = 3_000 * bigger;
    for seed in 0..4u64 {
        let mut rng = Rng::new(0xC0FF_EE00 + seed);
        let mut ed = asking(if seed % 2 == 0 { Mode::Desktop } else { Mode::Tablet });
        let words = ["", "a", "report", "a/b", "CON", "..", "é文", "\n", "\t", "x\u{0}y", "  ", ".pdf", "😀", "\r\n"];
        for step in 0..rounds {
            let pos = [rng.coord(1300.0), rng.coord(1000.0)];
            match rng.below(16) {
                0 => ed.mouse_moved(pos),
                1 => ed.mouse_button(if rng.chance(0.5) { ButtonState::Pressed } else { ButtonState::Released }, MouseButton::Left),
                2 => ed.wheel(Wheel::Lines(rng.coord(5.0), rng.coord(5.0))),
                3 => ed.wheel(Wheel::Pixels(rng.coord(500.0), rng.coord(500.0))),
                4 => ed.pointer_event(pen(rng.pick(&[Phase::Down, Phase::Move, Phase::Up, Phase::Cancel, Phase::Hover, Phase::Leave]), TIP, &[pos])),
                5 => ed.pointer_event(touch(rng.below(3) as u32, rng.pick(&[Phase::Down, Phase::Move, Phase::Up, Phase::Cancel]), pos, 30.0)),
                6 => {
                    ed.press(Key::Text(rng.pick(&words)));
                }
                7 => {
                    ed.press(rng.pick(&[
                        Key::Backspace,
                        Key::Delete,
                        Key::Home(false),
                        Key::End(true),
                        Key::Arrow(Dir::Left, false),
                        Key::Arrow(Dir::Right, true),
                        Key::Arrow(Dir::Up, false),
                        Key::Arrow(Dir::Down, false),
                        Key::Chord('a'),
                    ]));
                }
                8 => {
                    ed.press(Key::Escape);
                }
                9 => {
                    ed.shortcut(rng.pick(&["s", "z", "a"]), rng.chance(0.5));
                }
                10 => {
                    let count = rng.pick(&[0, 1, 5, 80, MAX_ENTRIES + 100]);
                    let list = (0..count).map(|i| Entry { name: format!("{}{i}", rng.pick(&words)), folder: rng.chance(0.5) }).collect();
                    let dir = if rng.chance(0.5) { PathBuf::from(DOCUMENTS) } else { PathBuf::from("/home/me") };
                    ed.folder_listed(&dir, if rng.chance(0.9) { Ok(list) } else { Err("denied".into()) });
                }
                11 => ed.save_as_done(if rng.chance(0.5) { Ok(()) } else { Err("full".into()) }),
                12 => ed.set_viewport([rng.pick(&[200.0, 480.0, 1200.0, 3000.0]), rng.pick(&[300.0, 800.0, 900.0, 2000.0])]),
                13 => {
                    if !ed.save_as_open() {
                        ed.ask_save_as(DOCUMENTS.into(), rng.pick(&["Untitled.pdf", "", "a\nb", "😀.pdf"]), places());
                    }
                }
                14 => tap(&mut ed, By::Mouse, rng.pick(&[SaveAsHit::Save, SaveAsHit::Up, SaveAsHit::Row(0), SaveAsHit::Place(0), SaveAsHit::Name])),
                _ => while ed.requests.pop().is_some() {},
            }
            ed.assert_save_as_sound();
            if step % 50 == 0 {
                let mut ui = Ui::default();
                ed.draw_save_as(&mut ui);
                assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())), "seed {seed} step {step}");
                assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite())), "seed {seed} step {step}");
            }
        }
    }
}
