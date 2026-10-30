//! The editor under hostile and under realistic input, and with the prompt up.

use super::*;

#[test]
fn stress_the_editor_survives_a_million_hostile_events_and_keeps_its_invariants() {
    let events = 60_000 * scale();
    for seed in 1..=4u64 {
        let mut rng = Rng::new(seed);
        let mut ed = editor_with_pages(1 + seed as usize);
        for i in 0..events {
            let event = poke(&mut ed, &mut rng, true);
            check_view(&ed, &format!("seed {seed}, event {i}"), &event);
            if i % 500 == 0 {
                check_editor(&ed, &format!("seed {seed}, event {i}"));
            }
        }
        check_editor(&ed, &format!("seed {seed}, end"));
    }
}

#[test]
fn stress_realistic_input_draws_lots_of_ink_that_always_undoes_cleanly() {
    // Events a device could really send (finite, in and around the window): strokes get made, erased,
    // selected and moved. Whatever happens, undoing everything must leave an empty, clean page.
    let mut rng = Rng::new(99);
    let mut ed = editor_with_pages(3);
    PAGES_ADDED.with(|n| n.set(0));
    let (mut created, mut last_len) = (0usize, 0usize);
    for i in 0..(80_000 * scale()) {
        let event = poke(&mut ed, &mut rng, false);
        created += ed.ink.len().saturating_sub(last_len);
        last_len = ed.ink.len();
        check_view(&ed, &format!("event {i}"), &event);
        if i % 1000 == 0 {
            check_editor(&ed, &format!("event {i}"));
        }
    }
    ed.pointer_event(PointerEvent { device: Device::Pen, id: 0, phase: Phase::Cancel, samples: vec![], pen: PenButtons::default(), contact_px: 0.0 });
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    eprintln!("strokes made during the storm: {created}; alive at the end: {}", ed.ink.len());
    assert!(created > 300, "the storm should have drawn plenty of ink ({created})");
    let added = PAGES_ADDED.with(Cell::get);
    eprintln!("blank pages added by pulling: {added}");
    assert!(added > 5, "the storm should have pulled in new pages ({added})");
    let mut steps = 0;
    while ed.ink.can_undo() && steps < 100_000 {
        ed.undo();
        steps += 1;
    }
    check_editor(&ed, "after undoing everything");
    if steps < 500 {
        assert_eq!(ed.ink.len(), 0, "undoing every step leaves no ink (took {steps} steps)");
    }
}

#[test]
fn stress_while_the_prompt_is_up_nothing_underneath_changes_whatever_arrives() {
    let mut rng = Rng::new(404);
    let mut ed = editor_with_pages(3);
    // Some ink to protect, and a selection.
    for i in 0..20 {
        ed.ink.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[50.0 + i as f32 * 9.0, 100.0], [80.0 + i as f32 * 9.0, 160.0]]);
    }
    let (mut blocked, mut answered) = (0, 0);
    for i in 0..(60_000 * scale()) {
        if ed.chrome.prompt.is_none() {
            // Leave nothing pending from before, so any flag that appears came from the events.
            ed.requests.take(&Request::Save);
            ed.requests.take(&Request::Close);
            ed.requests.take(&Request::Fullscreen);
            ed.requests.take_page_edits();
            ed.requests.clear_urls();
            ed.requests.take(&Request::SettingsChanged);
            if let Some(choice) = ed.requests.take_answer() {
                let _ = choice;
                answered += 1;
            }
            ed.ask_unsaved();
        }
        let (revision, strokes) = (ed.ink.revision(), ed.ink.len());
        let event = if rng.chance(0.3) { gesture(&mut ed, &mut rng) } else { poke(&mut ed, &mut rng, true) };
        // Events that replace the whole document or stand in for the shell are not the person's input.
        let shell_stepped_in = event.starts_with("load") || event.starts_with("close") || event.starts_with("add a page") || event.contains("shell");
        if ed.chrome.prompt.is_some() && !shell_stepped_in {
            blocked += 1;
            assert_eq!((ed.ink.revision(), ed.ink.len()), (revision, strokes), "event {i} ({event}) changed the ink under the prompt");
            assert!(
                !(ed.requests.has(&Request::Save)
                    || ed.requests.has(&Request::Close)
                    || ed.requests.has(&Request::Fullscreen)
                    || ed.requests.page_edit().is_some())
                    && ed.requests.url().is_none(),
                "event {i} ({event}) asked the shell for something under the prompt"
            );
        }
        if i % 500 == 0 {
            check_editor(&ed, &format!("event {i}"));
        }
    }
    eprintln!("events heard by the prompt alone: {blocked}; answers given: {answered}");
    assert!(blocked > 1000 && answered > 20, "the prompt should have been up and answered plenty ({blocked}, {answered})");
}
