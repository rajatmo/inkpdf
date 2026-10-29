//! What the editor costs between a pen sample reaching it and the stroke being ready to draw, so that a change that makes the
//! pen path slower is seen before it is felt. It is a benchmark, not a test of behaviour, so it is ignored by default; run it in
//! release, where the numbers mean something:
//!
//! `cargo test --release -p inkpdf --bin inkpdf -- latency_budget --ignored --nocapture`
//!
//! The editor's share is tiny next to the display's (about a microsecond against a frame of 8 ms at 120 Hz), which is what lets
//! the code above it be abstracted freely; the bound below is far looser than today's numbers and only catches something gross
//! (an allocation per sample, a lock, a copy of the whole stroke on every point). The real figures, pen sample to frame
//! submitted, are measured on the display with `inkpdf --stats` (see `docs/PORTING.md`).

use std::time::{Duration, Instant};

use super::*;

/// The most a batch of pen moves may cost on average (release builds; today it is about a thousandth of this).
const MOVE_BUDGET: Duration = Duration::from_micros(50);

#[test]
#[ignore = "a benchmark: run it in release with --ignored --nocapture"]
fn latency_budget_a_batch_of_pen_moves_costs_the_editor_next_to_nothing() {
    let mut ed = editor();
    ed.set_tool(Tool::Pen);
    ed.chrome.panel = None;
    let (strokes, batches) = (400, 60);
    let (mut down, mut moves, mut up) = (Vec::new(), Vec::new(), Vec::new());
    for s in 0..strokes {
        let y = 200.0 + (s % 50) as f32 * 8.0;
        // (The events are made before they are timed: only what the editor does with them is.)
        let first = pen(Phase::Down, TIP, &[on_page(&ed, 50.0, y)]);
        let batched: Vec<PointerEvent> = (0..batches)
            .map(|b| {
                // Four samples at a time, as Windows batches what a pen reports between two messages.
                let points: Vec<[f32; 2]> =
                    (0..4).map(|i| on_page(&ed, 50.0 + (b * 4 + i) as f32 * 2.0, y + ((b * 4 + i) as f32 * 0.3).sin() * 20.0)).collect();
                pen(Phase::Move, TIP, &points)
            })
            .collect();
        let last = pen(Phase::Up, TIP, &[on_page(&ed, 300.0, y)]);
        let t = Instant::now();
        ed.pointer_event(first);
        down.push(t.elapsed());
        for ev in batched {
            let t = Instant::now();
            ed.pointer_event(ev);
            moves.push(t.elapsed());
        }
        let t = Instant::now();
        ed.pointer_event(last);
        up.push(t.elapsed());
    }
    let describe = |name: &str, times: &mut Vec<Duration>| -> Duration {
        times.sort_unstable();
        let mean = times.iter().sum::<Duration>() / times.len() as u32;
        eprintln!(
            "{name:>5}: mean {mean:>9.2?}  p50 {:>9.2?}  p99 {:>9.2?}  worst {:>9.2?}  ({} events)",
            times[times.len() / 2],
            times[times.len() * 99 / 100],
            times[times.len() - 1],
            times.len()
        );
        mean
    };
    describe("down", &mut down);
    let move_mean = describe("move", &mut moves);
    describe("up", &mut up);
    // (In a debug build everything is many times slower; the bound is about release.)
    if !cfg!(debug_assertions) {
        assert!(move_mean < MOVE_BUDGET, "a batch of pen moves cost {move_mean:?} on average, over the budget of {MOVE_BUDGET:?}");
    }
}
