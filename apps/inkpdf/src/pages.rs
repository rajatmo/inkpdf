//! What can be done to the number and the order of the pages, and what that does to everything that is kept by page.
//!
//! A [`PageEdit`] is the one value that says what happened to the pages. Everything that has a page number in it
//! (the page table of the document, the ink, the links, the selection, the view) applies the same edit, so a new kind
//! of edit is one variant here and not a change in each of them: [`PageEdit::order`] says which page each page of the
//! result was (the one rule), [`PageEdit::apply`] carries any list kept by page through it, and a [`Remap`] says, for
//! each page there was, where it is now and how far its place on the screen has moved. Pure arithmetic: no window,
//! file or renderer.

use crate::view::Layout;

/// Most pages a document can have (a request for one of them carries its number in 20 bits).
pub const MAX_PAGES: usize = 1 << 20;

/// A change to the pages. Page numbers are those the document has before the edit.
#[derive(Clone, Debug, PartialEq)]
pub enum PageEdit {
    /// A blank page `size` points across becomes page `at` (0 to the number of pages: the end).
    Insert { at: usize, size: [f32; 2] },
    /// These pages go, with what is on them. The document keeps at least one page. `grave` names where the document
    /// keeps them, so that [`PageEdit::Restore`] can bring them back (0 for nowhere: the editor gives each delete its own).
    Delete { pages: Vec<usize>, grave: u64 },
    /// These pages (in any order; each counts once) are taken out and put back together, in the order they had, so that
    /// the first of them is page `to` of the result (`to` counts the pages that stay, and is at most as many).
    Move { pages: Vec<usize>, to: usize },
    /// Every page to the place `order` gives it: `order[new]` is the page that becomes page `new`. Any rearrangement
    /// is one of these, and so is the undoing of one.
    Arrange { order: Vec<usize> },
    /// What was in `grave` comes back: each page (with the size it had) as the page of the result it is given.
    Restore { grave: u64, pages: Vec<(usize, [f32; 2])> },
    /// Copies of these pages (each once, with what is on it) are put in together, in the order the pages have, right
    /// after the last of them.
    Duplicate { pages: Vec<usize> },
}

impl PageEdit {
    /// One page deleted (the editor sees to where it is kept).
    pub fn delete(page: usize) -> Self {
        PageEdit::Delete { pages: vec![page], grave: 0 }
    }

    /// The edit that undoes this one, for a document laid out as `before`; `None` if this cannot be done to it.
    pub fn inverse(&self, before: &Layout) -> Option<PageEdit> {
        let order = self.order(before.len())?;
        Some(match self {
            PageEdit::Insert { at, .. } => PageEdit::Delete { pages: vec![*at], grave: 0 },
            PageEdit::Delete { pages, grave } => {
                let sizes = unique(pages, before.len())?.into_iter().map(|p| before.page(p).map(|r| (p, [r.w, r.h]))).collect::<Option<Vec<_>>>()?;
                PageEdit::Restore { grave: *grave, pages: sizes }
            }
            PageEdit::Restore { grave, pages } => PageEdit::Delete { pages: pages.iter().map(|p| p.0).collect(), grave: *grave },
            PageEdit::Duplicate { .. } => PageEdit::Delete { pages: self.copies(before.len()).into_iter().map(|(new, _)| new).collect(), grave: 0 },
            PageEdit::Move { .. } | PageEdit::Arrange { .. } => {
                let mut back = vec![0; order.len()];
                for (new, old) in order.iter().enumerate() {
                    back[(*old)?] = new;
                }
                PageEdit::Arrange { order: back }
            }
        })
    }

