//! The card that says something went wrong (a file that could not be opened or saved), where the system has no message box of its own to say it
//! (Linux, Android): a floating card at the bottom of the window with a short heading, the reason in a few lines, and a close button. It is not
//! modal, so the page stays in view, and it does not time out: a mistake that is not read is the worst of it. Plain data, as the other cards: the
//! editor holds the text and the shell draws what this makes.

use crate::ink::rect_contains;
use crate::text;

use super::icons::icons;
use super::theme::{Align, Label, Ui};

/// Lines of the reason the card shows at most (the rest is cut short with an ellipsis).
pub const LINES: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct ErrorView {
    /// What went wrong, in a few words ("Could not save").
    pub heading: String,
    /// Why, as the system worded it, cut into the lines that fit the card.
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorLayout {
    pub card: [f32; 4],
    pub close: [f32; 4],
    /// The width the reason's lines may take, in pixels.
    pub room: f32,
    unit: f32,
    px: f32,
}

impl ErrorLayout {
    /// The card in a window of `viewport` pixels, along the bottom, with room for `lines` lines of the reason.
    pub fn new(viewport: [f32; 2], dpr: f32, lines: usize) -> ErrorLayout {
        let u = Self::unit_at(dpr);
        let margin = 12.0 * dpr;
        let width = (20.0 * u).min((viewport[0] - 2.0 * margin).max(8.0 * u));
        let x0 = (viewport[0] - width) / 2.0;
        let (pad, h) = (0.4 * u, 1.1 * u);
        let height = pad + h + lines.min(LINES) as f32 * 0.8 * u + pad;
        let y1 = (viewport[1] - margin).max(height);
        let card = [x0, y1 - height, x0 + width, y1];
        let close = [card[2] - pad - h, card[1] + pad, card[2] - pad, card[1] + pad + h];
        ErrorLayout { card, close, room: (close[0] - card[0] - 1.5 * pad).max(u), unit: u, px: dpr }
    }

    /// The size of a button, which the card's measures are of, at scale `dpr`.
    fn unit_at(dpr: f32) -> f32 {
        (28.0 * dpr).clamp(20.0 * dpr, 34.0 * dpr)
    }

    /// The size of the reason's text, in pixels, at scale `dpr` (what the lines must be cut to fit at).
    pub fn text_size(dpr: f32) -> f32 {
        0.42 * Self::unit_at(dpr)
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    /// The middle of the close button, for the `--layout` file.
    pub fn close_middle(&self) -> [f32; 2] {
        [(self.close[0] + self.close[2]) / 2.0, (self.close[1] + self.close[3]) / 2.0]
    }

    /// Whether `p` is on the close button.
    pub fn on_close(&self, p: [f32; 2]) -> bool {
        rect_contains(self.close, p)
    }

    pub fn draw(&self, view: &ErrorView, ui: &mut Ui) {
        let u = self.unit;
        ui.floating(self.card, 0.4 * u, self.px);
        let line = (u * 0.04).max(1.4 * self.px);
        let mid = |r: [f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let x = self.card[0] + 0.4 * u;
        let say = |ui: &mut Ui, words: &str, size: f32, color: [u8; 4], y: f32| {
            if !words.is_empty() {
                ui.labels.push(Label { text: words.to_string(), size, color, at: [x, y], align: Align::Left, alpha: 1.0 });
            }
        };
        ui.panel(self.close, 0.3 * u, ui.pal.danger_wash);
        let [cx, cy] = mid(self.close);
        icons().close.draw(&mut ui.lines, cx, cy, 0.3 * u, line, ui.pal.danger);
        say(ui, &elide(&view.heading, 0.52 * u, self.room), 0.52 * u, ui.pal.danger, self.close[1] + 0.55 * u);
        for (i, words) in view.lines.iter().take(LINES).enumerate() {
            say(ui, words, Self::text_size(self.px), ui.pal.ink, self.close[3] + 0.4 * u + i as f32 * 0.8 * u);
        }
    }
}

/// How many lines of the reason the card has room for (what [`wrap`] cuts a reason into at most).
pub fn lines_at_most() -> usize {
    LINES
}

/// `words` cut short, with an ellipsis, so that it is no wider than `room` at `size`.
fn elide(words: &str, size: f32, room: f32) -> String {
    if text::measure(words, size)[0] <= room {
        return words.to_string();
    }
    let mut chars: Vec<char> = words.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let shorter: String = chars.iter().chain(&['…']).collect();
        if text::measure(&shorter, size)[0] <= room {
            return shorter;
        }
    }
    String::new()
}

/// A reason cut into lines no wider than `room` at `size`: at the spaces where there are some, anywhere where a word is wider than a line; at
/// most [`LINES`] of them, the last cut short with an ellipsis if there was more. (The system's words may hold new lines: each starts one.)
pub fn wrap(reason: &str, size: f32, room: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut more = false;
    for paragraph in reason.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let joined = if current.is_empty() { word.clone() } else { format!("{current} {word}") };
                if text::measure(&joined, size)[0] <= room {
                    current = joined;
                    break;
                }
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    continue;
                }
                // A word wider than the line: as much of it as fits goes on this line.
                let mut fit = String::new();
                for c in word.chars() {
                    fit.push(c);
                    if text::measure(&fit, size)[0] > room && fit.chars().count() > 1 {
                        fit.pop();
                        break;
                    }
                }
                word = word[fit.len()..].to_string();
                lines.push(fit);
                if word.is_empty() {
                    break;
                }
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if lines.len() > LINES {
            more = true;
            break;
        }
    }
    if lines.len() > LINES || more {
        lines.truncate(LINES);
        if let Some(last) = lines.last_mut() {
            *last = elide(&format!("{last} …"), size, room);
            if !last.ends_with('…') {
                last.push('…');
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 5] =
        [([1650.0, 1350.0], 1.5), ([1100.0, 900.0], 1.0), ([800.0, 600.0], 1.0), ([480.0, 800.0], 1.0), ([360.0, 640.0], 1.0)];

    #[test]
    fn the_card_is_inside_the_window_along_its_bottom_and_its_close_button_is_big_enough_for_a_stylus_tip() {
        for (viewport, dpr) in WINDOWS {
            for lines in [0, 1, LINES, 99] {
                let l = ErrorLayout::new(viewport, dpr, lines);
                assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] + 0.5 && l.card[3] <= viewport[1] + 0.5, "{:?} at {viewport:?}", l.card);
                assert!(l.close[0] >= l.card[0] && l.close[2] <= l.card[2] && l.close[1] >= l.card[1] && l.close[3] <= l.card[3]);
                assert!(l.close[2] - l.close[0] >= 20.0 * dpr && l.close[3] - l.close[1] >= 20.0 * dpr, "{:?} at {viewport:?}", l.close);
                assert!(l.room > 0.0 && l.contains([(l.close[0] + l.close[2]) / 2.0, (l.close[1] + l.close[3]) / 2.0]));
            }
        }
        let l = ErrorLayout::new([1100.0, 900.0], 1.0, 2);
        assert!(l.on_close([(l.close[0] + l.close[2]) / 2.0, (l.close[1] + l.close[3]) / 2.0]) && !l.on_close([l.card[0] + 2.0, l.card[1] + 2.0]));
        assert!(ErrorLayout::new([1100.0, 900.0], 1.0, 0).card[1] > l.card[1], "fewer lines, a shorter card");
    }

    #[test]
    fn a_reason_is_cut_into_lines_that_fit_at_the_spaces_a_long_word_is_broken_and_a_long_text_ends_in_an_ellipsis() {
        let room = 200.0;
        let lines = wrap("Could not write the file because the disk is full and nobody has said what to do about it", 14.0, room);
        assert!(lines.len() > 1 && lines.len() <= LINES, "{lines:?}");
        assert!(lines.iter().all(|l| text::measure(l, 14.0)[0] <= room + 0.5), "{lines:?}");
        assert_eq!(lines.join(" "), "Could not write the file because the disk is full and nobody has said what to do about it");
        let long_word = wrap(&"x".repeat(400), 14.0, room);
        assert!(long_word.iter().all(|l| text::measure(l, 14.0)[0] <= room + 0.5) && long_word.len() == LINES, "{}", long_word.len());
        assert!(long_word.last().is_some_and(|l| l.ends_with('…')), "cut short");
        let paragraphs = wrap("first line\nsecond line", 14.0, 400.0);
        assert_eq!(paragraphs, ["first line", "second line"], "a new line starts one");
        assert!(wrap("", 14.0, room).is_empty() && wrap("   \n  ", 14.0, room).is_empty());
        let many = wrap("a\nb\nc\nd\ne\nf", 14.0, room);
        assert_eq!(many.len(), LINES);
        assert!(many[LINES - 1].ends_with('…'));
    }

    #[test]
    fn it_says_the_heading_and_each_line_and_draws_only_finite_things_and_no_light_panel_in_the_dark() {
        let l = ErrorLayout::new([1100.0, 900.0], 1.0, 2);
        let view = ErrorView { heading: "Could not save".into(), lines: vec!["The disk is full".into(), "and so on".into()] };
        let mut ui = Ui::default();
        l.draw(&view, &mut ui);
        let words: Vec<&str> = ui.labels.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(words, ["Could not save", "The disk is full", "and so on"]);
        assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite())));
        assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&view, &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel));
    }
}
