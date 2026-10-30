//! The Save As dialog, drawn by the app itself in the toolbar's style: a white card over a dimmed window with the places to
//! start from, the folder you are in, its subfolders and PDFs, the name, and Cancel and Save. Like the prompt it is not a
//! window of the system's (those stay hidden when raised from a pen tap, and look different on every platform), and like
//! the pages overview it is only data: where everything is ([`SaveAsLayout`]), what is shown ([`SaveAsView`]) and the
//! panels, lines and labels that make it ([`draw`]); the editor holds its state and the shell draws what it made.

use crate::browse::{self, Entry, Place};
use crate::scene::Capsule;
use crate::text;

use super::theme::{Align, Label, Ui};

/// How dark the scrim over the document is (0 to 255).
const SCRIM_ALPHA: u8 = 104;

/// The card's width and the heights of what is in it, in units (one unit is a comfortable button, 44 pixels at 100%).
const CARD_W: f32 = 16.0;
const PAD: f32 = 0.55;
const TITLE_H: f32 = 1.0;
const CHIP_H: f32 = 0.95;
const CHIPS_PER_ROW: usize = 5;
const MAX_CHIP_ROWS: usize = 2;
const PATH_H: f32 = 1.0;
const ROW_H: f32 = 1.1;
const LIST_ROWS: f32 = 6.0;
const STATUS_H: f32 = 0.8;
const NAME_H: f32 = 1.2;
const BUTTON_H: f32 = 1.3;
const BUTTON_W: f32 = 3.6;

/// What a pointer can be over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAsHit {
    /// A place, by its index in the places given.
    Place(usize),
    /// The button that goes to the folder above.
    Up,
    /// An entry of the list, by its index.
    Row(usize),
    /// The list, where there is no entry.
    List,
    /// The name's field.
    Name,
    Save,
    Cancel,
}

/// How the folder's listing stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingView<'a> {
    Loading,
    Ready,
    Failed(&'a str),
}

/// What is shown: everything the dialog's look depends on.
#[derive(Clone, Copy, Debug)]
pub struct SaveAsView<'a> {
    pub places: &'a [Place],
    /// The place that is the folder you are in, if any is.
    pub place: Option<usize>,
    /// The folder you are in, as text.
    pub dir: &'a str,
    pub has_parent: bool,
    pub entries: &'a [Entry],
    pub listing: ListingView<'a>,
    /// How far the list is scrolled, in pixels.
    pub scroll: f32,
    pub name: &'a str,
    /// Where the caret is in the name, as a byte index on a character.
    pub caret: usize,
    /// How many bytes of the name, from its start, are selected (what is typed first replaces them).
    pub stem_selected: Option<usize>,
    /// What is wrong, or what the person must know.
    pub message: Option<&'a str>,
    /// Save has been pressed on a name that is taken: pressing it again replaces the file.
    pub replace: bool,
    /// The file is being written.
    pub busy: bool,
    pub hover: Option<SaveAsHit>,
    pub pressed: Option<SaveAsHit>,
}

/// Where everything is, for a window of `viewport` pixels at scale `dpr` and `places` places to go to.
#[derive(Clone, Debug)]
pub struct SaveAsLayout {
    /// `[x0, y0, x1, y1]`.
    pub card: [f32; 4],
    pub title: [f32; 2],
    /// The places, by their index.
    pub chips: Vec<(usize, [f32; 4])>,
    pub up: [f32; 4],
    pub path: [f32; 4],
    pub list: [f32; 4],
    pub status: [f32; 4],
    pub name: [f32; 4],
    pub cancel: [f32; 4],
    pub save: [f32; 4],
    unit: f32,
    px: f32,
    viewport: [f32; 2],
}