    /// For each page of the result, the page it was: `None` for a page that is new. `None` altogether if the edit
    /// cannot be done to a document of `count` pages (a page that is not there, none left, too many, a move to
    /// nowhere); doing nothing is not an edit either.
    pub fn order(&self, count: usize) -> Option<Vec<Option<usize>>> {
        match self {
            PageEdit::Insert { at, size } => {
                let sane = size.iter().all(|s| s.is_finite() && *s > 0.0);
                if !sane || *at > count || count >= MAX_PAGES {
                    return None;
                }
                let mut order: Vec<Option<usize>> = (0..count).map(Some).collect();
                order.insert(*at, None);
                Some(order)
            }
            PageEdit::Delete { pages, .. } => {
                let gone = unique(pages, count)?;
                if gone.is_empty() || gone.len() >= count {
                    return None;
                }
                Some((0..count).filter(|p| gone.binary_search(p).is_err()).map(Some).collect())
            }
            PageEdit::Move { pages, to } => {
                let moving = unique(pages, count)?;
                let mut rest: Vec<usize> = (0..count).filter(|p| moving.binary_search(p).is_err()).collect();
                if moving.is_empty() || *to > rest.len() {
                    return None;
                }
                let tail = rest.split_off(*to);
                let order: Vec<Option<usize>> = rest.into_iter().chain(moving).chain(tail).map(Some).collect();
                // Everything where it was is nothing done.
                (order.iter().enumerate().any(|(new, old)| *old != Some(new))).then_some(order)
            }
            PageEdit::Arrange { order } => {
                let mut seen = vec![false; count];
                let permutation = order.len() == count && order.iter().all(|&p| p < count && !std::mem::replace(&mut seen[p], true));
                (permutation && order.iter().enumerate().any(|(new, &old)| old != new)).then(|| order.iter().map(|&p| Some(p)).collect())
            }
            PageEdit::Duplicate { pages } => {
                let copied = unique(pages, count)?;
                let last = *copied.last()?;
                if count + copied.len() > MAX_PAGES {
                    return None;
                }
                Some((0..=last).map(Some).chain(copied.iter().map(|_| None)).chain((last + 1..count).map(Some)).collect())
            }
            PageEdit::Restore { pages, .. } => {
                let total = count + pages.len();
                let mut at: Vec<usize> = pages.iter().map(|p| p.0).collect();
                at.sort_unstable();
                let sane = pages.iter().all(|p| p.1.iter().all(|s| s.is_finite() && *s > 0.0));
                if pages.is_empty() || !sane || total > MAX_PAGES || at.windows(2).any(|w| w[0] == w[1]) || at.last().is_some_and(|&l| l >= total) {
                    return None;
                }
                let mut old = 0..count;
                Some((0..total).map(|new| if at.binary_search(&new).is_ok() { None } else { old.next() }).collect())
            }
        }
    }

    /// Carry a list with one entry per page through the edit: the entries of the pages that go are dropped, the
    /// others follow their pages, and `blank` makes the entry of each page that is new. `false`, and nothing done, if
    /// the edit cannot be done to this many pages.
    #[cfg(test)]
    pub fn apply<T>(&self, items: &mut Vec<T>, mut blank: impl FnMut() -> T) -> bool {
        self.apply_with(items, |_| blank())
    }

    /// Like [`PageEdit::apply`], for a list in which the entry of a copy is made from the entry of the page it is a copy
    /// of: `fresh` is given that entry for the entry of a copy, and nothing for the entry of a blank page.
    pub fn apply_with<T>(&self, items: &mut Vec<T>, mut fresh: impl FnMut(Option<&T>) -> T) -> bool {
        let count = items.len();
        let Some(order) = self.order(count) else { return false };
        let copies = self.copies(count);
        let sources: Vec<Option<usize>> = (0..order.len()).map(|new| copies.iter().find(|c| c.0 == new).map(|c| c.1)).collect();
        // (A copy's source entry is looked at before the entries are moved out.)
        let made: Vec<Option<T>> = sources.iter().map(|s| s.map(|from| fresh(items.get(from)))).collect();
        let mut old: Vec<Option<T>> = items.drain(..).map(Some).collect();
        let mut made = made.into_iter();
        items.extend(order.iter().map(|from| {
            let copy = made.next().flatten();
            match from {
                Some(p) => old[*p].take().expect("a page is taken once"),
                None => copy.unwrap_or_else(|| fresh(None)),
            }
        }));
        true
    }

