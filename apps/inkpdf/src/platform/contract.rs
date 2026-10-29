//! What every [`PointerSource`] must do, as tests that take any source: `source_contract` is given a way to make one and a way to
//! touch its screen. The scripted source runs it here; a real one (Windows' messages, Wayland's tablet protocol, Android's
//! `MotionEvent`) runs the same function in the test that has a way to inject for it, so the frame loop can rely on all of them
//! alike: *wait for the display, `pump`, `drain`, draw* (see `platform/mod.rs`).

use super::fake::{FakeSource, Injector};
use super::{MAX_EVENTS_PER_PUMP, PointerSource};
use crate::input::{self, Device, PenButtons, Phase, PointerEvent, Sample};

/// A source and the way to touch its screen.
type Make = dyn Fn() -> (Box<dyn PointerSource>, Injector);

fn sample(i: usize) -> Sample {
    Sample { pos: [i as f32, 2.0 * i as f32], pressure: 0.5, t_ms: 1.0 + i as f64 }
}

/// A pen stroke of `moves` moves, one sample each, with times that rise from `t0`.
pub(crate) fn stroke(id: u32, t0: usize, moves: usize) -> Vec<PointerEvent> {
    let pen = |contact| PenButtons { in_range: true, contact, eraser: false, barrel: false };
    let at = |i: usize| vec![sample(t0 + i)];
    let mut events = vec![PointerEvent { device: Device::Pen, id, phase: Phase::Down, samples: at(0), pen: pen(true), contact_px: 0.0 }];
    for i in 1..=moves {
        events.push(PointerEvent { device: Device::Pen, id, phase: Phase::Move, samples: at(i), pen: pen(true), contact_px: 0.0 });
    }
    events.push(PointerEvent { device: Device::Pen, id, phase: Phase::Up, samples: at(moves + 1), pen: pen(false), contact_px: 0.0 });
    events
}

/// What the frame loop relies on, of any source.
pub(crate) fn source_contract(make: &Make) {
    // A new source has nothing, and asking changes nothing.
    let (mut source, screen) = make();
    assert!(!source.pending() && source.drain().is_empty(), "a new source is empty");
    source.pump();
    assert!(!source.pending() && source.drain().is_empty(), "pumping nothing brings nothing");
    assert!(!source.take_posture_change(), "no posture change was told of");

    // What the OS has queued is in the frame after a pump, in order, once.
    let gesture = stroke(1, 0, 5);
    screen.push_all(gesture.clone());
    source.pump();
    assert!(source.pending(), "after a pump, what was queued is waiting");
    let got = source.drain();
    assert_eq!(got.len(), gesture.len(), "all of it, and only it");
    for (a, b) in got.iter().zip(&gesture) {
        assert_eq!((a.device, a.id, a.phase, &a.samples), (b.device, b.id, b.phase, &b.samples), "in the order it happened, as it was");
    }
    assert!(!source.pending() && source.drain().is_empty(), "a drain empties it");

    // `drain_into` appends to what the caller holds and keeps the caller's room (the frame loop reuses one `Vec`).
    screen.push_all(stroke(2, 0, 3));
    source.pump();
    let mut held = Vec::with_capacity(64);
    held.push(got[0].clone());
    let room = held.capacity();
    source.drain_into(&mut held);
    assert_eq!(held.len(), 1 + stroke(2, 0, 3).len(), "after what was there, in order");
    assert_eq!(held.capacity(), room, "and without growing it");
    assert!(!source.pending(), "and the source is empty");
    assert_eq!(input::check(&got), Ok(()), "and what a source hands over keeps the rules of a stream");

    // A flood does not hold the frame: each pump takes a bounded number, and nothing is lost, repeated or reordered.
    let total = 3 * MAX_EVENTS_PER_PUMP + 17;
    let flood: Vec<PointerEvent> = (0..total)
        .map(|i| PointerEvent {
            device: Device::Touch,
            id: 7,
            phase: Phase::Move,
            samples: vec![sample(i)],
            pen: PenButtons::default(),
            contact_px: 10.0,
        })
        .collect();
    screen.push_all(flood);
    let mut seen = Vec::new();
    for _ in 0..100 {
        source.pump();
        let got = source.drain();
        assert!(got.len() <= MAX_EVENTS_PER_PUMP, "a pump takes at most {MAX_EVENTS_PER_PUMP} events, took {}", got.len());
        seen.extend(got);
        if screen.waiting() == 0 && !source.pending() {
            break;
        }
    }
    assert_eq!(seen.len(), total, "none lost");
    assert!(seen.windows(2).all(|w| w[0].samples[0].t_ms < w[1].samples[0].t_ms), "none repeated or out of order");

    // The posture is told of once.
    screen.posture_changed();
    assert!(source.take_posture_change() && !source.take_posture_change(), "a change is told of once");

    // After it is taken away nothing more is brought in, and nothing waits.
    screen.push_all(stroke(2, 100, 3));
    source.pump();
    source.uninstall();
    assert!(!source.pending() && source.drain().is_empty(), "uninstall leaves nothing waiting");
    screen.push_all(stroke(3, 200, 3));
    source.pump();
    assert!(!source.pending() && source.drain().is_empty(), "and nothing comes in after it");
}

#[test]
fn the_scripted_source_keeps_the_contract() {
    source_contract(&|| {
        let (source, screen) = FakeSource::new();
        (Box::new(source), screen)
    });
}

#[test]
fn the_frame_loop_gets_the_same_document_from_a_source_as_from_the_events_handed_over_directly() {
    // A source adds nothing and drops nothing between the OS and the editor: the ink is the same either way.
    let events = stroke(1, 0, 40);
    let draw = |events: Vec<PointerEvent>| {
        let mut ed = crate::editor::Editor::new([1200.0, 900.0], 1.0);
        ed.load(crate::view::Layout::new(&[[612.0, 792.0]]), vec![]);
        for ev in events {
            ed.pointer_event(PointerEvent {
                samples: ev.samples.iter().map(|s| Sample { pos: [200.0 + s.pos[0] * 5.0, 300.0 + s.pos[1]], ..*s }).collect(),
                ..ev
            });
        }
        ed.ink.strokes().iter().map(|s| s.pts.clone()).collect::<Vec<_>>()
    };
    let (mut source, screen) = FakeSource::new();
    screen.push_all(events.clone());
    source.pump();
    assert_eq!(draw(source.drain()), draw(events));
}
