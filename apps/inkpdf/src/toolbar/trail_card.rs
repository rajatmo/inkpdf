//! The small pill that opens under the document's pill once a link has been followed: a button to go back to where the reader was and one to go
//! forward again, as in a web browser. It floats at the left, kept clear of the bar under the tools where that bar would be in its way. Plain data, as
//! the other cards: the editor holds the places and builds a [`TrailView`], and the shell draws what this makes.

use crate::ink::rect_contains;

use super::icons::icons;
use super::{Pill, Toolbar, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrailItem {
    Back,
    Forward,
}

/// What the pill is drawn from: which of its buttons can be pressed now, and which has the pointer on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrailView {
    pub can_back: bool,
    pub can_forward: bool,
    pub pressed: Option<TrailItem>,
}

#[derive(Clone, Copy, Debug)]
pub struct TrailLayout {
    pub card: [f32; 4],
    back: [f32; 4],
    forward: [f32; 4],
    unit: f32,
    px: f32,
}

impl TrailLayout {
    /// The pill under the document's pill (or its round button), with the buttons the toolbar's size; `bar` is the bar under the tools, if there is
    /// one: where the pill would touch it (a window too narrow to have them side by side) it goes under the bar.
    pub fn new(toolbar: &Toolbar, bar: Option<[f32; 4]>) -> TrailLayout {
        let px = toolbar.dpr;
        let u = toolbar.unit;
        let pill = toolbar.pills[0];
        let pill = if pill[2] > pill[0] { pill } else { toolbar.round_of(Pill::Document) };
        let (pad, gap) = (8.0 * px, 6.0 * px);
        let (w, h) = (2.0 * u + gap + 2.0 * pad, u + 2.0 * pad);
        let (x0, mut y0) = (pill[0], pill[3] + 8.0 * px);
        if let Some(bar) = bar.filter(|b| x0 < b[2] + 8.0 * px && x0 + w > b[0] - 8.0 * px && y0 < b[3] + 8.0 * px && y0 + h > b[1] - 8.0 * px) {
            y0 = bar[3] + 8.0 * px;
        }
        let card = [x0, y0, x0 + w, y0 + h];
        let back = [x0 + pad, y0 + pad, x0 + pad + u, y0 + pad + u];
        let forward = [back[2] + gap, back[1], back[2] + gap + u, back[3]];
        TrailLayout { card, back, forward, unit: u, px }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    /// The button at `p`, if it is on one.
    pub fn hit(&self, p: [f32; 2]) -> Option<TrailItem> {
        [(self.back, TrailItem::Back), (self.forward, TrailItem::Forward)].into_iter().find(|(r, _)| rect_contains(*r, p)).map(|(_, item)| item)
    }

    /// The middle of a button.
    pub fn centre_of(&self, item: TrailItem) -> [f32; 2] {
        let r = match item {
            TrailItem::Back => self.back,
            TrailItem::Forward => self.forward,
        };
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    pub fn draw(&self, view: &TrailView, ui: &mut Ui) {
        let u = self.unit;
        ui.floating(self.card, (self.card[3] - self.card[1]) / 2.0, self.px);
        let line = (u * 0.045).max(1.4 * self.px);
        let all = icons();
        for (item, icon, can) in [(TrailItem::Back, &all.back, view.can_back), (TrailItem::Forward, &all.forward, view.can_forward)] {
            let [cx, cy] = self.centre_of(item);
            let r = if item == TrailItem::Back { self.back } else { self.forward };
            if view.pressed == Some(item) && can {
                ui.panel(r, u / 2.0, ui.pal.press(44));
            }
            icon.draw(&mut ui.lines, cx, cy, 0.4 * u, line, if can { ui.pal.ink } else { ui.pal.ink_dim });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 5] =
        [([1650.0, 1350.0], 1.5), ([1100.0, 900.0], 1.0), ([800.0, 600.0], 1.0), ([480.0, 800.0], 1.0), ([360.0, 640.0], 1.0)];

    fn bar(viewport: [f32; 2], dpr: f32) -> Toolbar {
        Toolbar::layout(viewport, dpr)
    }

    #[test]
    fn the_pill_hangs_under_the_documents_pill_inside_the_window_with_two_buttons_big_enough_to_tap() {
        for (viewport, dpr) in WINDOWS {
            let toolbar = bar(viewport, dpr);
            let l = TrailLayout::new(&toolbar, None);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] && l.card[3] <= viewport[1], "{:?} at {viewport:?}", l.card);
            let pill = if toolbar.pills[0][2] > toolbar.pills[0][0] { toolbar.pills[0] } else { toolbar.round_of(Pill::Document) };
            assert!(l.card[1] > pill[3] && l.card[0] == pill[0], "under the document's pill, at its left edge: {:?} vs {pill:?}", l.card);
            for item in [TrailItem::Back, TrailItem::Forward] {
                let c = l.centre_of(item);
                assert_eq!(l.hit(c), Some(item));
                assert!(l.contains(c) && !toolbar.contains(c), "{item:?} is not on the toolbar at {viewport:?}");
            }
            assert!(l.back[2] <= l.forward[0], "the buttons do not overlap");
            assert!(l.back[2] - l.back[0] >= 20.0 * dpr * 0.99, "a button to tap: {}", l.back[2] - l.back[0]);
            assert_eq!(l.hit([l.card[0] + 1.0, l.card[1] + 1.0]), None, "the corner of the pill is not a button");
        }
    }

    #[test]
    fn where_the_bar_under_the_tools_would_be_in_the_way_the_pill_goes_under_it() {
        let toolbar = bar([480.0, 800.0], 1.0);
        let wide_bar = [10.0, toolbar.bottom() + 8.0, 470.0, toolbar.bottom() + 60.0];
        let l = TrailLayout::new(&toolbar, Some(wide_bar));
        assert!(l.card[1] >= wide_bar[3], "{:?} under {wide_bar:?}", l.card);
        let far_bar = [300.0, toolbar.bottom() + 8.0, 470.0, toolbar.bottom() + 60.0];
        assert!(TrailLayout::new(&toolbar, Some(far_bar)).card[1] < far_bar[3], "a bar that is not in the way is not avoided");
    }

    #[test]
    fn a_button_that_cannot_be_pressed_is_dim_a_pressed_one_is_washed_and_the_dark_pill_uses_no_light_colour() {
        let l = TrailLayout::new(&bar([1100.0, 900.0], 1.0), None);
        let draw = |view: TrailView| {
            let mut ui = Ui::default();
            l.draw(&view, &mut ui);
            ui
        };
        let both = draw(TrailView { can_back: true, can_forward: true, pressed: None });
        let one = draw(TrailView { can_back: true, can_forward: false, pressed: None });
        assert!(both.lines.iter().all(|c| c.color == both.pal.ink) && one.lines.iter().any(|c| c.color == one.pal.ink_dim));
        let pressed = draw(TrailView { can_back: true, can_forward: false, pressed: Some(TrailItem::Back) });
        assert_eq!(pressed.panels.len(), one.panels.len() + 1, "the pressed button is washed");
        let dead = draw(TrailView { can_back: true, can_forward: false, pressed: Some(TrailItem::Forward) });
        assert_eq!(dead.panels.len(), one.panels.len(), "one that cannot be pressed shows no press");
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&TrailView { can_back: true, can_forward: true, pressed: None }, &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel));
        assert!(both.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
    }
}
