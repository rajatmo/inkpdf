//! The card of an item (see `inkpdf_api::Card`): its title and a close button, the rows of text it holds (a thread), the name written on
//! what is added, a field to write in, and its buttons with the program's own delete button at the end. It floats at the right, under the
//! bars, like the search card, so the page stays in view. Plain data: the editor holds the state and builds an [`ItemCardView`], and the shell
//! draws what this makes. Every part is a button for a stylus tip or a mouse.

use crate::ink::rect_contains;
use crate::scene::Capsule;
use crate::text;

use super::Toolbar;
use super::icons::icons;
use super::search_card::{elide, visible_name};
use super::theme::{Align, Label, Ui};

/// Rows of text the card shows at most (the latest ones).
pub const ROWS: usize = 4;
/// Buttons of the kind's own the card shows at most (beside the program's delete).
pub const ACTIONS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardItem {
    /// The name written on what is added.
    Name,
    /// The field to write in.
    Field,
    /// Send what is in the field.
    Send,
    Close,
    /// The program's own: take the item away.
    Delete,
    /// A button of the kind's (counted from the first).
    Action(usize),
}

/// Everything the card's look depends on.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemCardView {
    pub title: String,
    /// The rows shown, each a small line over some text.
    pub rows: Vec<(String, String)>,
    /// How many rows there are beyond the ones shown (they are the older ones).
    pub older: usize,
    /// The name written on what is added, with its caret (a byte index on a character).
    pub name: String,
    pub name_caret: usize,
    /// What the field says while it is empty, if the item has a field.
    pub hint: Option<String>,
    pub text: String,
    pub caret: usize,
    /// The words on the kind's buttons.
    pub actions: Vec<String>,
    /// Whether the name has the keyboard (else the field has).
    pub typing_name: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ItemCardLayout {
    pub card: [f32; 4],
    close: [f32; 4],
    rows: [[f32; 4]; ROWS],
    row_count: usize,
    name: [f32; 4],
    field: Option<[f32; 4]>,
    send: [f32; 4],
    actions: [[f32; 4]; ACTIONS],
    action_count: usize,
    delete: [f32; 4],
    unit: f32,
    px: f32,
    title: [f32; 4],
    name_label: [f32; 4],
    /// A card with no name to write on what is sent and no button to delete what it is about (a link's).
    plain: bool,
}

impl ItemCardLayout {
    /// The card in a window of `viewport` pixels, at the right, with its top `top` pixels down, for `rows` rows of text, `actions` buttons of the
    /// kind's and a field (or not).
    pub fn new(toolbar: &Toolbar, viewport: [f32; 2], top: f32, rows: usize, actions: usize, has_field: bool) -> ItemCardLayout {
        Self::placed(toolbar, viewport, top, rows, actions, has_field, false)
    }

    /// The same for a card that is about something that is not an item (an address a link goes to): no name and no delete button.
    pub fn plain(toolbar: &Toolbar, viewport: [f32; 2], top: f32, rows: usize, actions: usize) -> ItemCardLayout {
        Self::placed(toolbar, viewport, top, rows, actions, false, true)
    }

    /// The card at `top`, or, in a window too short to hold it there, as far up as it must go to be whole (over the bars: a card cut off at the bottom of
    /// the window loses the buttons that are on it).
    fn placed(toolbar: &Toolbar, viewport: [f32; 2], top: f32, rows: usize, actions: usize, has_field: bool, plain: bool) -> ItemCardLayout {
        let limit = viewport[1] - 12.0 * toolbar.dpr;
        let at_top = Self::build(toolbar, viewport, top, rows, actions, has_field, plain);
        if at_top.card[3] <= limit {
            return at_top;
        }
        // (The rows are as many as fit under `top`; what is left is the card without rows, which has to go up.)
        let height = at_top.card[3] - at_top.card[1];
        Self::build(toolbar, viewport, (limit - height).max(0.0), rows, actions, has_field, plain)
    }