impl SaveAsLayout {
    pub fn new(viewport: [f32; 2], dpr: f32, places: usize) -> Self {
        let nominal = 44.0 * dpr;
        let chip_rows = places.div_ceil(CHIPS_PER_ROW).clamp(usize::from(places > 0), MAX_CHIP_ROWS) as f32;
        let height_units = PAD + TITLE_H + chip_rows * (CHIP_H + 0.1) + PATH_H + LIST_ROWS * ROW_H + STATUS_H + NAME_H + BUTTON_H + 2.0 * PAD;
        // A comfortable size, shrunk to fit a small window.
        let fit = ((viewport[0] - 0.8 * nominal) / (CARD_W * nominal)).min((viewport[1] - 0.8 * nominal) / (height_units * nominal)).clamp(0.4, 1.0);
        let u = nominal * fit;
        let (w, h) = (CARD_W * u, height_units * u);
        let (x0, y0) = (((viewport[0] - w) / 2.0).max(0.0), ((viewport[1] - h) / 2.0).max(0.0));
        let card = [x0, y0, x0 + w, y0 + h];
        let (left, right) = (card[0] + PAD * u, card[2] - PAD * u);
        let mut y = card[1] + PAD * u;
        let title = [left, y + TITLE_H * u / 2.0];
        y += TITLE_H * u;
        let mut chips = Vec::new();
        let per_row = CHIPS_PER_ROW as f32;
        let chip_w = (right - left) / per_row;
        for i in 0..places.min(CHIPS_PER_ROW * MAX_CHIP_ROWS) {
            let (row, col) = ((i / CHIPS_PER_ROW) as f32, (i % CHIPS_PER_ROW) as f32);
            let x = left + col * chip_w;
            let top = y + row * (CHIP_H + 0.1) * u;
            chips.push((i, [x + 0.05 * u, top, x + chip_w - 0.05 * u, top + CHIP_H * u]));
        }
        y += chip_rows * (CHIP_H + 0.1) * u;
        let up = [left, y, left + PATH_H * u, y + PATH_H * u];
        let path = [up[2] + 0.3 * u, y, right, y + PATH_H * u];
        y += PATH_H * u;
        let list = [left, y + 0.1 * u, right, y + 0.1 * u + LIST_ROWS * ROW_H * u];
        y = list[3] + 0.1 * u;
        let status = [left, y, right, y + STATUS_H * u];
        y += STATUS_H * u;
        let name = [left, y, right, y + NAME_H * u];
        y += NAME_H * u + 0.3 * u;
        let (bw, bh) = (BUTTON_W * u, BUTTON_H * u);
        let save = [right - bw, y, right, y + bh];
        let cancel = [save[0] - 0.4 * u - bw, y, save[0] - 0.4 * u, y + bh];
        Self { card, title, chips, up, path, list, status, name, cancel, save, unit: u, px: dpr, viewport }
    }

    /// The height of a row of the list.
    pub fn row_height(&self) -> f32 {
        ROW_H * self.unit
    }

    /// How far `entries` rows can be scrolled.
    pub fn max_scroll(&self, entries: usize) -> f32 {
        (entries as f32 * self.row_height() - (self.list[3] - self.list[1])).max(0.0)
    }

    /// The rectangle of row `i` when the list is scrolled `scroll` pixels (it may be partly or wholly outside the list).
    pub fn row_rect(&self, i: usize, scroll: f32) -> [f32; 4] {
        let top = self.list[1] - scroll + i as f32 * self.row_height();
        [self.list[0], top, self.list[2] - 0.5 * self.unit, top + self.row_height()]
    }

    /// What is at `p`: `None` off the card, or on the card where nothing is.
    pub fn hit(&self, p: [f32; 2], scroll: f32, entries: usize) -> Option<SaveAsHit> {
        let inside = |r: [f32; 4]| crate::ink::rect_contains(r, p);
        if !inside(self.card) {
            return None;
        }
        if inside(self.save) {
            return Some(SaveAsHit::Save);
        }
        if inside(self.cancel) {
            return Some(SaveAsHit::Cancel);
        }
        if inside(self.up) {
            return Some(SaveAsHit::Up);
        }
        if inside(self.name) {
            return Some(SaveAsHit::Name);
        }
        if let Some((i, _)) = self.chips.iter().find(|(_, r)| inside(*r)) {
            return Some(SaveAsHit::Place(*i));
        }
        if inside(self.list) {
            let i = ((p[1] - self.list[1] + scroll) / self.row_height()).floor();
            return Some(if i >= 0.0 && (i as usize) < entries && p[0] <= self.list[2] - 0.5 * self.unit {
                SaveAsHit::Row(i as usize)
            } else {
                SaveAsHit::List
            });
        }
        None
    }

