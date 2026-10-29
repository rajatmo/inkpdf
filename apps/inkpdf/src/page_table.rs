//! The page table: for each page of the document, its size and geometry and where it comes from (which page of the renderer's file
//! draws it, which page of the file as last saved it is, which it is a copy of), and the pages deleted since the file was last
//! written, kept until the deletion can no longer be undone. Pure bookkeeping, so it is tested with no PDF and no renderer; `doc`
//! holds one next to the renderer and the saved bytes.

use std::collections::HashMap;

use crate::coords::PageGeom;
use crate::pages::PageEdit;

/// The page size a blank document falls back to when the display is not known, in points (A4).
pub const BLANK_PAGE: [f32; 2] = [595.0, 842.0];

/// Where one page of the document comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    /// Which page of the renderer's file draws it; `None` for a page that was added, which is blank
    /// and never rendered.
    render: Option<usize>,
    /// Which page of the file as last saved it is; `None` for a page that was added since.
    file: Option<usize>,
    /// For a page that is a copy of one: which page of the file as last saved it is a copy of (so that saving can make
    /// the copy from it); `None` for a page that is not a copy.
    dup_of: Option<usize>,
}

/// A page that was deleted, kept until the deletion can no longer be undone.
struct Buried {
    slot: Slot,
    geom: PageGeom,
}

pub struct PageTable {
    /// Displayed size of each page in points.
    sizes: Vec<[f32; 2]>,
    geoms: Vec<PageGeom>,
    /// Each page, in order. Pages can be added and deleted anywhere, so this is kept for every page.
    slots: Vec<Slot>,
    /// How many pages of the file the slots stood for when it was read or saved: the pages of the file that were deleted are
    /// those of these with no slot (pages beyond what the app can show are neither shown nor deleted).
    file_pages: usize,
    /// The pages deleted since the file was last written, each deletion under the name the editor gave it.
    graves: HashMap<u64, Vec<Buried>>,
}

