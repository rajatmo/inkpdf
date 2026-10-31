//! Editing the pages: adding, deleting and rearranging them, and everything that goes with a page when it moves.
//!
//! Every change to the pages is a [`PageEdit`], done here and nowhere else: the ink and the history take it (as one
//! step, so it can be undone), the links, the selection, the layout and the view follow, and the shell is asked, with
//! `Request::PagesEdited`, to do the same to the file's page table and what it has drawn. Undoing and redoing a step of
//! the history that was one comes back through the same follow-up.

use crate::links::{self, Link};
use crate::outline;
use crate::pages::{PageEdit, Remap};
use crate::requests::Request;
use crate::view::Layout;

use super::{Act, Editor};

/// What a deletion took with the pages besides their ink, kept so that undoing it can give it back.
#[derive(Default)]
pub(super) struct Grave {
    /// The links on the pages and to them, each with its place in the list.
    links: Vec<(usize, Link)>,
    /// The entries of the outline that went to them, each with the page it was.
    outline: Vec<(usize, usize)>,
}

impl Editor {
    /// Do `edit` to the pages as one step of the history. `false`, and nothing done, if it cannot be done to this
    /// document (a page that is not there, none left, too many).
    pub fn edit_pages(&mut self, mut edit: PageEdit) -> bool {
        // A deleted page is kept, under a name of its own, so that undoing the deletion can bring it back.
        if let PageEdit::Delete { grave, .. } = &mut edit
            && *grave == 0
        {
            self.grave += 1;
            *grave = self.grave;
        }
        let before = self.ws.layout.clone();
        let Some(after) = before.edited(&edit) else { return false };
        let (Some(undo), Some(forward)) = (edit.inverse(&before), Remap::new(&edit, &before, &after)) else { return false };
        let Some(backward) = Remap::new(&undo, &after, &before) else { return false };
        self.stop_what_was_under_way();
        self.ws.ink.edit_pages(edit.clone(), undo, forward, backward);
        // A deletion the history has now let go of cannot be undone: what was kept for it goes, here and in the document.
        let forgotten = self.ws.ink.take_evicted_graves();
        if !forgotten.is_empty() {
            for grave in &forgotten {
                self.graves.remove(grave);
            }
            self.ws.requests.push(Request::GravesForgotten(forgotten));
        }
        self.pages_followed(&edit, &before, after);
        true
    }

    /// A blank page after page `after`, as big as it.
    pub fn insert_page_after(&mut self, after: usize) -> bool {
        let Some(like) = self.ws.layout.page(after) else { return false };
        self.edit_pages(PageEdit::Insert { at: after + 1, size: [like.w, like.h] })
    }

    /// A blank page after the last, as big as it.
    pub fn add_page(&mut self) -> bool {
        match self.ws.layout.len() {
            0 => false,
            n => self.insert_page_after(n - 1),
        }
    }

    /// The page being looked at goes (the last page there is stays).
    pub fn delete_current_page(&mut self) -> bool {
        let page = self.ws.view.current_page(&self.ws.layout);
        self.edit_pages(PageEdit::delete(page))
    }

    /// If the step of the history that was just undone or redone was an edit of the pages, the rest of the document
    /// follows it (the ink did already, as part of the step).
    pub(super) fn page_step_followed(&mut self) {
        let Some(edit) = self.ws.ink.take_page_edit() else { return };
        self.stop_what_was_under_way();
        let before = self.ws.layout.clone();
        if let Some(after) = before.edited(&edit) {
            self.pages_followed(&edit, &before, after);
        }
    }

    /// The table of contents the file came with: its flags are the ones the person made (see [`outline::FLAG`]) and are kept apart from it.
    pub fn set_outline(&mut self, mut items: Vec<crate::outline::OutlineItem>) {
        self.flags = outline::take_flags(&mut items);
        self.outline = items;
    }

    /// Flag page `page`, or take its flag away: the page shows a flag in the overview and the file gets an outline entry for it when it is saved
    /// (so it is a bookmark in any reader). It is a mark, not a step of the history: pressing it again is how it is undone.
    pub fn toggle_flag(&mut self, page: usize) -> bool {
        if page >= self.ws.layout.len() {
            return false;
        }
        if !self.flags.remove(&page) {
            self.flags.insert(page);
        }
        // (The file has to be written for it: the page counts as changed.)
        self.ws.ink.mark_dirty(page as u32);
        // (With only the flagged pages shown, one taken away leaves the grid: it is brought back in range, and with none left it shows every page.)
        self.overview_settled();
        self.ws.redraw = true;
        true
    }

    /// The links and the outline take `edit`; what a deletion takes is kept, and an undone deletion gives it back.
    fn carry_by_page(&mut self, edit: &PageEdit, before: &Layout, map: &Remap) {
        // The flags go where their pages go (a page that is gone takes its flag with it).
        self.flags = self.flags.iter().filter_map(|&page| map.page(page)).collect();
        match edit {
            PageEdit::Delete { grave, .. } => {
                let links = links::remap(&mut self.links, before, map);
                let outline = outline::remap(&mut self.outline, map);
                // (A page that is only a copy, or blank, has nothing to come back with: its deletion has no grave.)
                if *grave != 0 {
                    self.graves.insert(*grave, Grave { links, outline });
                }
            }
            PageEdit::Duplicate { .. } => {
                let copies = links::copies(&self.links, before, map);
                links::remap(&mut self.links, before, map);
                self.links.extend(copies);
                outline::remap(&mut self.outline, map);
            }
            PageEdit::Restore { grave, .. } => {
                let kept = self.graves.remove(grave).unwrap_or_default();
                links::restore(&mut self.links, before, map, kept.links);
                outline::restore(&mut self.outline, map, &kept.outline);
            }
            PageEdit::Insert { .. } | PageEdit::Move { .. } | PageEdit::Arrange { .. } => {
                links::remap(&mut self.links, before, map);
                outline::remap(&mut self.outline, map);
            }
        }
    }

    /// Whatever the pen or a tool was in the middle of was on the old pages: it ends, and what is typed is kept.
    pub(super) fn stop_what_was_under_way(&mut self) {
        if !matches!(self.chrome.act, Act::Idle) {
            self.end_act();
        }
        self.tools.end_all(&mut self.ws);
        self.settle();
    }

    /// Everything that is kept by page, other than the ink, takes `edit`; the view glides to the page it touched and the
    /// shell is asked to do the same to the file.
    fn pages_followed(&mut self, edit: &PageEdit, before: &Layout, after: Layout) {
        let map = Remap::new(edit, before, &after);
        if let Some(map) = &map {
            self.carry_by_page(edit, before, map);
        }
        let shown = map.as_ref().and_then(|m| m.touched().first().copied()).unwrap_or(0).min(after.len().saturating_sub(1));
        self.ws.layout = after;
        self.ws.prune_selection();
        self.tools.pages_edited(&mut self.ws);
        self.forget_trail();
        self.ws.scroll_to_page(shown);
        self.overview_follow(map.as_ref(), before.len());
        self.ws.requests.push(Request::PagesEdited(edit.clone()));
        self.ws.redraw = true;
    }
}
