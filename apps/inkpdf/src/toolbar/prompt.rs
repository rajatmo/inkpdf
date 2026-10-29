//! The "save your changes?" prompt, drawn by the app itself in the toolbar's style: a white card
//! over a dimmed page, with a document that has unsaved changes and three round buttons (a bin to
//! throw them away, an arrow to go back, a disk to save).
//!
//! It is not a Windows message box on purpose: those stay hidden when raised from a pen tap, which
//! left the app waiting on a window nobody could see.

use crate::requests::Choice;

use super::icons::icons;
use super::theme::{ACCENT, INK, Ui};
use crate::scene::Xform;

/// What is highlighted while the prompt is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromptView {
    /// The button the pointer is over.
    pub hover: Option<Choice>,
    /// The button being pressed, while the pointer is still on it.
    pub pressed: Option<Choice>,
}

const DANGER: [u8; 4] = [214, 52, 62, 255];
const DANGER_WASH: [u8; 4] = [214, 52, 62, 34];
const NEUTRAL_WASH: [u8; 4] = [20, 24, 40, 20];
const SCRIM: [u8; 4] = [16, 20, 44, 104];

/// Where everything is, for a window of `viewport` pixels at scale `dpr`.
#[derive(Clone, Copy, Debug)]
pub struct PromptLayout {
    /// `[x0, y0, x1, y1]`.
    pub card: [f32; 4],
    /// Left to right: throw away, go back, save.
    pub buttons: [(Choice, [f32; 4]); 3],
    unit: f32,
    px: f32,
    viewport: [f32; 2],
}