impl PageTable {
    /// The pages of a file as it was read: each draws from, and is, the page of the same number.
    pub fn of_file(sizes: Vec<[f32; 2]>, geoms: Vec<PageGeom>) -> Self {
        let slots = (0..sizes.len()).map(|i| Slot { render: Some(i), file: Some(i), dup_of: None }).collect();
        let file_pages = sizes.len();
        Self { sizes, geoms, slots, file_pages, graves: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    /// Displayed size of each page in points.
    pub fn sizes(&self) -> &[[f32; 2]] {
        &self.sizes
    }

    pub fn geoms(&self) -> &[PageGeom] {
        &self.geoms
    }

    /// The pages kept for these deletions can go: the deletions can no longer be undone.
    pub fn forget_graves(&mut self, graves: &[u64]) {
        for grave in graves {
            self.graves.remove(grave);
        }
    }

    /// Do an edit of the pages (see [`PageEdit`]): the page table follows, and the file is changed when the document is
    /// saved. `false`, and nothing done, if the edit cannot be done to this document, or puts back pages that are no
    /// longer kept. The editor does the same edit to everything it keeps by page.
    pub fn apply(&mut self, edit: &PageEdit) -> bool {
        let count = self.sizes.len();
        if edit.order(count).is_none() {
            return false;
        }
        let mut raised: Vec<Buried> = Vec::new();
        match edit {
            PageEdit::Restore { grave, pages } => match self.graves.remove(grave) {
                Some(found) if found.len() == pages.len() => raised = found,
                Some(found) => {
                    self.graves.insert(*grave, found);
                    return false;
                }
                None => return false,
            },
            PageEdit::Delete { pages, grave } if *grave != 0 => {
                let mut gone = pages.clone();
                gone.sort_unstable();
                gone.dedup();
                let kept = gone.iter().map(|&p| Buried { slot: self.slots[p], geom: self.geoms[p] }).collect();
                self.graves.insert(*grave, kept);
            }
            _ => {}
        }
        let news = edit.new_page_sizes();
        let blank_size = {
            let mut next = news.into_iter();
            move || next.next().unwrap_or(BLANK_PAGE)
        };
        let mut sizes = blank_size.clone();
        edit.apply_with(&mut self.sizes, |source| source.copied().unwrap_or_else(&mut sizes));
        let mut back = raised.iter().map(|b| b.geom);
        let mut sizes = blank_size;
        edit.apply_with(&mut self.geoms, |source| match (source, back.next()) {
            (Some(geom), _) => *geom,
            (None, Some(geom)) => geom,
            (None, None) => {
                let size = sizes();
                PageGeom { width: size[0], height: size[1], crop: [0.0, 0.0, size[0], size[1]], rotation: 0 }
            }
        });
        let mut back = raised.iter().map(|b| b.slot);
        edit.apply_with(&mut self.slots, |source| match (source, back.next()) {
            // A copy draws from the page it copies, and is made from the file's page when the file is written.
            (Some(slot), _) => Slot { render: slot.render, file: None, dup_of: slot.file.or(slot.dup_of) },
            (None, Some(slot)) => slot,
            (None, None) => Slot { render: None, file: None, dup_of: None },
        });
        true
    }

    /// For each page of the file as last saved, which page of the document it is now (`None`: it was deleted).
    pub fn doc_of_file(&self) -> Vec<Option<usize>> {
        let mut map = vec![None; self.file_pages];
        for (page, slot) in self.slots.iter().enumerate() {
            if let Some(entry) = slot.file.and_then(|f| map.get_mut(f)) {
                *entry = Some(page);
            }
        }
        map
    }

    /// The page of the file the renderer has that draws page `page` of the document, if there is one (not for a page that
    /// was added). A picture is kept under this number, not the page's own: it stays the right one when pages are moved,
    /// deleted, copied or brought back, and a copy shares its original's.
    pub fn render_page(&self, page: usize) -> Option<u32> {
        self.slots.get(page).and_then(|s| s.render).map(|r| r as u32)
    }

    /// The pages that are copies made since the file was written, as `(page, page of the file it copies)`.
    pub fn copies(&self) -> Vec<(usize, usize)> {
        self.slots.iter().enumerate().filter_map(|(page, s)| s.dup_of.filter(|_| s.file.is_none()).map(|from| (page, from))).collect()
    }

    /// The file has been written with exactly these pages, in this order: what was added or copied is in it, and what was
    /// deleted is out of it for good.
    pub fn written(&mut self) {
        self.slots.iter_mut().enumerate().for_each(|(i, s)| (s.file, s.dup_of) = (Some(i), None));
        self.file_pages = self.slots.len();
        self.graves.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(w: f32) -> PageGeom {
        PageGeom { width: w, height: 800.0, crop: [0.0, 0.0, w, 800.0], rotation: 0 }
    }

    /// Three pages, 100, 200 and 300 points wide, as a file that was just read.
    fn table() -> PageTable {
        PageTable::of_file(vec![[100.0, 800.0], [200.0, 800.0], [300.0, 800.0]], vec![geom(100.0), geom(200.0), geom(300.0)])
    }

    fn widths(t: &PageTable) -> Vec<f32> {
        t.sizes().iter().map(|s| s[0]).collect()
    }

    #[test]
    fn a_file_just_read_has_each_page_drawn_from_and_standing_for_the_page_of_its_number() {
        let t = table();
        assert_eq!((t.len(), t.render_page(1), t.render_page(9)), (3, Some(1), None));
        assert_eq!(t.doc_of_file(), [Some(0), Some(1), Some(2)]);
        assert!(t.copies().is_empty());
    }

    #[test]
    fn an_inserted_page_is_blank_and_never_rendered_and_the_pages_after_it_move_down() {
        let mut t = table();
        assert!(t.apply(&PageEdit::Insert { at: 1, size: [150.0, 800.0] }));
        assert_eq!(widths(&t), [100.0, 150.0, 200.0, 300.0]);
        assert_eq!((t.render_page(1), t.render_page(2)), (None, Some(1)));
        assert_eq!(t.doc_of_file(), [Some(0), Some(2), Some(3)], "the pages of the file are where they moved to");
        assert_eq!(t.geoms()[1].width, 150.0, "and a blank page has geometry of its size");
    }

    #[test]
    fn a_deleted_page_leaves_the_file_map_and_comes_back_with_its_picture_and_geometry() {
        let mut t = table();
        assert!(t.apply(&PageEdit::Delete { pages: vec![1], grave: 7 }));
        assert_eq!((widths(&t), t.doc_of_file()), (vec![100.0, 300.0], vec![Some(0), None, Some(1)]));
        assert!(t.apply(&PageEdit::Restore { grave: 7, pages: vec![(1, [200.0, 800.0])] }));
        assert_eq!((widths(&t), t.render_page(1), t.geoms()[1].width), (vec![100.0, 200.0, 300.0], Some(1), 200.0));
        assert!(!t.apply(&PageEdit::Restore { grave: 7, pages: vec![(1, [200.0, 800.0])] }), "a grave can be opened once");
    }

    #[test]
    fn what_cannot_be_done_is_refused_and_changes_nothing() {
        let mut t = table();
        assert!(!t.apply(&PageEdit::Delete { pages: vec![5], grave: 0 }), "a page that is not there");
        assert!(!t.apply(&PageEdit::Restore { grave: 3, pages: vec![(0, [1.0, 1.0])] }), "a grave that is not kept");
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn a_copy_draws_from_its_original_and_is_made_from_the_file_page_when_written_then_it_is_a_page_of_the_file() {
        let mut t = table();
        assert!(t.apply(&PageEdit::Duplicate { pages: vec![0] }));
        assert_eq!(t.len(), 4);
        assert_eq!(t.render_page(1), Some(0), "it draws from the page it copies");
        assert_eq!(t.copies(), [(1, 0)]);
        t.written();
        assert!(t.copies().is_empty());
        assert_eq!(t.doc_of_file(), [Some(0), Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn writing_the_file_makes_what_was_deleted_gone_for_good() {
        let mut t = table();
        assert!(t.apply(&PageEdit::Delete { pages: vec![2], grave: 4 }));
        t.written();
        assert!(!t.apply(&PageEdit::Restore { grave: 4, pages: vec![(2, [300.0, 800.0])] }));
        let mut u = table();
        u.apply(&PageEdit::Delete { pages: vec![2], grave: 4 });
        u.forget_graves(&[4]);
        assert!(!u.apply(&PageEdit::Restore { grave: 4, pages: vec![(2, [300.0, 800.0])] }), "and so is what the editor let go of");
    }
}
