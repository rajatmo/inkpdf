//! A recorded session, written as text and read back, gives the same document when played into a fresh editor: the same stream
//! gives the same result on every OS (see `input::record`).

use super::*;
use crate::input::record::{self, Recorded};
use crate::input::{InputEvent, KeyInput, check};

/// A session of every kind of device: strokes with the pen, a pan with a finger, a mouse wheel, a page of text typed.
fn session(ed: &Editor) -> Vec<InputEvent> {
    let a = on_page(ed, 100.0, 100.0);
    let b = on_page(ed, 300.0, 140.0);
    let c = on_page(ed, 200.0, 260.0);
    let mut events = vec![
        InputEvent::Pointer(pen(Phase::Hover, HOVER, &[a])),
        InputEvent::Pointer(pen(Phase::Down, TIP, &[a])),
        InputEvent::Pointer(pen(Phase::Move, TIP, &[b, c, a])),
        InputEvent::Pointer(pen(Phase::Up, TIP, &[a])),
        InputEvent::Pointer(pen(Phase::Leave, PenButtons::default(), &[a])),
        InputEvent::Wheel(Wheel::Lines(0.0, -2.0)),
        InputEvent::MouseMoved([600.0, 300.0]),
        InputEvent::Shortcut { letter: "z".into(), shift: false },
        InputEvent::Shortcut { letter: "z".into(), shift: true },
        InputEvent::Pointer(pen(Phase::Down, TIP, &[b])),
        InputEvent::Pointer(pen(Phase::Move, TIP, &[c, a])),
        InputEvent::Pointer(pen(Phase::Up, TIP, &[a])),
        InputEvent::Key(KeyInput::Escape),
    ];
    // (The fixture stamps every pen event alike; a real stream's time goes forward.)
    for (i, e) in events.iter_mut().enumerate() {
        if let InputEvent::Pointer(p) = e {
            p.samples.iter_mut().for_each(|s| s.t_ms += i as f64 * 100.0);
        }
    }
    events
}

/// What a session left behind that two runs must agree on.
fn outcome(ed: &Editor) -> (usize, Vec<Vec<crate::ink::P>>, [f32; 2], f32) {
    let strokes = ed.ink.strokes().iter().map(|s| s.pts.clone()).collect();
    (ed.ink.len(), strokes, ed.view.scroll, ed.view.zoom)
}

#[test]
fn a_recorded_session_replays_to_the_same_document() {
    let mut first = editor();
    let events = session(&first);
    let recorded: Vec<Recorded> = events.iter().enumerate().map(|(i, e)| Recorded { at_ms: 1000.0 + i as f64 * 8.0, event: e.clone() }).collect();
    for e in &events {
        first.handle(e.clone());
    }
    assert!(first.ink.len() > 0, "the session drew something");

    // Through the text format and into a fresh editor.
    let text = record::encode(&recorded);
    let mut second = editor();
    for r in record::decode(&text).expect("readable") {
        second.handle(r.event);
    }
    assert_eq!(outcome(&first), outcome(&second));
}

#[test]
fn the_pointer_events_of_a_recording_keep_the_rules_of_a_stream() {
    let ed = editor();
    let pointers: Vec<_> = session(&ed)
        .into_iter()
        .filter_map(|e| match e {
            InputEvent::Pointer(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(check(&pointers), Ok(()));
}