    /// The sizes of the pages the edit makes, in the order they stand in the result.
    pub fn new_page_sizes(&self) -> Vec<[f32; 2]> {
        match self {
            PageEdit::Insert { size, .. } => vec![*size],
            PageEdit::Restore { pages, .. } => {
                let mut sized = pages.clone();
                sized.sort_by_key(|p| p.0);
                sized.into_iter().map(|p| p.1).collect()
            }
            PageEdit::Delete { .. } | PageEdit::Move { .. } | PageEdit::Arrange { .. } | PageEdit::Duplicate { .. } => Vec::new(),
        }
    }

    /// For an edit that copies pages: each copy's place in the result and the page it is a copy of.
    pub fn copies(&self, count: usize) -> Vec<(usize, usize)> {
        let PageEdit::Duplicate { pages } = self else { return Vec::new() };
        let Some(copied) = unique(pages, count) else { return Vec::new() };
        let Some(&last) = copied.last() else { return Vec::new() };
        copied.into_iter().enumerate().map(|(k, from)| (last + 1 + k, from)).collect()
    }
}

/// The pages in `pages`, once each and in order; `None` if one of them is not among `count` pages.
fn unique(pages: &[usize], count: usize) -> Option<Vec<usize>> {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.iter().all(|&p| p < count).then_some(sorted)
}

impl Layout {
    /// The layout after `edit`, or `None` if the edit cannot be done to this many pages.
    pub fn edited(&self, edit: &PageEdit) -> Option<Layout> {
        let mut sizes: Vec<[f32; 2]> = (0..self.len()).filter_map(|p| self.page(p)).map(|r| [r.w, r.h]).collect();
        let mut new = edit.new_page_sizes().into_iter();
        edit.apply_with(&mut sizes, |source| source.copied().unwrap_or_else(|| new.next().unwrap_or([0.0; 2]))).then(|| Layout::new(&sizes))
    }
}

/// Where a page went, and how far its place moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moved {
    pub page: usize,
    /// How far the page's top left corner moved in document space, so that what is on it moves as far.
    pub dx: f32,
    pub dy: f32,
}

/// What an edit did to each page there was.
#[derive(Clone, Debug, PartialEq)]
pub struct Remap {
    /// Each page before the edit: where it is now, or `None` if it went.
    pages: Vec<Option<Moved>>,
    /// The pages that are new.
    added: Vec<usize>,
    /// The pages that are copies, and where each is a copy of.
    copies: Vec<Copied>,
    /// How many pages there are now.
    len: usize,
}

/// A page that is a copy of another: the number it has now, the number the page it is a copy of had before, and how far
/// from that page's old place the copy's is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Copied {
    pub page: usize,
    pub from: usize,
    pub dx: f32,
    pub dy: f32,
}

impl Remap {
    /// What `edit` did to a document laid out as `before`, which is `after` now (see [`Layout::edited`]).
    pub fn new(edit: &PageEdit, before: &Layout, after: &Layout) -> Option<Remap> {
        let order = edit.order(before.len())?;
        if order.len() != after.len() {
            return None;
        }
        let mut pages = vec![None; before.len()];
        let mut added = Vec::new();
        for (page, from) in order.iter().enumerate() {
            match from {
                Some(old) => {
                    let (was, now) = (before.page(*old)?, after.page(page)?);
                    pages[*old] = Some(Moved { page, dx: now.x - was.x, dy: now.y - was.y });
                }
                None => added.push(page),
            }
        }
        let copies = edit
            .copies(before.len())
            .into_iter()
            .map(|(page, from)| {
                let (was, now) = (before.page(from)?, after.page(page)?);
                Some(Copied { page, from, dx: now.x - was.x, dy: now.y - was.y })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Remap { pages, added, copies, len: after.len() })
    }

    /// Where page `old` is now; `None` if it went (or was never there).
    pub fn moved(&self, old: usize) -> Option<Moved> {
        self.pages.get(old).copied().flatten()
    }

    /// The number page `old` has now.
    pub fn page(&self, old: usize) -> Option<usize> {
        self.moved(old).map(|m| m.page)
    }

    /// The pages that are new, by their numbers now.
    pub fn added(&self) -> &[usize] {
        &self.added
    }

    /// The pages that are copies of others.
    pub fn copies(&self) -> &[Copied] {
        &self.copies
    }

    /// The pages that stayed, by their numbers before: each with where it is now.
    pub fn survivors(&self) -> impl Iterator<Item = (usize, Moved)> + '_ {
        self.pages.iter().enumerate().filter_map(|(old, m)| m.map(|m| (old, m)))
    }