    /// The rows that can be seen when the list is scrolled `scroll`, from the first to one past the last.
    pub fn visible_rows(&self, scroll: f32, entries: usize) -> std::ops::Range<usize> {
        let h = self.row_height();
        let first = ((scroll / h).floor().max(0.0) as usize).min(entries);
        let last = (((scroll + self.list[3] - self.list[1]) / h).ceil().max(0.0) as usize).min(entries);
        first..last.max(first)
    }

    /// Drawn: the scrim over the window, then the card with everything on it.
    pub fn draw(&self, view: &SaveAsView<'_>, ui: &mut Ui) {
        draw(self, view, ui);
    }
}

/// The part of `name` to show so that the caret is in it and it fits (`fits` says whether a text is narrow enough): the
/// text before the caret loses its beginning first, and what is after it its end. Returns a range of bytes.
pub fn visible_name(name: &str, caret: usize, fits: impl Fn(&str) -> bool) -> std::ops::Range<usize> {
    let caret = caret.min(name.len());
    let mut start = 0;
    while start < caret && !fits(&name[start..caret]) {
        start += name[start..].chars().next().map_or(1, char::len_utf8);
    }
    let mut end = caret;
    while end < name.len() {
        let next = end + name[end..].chars().next().map_or(1, char::len_utf8);
        if !fits(&name[start..next]) {
            break;
        }
        end = next;
    }
    start..end
}

fn label(ui: &mut Ui, text: &str, size: f32, color: [u8; 4], at: [f32; 2], align: Align, alpha: f32) {
    if !text.is_empty() {
        ui.labels.push(Label { text: text.to_string(), size, color, at, align, alpha });
    }
}

fn with_alpha(mut color: [u8; 4], alpha: f32) -> [u8; 4] {
    color[3] = (f32::from(color[3]) * alpha.clamp(0.0, 1.0)).round() as u8;
    color
}

/// The part of `r` that is inside `clip`, if any.
fn clipped(r: [f32; 4], clip: [f32; 4]) -> Option<[f32; 4]> {
    let c = [r[0].max(clip[0]), r[1].max(clip[1]), r[2].min(clip[2]), r[3].min(clip[3])];
    (c[2] > c[0] && c[3] > c[1]).then_some(c)
}

