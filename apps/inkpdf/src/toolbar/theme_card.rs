//! The round button that hangs from the theme button once it is pressed: whether the pages follow the look (dark pages). It is one circle
//! with a picture on it (a moon on a page), in the accent colour while it is on, floating just under the theme button. The editor says
//! when it is shown and builds a [`ThemeView`]; the shell draws what this makes.

use super::{Item, Toolbar, Ui, icons::icons};

/// What the button is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeView {
    /// The pages follow the look (the setting; it only does something while the look is dark).
    pub pages_follow: bool,
    /// The look is dark now.
    pub dark: bool,
}

/// Where the button is, in window pixels.
#[derive(Clone, Copy, Debug)]
pub struct ThemeLayout {
    /// The circle's box.
    pub rect: [f32; 4],
    px: f32,
}

impl ThemeLayout {
    /// The button under the theme button: as big as it, a little below its pill, kept inside the window.
    pub fn new(toolbar: &Toolbar, viewport: [f32; 2]) -> ThemeLayout {
        let px = toolbar.dpr;
        let theme = toolbar.items().iter().find(|(item, _)| *item == Item::Theme).map_or([0.0; 4], |&(_, r)| r);
        let centre = [(theme[0] + theme[2]) / 2.0, (theme[1] + theme[3]) / 2.0];
        let pill_bottom = toolbar.pills.iter().find(|p| crate::ink::rect_contains(**p, centre)).map_or(theme[3], |p| p[3]);
        let d = (toolbar.unit * 1.5).clamp(34.0 * px, 52.0 * px);
        let x0 = (centre[0] - d / 2.0).clamp(4.0 * px, (viewport[0] - d - 4.0 * px).max(4.0 * px));
        let y0 = pill_bottom + 8.0 * px;
        ThemeLayout { rect: [x0, y0, x0 + d, y0 + d], px }
    }

    /// Whether `p` is on the button.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let r = self.rect;
        let (cx, cy, radius) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0, (r[2] - r[0]) / 2.0);
        (p[0] - cx).hypot(p[1] - cy) <= radius
    }

    pub fn centre(&self) -> [f32; 2] {
        [(self.rect[0] + self.rect[2]) / 2.0, (self.rect[1] + self.rect[3]) / 2.0]
    }

    pub fn draw(&self, view: &ThemeView, ui: &mut Ui) {
        let d = self.rect[2] - self.rect[0];
        ui.floating(self.rect, d / 2.0, self.px);
        let [cx, cy] = self.centre();
        let color = if view.pages_follow { ui.pal.accent } else { ui.pal.ink };
        icons().dark_pages.draw(&mut ui.lines, cx, cy, 0.3 * d, (0.045 * d).max(1.4 * self.px), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolbar::{Pill, Toolbar};

    /// The toolbar with the look's pill open if it is folded (the button is shown only while the theme button can be pressed).
    fn bar(viewport: [f32; 2]) -> Toolbar {
        Toolbar::layout_for(&crate::tool::Registry::builtin(), viewport, 1.0, Some(Pill::View))
    }

    #[test]
    fn the_button_is_a_circle_under_the_theme_button_inside_the_window_at_every_size() {
        for viewport in [[1600.0, 900.0], [1100.0, 800.0], [470.0, 940.0], [320.0, 600.0]] {
            let bar = bar(viewport);
            let l = ThemeLayout::new(&bar, viewport);
            let r = l.rect;
            assert!(r[0] >= 0.0 && r[2] <= viewport[0] && r[3] <= viewport[1], "{viewport:?}: {r:?}");
            assert!((r[2] - r[0] - (r[3] - r[1])).abs() < 0.01, "round");
            assert!(l.contains(l.centre()) && !l.contains([r[0], r[1]]), "a circle: its corner is not on it");
            assert!(!bar.contains(l.centre()), "{viewport:?}: it hangs below the bar, not over it");
        }
    }

    #[test]
    fn it_is_drawn_in_the_accent_when_the_pages_follow_and_in_the_ink_when_they_do_not_and_the_dark_card_uses_no_light_colour() {
        let viewport = [1100.0, 900.0];
        let l = ThemeLayout::new(&bar(viewport), viewport);
        let draw = |view: ThemeView| {
            let mut ui = Ui::default();
            l.draw(&view, &mut ui);
            ui
        };
        let (off, on) = (draw(ThemeView { pages_follow: false, dark: false }), draw(ThemeView { pages_follow: true, dark: true }));
        assert!(!off.lines.is_empty() && off.lines.iter().all(|c| c.color == off.pal.ink));
        assert!(on.lines.iter().all(|c| c.color == on.pal.accent));
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&ThemeView { pages_follow: true, dark: true }, &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "the panel is the dark scheme's");
    }
}