    /// The pages (by their numbers now) that a change like this leaves to be written: the new ones, the one that took the
    /// place of a page that went, and, for pages that only changed places, the first that did. There is always one.
    pub fn touched(&self) -> Vec<usize> {
        let mut touched = self.added.clone();
        if let Some(gone) = self.gone().next() {
            touched.push(gone.min(self.len.saturating_sub(1)));
        } else if self.added.is_empty()
            && let Some((_, moved)) = self.survivors().find(|(old, m)| m.page != *old)
        {
            touched.push(moved.page);
        }
        touched
    }

    /// The pages that went, by their numbers before.
    pub fn gone(&self) -> impl Iterator<Item = usize> + '_ {
        self.pages.iter().enumerate().filter(|(_, m)| m.is_none()).map(|(old, _)| old)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: [f32; 2] = [600.0, 800.0];

    fn order(edit: &PageEdit, count: usize) -> Option<Vec<Option<usize>>> {
        edit.order(count)
    }

    #[test]
    fn an_insert_makes_a_new_page_where_it_says_and_the_pages_after_it_move_down() {
        let edit = PageEdit::Insert { at: 1, size: A4 };
        assert_eq!(order(&edit, 3), Some(vec![Some(0), None, Some(1), Some(2)]));
        assert_eq!(order(&PageEdit::Insert { at: 3, size: A4 }, 3), Some(vec![Some(0), Some(1), Some(2), None]), "at the end");
        assert_eq!(order(&PageEdit::Insert { at: 0, size: A4 }, 1), Some(vec![None, Some(0)]), "at the front");
        assert_eq!(order(&PageEdit::Insert { at: 4, size: A4 }, 3), None, "not beyond the end");
        assert_eq!(order(&PageEdit::Insert { at: 0, size: [0.0, 5.0] }, 3), None, "a page has a size");
        assert_eq!(order(&PageEdit::Insert { at: 0, size: [f32::NAN, 5.0] }, 3), None);
        assert_eq!(order(&PageEdit::Insert { at: 0, size: A4 }, MAX_PAGES), None, "and there is a most");
    }

    #[test]
    fn a_delete_drops_the_pages_named_once_each_and_never_all_of_them() {
        assert_eq!(order(&PageEdit::delete(1), 3), Some(vec![Some(0), Some(2)]));
        assert_eq!(order(&PageEdit::Delete { pages: vec![2, 0, 2], grave: 0 }, 4), Some(vec![Some(1), Some(3)]));
        assert_eq!(order(&PageEdit::delete(3), 3), None, "there is no such page");
        assert_eq!(order(&PageEdit::delete(0), 1), None, "a document has a page");
        assert_eq!(order(&PageEdit::Delete { pages: vec![0, 1], grave: 0 }, 2), None, "not all of them");
        assert_eq!(order(&PageEdit::Delete { pages: vec![], grave: 0 }, 2), None, "and none is not an edit");
    }

