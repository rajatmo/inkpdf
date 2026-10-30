//! The ink store and its history, and the cost of a crowded page.

use super::*;

#[test]
fn stress_the_ink_store_undoes_and_redoes_exactly_whatever_was_done() {
    for seed in 1..=20u64 {
        let mut rng = Rng::new(seed);
        let mut store = InkStore::new();
        let initial: Vec<PageStroke> = (0..rng.below(30))
            .map(|_| {
                let (page, style, pts) = random_stroke(&mut rng, 3);
                (page, style, pts, None, None, None)
            })
            .collect();
        store.load(initial);
        let start = store.strokes().to_vec();
        let ops = 100 + rng.below(200 * scale());
        for _ in 0..ops {
            let ids: Vec<u64> = store.strokes().iter().map(|s| s.id).collect();
            match rng.below(7) {
                0 | 1 => {
                    let (page, style, pts) = random_stroke(&mut rng, 3);
                    store.add(page, style, pts);
                }
                2 if !ids.is_empty() => {
                    let pick: Vec<u64> = (0..1 + rng.below(3)).map(|_| rng.pick(&ids)).collect();
                    store.remove(&pick);
                }
                3 if !ids.is_empty() => {
                    let pick: Vec<u64> = (0..1 + rng.below(3)).map(|_| rng.pick(&ids)).collect();
                    let to_page = rng.chance(0.3).then(|| rng.below(3) as u32);
                    store.move_by(&pick, rng.range(-50.0, 50.0), rng.range(-50.0, 50.0), to_page);
                }
                4 => {
                    store.undo();
                }
                5 => {
                    store.redo();
                }
                _ => {
                    // Hostile points are refused, not stored.
                    store.add(0, Style { rgba: [0; 4], width: 1.0 }, vec![[f32::NAN, 1.0], [f32::INFINITY, 2.0]]);
                }
            }
            assert!(store.dirty_pages().iter().all(|&p| p < 3), "dirty pages stay real pages");
        }
        // Rewind completely, then replay completely.
        let end = store.strokes().to_vec();
        let mut undone = 0;
        while store.undo() {
            undone += 1;
        }
        // The store keeps the last 500 steps. A run with fewer operations than that lost none of them
        // (a long run that undid its way back below 500 steps may still have dropped its oldest).
        if ops < 500 {
            assert!(same_ink(store.strokes(), &start), "seed {seed}: undoing everything restores the start");
            // Redo exactly the steps just undone (an earlier undo may have left more redo behind).
            for _ in 0..undone {
                assert!(store.redo(), "seed {seed}: a step that was undone can be redone");
            }
            assert!(same_ink(store.strokes(), &end), "seed {seed}: redoing them restores the end");
        }
    }
}

#[test]
fn stress_a_huge_drawing_stays_fast() {
    let mut rng = Rng::new(5);
    let mut store = InkStore::new();
    let (strokes, per_page) = (20_000 * scale().min(5), 50);
    let t = Instant::now();
    for i in 0..strokes {
        let (_, style, pts) = random_stroke(&mut rng, 1);
        store.add((i / per_page) as u32, style, pts);
    }
    let added = t.elapsed();
    let t = Instant::now();
    let mut hits = 0;
    for _ in 0..2000 {
        hits += usize::from(store.hit_test([rng.range(0.0, 600.0), rng.range(0.0, 2000.0)], 5.0).is_some());
    }
    let hit_test = t.elapsed() / 2000;
    let t = Instant::now();
    let many = store.in_polygon(&[[0.0, 0.0], [600.0, 0.0], [600.0, 2000.0], [0.0, 2000.0]]).len();
    let lassoed = t.elapsed();
    eprintln!("{strokes} strokes: add {added:?}; hit test {hit_test:?} each ({hits} hit); lasso of {many} took {lassoed:?}");
    assert!(hit_test.as_micros() < 5_000, "a hit test stays well under a frame: {hit_test:?}");
    assert!(added.as_secs() < 5, "{added:?}");
}

#[test]
fn stress_rebuilding_the_ink_buffer_for_a_crowded_page_is_cheap_enough() {
    // What the shell does when ink changes: flatten every stroke into capsules again.
    let mut rng = Rng::new(3);
    let strokes: Vec<Vec<P>> = (0..5_000).map(|_| random_stroke(&mut rng, 1).2).collect();
    let t = Instant::now();
    let capsules: usize = strokes.iter().map(|pts| curve_through(pts, 0.75).len()).sum();
    let took = t.elapsed();
    eprintln!("flattening {} strokes into {capsules} segments took {took:?}", strokes.len());
    assert!(took.as_millis() < 100, "{took:?}");
}
