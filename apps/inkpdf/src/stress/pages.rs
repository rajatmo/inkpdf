//! Pages inserted among ink, and every tool and interaction together.

use super::*;

#[test]
fn stress_pages_inserted_anywhere_among_ink_keep_every_stroke_on_its_page_through_undo_save_and_reopen() {
    let dir = temp_dir("insert-pages");
    let path = dir.join("pages.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut ed = Editor::new([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(doc.layout(), vec![]);
    let mut rng = Rng::new(606);
    let (mut inserted, mut deleted, mut saves, mut moved) = (0, 0, 0, 0);
    for round in 0..(300 * scale().min(4)) {
        match rng.below(14) {
            0..=2 if doc.page_count() < 60 => {
                // A page somewhere among the others, the size of one of them.
                let (at, like) = (rng.below(doc.page_count() + 1), rng.below(doc.page_count()));
                let size = doc.page_size(like as u32);
                assert!(ed.edit_pages(PageEdit::Insert { at, size }));
                follow_pages(&mut ed, &mut doc);
                inserted += 1;
            }
            3..=6 => {
                let page = rng.below(doc.page_count()) as u32;
                let r = doc.layout().page(page as usize).expect("page");
                let (x, y) = (r.x + rng.range(10.0, r.w - 10.0), r.y + rng.range(10.0, r.h - 10.0));
                ed.ink.add(
                    page,
                    Style { rgba: [rng.below(256) as u8, 0, 0, 255], width: rng.range(0.5, 6.0) },
                    (0..2 + rng.below(8))
                        .map(|i| [(x + i as f32 * 3.0).min(r.x + r.w - 1.0), (y + (i as f32).sin() * 4.0).min(r.bottom() - 1.0)])
                        .collect(),
                );
            }
            7 => {
                ed.undo();
                follow_pages(&mut ed, &mut doc);
            }
            8 => {
                ed.redo();
                follow_pages(&mut ed, &mut doc);
            }
            // A page goes, somewhere among the others, with the ink on it.
            9 | 10 if doc.page_count() > 1 => {
                let at = rng.below(doc.page_count());
                assert!(ed.edit_pages(PageEdit::delete(at)), "page {at} of {} is deleted", doc.page_count());
                follow_pages(&mut ed, &mut doc);
                deleted += 1;
            }
            // Some pages are taken out and put back somewhere else.
            11 | 12 if doc.page_count() > 1 => {
                let n = doc.page_count();
                let pages: Vec<usize> = (0..1 + rng.below(3.min(n - 1))).map(|_| rng.below(n)).collect();
                let mut distinct = pages.clone();
                distinct.sort_unstable();
                distinct.dedup();
                let to = rng.below(n - distinct.len() + 1);
                // (Putting them where they were is no edit: it is refused, and nothing happens.)
                if ed.edit_pages(PageEdit::Move { pages, to }) {
                    moved += 1;
                }
                follow_pages(&mut ed, &mut doc);
            }
            _ => {
                doc.save(&ed.ink).expect("save");
                ed.document_saved();
                saves += 1;
                let (back, strokes) = Doc::open(&path).expect("reopen");
                assert_eq!(back.page_count(), doc.page_count(), "round {round}: the pages are all in the file");
                assert_eq!(strokes.len(), ed.ink.len(), "round {round}: every stroke came back");
                // (The file keeps ink page by page, so compare as sets: each stroke by its page and first point.)
                let key = |page: u32, p: &[f32; 2]| (page, (p[0] * 10.0).round() as i64, (p[1] * 10.0).round() as i64);
                let mut want: Vec<_> = ed.ink.strokes().iter().map(|s| key(s.page, &s.pts[0])).collect();
                let mut got: Vec<_> = strokes.iter().map(|(page, _, pts, ..)| key(*page, &pts[0])).collect();
                want.sort_unstable();
                got.sort_unstable();
                assert_eq!(want, got, "round {round}: the strokes are not on the pages they were");
            }
        }
        // Whatever was done: every stroke is on the page it says it is.
        for s in ed.ink.strokes() {
            let r = doc.layout().page(s.page as usize).expect("the stroke's page exists");
            assert!(
                s.pts.iter().all(|p| p[0] >= r.x - 0.01 && p[0] <= r.x + r.w + 0.01 && p[1] >= r.y - 0.01 && p[1] <= r.bottom() + 0.01),
                "round {round}: stroke {} is off page {}",
                s.id,
                s.page
            );
        }
    }
    eprintln!("pages inserted: {inserted}; deleted: {deleted}; moved: {moved}; saves checked: {saves}; strokes at the end: {}", ed.ink.len());
    assert!(inserted > 20 && deleted > 10 && moved > 10 && saves > 5);
    // Everything the history holds can be undone and redone, with the document following, and the pages stay laid out alike.
    ed.finish_text();
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
    doc.save(&ed.ink).expect("save at the end");
    assert_eq!(Doc::open(&path).expect("reopen").0.page_count(), doc.page_count(), "and the file has the pages the document has");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_every_tool_and_interaction_together_stays_consistent_through_undo_pages_save_and_reopen() {
    // (The ink piles up, and every save writes it all: more than three times the work is slow, not better.)
    for seed in [2024, 7, 31_337] {
        every_tool_together(seed, 2_000 * scale().min(3));
    }
}