    #[test]
    fn a_move_takes_pages_out_and_puts_them_back_together_in_their_order() {
        let moved = |pages: &[usize], to| {
            order(&PageEdit::Move { pages: pages.to_vec(), to }, 5).map(|o| o.into_iter().map(Option::unwrap).collect::<Vec<_>>())
        };
        assert_eq!(moved(&[0], 2), Some(vec![1, 2, 0, 3, 4]), "one page later");
        assert_eq!(moved(&[3], 0), Some(vec![3, 0, 1, 2, 4]), "one page to the front");
        assert_eq!(moved(&[0], 4), Some(vec![1, 2, 3, 4, 0]), "to the end");
        assert_eq!(moved(&[3, 1], 0), Some(vec![1, 3, 0, 2, 4]), "two together, in the order they had");
        assert_eq!(moved(&[1, 2], 1), None, "back where they were is nothing done");
        assert_eq!(moved(&[0], 5), None, "to is counted among the pages that stay");
        assert_eq!(moved(&[5], 0), None, "no such page");
        assert_eq!(moved(&[], 0), None);
        assert_eq!(moved(&[0, 1, 2, 3, 4], 0), None, "all of them cannot go anywhere");
    }

    #[test]
    fn an_arrangement_must_be_a_permutation_that_changes_something_and_a_restore_puts_pages_at_places_of_the_result() {
        assert_eq!(order(&PageEdit::Arrange { order: vec![2, 0, 1] }, 3), Some(vec![Some(2), Some(0), Some(1)]));
        assert_eq!(order(&PageEdit::Arrange { order: vec![0, 1, 2] }, 3), None, "nothing changes");
        assert_eq!(order(&PageEdit::Arrange { order: vec![0, 0, 2] }, 3), None, "a page twice");
        assert_eq!(order(&PageEdit::Arrange { order: vec![0, 1] }, 3), None, "a page missing");
        assert_eq!(order(&PageEdit::Arrange { order: vec![0, 1, 5] }, 3), None, "a page that is not there");
        let back = PageEdit::Restore { grave: 7, pages: vec![(3, A4), (1, A4)] };
        assert_eq!(order(&back, 3), Some(vec![Some(0), None, Some(1), None, Some(2)]));
        assert_eq!(order(&PageEdit::Restore { grave: 7, pages: vec![(5, A4)] }, 3), None, "beyond the end of the result");
        assert_eq!(order(&PageEdit::Restore { grave: 7, pages: vec![(1, A4), (1, A4)] }, 3), None, "two on one place");
        assert_eq!(order(&PageEdit::Restore { grave: 7, pages: vec![] }, 3), None);
        assert_eq!(back.new_page_sizes().len(), 2);
    }

    #[test]
    fn every_edit_is_undone_by_its_inverse_and_the_inverse_by_the_edit() {
        let sizes = [[600.0, 800.0], [300.0, 400.0], [500.0, 700.0], [100.0, 200.0]];
        let layout = Layout::new(&sizes);
        let edits = [
            PageEdit::Insert { at: 2, size: [50.0, 60.0] },
            PageEdit::Insert { at: 4, size: [50.0, 60.0] },
            PageEdit::Delete { pages: vec![1, 3], grave: 9 },
            PageEdit::Move { pages: vec![3, 0], to: 1 },
            PageEdit::Arrange { order: vec![3, 1, 0, 2] },
        ];
        for edit in edits {
            let after = layout.edited(&edit).expect("layout");
            let undo = edit.inverse(&layout).expect("inverse");
            let restored = after.edited(&undo).unwrap_or_else(|| panic!("{undo:?} cannot be done after {edit:?}"));
            assert_eq!(restored.len(), layout.len(), "{edit:?}");
            for p in 0..layout.len() {
                assert_eq!(restored.page(p), layout.page(p), "page {p} after {edit:?} and its undoing");
            }
            // And the undoing is undone by what it is the inverse of (as a redo is).
            let redo = undo.inverse(&after).unwrap_or_else(|| panic!("no inverse of {undo:?}"));
            let again = restored.edited(&redo).unwrap_or_else(|| panic!("{redo:?} cannot be done"));
            for p in 0..after.len() {
                assert_eq!(again.page(p), after.page(p), "page {p} after {edit:?}, undone and redone");
            }
        }
        // A deleted page comes back with the size it had, under the same name for the grave.
        let PageEdit::Restore { grave, pages } = PageEdit::Delete { pages: vec![1], grave: 4 }.inverse(&layout).expect("inverse") else {
            panic!("not a restore")
        };
        assert_eq!((grave, pages), (4, vec![(1, [300.0, 400.0])]));
        assert!(PageEdit::delete(9).inverse(&layout).is_none());
    }

