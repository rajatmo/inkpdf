//! The card of the PDFs opened last: a list of up to ten names (the newest first, the one that is open lit), each with the folder it is in where there
//! is room to say it; a press on one opens it again, as it was left. It hangs under the document's pill (or its round button) at the left, below the
//! pill that goes back and forward and the bar under the tools where those are in its way, and its rows are made lower in a window too short for
//! them all. Plain data, as the other cards: the editor holds the list and builds a [`RecentView`], and the shell draws what this makes.

use crate::ink::rect_contains;
use crate::text;

use super::search_card::elide;
use super::theme::{Align, Label, Ui};
use super::{Pill, Toolbar};

/// Rows the card shows at most: every document the list keeps.
pub const ROWS: usize = crate::recent::MAX;

/// One document of the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentRow {
    pub name: String,
    /// The folder it is in (empty when there is none to show).
    pub folder: String,
    /// It is the document that is open.
    pub current: bool,
}

/// Everything the card's look depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentView {
    pub rows: Vec<RecentRow>,
    /// The row being pressed.
    pub pressed: Option<usize>,
}

/// `text` with its beginning cut off, behind an ellipsis, so that it is no wider than `room` at `size` (the end of a folder says the most).
fn elide_start(text: &str, size: f32, room: f32) -> String {
    if text::measure(text, size)[0] <= room {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for from in 1..chars.len() {
        let shorter: String = std::iter::once('…').chain(chars[from..].iter().copied()).collect();
        if text::measure(&shorter, size)[0] <= room {
            return shorter;
        }
    }
    String::new()
}

#[derive(Clone, Copy, Debug)]
pub struct RecentLayout {
    pub card: [f32; 4],
    rows: [[f32; 4]; ROWS],
    row_count: usize,
    unit: f32,
    px: f32,
}

impl RecentLayout {
    /// The card in a window of `viewport` pixels, under the document's pill, with `rows` rows (as many of them as the window has room for, each as
    /// low as it must be and no lower than a stylus can press); `avoid` is what else floats near the top (the pill that goes back and forward, the bar
    /// under the tools): where the card would touch one, it begins under it.
    pub fn new(toolbar: &Toolbar, viewport: [f32; 2], avoid: &[[f32; 4]], rows: usize) -> RecentLayout {
        let px = toolbar.dpr;
        let u = toolbar.unit.clamp(20.0 * px, 34.0 * px);
        let (margin, gap) = (12.0 * px, 8.0 * px);
        let pill = toolbar.pills[0];
        let pill = if pill[2] > pill[0] { pill } else { toolbar.round_of(Pill::Document) };
        let x0 = pill[0];
        let x1 = (x0 + 15.0 * u).min(viewport[0] - margin).max(x0 + 4.0 * u);
        let mut y0 = pill[3] + gap;
        // (Twice: going under one may bring it beside another.)
        for _ in 0..2 {
            for r in avoid.iter().filter(|r| r[2] > r[0]) {
                if x0 < r[2] + gap && x1 > r[0] - gap && y0 < r[3] + gap {
                    y0 = r[3] + gap;
                }
            }
        }
        let pad = 0.3 * u;
        let room = (viewport[1] - margin - y0 - 2.0 * pad).max(0.0);
        let rows = rows.min(ROWS);
        let row_h = (0.95 * u).min(room / rows.max(1) as f32).max(22.0 * px);
        let row_count = rows.min((room / row_h).floor() as usize);
        let mut list = [[0.0; 4]; ROWS];
        for (i, r) in list.iter_mut().enumerate().take(row_count) {
            *r = [x0 + pad, y0 + pad + i as f32 * row_h, x1 - pad, y0 + pad + (i + 1) as f32 * row_h];
        }
        let bottom = y0 + 2.0 * pad + row_count as f32 * row_h;
        RecentLayout { card: [x0, y0, x1, bottom], rows: list, row_count, unit: u, px }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    /// The row at `p`, if it is on one.
    pub fn hit(&self, p: [f32; 2]) -> Option<usize> {
        self.rows[..self.row_count].iter().position(|r| rect_contains(*r, p))
    }

    /// How many rows there is room for.
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// The middle of a row (the last one for a row there is not).
    pub fn centre_of(&self, row: usize) -> [f32; 2] {
        let r = self.rows[row.min(self.row_count.saturating_sub(1))];
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    pub fn draw(&self, view: &RecentView, ui: &mut Ui) {
        let (u, px) = (self.unit, self.px);
        ui.floating(self.card, 0.4 * u, px);
        for (i, (rect, row)) in self.rows[..self.row_count].iter().zip(&view.rows).enumerate() {
            if view.pressed == Some(i) {
                ui.panel(*rect, 0.25 * u, ui.pal.press(44));
            } else if row.current {
                ui.panel(*rect, 0.25 * u, ui.pal.accent_wash);
            }
            let y = (rect[1] + rect[3]) / 2.0;
            let size = (0.46 * u).min(0.62 * (rect[3] - rect[1]));
            let x = rect[0] + 0.3 * u;
            let room = (rect[2] - x - 0.3 * u).max(u);
            let name = elide(&row.name, size, room);
            let used = text::measure(&name, size)[0];
            if !name.is_empty() {
                ui.labels.push(Label { text: name, size, color: ui.pal.ink, at: [x, y], align: Align::Left, alpha: 1.0 });
            }
            // The folder, after the name, where there is room left to say something of it.
            let (small, from) = (0.82 * size, x + used + 0.5 * u);
            let left = rect[2] - 0.3 * u - from;
            if !row.folder.is_empty() && left >= 3.0 * u {
                let folder = elide_start(&row.folder, small, left);
                if !folder.is_empty() {
                    ui.labels.push(Label { text: folder, size: small, color: ui.pal.ink_dim, at: [from, y], align: Align::Left, alpha: 1.0 });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 7] = [
        ([1650.0, 1350.0], 1.5),
        ([1100.0, 900.0], 1.0),
        ([800.0, 600.0], 1.0),
        ([480.0, 800.0], 1.0),
        ([360.0, 640.0], 1.0),
        ([360.0, 480.0], 1.0),
        ([2400.0, 1080.0], 2.6),
    ];

    fn view(rows: usize) -> RecentView {
        let row = |i: usize| RecentRow { name: format!("paper {i}.pdf"), folder: "C:\\Users\\me\\Documents\\papers".into(), current: i == 0 };
        RecentView { rows: (0..rows).map(row).collect(), pressed: None }
    }

    #[test]
    fn the_card_hangs_under_the_documents_pill_inside_the_window_with_every_row_big_enough_to_press() {
        for (viewport, dpr) in WINDOWS {
            let toolbar = Toolbar::layout(viewport, dpr);
            let l = RecentLayout::new(&toolbar, viewport, &[], ROWS);
            let pill = if toolbar.pills[0][2] > toolbar.pills[0][0] { toolbar.pills[0] } else { toolbar.round_of(Pill::Document) };
            assert!(l.card[1] > pill[3] && l.card[0] == pill[0], "under the document's pill, at its left edge: {:?} vs {pill:?}", l.card);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] && l.card[3] <= viewport[1], "{:?} in {viewport:?}", l.card);
            assert_eq!(l.row_count(), ROWS, "all ten have room at {viewport:?}");
            for i in 0..l.row_count() {
                let r = l.rows[i];
                assert!(r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3] + 0.01, "{r:?} in {:?}", l.card);
                assert!(r[3] - r[1] >= 22.0 * dpr * 0.99 && r[2] - r[0] >= 100.0 * dpr, "a row to press at {viewport:?}: {r:?}");
                assert!(i == 0 || l.rows[i - 1][3] <= r[1] + 0.01, "the rows do not overlap");
                assert_eq!(l.hit(l.centre_of(i)), Some(i));
                assert!(l.contains(l.centre_of(i)) && !toolbar.contains(l.centre_of(i)));
            }
            assert_eq!(l.hit([l.card[0] + 1.0, l.card[1] + 1.0]), None, "the corner of the card is not a row");
        }
    }

    #[test]
    fn fewer_documents_make_a_shorter_card_and_no_more_rows_than_the_list_holds() {
        let viewport = [1100.0, 900.0];
        let toolbar = Toolbar::layout(viewport, 1.0);
        let (three, ten, many) = (
            RecentLayout::new(&toolbar, viewport, &[], 3),
            RecentLayout::new(&toolbar, viewport, &[], ROWS),
            RecentLayout::new(&toolbar, viewport, &[], 99),
        );
        assert_eq!((three.row_count(), ten.row_count(), many.row_count()), (3, ROWS, ROWS));
        assert!(three.card[3] < ten.card[3]);
        assert_eq!(three.hit(ten.centre_of(7)), None, "a row there is not is not pressed");
        assert_eq!(three.centre_of(99), three.centre_of(2));
    }

    #[test]
    fn it_begins_under_what_is_in_its_way_and_a_short_window_has_lower_rows_or_fewer() {
        let viewport = [480.0, 800.0];
        let toolbar = Toolbar::layout(viewport, 1.0);
        let free = RecentLayout::new(&toolbar, viewport, &[], ROWS);
        let trail = [free.card[0], free.card[1], free.card[0] + 100.0, free.card[1] + 54.0];
        let bar = [10.0, trail[3] + 8.0, 470.0, trail[3] + 60.0];
        let under = RecentLayout::new(&toolbar, viewport, &[bar, trail], ROWS);
        assert!(under.card[1] >= bar[3] + 7.9, "under the pill and then under the bar that was then in its way: {:?}", under.card);
        let far = [300.0, free.card[1], 470.0, free.card[1] + 50.0];
        assert!(
            RecentLayout::new(&toolbar, [1100.0, 900.0], &[[900.0, 70.0, 1000.0, 120.0]], ROWS).card[1] < 80.0,
            "what is not in the way is not avoided"
        );
        assert!(RecentLayout::new(&toolbar, viewport, &[far, [0.0; 4]], ROWS).card[1] <= far[3] + 8.0);
        // A window with little room below: the rows are lower, and then there are fewer of them, and the card is in the window.
        let low = [0.0, 0.0, 480.0, 420.0];
        let squeezed = RecentLayout::new(&toolbar, viewport, &[low], ROWS);
        assert!(squeezed.row_count() == ROWS && squeezed.rows[0][3] - squeezed.rows[0][1] < free.rows[0][3] - free.rows[0][1]);
        let lower = [0.0, 0.0, 480.0, 640.0];
        let few = RecentLayout::new(&toolbar, viewport, &[lower], ROWS);
        assert!(few.row_count() < ROWS && few.row_count() > 0 && few.card[3] <= viewport[1], "{:?}", few.card);
        assert!(few.rows[0][3] - few.rows[0][1] >= 22.0 * 0.99);
    }

    #[test]
    fn it_names_each_document_says_its_folder_where_there_is_room_and_lights_the_open_and_the_pressed_one() {
        let viewport = [1100.0, 900.0];
        let l = RecentLayout::new(&Toolbar::layout(viewport, 1.0), viewport, &[], 3);
        let mut ui = Ui::default();
        l.draw(&view(3), &mut ui);
        let words: Vec<&str> = ui.labels.iter().map(|t| t.text.as_str()).collect();
        for name in ["paper 0.pdf", "paper 1.pdf", "paper 2.pdf"] {
            assert!(words.contains(&name), "{words:?}");
        }
        assert_eq!(words.iter().filter(|t| t.ends_with("papers")).count(), 3, "the folder of each: {words:?}");
        assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite()) && t.at[0] >= l.card[0] && t.at[0] <= l.card[2]));
        // A name too long for the card is cut short, and then there is no room for its folder.
        let mut long = view(1);
        long.rows[0].name = "x".repeat(300);
        let mut cut = Ui::default();
        l.draw(&long, &mut cut);
        assert_eq!(cut.labels.len(), 1);
        assert!(cut.labels[0].text.ends_with('…') && cut.labels[0].text.len() < 300);
        assert!(text::measure(&cut.labels[0].text, cut.labels[0].size)[0] <= l.card[2] - l.card[0]);
        // The one that is open is washed; a pressed one is too, in the press's colour; a row with nothing lit adds no panel.
        let (mut none, mut pressed) = (Ui::default(), Ui::default());
        l.draw(&RecentView { rows: view(3).rows.into_iter().map(|r| RecentRow { current: false, ..r }).collect(), pressed: None }, &mut none);
        l.draw(&RecentView { pressed: Some(0), ..view(3) }, &mut pressed);
        assert_eq!(ui.panels.len(), none.panels.len() + 1, "the open document is lit");
        assert_eq!(pressed.panels.len(), ui.panels.len());
        assert_ne!(format!("{:?}", pressed.panels), format!("{:?}", ui.panels), "a press shows in its own colour");
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&view(3), &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "no white panel in the dark card");
    }

    #[test]
    fn a_folder_too_long_loses_its_beginning() {
        let folder = "C:\\Users\\somebody\\Documents\\university\\papers";
        let size = 14.0;
        assert_eq!(elide_start(folder, size, 1.0e4), folder);
        let room = text::measure(folder, size)[0] * 0.5;
        let cut = elide_start(folder, size, room);
        assert!(cut.starts_with('…') && cut.ends_with("papers") && text::measure(&cut, size)[0] <= room, "{cut}");
        assert_eq!(elide_start(folder, size, 0.0), "");
    }
}
