//! The round button under the theme button: whether the pages follow the look. The theme button cycles the look and shows it; a press on it
//! switches the setting; a press anywhere else (or Esc, or another button) puts it away. Every device does all of it with a press.

use crate::toolbar::{Item, ThemeLayout, ThemeView};

use super::Editor;

impl Editor {
    #[cfg(test)]
    pub fn theme_menu_open(&self) -> bool {
        self.chrome.theme_menu
    }

    /// The button, where it is, if it is shown (not while a prompt has the window).
    pub fn theme_layout(&self) -> Option<ThemeLayout> {
        // (It hangs from the theme button: with the look's pill folded there is no button to hang from, and nothing is shown.)
        let bar = self.toolbar();
        let hangs = bar.items().iter().any(|(item, _)| *item == Item::Theme);
        let circle = (self.chrome.theme_menu && self.chrome.prompt.is_none() && hangs).then(|| ThemeLayout::new(&bar, self.ws.view.viewport))?;
        // (A bar unfolded over the place it hangs puts it away, until the bar folds again.)
        let under_bar = self.context_bar().is_some_and(|b| {
            let (a, c) = (b.rect, circle.rect);
            !b.is_folded() && a[0] < c[2] && c[0] < a[2] && a[1] < c[3] && c[1] < a[3]
        });
        (!under_bar).then_some(circle)
    }

    /// What the button is drawn from.
    pub fn theme_view(&self) -> Option<ThemeView> {
        self.theme_layout()?;
        self.chrome.theme_menu.then(|| ThemeView { pages_follow: self.ws.look.dark_pages, dark: self.scheme() == crate::palette::Scheme::Dark })
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

    /// A press is beginning at `pos`: the button goes unless the press is on it or on the theme button that shows it.
    pub(super) fn dismiss_theme_menu(&mut self, pos: [f32; 2]) {
        if !self.chrome.theme_menu {
            return;
        }
        let on_button = self.theme_layout().is_some_and(|c| c.contains(pos));
        if !on_button && self.toolbar().hit(pos) != Some(Item::Theme) {
            self.close_theme_menu();
        }
    }
}