    #[test]
    fn a_duplicate_puts_copies_together_after_the_last_page_copied() {
        let dup = |pages: &[usize]| PageEdit::Duplicate { pages: pages.to_vec() };
        assert_eq!(order(&dup(&[1]), 4), Some(vec![Some(0), Some(1), None, Some(2), Some(3)]));
        assert_eq!(
            order(&dup(&[3, 1]), 5),
            Some(vec![Some(0), Some(1), Some(2), Some(3), None, None, Some(4)]),
            "both copies after the last, in order"
        );
        assert_eq!(order(&dup(&[2, 2]), 3), Some(vec![Some(0), Some(1), Some(2), None]), "a page counts once");
        assert_eq!(order(&dup(&[]), 3), None);
        assert_eq!(order(&dup(&[3]), 3), None, "no such page");
        assert_eq!(dup(&[3, 1]).copies(5), vec![(4, 1), (5, 3)], "where each copy is and what it copies");
        assert!(dup(&[0]).copies(0).is_empty());
        let layout = Layout::new(&[[100.0, 200.0], [300.0, 400.0], [500.0, 600.0]]);
        let after = layout.edited(&dup(&[0, 2])).expect("layout");
        assert_eq!(
            (0..5).map(|p| after.page(p).map(|r| r.w)).collect::<Vec<_>>(),
            [Some(100.0), Some(300.0), Some(500.0), Some(100.0), Some(500.0)],
            "a copy is as big as its page"
        );
        let undo = dup(&[0, 2]).inverse(&layout).expect("inverse");
        assert_eq!(undo, PageEdit::Delete { pages: vec![3, 4], grave: 0 }, "the copies go to undo it");
        assert_eq!(after.edited(&undo).expect("layout").len(), 3);
        let map = Remap::new(&dup(&[0, 2]), &layout, &after).expect("remap");
        assert_eq!(map.added(), [3, 4]);
        assert_eq!(map.copies().iter().map(|c| (c.page, c.from)).collect::<Vec<_>>(), [(3, 0), (4, 2)]);
        let c = map.copies()[0];
        assert!(
            ((c.dy) - (after.page(3).expect("page").y - layout.page(0).expect("page").y)).abs() < 1e-3 && c.dx.abs() < 1e-3,
            "{c:?}: the copy's place, from the old page's"
        );
        let mut names = vec!["a", "b", "c"];
        assert!(dup(&[0, 2]).apply_with(&mut names, |source| source.copied().unwrap_or("new")));
        assert_eq!(names, ["a", "b", "c", "a", "c"], "an entry of a copy is made from its page's");
    }

    #[test]
    fn apply_carries_any_list_kept_by_page_through_an_edit() {
        let mut names = vec!["a", "b", "c", "d"];
        assert!(PageEdit::Insert { at: 2, size: A4 }.apply(&mut names, || "new"));
        assert_eq!(names, ["a", "b", "new", "c", "d"]);
        assert!(PageEdit::Delete { pages: vec![0, 3], grave: 0 }.apply(&mut names, || unreachable!()));
        assert_eq!(names, ["b", "new", "d"]);
        assert!(PageEdit::Move { pages: vec![2], to: 0 }.apply(&mut names, || unreachable!()));
        assert_eq!(names, ["d", "b", "new"]);
        let before = names.clone();
        assert!(!PageEdit::delete(9).apply(&mut names, || "x"), "an edit that cannot be done does nothing");
        assert_eq!(names, before);
    }

