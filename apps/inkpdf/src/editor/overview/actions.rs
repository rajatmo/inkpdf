//! What the pages overview's buttons do, and how it follows an edit of the pages (the slide, the scroll back into range).

use super::*;

impl Editor {
    pub(super) fn overview_activate(&mut self, on: Hover) {
        match on {
            Hover::Control(Control::Close) => self.close_overview(),
            Hover::Control(Control::Add) => {
                let current = self.ws.view.current_page(&self.ws.layout);
                if self.insert_page_after(current) {
                    self.overview_show_page(current + 1);
                }
            }
            Hover::Control(Control::Edit) => {
                if let Some(state) = &mut self.chrome.overview {
                    state.edit = !state.edit;
                    state.selected.clear();
                }
                self.overview_settled();
            }
            Hover::Control(Control::Undo) => {
                self.undo();
                self.overview_settled();
            }
            Hover::Control(Control::Redo) => {
                self.redo();
                self.overview_settled();
            }
            Hover::Action(action) => {
                self.apply_action(action);
            }
            Hover::Number(page) => {
                if let Some(state) = &mut self.chrome.overview {
                    state.menu = if state.menu == Some(page) { None } else { Some(page) };
                }
            }
            // (A menu item is done when the menu is open, where a press is taken first: see `do_menu_item`.)
            Hover::Menu(_) => {}
            Hover::Page(page) => {
                // In edit mode a tap chooses the page, or lets go of it; otherwise it goes there.
                match &mut self.chrome.overview {
                    Some(state) if state.edit => {
                        if !state.selected.remove(&page) {
                            state.selected.insert(page);
                        }
                    }
                    _ => self.overview_go_to(page),
                }
            }
            Hover::Toc(TocHit::Title(k)) => {
                let target = self.toc_rows().get(k).and_then(|row| self.outline[row.item].page);
                if let Some(page) = target {
                    self.overview_go_to(page);
                }
            }
            Hover::Toc(TocHit::Arrow(k)) => {
                if let Some(row) = self.toc_rows().get(k).copied()
                    && let Some(state) = &mut self.chrome.overview
                {
                    if !state.folded.remove(&row.item) {
                        state.folded.insert(row.item);
                    }
                    state.rows = std::cell::OnceCell::new();
                }
            }
        }
        self.ws.redraw = true;
    }

    /// Do an item of the menu of the page it is open for, and put the menu away.
    pub(super) fn do_menu_item(&mut self, item: MenuItem) {
        let Some(page) = self.chrome.overview.as_mut().and_then(|o| o.menu.take()) else { return };
        match item {
            MenuItem::Before => {
                if let Some(like) = self.ws.layout.page(page)
                    && self.edit_pages(PageEdit::Insert { at: page, size: [like.w, like.h] })
                {
                    self.overview_show_page(page);
                }
            }
            MenuItem::After => {
                if self.insert_page_after(page) {
                    self.overview_show_page(page + 1);
                }
            }
            MenuItem::Duplicate => {
                if self.edit_pages(PageEdit::Duplicate { pages: vec![page] }) {
                    self.overview_show_page(page + 1);
                }
            }
            MenuItem::Delete => {
                self.edit_pages(PageEdit::delete(page));
            }
        }
        self.ws.redraw = true;
    }

    /// Do `action` to the chosen pages (or choose them all). `true` if it did something.
    pub(super) fn apply_action(&mut self, action: Action) -> bool {
        let Some(state) = &self.chrome.overview else { return false };
        let enabled = self.overview_enabled();
        let index = Action::ALL.iter().position(|a| *a == action).unwrap_or(0);
        if !enabled[index] {
            return false;
        }
        let chosen: Vec<usize> = state.selected.iter().copied().collect();
        let (count, first) = (self.ws.layout.len(), chosen.first().copied().unwrap_or(0));
        match action {
            Action::SelectAll => {
                if let Some(state) = &mut self.chrome.overview {
                    state.selected = if chosen.len() == count { BTreeSet::new() } else { (0..count).collect() };
                }
                true
            }
            Action::Delete => self.edit_pages(PageEdit::Delete { pages: chosen, grave: 0 }),
            Action::Duplicate => {
                let edit = PageEdit::Duplicate { pages: chosen };
                let copies: Vec<usize> = edit.copies(count).into_iter().map(|(page, _)| page).collect();
                let done = self.edit_pages(edit);
                if done && let Some(state) = &mut self.chrome.overview {
                    // The copies are the pages chosen now, ready to be moved.
                    state.selected = copies.iter().copied().collect();
                    if let Some(&page) = copies.first() {
                        self.overview_show_page(page);
                    }
                }
                done
            }
            Action::Earlier | Action::Later => {
                let to = if action == Action::Earlier { first - 1 } else { first + 1 };
                let done = self.edit_pages(PageEdit::Move { pages: chosen, to });
                if done {
                    self.overview_show_page(to);
                }
                done
            }
        }
    }

