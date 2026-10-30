//! The table of contents of a PDF (its outline, or bookmarks): a list of titles, each at a depth and going to a page.
//! Kept flat, in reading order, as it is shown; which entries are folded away is for whoever shows them. The pages of
//! its entries follow the page edits like everything else that is kept by page (see [`crate::pages`]).

use std::collections::BTreeSet;

use crate::pages::Remap;

/// What the title of one of the program's own flags begins with: a flag is an outline entry for a page the person flagged (`Flag: page 3`),
/// at the top level at the end of the outline, which the program writes, finds again and keeps out of the table of contents it shows.
pub const FLAG: &str = "Flag: page ";

/// The pages that are flagged in items (the program's own entries: see [`FLAG`]), which are taken out of it: they are not part of the contents.
pub fn take_flags(items: &mut Vec<OutlineItem>) -> BTreeSet<usize> {
    let mut flags = BTreeSet::new();
    items.retain(|item| match item.page {
        Some(page) if item.depth == 0 && item.title.starts_with(FLAG) => {
            flags.insert(page);
            false
        }
        _ => true,
    });
    flags
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    /// 0 for the top level, 1 for what is under it, and so on.
    pub depth: usize,
    /// The page it goes to (0-based), if it goes to one in this document.
    pub page: Option<usize>,
    /// Whether the file shows it opened out (its children listed) to begin with.
    pub open: bool,
    /// Whether the next entry is under this one.
    pub has_children: bool,
}

/// What a damaged file can say: an entry that goes to a page the document does not show (it has `pages` pages) goes nowhere.
pub fn limit_to(items: &mut [OutlineItem], pages: usize) {
    for item in items {
        item.page = item.page.filter(|&p| p < pages);
    }
}

/// Carry the entries through an edit of the pages: an entry that goes to a page goes to the number it has now, and one
/// that goes to a page that is gone goes nowhere. Returns each entry that lost its page, with the page it was, so that
/// [`restore`] can give it back if the pages come back.
pub fn remap(items: &mut [OutlineItem], map: &Remap) -> Vec<(usize, usize)> {
    let mut lost = Vec::new();
    for (i, item) in items.iter_mut().enumerate() {
        if let Some(page) = item.page {
            item.page = map.page(page);
            if item.page.is_none() {
                lost.push((i, page));
            }
        }
    }
    lost
}

/// Undo what [`remap`] did for an edit that deleted pages: `map` is what the pages' coming back did to the numbers
/// (see [`Remap`]), and `lost` what [`remap`] returned for the deletion.
pub fn restore(items: &mut [OutlineItem], map: &Remap, lost: &[(usize, usize)]) {
    remap(items, map);
    for &(i, page) in lost {
        if let Some(item) = items.get_mut(i) {
            item.page = Some(page);
        }
    }
}

/// The entry that holds `page`: the one that goes furthest into the document without going past it; of several that go
/// to the same page, the last in reading order (the deepest section). `None` if no entry goes to `page` or before it.
pub fn section_of(items: &[OutlineItem], page: usize) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (i, item) in items.iter().enumerate() {
        if let Some(p) = item.page.filter(|&p| p <= page)
            && best.is_none_or(|(_, q)| p >= q)
        {
            best = Some((i, p));
        }
    }
    best.map(|(i, _)| i)
}

/// The entries to show when the ones in `folded` (by index) are folded closed: every entry that is not under a folded one.
pub fn visible(items: &[OutlineItem], folded: &dyn Fn(usize) -> bool) -> Vec<usize> {
    let mut shown = Vec::with_capacity(items.len());
    let mut hidden_below: Option<usize> = None;
    for (i, item) in items.iter().enumerate() {
        if hidden_below.is_some_and(|d| item.depth > d) {
            continue;
        }
        hidden_below = None;
        shown.push(i);
        if item.has_children && folded(i) {
            hidden_below = Some(item.depth);
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::PageEdit;
    use crate::view::Layout;

    fn item(title: &str, depth: usize, page: Option<usize>) -> OutlineItem {
        OutlineItem { title: title.into(), depth, page, open: false, has_children: false }
    }

    /// The sample outline, as a list: `Chapter 1 > (1.1, 1.2)`, `Chapter 2`, and `Appendix`, which goes nowhere.
    fn sample() -> Vec<OutlineItem> {
        let mut items = vec![
            item("Chapter 1", 0, Some(0)),
            item("1.1 Basics", 1, Some(1)),
            item("1.2 More", 1, Some(2)),
            item("Chapter 2", 0, Some(3)),
            item("Appendix", 0, None),
        ];
        items[0].has_children = true;
        items[0].open = true;
        items
    }

    fn through(edit: &PageEdit, pages: usize) -> Remap {
        let before = Layout::new(&vec![[600.0, 800.0]; pages]);
        let after = before.edited(edit).expect("layout");
        Remap::new(edit, &before, &after).expect("remap")
    }

    #[test]
    fn an_entry_that_goes_to_a_page_that_is_not_there_goes_nowhere() {
        let mut items = sample();
        limit_to(&mut items, 3);
        assert_eq!(items.iter().map(|i| i.page).collect::<Vec<_>>(), [Some(0), Some(1), Some(2), None, None]);
    }

    #[test]
    fn entries_follow_their_pages_and_the_one_whose_page_goes_goes_nowhere_until_it_is_back() {
        let mut items = sample();
        let del = PageEdit::delete(1);
        let lost = remap(&mut items, &through(&del, 4));
        assert_eq!(items.iter().map(|i| i.page).collect::<Vec<_>>(), [Some(0), None, Some(1), Some(2), None], "later pages are one less");
        assert_eq!(lost, vec![(1, 1)]);
        // The pages come back (as an undo does): every entry is as it was.
        let back = PageEdit::Restore { grave: 1, pages: vec![(1, [600.0, 800.0])] };
        restore(&mut items, &through(&back, 3), &lost);
        assert_eq!(items, sample());
        // A move renumbers entries without losing any.
        let mv = PageEdit::Move { pages: vec![3], to: 0 };
        assert!(remap(&mut items, &through(&mv, 4)).is_empty());
        assert_eq!(items.iter().map(|i| i.page).collect::<Vec<_>>(), [Some(1), Some(2), Some(3), Some(0), None]);
    }

    #[test]
    fn the_section_of_a_page_is_the_deepest_entry_that_starts_at_or_before_it() {
        let items = sample();
        assert_eq!(section_of(&items, 0), Some(0));
        assert_eq!(section_of(&items, 1), Some(1), "a page of its own section");
        assert_eq!(section_of(&items, 2), Some(2));
        assert_eq!(section_of(&items, 3), Some(3));
        assert_eq!(section_of(&items, 9), Some(3), "the last chapter holds what comes after it");
        assert_eq!(section_of(&[item("late", 0, Some(5))], 2), None, "nothing starts before it");
        assert_eq!(section_of(&[item("a", 0, Some(1)), item("b", 1, Some(1))], 1), Some(1), "of two on one page, the later one");
    }

    #[test]
    fn folding_an_entry_hides_what_is_under_it_and_nothing_else() {
        let items = sample();
        assert_eq!(visible(&items, &|_| false), [0, 1, 2, 3, 4]);
        assert_eq!(visible(&items, &|i| i == 0), [0, 3, 4], "chapter 1 folded");
        assert_eq!(visible(&items, &|i| i == 3), [0, 1, 2, 3, 4], "an entry with nothing under it folds nothing");
        assert!(visible(&[], &|_| true).is_empty());
    }
}