fn middle(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

fn draw(l: &SaveAsLayout, v: &SaveAsView<'_>, ui: &mut Ui) {
    let u = l.unit;
    let line = (0.05 * u).max(1.5 * l.px);
    let washed = |ui: &mut Ui, r: [f32; 4], radius: f32, which: SaveAsHit| {
        if v.pressed == Some(which) {
            ui.panel(r, radius, ui.pal.press(44));
        } else if v.hover == Some(which) {
            ui.panel(r, radius, ui.pal.hover_wash);
        }
    };
    ui.panel([0.0, 0.0, l.viewport[0], l.viewport[1]], 0.0, ui.pal.shadow(SCRIM_ALPHA));
    ui.floating(l.card, 0.5 * u, l.px);
    label(ui, "Save as", 0.62 * u, ui.pal.ink, l.title, Align::Left, 1.0);

    // The places.
    for &(i, r) in &l.chips {
        let Some(place) = v.places.get(i) else { continue };
        let active = v.place == Some(i);
        ui.panel(r, 0.3 * u, if active { ui.pal.accent_wash } else { ui.pal.separator });
        washed(ui, r, 0.3 * u, SaveAsHit::Place(i));
        // (The width of the text is what is left after the chip's own margins: a long name is shortened, not spilled.)
        let text = browse::elide_left(&place.label, |s| text::measure(s, 0.42 * u)[0] <= r[2] - r[0] - 0.3 * u);
        label(ui, &text, 0.42 * u, if active { ui.pal.accent } else { ui.pal.ink }, middle(r), Align::Centre, 1.0);
    }

    // Up, and where you are.
    let up_ink = if v.has_parent { ui.pal.ink } else { ui.pal.ink_dim };
    ui.panel(l.up, 0.3 * u, ui.pal.separator);
    if v.has_parent {
        washed(ui, l.up, 0.3 * u, SaveAsHit::Up);
    }
    let c = middle(l.up);
    ui.line([c[0] - 0.22 * u, c[1] + 0.1 * u], [c[0], c[1] - 0.12 * u], line, up_ink);
    ui.line([c[0], c[1] - 0.12 * u], [c[0] + 0.22 * u, c[1] + 0.1 * u], line, up_ink);
    let room = l.path[2] - l.path[0] - 0.4 * u;
    let shown = browse::elide_left(v.dir, |s| text::measure(s, 0.46 * u)[0] <= room);
    label(ui, &shown, 0.46 * u, ui.pal.ink, [l.path[0] + 0.2 * u, middle(l.path)[1]], Align::Left, 1.0);

    // The list.
    ui.panel(l.list, 0.3 * u, ui.pal.wash(10));
    let row_h = l.row_height();
    for i in l.visible_rows(v.scroll, v.entries.len()) {
        let full = l.row_rect(i, v.scroll);
        let Some(shown) = clipped(full, l.list) else { continue };
        // A row cut by the edge of the list fades out with how much of it is gone, so that nothing is drawn over what is above or below.
        let seen = (shown[3] - shown[1]) / row_h;
        let alpha = ((seen - 0.5) * 2.0).clamp(0.0, 1.0);
        if v.pressed == Some(SaveAsHit::Row(i)) {
            ui.panel(shown, 0.25 * u, with_alpha(ui.pal.press(44), alpha));
        } else if v.hover == Some(SaveAsHit::Row(i)) {
            ui.panel(shown, 0.25 * u, with_alpha(ui.pal.hover_wash, alpha));
        }
        let entry = &v.entries[i];
        let mid = [full[0] + 0.7 * u, middle(full)[1]];
        if entry.folder {
            ui.panel([mid[0] - 0.3 * u, mid[1] - 0.26 * u, mid[0] - 0.02 * u, mid[1] - 0.1 * u], 0.06 * u, with_alpha(ui.pal.folder_tab, alpha));
            ui.panel([mid[0] - 0.3 * u, mid[1] - 0.16 * u, mid[0] + 0.3 * u, mid[1] + 0.24 * u], 0.08 * u, with_alpha(ui.pal.folder, alpha));
        } else {
            ui.panel([mid[0] - 0.25 * u, mid[1] - 0.32 * u, mid[0] + 0.25 * u, mid[1] + 0.32 * u], 0.07 * u, with_alpha(ui.pal.page_rim, alpha));
            ui.panel([mid[0] - 0.23 * u, mid[1] - 0.3 * u, mid[0] + 0.23 * u, mid[1] + 0.3 * u], 0.06 * u, with_alpha(ui.pal.page, alpha));
            ui.panel([mid[0] - 0.23 * u, mid[1] + 0.06 * u, mid[0] + 0.23 * u, mid[1] + 0.2 * u], 0.03 * u, with_alpha(ui.pal.danger, alpha * 0.8));
        }
        let room = full[2] - full[0] - 1.6 * u;
        let name = browse::elide_left(&entry.name, |s| text::measure(s, 0.5 * u)[0] <= room);
        label(ui, &name, 0.5 * u, if entry.folder { ui.pal.ink } else { ui.pal.file_ink }, [full[0] + 1.35 * u, middle(full)[1]], Align::Left, alpha);
    }
    let centre = middle(l.list);
    match v.listing {
        ListingView::Loading if v.entries.is_empty() => label(ui, "Loading…", 0.5 * u, ui.pal.ink_dim, centre, Align::Centre, 1.0),
        ListingView::Failed(why) => {
            let room = l.list[2] - l.list[0] - u;
            let why = browse::elide_left(why, |s| text::measure(s, 0.46 * u)[0] <= room);
            label(ui, &why, 0.46 * u, ui.pal.danger, centre, Align::Centre, 1.0)
        }
        ListingView::Ready if v.entries.is_empty() => label(ui, "No folders or PDFs here", 0.5 * u, ui.pal.ink_dim, centre, Align::Centre, 1.0),
        _ => {}
    }
    // A thin bar at the right edge says how much more there is.
    let (list_h, content) = (l.list[3] - l.list[1], v.entries.len() as f32 * row_h);
    if content > list_h {
        let track = [l.list[2] - 0.32 * u, l.list[1] + 0.2 * u, l.list[2] - 0.12 * u, l.list[3] - 0.2 * u];
        ui.panel(track, 0.1 * u, ui.pal.track);
        let (th, range) = (track[3] - track[1], l.max_scroll(v.entries.len()).max(1.0));
        let len = (th * list_h / content).max(0.8 * u).min(th);
        let top = track[1] + (th - len) * (v.scroll / range).clamp(0.0, 1.0);
        ui.panel([track[0], top, track[2], top + len], 0.1 * u, ui.pal.rim);
    }

    // What is wrong, if anything.
    if let Some(message) = v.message {
        let room = l.status[2] - l.status[0];
        let message = browse::elide_left(message, |s| text::measure(s, 0.44 * u)[0] <= room);
        label(ui, &message, 0.44 * u, ui.pal.danger, [l.status[0] + 0.1 * u, middle(l.status)[1]], Align::Left, 1.0);
    }

    // The name.
    ui.panel(l.name, 0.3 * u, ui.pal.accent);
    ui.panel([l.name[0] + 0.05 * u, l.name[1] + 0.05 * u, l.name[2] - 0.05 * u, l.name[3] - 0.05 * u], 0.27 * u, ui.pal.white);
    let field = l.name[2] - l.name[0] - 0.8 * u;
    let size = 0.55 * u;
    let range = visible_name(v.name, v.caret, |s| text::measure(s, size)[0] <= field);
    let (x0, mid_y) = (l.name[0] + 0.4 * u, middle(l.name)[1]);
    if let Some(stem) = v.stem_selected.filter(|s| *s > 0 && range.start == 0) {
        let width = text::measure(&v.name[..stem.min(range.end)], size)[0];
        ui.panel([x0 - 0.05 * u, mid_y - 0.42 * u, x0 + width + 0.05 * u, mid_y + 0.42 * u], 0.12 * u, ui.pal.accent_wash);
    }
    label(ui, &v.name[range.clone()], size, ui.pal.ink, [x0, mid_y], Align::Left, 1.0);
    let caret_x = x0 + text::measure(&v.name[range.start..v.caret.clamp(range.start, range.end)], size)[0];
    ui.lines.push(Capsule::new([caret_x, mid_y - 0.4 * u], [caret_x, mid_y + 0.4 * u], 0.07 * u, ui.pal.accent));

    // Cancel and Save.
    for (r, which, text, ink, fill) in [
        (l.cancel, SaveAsHit::Cancel, "Cancel", ui.pal.ink, ui.pal.separator),
        (
            l.save,
            SaveAsHit::Save,
            if v.busy {
                "Saving…"
            } else if v.replace {
                "Replace"
            } else {
                "Save"
            },
            ui.pal.white,
            if v.busy {
                ui.pal.ink_dim
            } else if v.replace {
                ui.pal.danger
            } else {
                ui.pal.accent
            },
        ),
    ] {
        ui.panel(r, 0.42 * u, fill);
        washed(ui, r, 0.42 * u, which);
        label(ui, text, 0.52 * u, ink, middle(r), Align::Centre, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [([f32; 2], f32); 7] = [
        ([1650.0, 1350.0], 1.5),
        ([1100.0, 900.0], 1.0),
        ([800.0, 600.0], 1.0),
        ([480.0, 800.0], 1.0),
        ([360.0, 640.0], 1.0),
        ([200.0, 300.0], 1.0),
        ([3840.0, 2160.0], 2.0),
    ];

    fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
        inner[0] >= outer[0] - 0.5 && inner[2] <= outer[2] + 0.5 && inner[1] >= outer[1] - 0.5 && inner[3] <= outer[3] + 0.5
    }

    fn places(n: usize) -> Vec<Place> {
        (0..n).map(|i| Place { label: format!("Place {i}"), path: format!("/p{i}").into() }).collect()
    }

    #[test]
    fn everything_is_on_the_card_and_the_card_in_the_window_at_any_size_and_with_any_number_of_places() {
        for (viewport, dpr) in SIZES {
            for n in [0, 1, 4, 5, 6, 10, 30] {
                let l = SaveAsLayout::new(viewport, dpr, n);
                assert!(
                    inside(l.card, [0.0, 0.0, viewport[0], viewport[1]]) || viewport[1] < 400.0 * dpr,
                    "card in window at {viewport:?}: {:?}",
                    l.card
                );
                for (name, r) in
                    [("up", l.up), ("path", l.path), ("list", l.list), ("status", l.status), ("name", l.name), ("cancel", l.cancel), ("save", l.save)]
                {
                    assert!(inside(r, l.card), "{name} {r:?} on the card {:?} at {viewport:?} with {n} places", l.card);
                }
                assert!(l.chips.len() <= CHIPS_PER_ROW * MAX_CHIP_ROWS && l.chips.len() == n.min(10));
                for (_, r) in &l.chips {
                    assert!(inside(*r, l.card), "a place on the card");
                    assert!(r[3] <= l.up[1] + 0.5, "places above the path");
                }
            }
        }
    }

    #[test]
    fn the_parts_do_not_overlap_and_the_buttons_are_big_enough_to_tap() {
        for (viewport, dpr) in SIZES {
            let l = SaveAsLayout::new(viewport, dpr, 6);
            let order = [l.up, l.list, l.status, l.name, l.save];
            for pair in order.windows(2) {
                assert!(pair[0][3] <= pair[1][1] + 0.5, "one above the next at {viewport:?}");
            }
            assert!(l.cancel[2] < l.save[0], "a gap between the buttons");
            // (A window too small for the card at its comfortable size makes it smaller: this is about the ones that are not.)
            if viewport[0] >= 800.0 && viewport[1] >= 800.0 * dpr {
                assert!(l.save[3] - l.save[1] >= 50.0 * dpr * 0.99, "a fingertip-sized button");
                assert!(l.row_height() >= 40.0 * dpr * 0.99, "a row a fingertip can pick");
            }
        }
    }

    #[test]
    fn a_press_finds_what_is_under_it_and_nothing_off_the_card() {
        let l = SaveAsLayout::new([1100.0, 900.0], 1.0, 5);
        let at = |r: [f32; 4]| middle(r);
        assert_eq!(l.hit(at(l.save), 0.0, 3), Some(SaveAsHit::Save));
        assert_eq!(l.hit(at(l.cancel), 0.0, 3), Some(SaveAsHit::Cancel));
        assert_eq!(l.hit(at(l.up), 0.0, 3), Some(SaveAsHit::Up));
        assert_eq!(l.hit(at(l.name), 0.0, 3), Some(SaveAsHit::Name));
        for (i, r) in &l.chips {
            assert_eq!(l.hit(at(*r), 0.0, 3), Some(SaveAsHit::Place(*i)));
        }
        for i in 0..3 {
            assert_eq!(l.hit(at(l.row_rect(i, 0.0)), 0.0, 3), Some(SaveAsHit::Row(i)));
        }
        assert_eq!(l.hit(at(l.row_rect(3, 0.0)), 0.0, 3), Some(SaveAsHit::List), "below the last entry is the list, not a row");
        assert_eq!(l.hit([2.0, 2.0], 0.0, 3), None, "off the card");
        assert_eq!(l.hit([l.card[0] + 1.0, l.card[1] + 1.0], 0.0, 3), None, "on the card, on nothing");
    }

    #[test]
    fn scrolling_moves_the_rows_under_the_pointer_and_stops_at_the_ends() {
        let l = SaveAsLayout::new([1100.0, 900.0], 1.0, 4);
        assert_eq!(l.max_scroll(3), 0.0, "a short list does not scroll");
        let n = 100;
        let max = l.max_scroll(n);
        assert!((max - (n as f32 * l.row_height() - (l.list[3] - l.list[1]))).abs() < 0.01);
        let p = middle(l.list);
        let at_top = l.hit(p, 0.0, n);
        let scrolled = l.hit(p, 5.0 * l.row_height(), n);
        let (Some(SaveAsHit::Row(a)), Some(SaveAsHit::Row(b))) = (at_top, scrolled) else { panic!("rows: {at_top:?} {scrolled:?}") };
        assert_eq!(b, a + 5);
        assert_eq!(l.visible_rows(0.0, n).start, 0);
        let last = l.visible_rows(max, n);
        assert_eq!(last.end, n, "the last row can be reached");
        assert!(
            l.visible_rows(f32::MAX / 2.0, 3).is_empty() || l.visible_rows(f32::MAX / 2.0, 3).start <= 3,
            "a wild scroll shows nothing, not a crash"
        );
        assert!(l.visible_rows(-1.0e9, 3).start == 0);
    }

    #[test]
    fn the_name_shows_the_caret_and_the_part_of_the_name_that_fits() {
        let room = |n: usize| move |s: &str| s.chars().count() <= n;
        assert_eq!(visible_name("abc", 3, room(10)), 0..3);
        assert_eq!(visible_name("abcdefghij", 10, room(4)), 6..10, "the end, where the caret is");
        assert_eq!(visible_name("abcdefghij", 0, room(4)), 0..4, "the beginning, where the caret is");
        let r = visible_name("abcdefghij", 5, room(4));
        assert!(r.start <= 5 && r.end >= 5 && r.len() <= 4, "{r:?}");
        let r = visible_name("éé文書é", "éé文".len(), room(2));
        assert!("éé文書é".is_char_boundary(r.start) && "éé文書é".is_char_boundary(r.end), "never inside a character: {r:?}");
        assert_eq!(visible_name("", 0, room(3)), 0..0);
        assert_eq!(visible_name("abc", 99, room(3)), 0..3, "a caret past the end is the end");
    }

    #[test]
    fn it_dims_the_window_then_draws_the_card_and_everything_is_a_number_inside_the_window() {
        let entries: Vec<Entry> = (0..40)
            .map(|i| Entry { name: format!("{i:02} a rather long folder name that cannot fit in a row of any card"), folder: i % 3 == 0 })
            .collect();
        let places = places(6);
        for (viewport, dpr) in SIZES {
            let l = SaveAsLayout::new(viewport, dpr, places.len());
            for scroll in [0.0, 123.4, l.max_scroll(entries.len())] {
                let view = SaveAsView {
                    places: &places,
                    place: Some(1),
                    dir: "/home/someone/Documents/a/very/long/path/of/folders",
                    has_parent: true,
                    entries: &entries,
                    listing: ListingView::Ready,
                    scroll,
                    name: "Untitled.pdf",
                    caret: 8,
                    stem_selected: Some(8),
                    message: Some("A name cannot contain /"),
                    replace: false,
                    busy: false,
                    hover: Some(SaveAsHit::Row(2)),
                    pressed: Some(SaveAsHit::Save),
                };
                let mut ui = Ui::default();
                l.draw(&view, &mut ui);
                assert_eq!(ui.panels[0].rect, [0.0, 0.0, viewport[0], viewport[1]], "the scrim covers the window first");
                assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
                assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite()) && (0.0..=1.0).contains(&t.alpha)), "labels at {viewport:?}");
                assert!(
                    ui.labels.iter().any(|t| t.text == "Save as")
                        && ui.labels.iter().any(|t| t.text == "Save")
                        && ui.labels.iter().any(|t| t.text == "Cancel")
                );
            }
        }
    }

    #[test]
    fn the_save_button_says_what_it_will_do_and_the_list_says_what_it_is_waiting_for() {
        let l = SaveAsLayout::new([1100.0, 900.0], 1.0, 2);
        let places = places(2);
        let base = SaveAsView {
            places: &places,
            place: None,
            dir: "/x",
            has_parent: false,
            entries: &[],
            listing: ListingView::Loading,
            scroll: 0.0,
            name: "a.pdf",
            caret: 5,
            stem_selected: None,
            message: None,
            replace: false,
            busy: false,
            hover: None,
            pressed: None,
        };
        let texts = |v: &SaveAsView<'_>| {
            let mut ui = Ui::default();
            l.draw(v, &mut ui);
            ui.labels.into_iter().map(|t| t.text).collect::<Vec<_>>()
        };
        assert!(texts(&base).contains(&"Save".to_string()) && texts(&base).contains(&"Loading…".to_string()));
        assert!(texts(&SaveAsView { replace: true, ..base }).contains(&"Replace".to_string()));
        assert!(texts(&SaveAsView { busy: true, ..base }).contains(&"Saving…".to_string()));
        assert!(texts(&SaveAsView { listing: ListingView::Ready, ..base }).contains(&"No folders or PDFs here".to_string()));
        assert!(texts(&SaveAsView { listing: ListingView::Failed("no such folder"), ..base }).contains(&"no such folder".to_string()));
    }
}