    /// Look at `page` in the document, and put the overview away.
    pub(super) fn overview_go_to(&mut self, page: usize) {
        self.close_overview();
        self.ws.scroll_to_page(page);
    }

    /// Scroll the pages so that `page` is in view.
    pub(super) fn overview_show_page(&mut self, page: usize) {
        let layout = self.overview_layout();
        let count = self.ws.layout.len();
        if let Some(state) = &mut self.chrome.overview {
            state.goal = None;
            state.scroll = layout.show(page, count, state.scroll);
        }
    }

    /// After the pages changed while the overview is up (a page added, or an undo that put pages back or took them
    /// away), or after its mode changed: what it keeps still makes sense. `map` is what the change did to the pages, if it
    /// was one: the pages chosen go where their pages went.
    pub(in crate::editor) fn overview_follow(&mut self, map: Option<&Remap>, before: usize) {
        let was = self.chrome.overview.as_ref().map(|s| s.scroll);
        if let (Some(state), Some(map)) = (&mut self.chrome.overview, map) {
            state.selected = state.selected.iter().filter_map(|&p| map.page(p)).collect();
            state.ticked = state.selected.clone();
            state.ticks = Slides::default();
        }
        self.overview_settled();
        if let (Some(map), Some(was)) = (map, was) {
            self.overview_slide(map, before, was);
        }
    }

    /// The pages of a change made while the overview is up go from where their pictures were to where they are: a page
    /// that moved slides, a new one (or a copy) grows in.
    pub(super) fn overview_slide(&mut self, map: &Remap, before: usize, was: f32) {
        let layout = self.overview_layout();
        let count = self.ws.layout.len();
        let prefs = self.motions.prefs();
        let Some(state) = &mut self.chrome.overview else { return };
        let now = state.scroll;
        state.slides.rekey(|page| map.page(page));
        state.grows.rekey(|page| map.page(page));
        for old in layout.visible_pages(before, was) {
            if let Some(new) = map.page(old) {
                let (from, to) = (layout.cell(old, was), layout.cell(new, now));
                state.slides.start(new, [from[0] - to[0], from[1] - to[1]], prefs.time(seconds::MEDIUM));
            }
        }
        let new_pages = map.added().iter().copied().chain(map.copies().iter().map(|c| c.page));
        for page in new_pages.filter(|&p| p < count) {
            state.grows.start(page, [1.0, 0.0], prefs.time(seconds::MEDIUM));
        }
        self.ws.redraw = true;
    }

    /// What the overview keeps is brought back into range.
    pub(in crate::editor) fn overview_settled(&mut self) {
        let layout = self.overview_layout();
        let (pages, rows) = (self.ws.layout.len(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.goal = state.goal.map(|g| layout.clamp_scroll(g, pages));
            state.toc_goal = state.toc_goal.map(|g| layout.clamp_toc_scroll(g, rows));
            state.scroll = layout.clamp_scroll(state.scroll, pages);
            state.toc_scroll = layout.clamp_toc_scroll(state.toc_scroll, rows);
            state.selected.retain(|&p| p < pages);
            if !state.edit {
                state.selected.clear();
            }
            state.press = None;
            state.drag = None;
            // (A menu is for a page: it goes with a change of the pages.)
            state.menu = None;
        }
    }
}