    #[test]
    fn the_layout_after_an_edit_is_the_layout_of_the_pages_after_it() {
        let before = Layout::new(&[[600.0, 800.0], [300.0, 400.0], [500.0, 700.0]]);
        let moved = before.edited(&PageEdit::Move { pages: vec![2], to: 0 }).expect("layout");
        assert_eq!((0..3).map(|p| moved.page(p).expect("page").h).collect::<Vec<_>>(), [700.0, 800.0, 400.0]);
        let longer = before.edited(&PageEdit::Insert { at: 1, size: [100.0, 100.0] }).expect("layout");
        assert_eq!(longer.len(), 4);
        assert_eq!(longer.page(1).map(|r| (r.w, r.h)), Some((100.0, 100.0)));
        assert!(before.edited(&PageEdit::delete(7)).is_none());
        // Laid out from the sizes, it is the layout the sizes make.
        let sizes = [[600.0, 800.0], [500.0, 700.0]];
        assert_eq!(before.edited(&PageEdit::delete(1)).expect("layout").page(1), Layout::new(&sizes).page(1));
    }

    #[test]
    fn a_remap_says_where_each_page_went_and_how_far_its_place_moved() {
        let before = Layout::new(&[[600.0, 800.0], [600.0, 800.0], [600.0, 800.0]]);
        let edit = PageEdit::Insert { at: 1, size: [600.0, 800.0] };
        let after = before.edited(&edit).expect("layout");
        let remap = Remap::new(&edit, &before, &after).expect("remap");
        assert_eq!((remap.page(0), remap.page(1), remap.page(2), remap.page(3)), (Some(0), Some(2), Some(3), None));
        assert_eq!(remap.added(), [1]);
        assert_eq!(remap.len, 4, "how many pages there are now");
        let gap = 800.0 + crate::view::PAGE_GAP;
        let m = remap.moved(1).expect("moved");
        assert!((m.dy - gap).abs() < 1e-3 && m.dx.abs() < 1e-3, "{m:?}");
        assert_eq!(remap.moved(0).map(|m| m.dy), Some(0.0), "the page before did not move");

        let edit = PageEdit::Delete { pages: vec![0], grave: 0 };
        let after = before.edited(&edit).expect("layout");
        let remap = Remap::new(&edit, &before, &after).expect("remap");
        assert_eq!((remap.page(0), remap.page(1), remap.gone().collect::<Vec<_>>()), (None, Some(0), vec![0]));
        assert!((remap.moved(2).expect("moved").dy + gap).abs() < 1e-3, "one page up");
        assert_eq!(remap.touched(), vec![0], "the page that took the deleted one's place is what is left to write");
    }

    #[test]
    fn pages_of_other_widths_move_sideways_as_the_layout_centres_them() {
        let before = Layout::new(&[[600.0, 800.0], [300.0, 400.0]]);
        let edit = PageEdit::Move { pages: vec![1], to: 0 };
        let after = before.edited(&edit).expect("layout");
        let remap = Remap::new(&edit, &before, &after).expect("remap");
        let narrow = remap.moved(1).expect("moved");
        assert_eq!(narrow.page, 0);
        assert!(narrow.dx.abs() < 1e-3 && narrow.dy < 0.0, "centred either way, now at the top: {narrow:?}");
        let wide = remap.moved(0).expect("moved");
        assert!(wide.dy > 0.0, "{wide:?}");
    }

    #[test]
    fn a_remap_of_an_edit_that_does_not_match_the_layouts_is_refused() {
        let before = Layout::new(&[A4, A4]);
        let edit = PageEdit::delete(0);
        assert!(Remap::new(&edit, &before, &before).is_none(), "the layout after has as many pages as before");
        assert!(Remap::new(&PageEdit::delete(5), &before, &before).is_none());
    }
}
