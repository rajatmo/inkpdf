//! The card under the theme button: whether the pages follow the look. The button cycles the look and opens it; a press on its row switches
//! the setting; a press anywhere else (or Esc, or another button) puts it away. Every device does all of it with a press.

use crate::toolbar::{Item, ThemeLayout, ThemeView};

use super::Editor;

impl Editor {
    #[cfg(test)]
    pub fn theme_menu_open(&self) -> bool {
        self.chrome.theme_menu
    }

    /// The card, where it is, if it is open (not while a prompt has the window).
    pub fn theme_layout(&self) -> Option<ThemeLayout> {
        (self.chrome.theme_menu && self.chrome.prompt.is_none()).then(|| ThemeLayout::new(&self.toolbar(), self.ws.view.viewport))
    }

    /// What the card is drawn from.
    pub fn theme_view(&self) -> Option<ThemeView> {
        self.chrome.theme_menu.then(|| ThemeView { pages_follow: self.ws.dark_pages, dark: self.scheme() == crate::palette::Scheme::Dark })
    }

    pub(super) fn open_theme_menu(&mut self) {
        if !self.chrome.theme_menu {
            self.chrome.theme_menu = true;
            self.ws.redraw = true;
        }
    }

    pub fn close_theme_menu(&mut self) {
        if self.chrome.theme_menu {
            self.chrome.theme_menu = false;
            self.ws.redraw = true;
        }
    }

    /// A press is beginning at `pos`: the card goes unless the press is on it or on the button that opens it.
    pub(super) fn dismiss_theme_menu(&mut self, pos: [f32; 2]) {
        if !self.chrome.theme_menu {
            return;
        }
        let on_card = self.theme_layout().is_some_and(|c| c.contains(pos));
        if !on_card && self.toolbar().hit(pos) != Some(Item::Theme) {
            self.close_theme_menu();
        }
    }

    /// The row was pressed: the pages follow the look, or do not.
    pub(super) fn press_theme_row(&mut self) {
        self.set_dark_pages(!self.dark_pages());
    }
}
