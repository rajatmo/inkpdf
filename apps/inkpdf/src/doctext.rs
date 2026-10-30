//! The text of the document's pages, as the editor keeps it: what the indexer has read so far, by the page of the file that has it, and
//! which page of the file each page of the document is drawn from. A page that was added has no text; a copy of a page has its original's;
//! a page edit changes only the second, never the first (that is why it is kept by file page, as the pictures are).

use std::collections::HashMap;
use std::sync::Arc;

use crate::pagetext::PageTextView;

#[allow(dead_code, reason = "the select tool and the search card read it (X4, X5)")]
#[derive(Default)]
pub struct DocText {
    /// What was read of a page of the file: `None` when it could not be.
    by_file: HashMap<u32, Option<Arc<dyn PageTextView>>>,
    /// The page of the file each page of the document is drawn from (`None` for an added page).
    files: Vec<Option<u32>>,
    /// Counts every change, so what was worked out from the text (search hits) can tell it is out of date.
    revision: u64,
}

#[allow(dead_code, reason = "the select tool and the search card read it (X4, X5)")]
impl DocText {
    /// Forget everything (another document was opened).
    pub fn clear(&mut self) {
        self.by_file.clear();
        self.files.clear();
        self.revision += 1;
    }

    /// Say which page of the file draws each page of the document. `true` if that is a change.
    pub fn set_files(&mut self, files: Vec<Option<u32>>) -> bool {
        if self.files == files {
            return false;
        }
        self.files = files;
        self.revision += 1;
        true
    }

    /// What was read of a page of the file.
    pub fn insert(&mut self, file: u32, view: Option<Arc<dyn PageTextView>>) {
        self.by_file.insert(file, view);
        self.revision += 1;
    }

    /// The text of a page of the document, if it has been read and the page has any.
    pub fn page(&self, page: usize) -> Option<&Arc<dyn PageTextView>> {
        let file = (*self.files.get(page)?)?;
        self.by_file.get(&file)?.as_ref().filter(|t| !t.is_empty())
    }

    /// Whether the text of a page of the document is settled: read (or unreadable), or there is none to read (an added page).
    pub fn settled(&self, page: usize) -> bool {
        match self.files.get(page) {
            Some(Some(file)) => self.by_file.contains_key(file),
            _ => true,
        }
    }

    /// How many pages of the document are settled, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        ((0..self.files.len()).filter(|&p| self.settled(p)).count(), self.files.len())
    }

    /// The pages of the file that have not been read, most urgent first: those nearest to `page`.
    pub fn wanted(&self, page: usize) -> Vec<u32> {
        let mut pages: Vec<usize> = (0..self.files.len()).filter(|&p| !self.settled(p)).collect();
        pages.sort_by_key(|&p| (p.abs_diff(page), p));
        let mut seen = std::collections::HashSet::new();
        pages.into_iter().filter_map(|p| self.files[p]).filter(|f| seen.insert(*f)).collect()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagetext::FakeText;

    fn text(lines: &[&str]) -> Option<Arc<dyn PageTextView>> {
        Some(Arc::new(FakeText::new(lines)))
    }

    #[test]
    fn a_page_has_the_text_of_the_page_of_the_file_that_draws_it() {
        let mut t = DocText::default();
        t.set_files(vec![Some(0), Some(1), None, Some(0)]);
        t.insert(0, text(&["one"]));
        t.insert(1, text(&["two"]));
        assert_eq!(t.page(0).map(|p| p.plain_text()), Some("one".into()));
        assert_eq!(t.page(3).map(|p| p.plain_text()), Some("one".into()), "a copy has its original's");
        assert!(t.page(2).is_none(), "an added page has none");
        assert!(t.page(9).is_none());
        // A page edit moves the pages, not the text.
        t.set_files(vec![Some(1), Some(0)]);
        assert_eq!(t.page(0).map(|p| p.plain_text()), Some("two".into()));
        assert_eq!(t.page(1).map(|p| p.plain_text()), Some("one".into()));
    }

    #[test]
    fn what_is_unread_is_asked_for_nearest_first_and_progress_counts_the_settled() {
        let mut t = DocText::default();
        t.set_files((0..6).map(Some).collect());
        assert_eq!(t.progress(), (0, 6));
        assert_eq!(t.wanted(3), vec![3, 2, 4, 1, 5, 0]);
        t.insert(3, text(&["x"]));
        t.insert(4, None);
        assert_eq!(t.progress(), (2, 6), "an unreadable page is settled too");
        assert_eq!(t.wanted(3), vec![2, 1, 5, 0]);
        assert!(t.page(4).is_none() && t.settled(4));
    }

    #[test]
    fn a_page_drawn_twice_is_asked_for_once_and_every_change_is_counted() {
        let mut t = DocText::default();
        let r0 = t.revision();
        assert!(t.set_files(vec![Some(0), Some(0), Some(1)]));
        assert!(!t.set_files(vec![Some(0), Some(0), Some(1)]), "the same is no change");
        assert_eq!(t.wanted(0), vec![0, 1]);
        t.insert(0, text(&["a"]));
        assert!(t.revision() > r0 + 1);
        t.clear();
        assert_eq!(t.progress(), (0, 0));
    }

    #[test]
    fn a_page_with_no_text_is_settled_but_has_none() {
        let mut t = DocText::default();
        t.set_files(vec![Some(0)]);
        t.insert(0, text(&[]));
        assert!(t.settled(0) && t.page(0).is_none(), "a scan has nothing to select or find");
    }
}
