//! Saving, reopening, damaged files and documents that grow.

use super::*;

#[test]
fn stress_corrupted_pdfs_are_an_error_or_a_document_never_a_crash() {
    let dir = temp_dir("corrupt");
    let seeds = [sample_pdf(), crate::testutil::pdf_with_links()];
    let mut rng = Rng::new(11);
    let (mut opened, mut refused) = (0, 0);
    for i in 0..(150 * scale()) {
        let mut bytes = seeds[i % seeds.len()].clone();
        match rng.below(5) {
            0 => {
                for _ in 0..1 + rng.below(30) {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.next() as u8;
                }
            }
            1 => bytes.truncate(rng.below(bytes.len())),
            2 => {
                let (a, b) = (rng.below(bytes.len()), rng.below(bytes.len()));
                let (a, b) = (a.min(b), a.max(b));
                let chunk = bytes[a..b].to_vec();
                let at = rng.below(bytes.len() + 1);
                bytes.splice(at..at, chunk);
            }
            3 => bytes = (0..rng.below(2000)).map(|_| rng.next() as u8).collect(),
            _ => {
                let at = rng.below(bytes.len());
                bytes.drain(at..(at + rng.below(200)).min(bytes.len()));
            }
        }
        let path = dir.join(format!("c{i}.pdf"));
        std::fs::write(&path, &bytes).expect("write");
        match Doc::open(&path) {
            Ok((mut doc, _strokes)) => {
                opened += 1;
                // Asking for pages from a damaged file must not take the process down either.
                let mut store = InkStore::new();
                store.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[50.0, 50.0], [90.0, 90.0]]);
                let _ = doc.save(&store);
            }
            Err(_) => refused += 1,
        }
    }
    eprintln!("corrupted files: {opened} opened, {refused} refused");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_saving_and_reopening_keeps_every_stroke() {
    let dir = temp_dir("roundtrip");
    let mut rng = Rng::new(21);
    for round in 0..(8 * scale()) {
        let path = dir.join(format!("r{round}.pdf"));
        std::fs::write(&path, sample_pdf()).expect("write");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let mut store = InkStore::new();
        let count = 1 + rng.below(300);
        let page = doc.layout().page(0).expect("page");
        for _ in 0..count {
            let n = 2 + rng.below(30);
            let (mut x, mut y) = (page.x + rng.range(20.0, 500.0), page.y + rng.range(20.0, 700.0));
            let pts: Vec<P> = (0..n)
                .map(|_| {
                    x = (x + rng.range(-6.0, 6.0)).clamp(page.x + 5.0, page.x + 600.0);
                    y = (y + rng.range(-6.0, 6.0)).clamp(page.y + 5.0, page.y + 780.0);
                    [x, y]
                })
                .collect();
            store.add(0, Style { rgba: [rng.below(256) as u8, rng.below(256) as u8, 0, 255], width: rng.range(0.5, 12.0) }, pts);
        }
        doc.save(&store).expect("save");
        let (_, back) = Doc::open(&path).expect("reopen");
        assert_eq!(back.len(), store.len(), "round {round}: every stroke survives a save and reopen");
        // A second save after erasing half replaces, not adds to, the first.
        let ids: Vec<u64> = store.strokes().iter().step_by(2).map(|s| s.id).collect();
        store.remove(&ids);
        doc.save(&store).expect("second save");
        let (_, after) = Doc::open(&path).expect("reopen again");
        assert_eq!(after.len(), store.len(), "round {round}: the erased half stays erased");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_saving_thousands_of_strokes_is_quick_and_complete() {
    let dir = temp_dir("big");
    let path = dir.join("big.pdf");
    std::fs::write(&path, sample_pdf()).expect("write");
    let (mut doc, _) = Doc::open(&path).expect("open");
    let mut rng = Rng::new(31);
    let mut store = InkStore::new();
    let page = doc.layout().page(0).expect("page");
    let count = 5_000 * scale().min(4);
    for _ in 0..count {
        let (x, y) = (page.x + rng.range(20.0, 500.0), page.y + rng.range(20.0, 700.0));
        store.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, (0..20).map(|i| [x + i as f32, y + (i as f32).sin() * 4.0]).collect());
    }
    let t = Instant::now();
    doc.save(&store).expect("save");
    let saved = t.elapsed();
    let size = std::fs::metadata(&path).expect("meta").len();
    let t = Instant::now();
    let (_, back) = Doc::open(&path).expect("reopen");
    let reopened = t.elapsed();
    eprintln!("{count} strokes: save {saved:?}, {} KB on disk, reopen {reopened:?}", size / 1024);
    assert_eq!(back.len(), count);
    assert!(saved.as_secs() < 10 && reopened.as_secs() < 10, "save {saved:?}, reopen {reopened:?}");
    // For trying the real app on a heavily inked file: `INKPDF_STRESS_KEEP=path.pdf`.
    if let Some(keep) = std::env::var_os("INKPDF_STRESS_KEEP") {
        std::fs::copy(&path, keep).expect("keep the file");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_documents_grow_save_and_reopen_without_losing_a_page_or_a_stroke() {
    let dir = temp_dir("model");
    let mut rng = Rng::new(77);
    for round in 0..(8 * scale()) {
        let path = dir.join(format!("m{round}.pdf"));
        // Half the documents start blank (and unsaved), half from a file.
        let mut saved = !rng.chance(0.5);
        let mut doc = if saved {
            std::fs::write(&path, sample_pdf()).expect("write");
            Doc::open(&path).expect("open").0
        } else {
            Doc::blank(crate::doc::BLANK_PAGE).expect("blank")
        };
        let mut store = InkStore::new();
        let mut strokes_on: Vec<usize> = vec![0; doc.page_count()];
        let check = |doc: &Doc, strokes_on: &[usize], what: &str| {
            let (back, strokes) = Doc::open(doc.path().expect("saved")).expect("reopen");
            assert_eq!(back.page_count(), strokes_on.len(), "round {round} {what}: pages");
            for (page, &n) in strokes_on.iter().enumerate() {
                assert_eq!(strokes.iter().filter(|s| s.0 as usize == page).count(), n, "round {round} {what}: strokes on page {page}");
            }
        };
        for _ in 0..(12 + rng.below(30)) {
            match rng.below(5) {
                0 => {
                    if let Some(page) = doc.add_blank_page() {
                        strokes_on.push(0);
                        store.mark_dirty(page as u32);
                    }
                }
                1..=2 => {
                    let page = rng.below(doc.page_count());
                    let rect = doc.layout().page(page).expect("page");
                    let (x, y) = (rect.x + rng.range(20.0, rect.w - 60.0), rect.y + rng.range(20.0, rect.h - 60.0));
                    store.add(
                        page as u32,
                        Style { rgba: [0, 0, 0, 255], width: 2.0 },
                        (0..8).map(|i| [x + i as f32 * 3.0, y + (i as f32).sin() * 5.0]).collect(),
                    );
                    strokes_on[page] += 1;
                }
                _ => {
                    if saved {
                        doc.save(&store).expect("save");
                    } else {
                        doc.save_as(&path, &store).expect("save as");
                        saved = true;
                    }
                    store.mark_clean();
                    check(&doc, &strokes_on, "after a save");
                }
            }
        }
        if !saved {
            doc.save_as(&path, &store).expect("save as");
        } else {
            doc.save(&store).expect("save");
        }
        check(&doc, &strokes_on, "at the end");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_saving_a_document_with_very_many_added_pages_is_quick_enough() {
    let dir = temp_dir("manypages");
    let mut doc = Doc::blank(crate::doc::BLANK_PAGE).expect("blank");
    let mut store = InkStore::new();
    let count = 500 * scale().min(8);
    let t = Instant::now();
    for _ in 0..count {
        let page = doc.add_blank_page().expect("add");
        store.mark_dirty(page as u32);
    }
    let added = t.elapsed();
    let path = dir.join("many.pdf");
    let t = Instant::now();
    doc.save_as(&path, &store).expect("save as");
    let saved = t.elapsed();
    let t = Instant::now();
    let (back, _) = Doc::open(&path).expect("reopen");
    let reopened = t.elapsed();
    eprintln!(
        "{count} pages added: {added:?}; saved in {saved:?} ({} KB); reopened in {reopened:?}",
        std::fs::metadata(&path).expect("meta").len() / 1024
    );
    assert_eq!(back.page_count(), count + 1);
    assert!(saved.as_secs() < 20, "saving {count} added pages took {saved:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stress_hostile_outlines_and_page_trees_open_or_are_refused_and_what_opens_edits_draws_and_saves() {
    let dir = temp_dir("hostile-outline");
    let mut rng = Rng::new(99);
    let (mut opened, mut refused, mut edited) = (0, 0, 0);
    let mut files: Vec<Vec<u8>> = (0..8).flat_map(|kind| [1, 4].map(|pages| hostile_pdf(kind, pages))).collect();
    // And the proper outline with bytes damaged: any of them may be a wrong number, a wrong name, or a cut.
    let good = crate::testutil::pdf_with_outline();
    for _ in 0..120 * scale().min(3) {
        let mut bytes = good.clone();
        for _ in 0..1 + rng.below(12) {
            let at = rng.below(bytes.len());
            bytes[at] = rng.next() as u8;
        }
        if rng.chance(0.2) {
            bytes.truncate(rng.below(bytes.len()));
        }
        files.push(bytes);
    }
    for (i, bytes) in files.iter().enumerate() {
        let path = dir.join(format!("h{i}.pdf"));
        std::fs::write(&path, bytes).expect("write");
        let Ok((mut doc, contents)) = Doc::open_contents(&path) else {
            refused += 1;
            continue;
        };
        opened += 1;
        let items = contents.outline;
        assert!(items.len() <= 20_000, "file {i}: {} outline entries", items.len());
        for entry in &items {
            assert!(entry.depth <= 40, "file {i}: an entry {} deep", entry.depth);
            assert!(entry.title.chars().count() <= 200 && !entry.title.chars().any(char::is_control), "file {i}: a title that is not clean");
            assert!(entry.page.is_none_or(|p| p < doc.page_count()), "file {i}: an entry goes to page {:?} of {}", entry.page, doc.page_count());
        }
        let mut ed = Editor::reference([1100.0, 800.0], 1.0, crate::motion::Prefs::REDUCED);
        ed.load(doc.layout(), contents.strokes);
        ed.outline = items;
        ed.links = contents.links;
        ed.open_overview();
        for _ in 0..10 {
            let n = ed.layout.len();
            let edit = match rng.below(5) {
                0 => PageEdit::Insert { at: rng.below(n + 1), size: [612.0, 792.0] },
                1 => PageEdit::delete(rng.below(n)),
                2 => PageEdit::Duplicate { pages: vec![rng.below(n)] },
                3 => PageEdit::Move { pages: vec![rng.below(n)], to: rng.below(n) },
                _ => PageEdit::Arrange { order: (0..n).rev().collect() },
            };
            edited += usize::from(ed.edit_pages(edit));
            follow_pages(&mut ed, &mut doc);
            check_overview(&ed, &doc, &format!("file {i}"));
            if rng.chance(0.3) {
                ed.undo();
                follow_pages(&mut ed, &mut doc);
            }
        }
        // Saved and read again, whatever the file was like: an error is fine, a panic is not.
        let _ = doc.save(&ed.ink);
        let _ = Doc::open(&path);
    }
    eprintln!("hostile files: {opened} opened, {refused} refused; page edits made on them: {edited}");
    assert!(opened > 10 && edited > 50, "most of them should open and take edits ({opened} opened, {edited} edits)");
    let _ = crate::pdf_read::outline; // (the reader under test, named so that a rename here is noticed)
    let _ = std::fs::remove_dir_all(dir);
}