impl PromptLayout {
    pub fn new(viewport: [f32; 2], dpr: f32) -> Self {
        // A comfortable size, shrunk to fit a narrow window.
        let nominal = 44.0 * dpr;
        let fit = ((viewport[0] - 0.8 * nominal) / (6.6 * nominal)).clamp(0.4, 1.0);
        let u = nominal * fit;
        let (w, h) = (6.6 * u, 4.9 * u);
        let (x0, y0) = ((viewport[0] - w) / 2.0, ((viewport[1] - h) / 2.0).max(0.0));
        let card = [x0, y0, x0 + w, y0 + h];
        let (size, gap) = (1.5 * u, 0.5 * u);
        let row = (card[0] + card[2]) / 2.0 - (3.0 * size + 2.0 * gap) / 2.0;
        let top = card[1] + 2.95 * u;
        let at = |i: usize| {
            let x = row + i as f32 * (size + gap);
            [x, top, x + size, top + size]
        };
        Self { card, buttons: [(Choice::Discard, at(0)), (Choice::Cancel, at(1)), (Choice::Save, at(2))], unit: u, px: dpr, viewport }
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<Choice> {
        self.buttons.iter().find(|(_, r)| crate::ink::rect_contains(*r, p)).map(|(c, _)| *c)
    }

    #[cfg(test)]
    pub fn draw(&self, view: PromptView, ui: &mut Ui) {
        self.draw_with(view, 1.0, ui);
    }

    /// Drawn `amount` (0 to 1) of the way in: the scrim clears, and the card grows from a little smaller as it does.
    pub fn draw_with(&self, view: PromptView, amount: f32, ui: &mut Ui) {
        let u = self.unit;
        ui.panel([0.0, 0.0, self.viewport[0], self.viewport[1]], 0.0, SCRIM);
        ui.fade(amount);
        let card = ui.mark();
        ui.floating(self.card, 0.5 * u, self.px);
        let icons = icons();
        let line = (0.05 * u).max(1.5 * self.px);
        // What is being asked about: the document, with a dot that says it has unsaved changes.
        let centre = [(self.card[0] + self.card[2]) / 2.0, self.card[1] + 1.5 * u];
        icons.save.draw(&mut ui.lines, centre[0], centre[1], 0.78 * u, 1.3 * line, INK);
        ui.disc([centre[0] + 0.7 * u, centre[1] - 0.7 * u], 0.36 * u, [255, 255, 255, 255]);
        ui.disc([centre[0] + 0.7 * u, centre[1] - 0.7 * u], 0.26 * u, ACCENT);
        for (choice, r) in self.buttons {
            let (fill, ink) = match choice {
                Choice::Save => (ACCENT, [255, 255, 255, 255]),
                Choice::Discard => (DANGER_WASH, DANGER),
                Choice::Cancel => (NEUTRAL_WASH, INK),
            };
            ui.panel(r, 0.42 * u, fill);
            if view.pressed == Some(choice) {
                ui.panel(r, 0.42 * u, [0, 0, 0, 44]);
            } else if view.hover == Some(choice) {
                ui.panel(r, 0.42 * u, [0, 0, 0, 20]);
            }
            let icon = match choice {
                Choice::Save => &icons.save,
                Choice::Discard => &icons.trash,
                Choice::Cancel => &icons.back,
            };
            icon.draw(&mut ui.lines, (r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0, 0.36 * u, line, ink);
        }
        let middle = [(self.card[0] + self.card[2]) / 2.0, (self.card[1] + self.card[3]) / 2.0];
        ui.apply(card, Xform::grow(middle, 0.92 + 0.08 * amount, amount));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [([f32; 2], f32); 6] = [
        ([1650.0, 1350.0], 1.5),
        ([1100.0, 900.0], 1.0),
        ([800.0, 600.0], 1.0),
        ([480.0, 800.0], 1.0),
        ([200.0, 300.0], 1.0),
        ([3840.0, 2160.0], 2.0),
    ];

    #[test]
    fn every_button_is_inside_the_card_inside_the_window_at_any_size() {
        for (viewport, dpr) in SIZES {
            let l = PromptLayout::new(viewport, dpr);
            assert!(l.card[0] >= -0.5 && l.card[2] <= viewport[0] + 0.5, "card fits across at {viewport:?}: {:?}", l.card);
            assert!(l.card[1] >= 0.0 && l.card[3] <= viewport[1] + 0.5 || viewport[1] < 4.9 * 44.0 * dpr, "card fits down at {viewport:?}");
            for (_, r) in l.buttons {
                assert!(
                    r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3],
                    "button {r:?} outside {:?} at {viewport:?}",
                    l.card
                );
            }
        }
    }

    #[test]
    fn buttons_do_not_overlap_and_are_big_enough_to_tap() {
        for (viewport, dpr) in SIZES {
            let l = PromptLayout::new(viewport, dpr);
            for pair in l.buttons.windows(2) {
                assert!(pair[0].1[2] < pair[1].1[0], "a gap between buttons at {viewport:?}");
            }
            let size = l.buttons[0].1[2] - l.buttons[0].1[0];
            if viewport[0] >= 800.0 {
                assert!(size >= 60.0 * dpr * 0.99, "a fingertip-sized button: {size}");
            }
        }
    }

    #[test]
    fn a_press_finds_the_button_under_it_and_the_gaps_and_card_hit_nothing() {
        let l = PromptLayout::new([1100.0, 900.0], 1.0);
        for (choice, r) in l.buttons {
            assert_eq!(l.hit([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]), Some(choice));
        }
        let gap = [(l.buttons[0].1[2] + l.buttons[1].1[0]) / 2.0, (l.buttons[0].1[1] + l.buttons[0].1[3]) / 2.0];
        assert_eq!(l.hit(gap), None);
        assert!(crate::ink::rect_contains(l.card, gap), "the gap is still on the card");
        assert!(!crate::ink::rect_contains(l.card, [5.0, 5.0]));
        assert_eq!(l.hit([5.0, 5.0]), None);
        // Save is the rightmost, discard the leftmost.
        assert_eq!(l.buttons.map(|b| b.0), [Choice::Discard, Choice::Cancel, Choice::Save]);
    }

    #[test]
    fn it_dims_the_whole_window_then_draws_the_card_and_marks_what_is_pressed() {
        let l = PromptLayout::new([1100.0, 900.0], 1.0);
        let (mut plain, mut pressed) = (Ui::default(), Ui::default());
        l.draw(PromptView::default(), &mut plain);
        l.draw(PromptView { hover: None, pressed: Some(Choice::Save) }, &mut pressed);
        assert_eq!(plain.panels[0].rect, [0.0, 0.0, 1100.0, 900.0], "the scrim covers the window first");
        assert_eq!(pressed.panels.len(), plain.panels.len() + 1, "a pressed button gets a darker wash");
        assert!(plain.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
    }
}
