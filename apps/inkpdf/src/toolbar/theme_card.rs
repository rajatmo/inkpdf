//! The card that drops down from the theme button: one choice that belongs to the look, whether the pages follow it (dark pages when the look is
//! dark) or stay as the document has them. A row you press, with a switch in it. Plain data, as the other cards: the editor holds whether
//! it is open and builds a [`ThemeView`], and the shell draws what this makes.

use crate::ink::rect_contains;

use super::Toolbar;
use super::icons::icons;
use super::theme::{Align, Label, Ui};

/// Everything the card's look depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeView {
    /// The pages follow the look (the setting; it only does something while the look is dark).
    pub pages_follow: bool,
    /// The look is dark now.
    pub dark: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ThemeLayout {
    pub card: [f32; 4],
    row: [f32; 4],
    switch: [f32; 4],
    unit: f32,
    px: f32,
}

impl ThemeLayout {
    /// The card under the right pill of `toolbar`, its right edge level with the pill's, in a window of `viewport` pixels.
    pub fn new(toolbar: &Toolbar, viewport: [f32; 2]) -> ThemeLayout {
        let px = toolbar.dpr;
        let u = toolbar.unit.clamp(20.0 * px, 34.0 * px);
        let pill = toolbar.pills[2];
        let width = (11.5 * u).min((viewport[0] - 24.0 * px).max(6.0 * u));
        let x1 = pill[2].max(width);
        let x0 = x1 - width;
        let top = pill[3] + 8.0 * px;
        let (pad, h) = (0.3 * u, 1.9 * u);
        let row = [x0 + pad, top + pad, x1 - pad, top + pad + h];
        let (sw, sh) = (1.7 * u, 0.9 * u);
        let mid = (row[1] + row[3]) / 2.0;
        let switch = [row[2] - 0.3 * u - sw, mid - sh / 2.0, row[2] - 0.3 * u, mid + sh / 2.0];
        ThemeLayout { card: [x0, top, x1, row[3] + pad], row, switch, unit: u, px }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    /// Whether a press is on the row that switches the setting (the whole row is the button).
    pub fn hit(&self, p: [f32; 2]) -> bool {
        rect_contains(self.row, p)
    }

    /// The middle of the row, for tests that press it.
    #[cfg(test)]
    pub fn centre(&self) -> [f32; 2] {
        [(self.row[0] + self.row[2]) / 2.0, (self.row[1] + self.row[3]) / 2.0]
    }

    pub fn draw(&self, view: &ThemeView, ui: &mut Ui) {
        let (u, px) = (self.unit, self.px);
        ui.floating(self.card, 0.4 * u, px);
        let say = |ui: &mut Ui, text: &str, size: f32, color: [u8; 4], at: [f32; 2]| {
            ui.labels.push(Label { text: text.to_string(), size, color, at, align: Align::Left, alpha: 1.0 });
        };
        let mid = (self.row[1] + self.row[3]) / 2.0;
        // The picture of a page half in shade, in the accent colour while it is on.
        let icon_at = [self.row[0] + 0.9 * u, mid];
        let on = view.pages_follow;
        icons().dark_pages.draw(
            &mut ui.lines,
            icon_at[0],
            icon_at[1],
            0.42 * u,
            (u * 0.045).max(1.4 * px),
            if on { ui.pal.accent } else { ui.pal.ink },
        );
        let text_x = self.row[0] + 1.9 * u;
        say(ui, "Pages follow the theme", 0.46 * u, ui.pal.ink, [text_x, mid - 0.3 * u]);
        let hint = if view.dark { "Dark pages, light text" } else { "Matters once the look is dark" };
        say(ui, hint, 0.36 * u, ui.pal.ink_dim, [text_x, mid + 0.36 * u]);
        // The switch: a track that is the accent when on, and a knob at its end.
        let [x0, y0, x1, y1] = self.switch;
        let (h, knob) = (y1 - y0, 0.34 * u);
        ui.panel(self.switch, h / 2.0, if on { ui.pal.accent } else { ui.pal.track });
        let at = if on { x1 - h / 2.0 } else { x0 + h / 2.0 };
        ui.disc([at, (y0 + y1) / 2.0], 2.0 * knob, ui.pal.white);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 5] =
        [([1650.0, 1350.0], 1.5), ([1100.0, 900.0], 1.0), ([800.0, 600.0], 1.0), ([620.0, 800.0], 1.0), ([360.0, 640.0], 1.0)];

    #[test]
    fn the_card_hangs_under_the_right_pill_inside_the_window_and_its_row_is_a_big_target() {
        for (viewport, dpr) in WINDOWS {
            let bar = Toolbar::layout(viewport, dpr);
            let l = ThemeLayout::new(&bar, viewport);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] + 0.5, "in the window at {viewport:?}: {:?}", l.card);
            assert!(l.card[1] > bar.pills[2][3], "below the pill");
            assert!(
                (l.card[2] - bar.pills[2][2]).abs() < 0.5 || l.card[2] - l.card[0] > bar.pills[2][2] - bar.pills[2][0],
                "level with the pill's right edge"
            );
            let r = l.row;
            assert!(r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3]);
            assert!(r[3] - r[1] >= 30.0 * dpr && r[2] - r[0] >= 100.0 * dpr, "a row to press with a stylus tip at {viewport:?}");
            assert!(l.hit(l.centre()) && l.contains(l.centre()));
            assert!(!l.hit([1.0, 1.0]) && !l.contains([1.0, 1.0]));
        }
    }

    #[test]
    fn it_says_what_it_is_and_the_switch_shows_the_setting_in_both_looks() {
        let bar = Toolbar::layout([1100.0, 900.0], 1.0);
        let l = ThemeLayout::new(&bar, [1100.0, 900.0]);
        let draw = |view| {
            let mut ui = Ui::default();
            l.draw(&view, &mut ui);
            ui
        };
        let (off, on) = (draw(ThemeView { pages_follow: false, dark: false }), draw(ThemeView { pages_follow: true, dark: true }));
        assert!(off.labels.iter().any(|t| t.text == "Pages follow the theme"));
        assert!(off.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite())));
        assert_ne!(format!("{:?}", off.panels), format!("{:?}", on.panels), "the track changes");
        assert_ne!(format!("{:?}", off.lines), format!("{:?}", on.lines), "the knob and the icon change");
        assert_ne!(off.labels.last().map(|t| &t.text), on.labels.last().map(|t| &t.text), "the hint follows the look");
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&ThemeView { pages_follow: true, dark: true }, &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "no white panel in the dark card");
    }
}