    fn build(toolbar: &Toolbar, viewport: [f32; 2], top: f32, rows: usize, actions: usize, has_field: bool, plain: bool) -> ItemCardLayout {
        let px = toolbar.dpr;
        let u = toolbar.unit.clamp(20.0 * px, 34.0 * px);
        let margin = 12.0 * px;
        let width = (17.0 * u).min((viewport[0] - 2.0 * margin).max(8.0 * u));
        let x1 = (viewport[0] - margin).max(width);
        let x0 = x1 - width;
        let (pad, h, gap) = (0.3 * u, 1.1 * u, 0.2 * u);
        let y0 = top.max(0.0);
        let row_a = y0 + pad;
        let close = [x1 - pad - h, row_a, x1 - pad, row_a + h];
        let title = [x0 + pad, row_a, close[0] - gap, row_a + h];
        let row_h = 1.45 * u;
        let first = row_a + h + gap;
        // (As many rows as fit in this window under the card's top, with the name, the field and the buttons below them: never longer than the window.)
        let below = (if plain { 0.0 } else { 0.9 * u + gap }) + (if has_field { h + gap } else { 0.0 }) + h + pad;
        let fits = ((viewport[1] - margin - first - below) / row_h).floor().max(0.0) as usize;
        let row_count = rows.min(ROWS).min(fits);
        let mut list = [[0.0; 4]; ROWS];
        for (i, r) in list.iter_mut().enumerate().take(row_count) {
            *r = [x0 + pad, first + i as f32 * row_h, x1 - pad, first + (i + 1) as f32 * row_h];
        }
        let after_rows = if row_count > 0 { list[row_count - 1][3] + gap } else { first };
        let name_label = [x0 + pad, after_rows, x0 + pad + 2.4 * u, after_rows + 0.9 * u];
        let name = [name_label[2], after_rows, x1 - pad, after_rows + 0.9 * u];
        // (A plain card has no row for the name: what follows the rows is its buttons.)
        let mut y = if plain { after_rows } else { name[3] + gap };
        let send = [x1 - pad - h, y, x1 - pad, y + h];
        let field = has_field.then(|| {
            let r = [x0 + pad, y, send[0] - gap, y + h];
            y = r[3] + gap;
            r
        });
        let action_count = actions.min(ACTIONS);
        let mut buttons = [[0.0; 4]; ACTIONS];
        let delete = [x1 - pad - h, y, x1 - pad, y + h];
        // (The buttons share the room before the delete button (all the width, in a plain card), as wide as 4.6 units each if there is room for that.)
        let room = if plain { x1 - pad - (x0 + pad) } else { delete[0] - gap - (x0 + pad) };
        let button_w = (4.6 * u).min((room - action_count.saturating_sub(1) as f32 * gap) / action_count.max(1) as f32);
        for (i, b) in buttons.iter_mut().enumerate().take(action_count) {
            let left = x0 + pad + i as f32 * (button_w + gap);
            *b = [left, y, left + button_w, y + h];
        }
        ItemCardLayout {
            card: [x0, y0, x1, y + h + pad],
            close,
            rows: list,
            row_count,
            name,
            field,
            send: [send[0], send[1], send[2], send[1] + h],
            actions: buttons,
            action_count,
            delete,
            unit: u,
            px,
            title,
            name_label,
            plain,
        }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    /// Every part there is and where it is.
    fn parts(&self) -> Vec<([f32; 4], CardItem)> {
        let mut parts = vec![(self.close, CardItem::Close)];
        if !self.plain {
            parts.extend([(self.delete, CardItem::Delete), (self.name, CardItem::Name)]);
        }
        parts.extend(self.field.map(|f| (f, CardItem::Field)));
        if self.field.is_some() {
            parts.push((self.send, CardItem::Send));
        }
        parts.extend(self.actions[..self.action_count].iter().enumerate().map(|(i, r)| (*r, CardItem::Action(i))));
        parts
    }

    /// Where each part is, by the name a real-input check presses it by (`--layout`).
    pub fn named_parts(&self) -> Vec<(String, [f32; 4])> {
        let name = |item: CardItem| match item {
            CardItem::Name => "name".to_string(),
            CardItem::Field => "field".to_string(),
            CardItem::Send => "send".to_string(),
            CardItem::Close => "close".to_string(),
            CardItem::Delete => "delete".to_string(),
            CardItem::Action(i) => format!("action{i}"),
        };
        self.parts().into_iter().map(|(r, item)| (name(item), r)).collect()
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<CardItem> {
        self.parts().into_iter().find(|(r, _)| rect_contains(*r, p)).map(|(_, item)| item)
    }

    /// The middle of a part, for tests that press it.
    #[cfg(test)]
    pub fn centre_of(&self, item: CardItem) -> [f32; 2] {
        let r = match item {
            CardItem::Name => self.name,
            CardItem::Field => self.field.expect("a field"),
            CardItem::Send => self.send,
            CardItem::Close => self.close,
            CardItem::Delete => self.delete,
            CardItem::Action(i) => self.actions[i.min(ACTIONS - 1)],
        };
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    pub fn draw(&self, view: &ItemCardView, ui: &mut Ui) {
        let (u, px) = (self.unit, self.px);
        ui.floating(self.card, 0.4 * u, px);
        let all = icons();
        let line = (u * 0.04).max(1.4 * px);
        let mid = |r: [f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let say = |ui: &mut Ui, text: &str, size: f32, color: [u8; 4], at: [f32; 2], align: Align| {
            if !text.is_empty() {
                ui.labels.push(Label { text: text.to_string(), size, color, at, align, alpha: 1.0 });
            }
        };
        let title = elide(&view.title, 0.5 * u, self.title[2] - self.title[0]);
        say(ui, &title, 0.5 * u, ui.pal.ink, [self.title[0] + 0.1 * u, mid(self.title)[1]], Align::Left);
        let [cx, cy] = mid(self.close);
        all.close.draw(&mut ui.lines, cx, cy, 0.3 * u, line, ui.pal.ink);
        // The rows: the latest ones, each a small line over its text.
        for (rect, (head, body)) in self.rows[..self.row_count].iter().zip(&view.rows) {
            let room = (rect[2] - rect[0] - 0.4 * u).max(u);
            let x = rect[0] + 0.2 * u;
            say(ui, &elide(head, 0.4 * u, room), 0.4 * u, ui.pal.ink_dim, [x, rect[1] + 0.32 * u], Align::Left);
            say(ui, &elide(&first_line(body), 0.46 * u, room), 0.46 * u, ui.pal.ink, [x, rect[1] + 0.95 * u], Align::Left);
        }
        if view.older > 0 && self.row_count > 0 {
            let at = [self.card[2] - 0.5 * u, self.rows[0][1] + 0.32 * u];
            say(ui, &format!("+{}", view.older), 0.36 * u, ui.pal.ink_dim, at, Align::Centre);
        }
        // The name, and the field: what is typed (or what to type), and the caret in the one that has the keyboard.
        let field_of = |ui: &mut Ui, rect: [f32; 4], typed: &str, caret: usize, hint: &str, active: bool| {
            ui.panel(rect, 0.3 * u, ui.pal.neutral_wash);
            let size = 0.5 * u;
            let room = (rect[2] - rect[0] - 0.5 * u).max(u);
            let (x0, y) = (rect[0] + 0.25 * u, mid(rect)[1]);
            if typed.is_empty() {
                say(ui, hint, size, ui.pal.ink_dim, [x0, y], Align::Left);
                if active {
                    ui.lines.push(Capsule::new([x0, y - 0.4 * u], [x0, y + 0.4 * u], 0.07 * u, ui.pal.accent));
                }
            } else {
                let range = visible_name(typed, caret, |s| text::measure(s, size)[0] <= room);
                say(ui, &typed[range.clone()], size, ui.pal.ink, [x0, y], Align::Left);
                if active {
                    let caret_x = x0 + text::measure(&typed[range.start..caret.clamp(range.start, range.end)], size)[0];
                    ui.lines.push(Capsule::new([caret_x, y - 0.4 * u], [caret_x, y + 0.4 * u], 0.07 * u, ui.pal.accent));
                }
            }
        };
        if !self.plain {
            say(ui, "Name", 0.4 * u, ui.pal.ink_dim, [self.name_label[0] + 0.1 * u, mid(self.name_label)[1]], Align::Left);
            field_of(ui, self.name, &view.name, view.name_caret, "Your name", view.typing_name);
        }
        if let (Some(rect), Some(hint)) = (self.field, &view.hint) {
            field_of(ui, rect, &view.text, view.caret, hint, !view.typing_name);
            let can = !view.text.trim().is_empty();
            let [cx, cy] = mid(self.send);
            all.later.draw(&mut ui.lines, cx, cy, 0.3 * u, line, if can { ui.pal.accent } else { ui.pal.ink_dim });
        }
        // The kind's buttons, and the program's own delete.
        for (rect, words) in self.actions[..self.action_count].iter().zip(&view.actions) {
            ui.panel(*rect, 0.3 * u, ui.pal.accent_wash);
            let room = rect[2] - rect[0] - 0.4 * u;
            say(ui, &elide(words, 0.44 * u, room), 0.44 * u, ui.pal.accent, mid(*rect), Align::Centre);
        }
        if !self.plain {
            let [cx, cy] = mid(self.delete);
            all.trash.draw(&mut ui.lines, cx, cy, 0.34 * u, line, ui.pal.danger);
        }
    }
}

/// The first line of a text (a card row shows one).
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 5] =
        [([1650.0, 1350.0], 1.5), ([1100.0, 900.0], 1.0), ([800.0, 600.0], 1.0), ([480.0, 800.0], 1.0), ([360.0, 640.0], 1.0)];

    fn layout(viewport: [f32; 2], dpr: f32, rows: usize, actions: usize, field: bool) -> ItemCardLayout {
        ItemCardLayout::new(&Toolbar::layout(viewport, dpr), viewport, 90.0 * dpr, rows, actions, field)
    }

    fn view() -> ItemCardView {
        ItemCardView {
            title: "Note".into(),
            rows: vec![("Ada, today".into(), "Is this right?".into()), ("Bob".into(), "Yes.\nSecond line".into())],
            older: 3,
            name: "Ada".into(),
            name_caret: 3,
            hint: Some("Reply".into()),
            text: String::new(),
            caret: 0,
            actions: vec!["Close".into()],
            typing_name: false,
        }
    }

    #[test]
    fn every_part_is_inside_the_card_inside_the_window_none_overlap_and_each_is_big_enough_for_a_stylus_tip() {
        for (viewport, dpr) in WINDOWS {
            let l = layout(viewport, dpr, ROWS, ACTIONS, true);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] + 0.5, "the card is in the window at {viewport:?}: {:?}", l.card);
            let mut parts = vec![l.close, l.name, l.delete, l.field.expect("field"), l.send];
            parts.extend(&l.actions[..l.action_count]);
            for r in parts.iter().chain(&l.rows[..l.row_count]) {
                assert!(r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3], "{r:?} in {:?} at {viewport:?}", l.card);
            }
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    assert!(
                        a[2] <= b[0] + 0.01 || b[2] <= a[0] + 0.01 || a[3] <= b[1] + 0.01 || b[3] <= a[1] + 0.01,
                        "{a:?} overlaps {b:?} at {viewport:?}"
                    );
                }
            }
            for r in [l.close, l.delete, l.actions[0], l.name, l.field.expect("field")] {
                assert!(r[3] - r[1] >= 18.0 * dpr && r[2] - r[0] >= 18.0 * dpr, "{r:?} is small at {viewport:?}");
            }
        }
    }

    #[test]
    fn a_press_finds_the_part_it_is_on_and_only_what_is_there() {
        let l = layout([1100.0, 900.0], 1.0, 2, 2, true);
        for item in [CardItem::Name, CardItem::Field, CardItem::Send, CardItem::Close, CardItem::Delete, CardItem::Action(0), CardItem::Action(1)] {
            assert_eq!(l.hit(l.centre_of(item)), Some(item), "{item:?}");
            assert!(l.contains(l.centre_of(item)));
        }
        assert_eq!(l.hit([2.0, 2.0]), None);
        assert!(!l.contains([2.0, 2.0]));
        let bare = layout([1100.0, 900.0], 1.0, 0, 0, false);
        assert!(bare.field.is_none() && bare.card[3] < l.card[3], "no rows, no field and no buttons make a shorter card");
        assert_eq!(layout([1100.0, 900.0], 1.0, 99, 99, true).row_count, ROWS, "no more rows than there is room for");
    }

    #[test]
    fn a_plain_card_has_no_name_and_no_delete_button_and_its_buttons_have_all_the_width() {
        for (viewport, dpr) in WINDOWS {
            let l = ItemCardLayout::plain(&Toolbar::layout(viewport, dpr), viewport, 90.0 * dpr, 3, 2);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] + 0.5 && l.card[3] <= viewport[1] + 600.0, "{:?}", l.card);
            let names: Vec<String> = l.named_parts().into_iter().map(|(n, _)| n).collect();
            assert_eq!(names, ["close", "action0", "action1"], "{names:?} at {viewport:?}");
            for (_, r) in l.named_parts() {
                assert!(r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3], "{r:?} in {:?}", l.card);
                assert!(r[3] - r[1] >= 18.0 * dpr, "{r:?} is small at {viewport:?}");
            }
            let full = ItemCardLayout::new(&Toolbar::layout(viewport, dpr), viewport, 90.0 * dpr, 3, 2, false);
            assert!(l.card[3] < full.card[3], "no row for the name makes it shorter");
            let mut ui = Ui::default();
            l.draw(
                &ItemCardView {
                    title: "Web link".into(),
                    rows: vec![("a".into(), "b".into())],
                    older: 0,
                    name: String::new(),
                    name_caret: 0,
                    hint: None,
                    text: String::new(),
                    caret: 0,
                    actions: vec!["Open".into(), "Copy".into()],
                    typing_name: false,
                },
                &mut ui,
            );
            let words: Vec<&str> = ui.labels.iter().map(|t| t.text.as_str()).collect();
            assert!(!words.contains(&"Name") && words.contains(&"Open") && words.contains(&"Copy"), "{words:?}");
        }
    }

    #[test]
    fn it_says_the_title_the_rows_the_name_and_what_the_field_is_for_and_draws_only_finite_things() {
        let l = layout([1100.0, 900.0], 1.0, 2, 1, true);
        let mut ui = Ui::default();
        l.draw(&view(), &mut ui);
        let words: Vec<&str> = ui.labels.iter().map(|t| t.text.as_str()).collect();
        for want in ["Note", "Ada, today", "Is this right?", "Yes.", "Ada", "Reply", "Close", "+3"] {
            assert!(words.contains(&want), "{want:?} in {words:?}");
        }
        assert!(!words.contains(&"Second line"), "a row shows one line");
        assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite())));
        assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
        // The caret is in whichever field has the keyboard.
        let mut name = Ui::default();
        l.draw(&ItemCardView { typing_name: true, ..view() }, &mut name);
        assert_ne!(format!("{:?}", name.lines), format!("{:?}", ui.lines), "the caret moves to the name");
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&view(), &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "no white panel in the dark card");
    }
}
