//! Drawing the pages overview as data (see [`Ui`]): the scrim and the card, the header, the table of contents and the
//! pictures of the pages with their numbers. Whatever leaves the area it belongs to (a row of pages half scrolled off
//! the top) is cut off at its edge or left out, so nothing of the grid is ever drawn over the header.

use crate::outline::OutlineItem;
use crate::scene::Xform;
use crate::text;
use crate::view::Layout;

use super::super::icons::icons;
use super::super::theme::{ACCENT, ACCENT_WASH, Align, HOVER_WASH, INK, INK_DIM, Image, Label, PANEL_EDGE, SEPARATOR, Ui};
use super::layout::{Action, Control, MenuItem, OverviewLayout, TocHit, TocRow};

const SCRIM: [u8; 4] = [16, 20, 44, 120];
const PAPER: [u8; 4] = [255, 255, 255, 255];
const PRESSED_WASH: [u8; 4] = [20, 24, 40, 30];
const DANGER: [u8; 4] = [214, 52, 62, 255];

/// What the pointer is over or pressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    Control(Control),
    Action(Action),
    Page(usize),
    /// The number under a page, which opens its menu.
    Number(usize),
    /// An item of the menu that is open.
    Menu(MenuItem),
    Toc(TocHit),
}

/// A page, or several, being dragged to another place.
#[derive(Clone, Copy, Debug)]
pub struct DragView<'a> {
    /// The pages being moved, in order.
    pub pages: &'a [usize],
    /// Where the pointer is.
    pub pos: [f32; 2],
    /// Where they would land if dropped now: before this page.
    pub slot: usize,
}

/// How a page's picture is moving, as plain numbers (see `editor::overview`): drawn at rest when all are nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageMotion {
    /// How far it is from where it belongs, in pixels (a page that was moved slides in).
    pub shift: [f32; 2],
    /// How much of a new page is still to come, 0 to 1: it grows in from smaller and clearer.
    pub grow: f32,
    /// The same for the tick that a page chosen has.
    pub tick: f32,
}

/// Everything the overview shows.
pub struct OverviewView<'a> {
    pub layout: &'a Layout,
    /// How far the pages are scrolled, in pixels.
    pub scroll: f32,
    /// The page being looked at in the document.
    pub current: usize,
    /// Pages chosen in edit mode, in order.
    pub selected: &'a [usize],
    pub edit: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub outline: &'a [OutlineItem],
    /// The rows of the table of contents that are shown (the others are folded away).
    pub rows: &'a [TocRow],
    pub toc_scroll: f32,
    /// The entry of the outline that holds the page being looked at.
    pub section: Option<usize>,
    /// Which entries are folded, by their index in the outline.
    pub folded: &'a dyn Fn(usize) -> bool,
    pub hover: Option<Hover>,
    pub pressed: Option<Hover>,
    pub drag: Option<DragView<'a>>,
    /// Which of the bar's actions can be done now, in the order of [`Action::ALL`].
    pub enabled: [bool; 5],
    /// The page whose menu is open, and which of its items can be used, in the order of [`MenuItem::ALL`].
    pub menu: Option<usize>,
    pub menu_enabled: [bool; 4],
    /// How each page's picture is moving.
    pub motion: &'a dyn Fn(usize) -> PageMotion,
}

/// `rect` cut to `bounds`; `None` if nothing is left.
fn clip(rect: [f32; 4], bounds: [f32; 4]) -> Option<[f32; 4]> {
    let r = [rect[0].max(bounds[0]), rect[1].max(bounds[1]), rect[2].min(bounds[2]), rect[3].min(bounds[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

/// Whether `inner` is all inside `outer`.
fn within(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

/// `words` shortened with dots to fit `width` pixels at `size`.
fn fit(words: &str, size: f32, width: f32) -> String {
    if text::measure(words, size)[0] <= width {
        return words.to_string();
    }
    let mut shorter: Vec<char> = words.chars().collect();
    while !shorter.is_empty() {
        shorter.pop();
        let candidate: String = shorter.iter().collect::<String>().trim_end().to_string() + "...";
        if text::measure(&candidate, size)[0] <= width {
            return candidate;
        }
    }
    String::new()
}

/// A rectangle's outline as four lines.
fn outline_rect(ui: &mut Ui, r: [f32; 4], width: f32, color: [u8; 4]) {
    let half = width / 2.0;
    let (x0, y0, x1, y1) = (r[0] - half, r[1] - half, r[2] + half, r[3] + half);
    for (a, b) in [([x0, y0], [x1, y0]), ([x1, y0], [x1, y1]), ([x1, y1], [x0, y1]), ([x0, y1], [x0, y0])] {
        ui.line(a, b, width, color);
    }
}

/// The outline of a circle of `diameter`, drawn as short lines (its inside is left as it is).
fn ring(ui: &mut Ui, centre: [f32; 2], diameter: f32, width: f32, color: [u8; 4]) {
    const SIDES: usize = 20;
    let r = diameter / 2.0;
    let at = |k: usize| {
        let t = k as f32 / SIDES as f32 * std::f32::consts::TAU;
        [centre[0] + r * t.cos(), centre[1] + r * t.sin()]
    };
    for k in 0..SIDES {
        ui.line(at(k), at(k + 1), width, color);
    }
}

/// A chevron: pointing right, or down when `open`.
fn chevron(ui: &mut Ui, centre: [f32; 2], size: f32, open: bool, width: f32, color: [u8; 4]) {
    let (a, b, c) = if open {
        ([centre[0] - size, centre[1] - size / 2.0], [centre[0], centre[1] + size / 2.0], [centre[0] + size, centre[1] - size / 2.0])
    } else {
        ([centre[0] - size / 2.0, centre[1] - size], [centre[0] + size / 2.0, centre[1]], [centre[0] - size / 2.0, centre[1] + size])
    };
    ui.line(a, b, width, color);
    ui.line(b, c, width, color);
}

/// Draw the overview into `ui`.
pub fn draw(view: &OverviewView<'_>, l: &OverviewLayout, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    ui.panels.push(crate::scene::Panel::solid([0.0, 0.0, l.viewport[0], l.viewport[1]], 0.0, SCRIM));
    ui.floating(l.card, 0.9 * u, px);
    header(view, l, ui);
    if l.toc.is_some() {
        contents(view, l, ui);
    }
    pages(view, l, ui);
    if l.bar.is_some() {
        bar(view, l, ui);
    }
    if let Some(drag) = view.drag {
        dragging(view, l, &drag, ui);
    }
    if let Some(page) = view.menu {
        menu(view, l, page, ui);
    }
}

/// The menu of one page, floating over the pages.
fn menu(view: &OverviewView<'_>, l: &OverviewLayout, page: usize, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let (rect, items) = l.menu(page, view.scroll);
    ui.floating(rect, 0.5 * u, px);
    for (k, (item, r)) in items.into_iter().enumerate() {
        let enabled = view.menu_enabled[k];
        if view.hover == Some(Hover::Menu(item)) && enabled {
            ui.panels.push(crate::scene::Panel::solid(r, 0.3 * u, HOVER_WASH));
        }
        let text = match item {
            MenuItem::Before => "Add page before",
            MenuItem::After => "Add page after",
            MenuItem::Duplicate => "Duplicate",
            MenuItem::Delete => "Delete",
        };
        let color = if !enabled {
            INK_DIM
        } else if item == MenuItem::Delete {
            DANGER
        } else {
            INK
        };
        ui.labels.push(Label {
            text: text.to_string(),
            size: 0.5 * u,
            color,
            at: [r[0] + 0.5 * u, (r[1] + r[3]) / 2.0],
            align: Align::Left,
            alpha: 1.0,
        });
    }
}

fn header(view: &OverviewView<'_>, l: &OverviewLayout, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let centre_y = (l.header[1] + l.header[3]) / 2.0;
    icons().pages.draw(&mut ui.lines, l.card[0] + 1.0 * u, centre_y, 0.5 * u, 1.6 * px, INK);
    let count = view.layout.len();
    ui.labels.push(Label {
        text: if view.edit && !view.selected.is_empty() {
            format!("{} selected", view.selected.len())
        } else if count == 1 {
            "All pages (1)".to_string()
        } else {
            format!("All pages ({count})")
        },
        size: 0.62 * u,
        color: INK,
        at: [l.card[0] + 1.8 * u, centre_y],
        align: Align::Left,
        alpha: 1.0,
    });
    for (control, r) in l.controls {
        let centre = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let enabled = match control {
            Control::Undo => view.can_undo,
            Control::Redo => view.can_redo,
            _ => true,
        };
        let on = control == Control::Edit && view.edit;
        let wash = if view.pressed == Some(Hover::Control(control)) && enabled {
            Some(PRESSED_WASH)
        } else if on {
            Some(ACCENT_WASH)
        } else if view.hover == Some(Hover::Control(control)) && enabled {
            Some(HOVER_WASH)
        } else {
            None
        };
        if let Some(color) = wash {
            ui.panels.push(crate::scene::Panel::solid(r, (r[2] - r[0]) / 2.0, color));
        }
        let color = if !enabled {
            INK_DIM
        } else if on {
            ACCENT
        } else {
            INK
        };
        let half = 0.45 * u;
        let icon = match control {
            Control::Close => &icons().close,
            Control::Redo => &icons().redo,
            Control::Undo => &icons().undo,
            Control::Edit => &icons().check,
            Control::Add => &icons().plus,
        };
        icon.draw(&mut ui.lines, centre[0], centre[1], half, 1.7 * px, color);
    }
    ui.line([l.header[0], l.header[3]], [l.header[2], l.header[3]], px, SEPARATOR);
}

fn contents(view: &OverviewView<'_>, l: &OverviewLayout, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let Some(toc) = l.toc else { return };
    ui.line([toc[2], toc[1]], [toc[2], toc[3] - 0.9 * u], px, SEPARATOR);
    ui.labels.push(Label {
        text: "Contents".to_string(),
        size: 0.46 * u,
        color: INK_DIM,
        at: [toc[0] + 0.9 * u, toc[1] + 0.6 * u],
        align: Align::Left,
        alpha: 1.0,
    });
    let size = 0.46 * u;
    for k in l.toc_visible(view.rows.len(), view.toc_scroll) {
        let row = view.rows[k];
        let r = l.toc_row(k, view.toc_scroll);
        if !within(l.toc_rows, r) {
            continue;
        }
        let item: &OutlineItem = &view.outline[row.item];
        let section = view.section == Some(row.item);
        let hovered = matches!(view.hover, Some(Hover::Toc(TocHit::Title(i) | TocHit::Arrow(i))) if i == k);
        if section {
            ui.panels.push(crate::scene::Panel::solid(r, 0.3 * u, ACCENT_WASH));
        } else if hovered {
            ui.panels.push(crate::scene::Panel::solid(r, 0.3 * u, HOVER_WASH));
        }
        let centre_y = (r[1] + r[3]) / 2.0;
        if row.foldable {
            let arrow = l.toc_arrow(row, k, view.toc_scroll);
            chevron(ui, [(arrow[0] + arrow[2]) / 2.0, centre_y], 0.14 * u, !(view.folded)(row.item), 1.5 * px, INK_DIM);
        }
        let x = l.toc_title_x(row, k, view.toc_scroll);
        let color = if section {
            ACCENT
        } else if item.page.is_some() {
            INK
        } else {
            INK_DIM
        };
        let title = fit(&item.title, size, r[2] - x - 0.2 * u);
        if !title.is_empty() {
            ui.labels.push(Label { text: title, size, color, at: [x, centre_y], align: Align::Left, alpha: 1.0 });
        }
    }
}

fn pages(view: &OverviewView<'_>, l: &OverviewLayout, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let count = view.layout.len();
    let selected = |i: usize| view.selected.binary_search(&i).is_ok();
    for i in l.visible_pages(count, view.scroll) {
        let Some(page) = view.layout.page(i) else { continue };
        let thumb = l.thumb(i, view.scroll, [page.w, page.h]);
        let Some(shown) = clip(thumb, l.grid) else { continue };
        let whole = within(l.grid, thumb);
        let hovered = view.hover == Some(Hover::Page(i));
        let current = i == view.current;
        let (mark, motion) = (ui.mark(), (view.motion)(i));
        // The paper behind the picture, with a hairline edge: what shows until the picture is there.
        if whole {
            ui.panels.push(crate::scene::Panel::new(
                [thumb[0] - px, thumb[1] - px + 2.0 * px, thumb[2] + px, thumb[3] + px + 2.0 * px],
                2.0 * px,
                6.0 * px,
                [16, 20, 44, 36],
            ));
        }
        ui.panels.push(crate::scene::Panel::solid([shown[0] - px, shown[1] - px, shown[2] + px, shown[3] + px], 2.0 * px, PANEL_EDGE));
        ui.panels.push(crate::scene::Panel::solid(shown, 1.5 * px, PAPER));
        // A page being dragged is left faded where it was.
        let faded = view.drag.is_some_and(|d| d.pages.contains(&i));
        ui.images.push(Image { page: i as u32, full: thumb, shown, alpha: if faded { 0.3 } else { 1.0 } });
        if hovered && !current {
            ui.panels.push(crate::scene::Panel::solid(shown, 1.5 * px, HOVER_WASH));
        }
        if whole {
            if selected(i) {
                outline_rect(ui, [thumb[0] - 3.0 * px, thumb[1] - 3.0 * px, thumb[2] + 3.0 * px, thumb[3] + 3.0 * px], 3.0 * px, ACCENT);
            } else if current {
                outline_rect(ui, [thumb[0] - 3.0 * px, thumb[1] - 3.0 * px, thumb[2] + 3.0 * px, thumb[3] + 3.0 * px], 2.5 * px, ACCENT);
            }
            if view.edit {
                // A circle to tick in the top right corner (where a page has the least on it): filled once the page is chosen.
                let centre = [thumb[2] - 0.55 * u, thumb[1] + 0.55 * u];
                let tick = ui.mark();
                if selected(i) {
                    ui.disc(centre, 0.8 * u, ACCENT);
                    ui.line(
                        [centre[0] - 0.2 * u, centre[1] + 0.02 * u],
                        [centre[0] - 0.05 * u, centre[1] + 0.18 * u],
                        2.0 * px,
                        [255, 255, 255, 255],
                    );
                    ui.line(
                        [centre[0] - 0.05 * u, centre[1] + 0.18 * u],
                        [centre[0] + 0.24 * u, centre[1] - 0.16 * u],
                        2.0 * px,
                        [255, 255, 255, 255],
                    );
                } else {
                    // An open ring (the page shows through it) that shows on white paper, with a pale one round it
                    // so that it shows on dark pages too.
                    ring(ui, centre, 0.7 * u, 3.6 * px, [255, 255, 255, 200]);
                    ring(ui, centre, 0.7 * u, 1.8 * px, INK_DIM);
                }
                // A tick just made pops in: it grows from half its size as it clears.
                ui.apply(tick, Xform::grow(centre, 1.0 - 0.5 * motion.tick, 1.0 - motion.tick));
            }
        }
        // The number, under the picture.
        let row = l.number_row(i, view.scroll);
        if within(l.grid, row) {
            let color = if current || selected(i) { ACCENT } else { INK };
            let (centre_x, centre_y) = ((row[0] + row[2]) / 2.0, (row[1] + row[3]) / 2.0 + 0.1 * u);
            if view.hover == Some(Hover::Number(i)) || view.menu == Some(i) {
                let half = (0.9 * u).min((row[2] - row[0]) / 2.0);
                ui.panels.push(crate::scene::Panel::solid(
                    [centre_x - half - 0.5 * u, row[1] + 0.15 * u, centre_x + half + 0.5 * u, row[3] - 0.05 * u],
                    0.4 * u,
                    HOVER_WASH,
                ));
            }
            let text = (i + 1).to_string();
            ui.labels.push(Label { text: text.clone(), size: 0.5 * u, color, at: [centre_x - 0.2 * u, centre_y], align: Align::Centre, alpha: 1.0 });
            let after = centre_x - 0.2 * u + text::measure(&text, 0.5 * u)[0] / 2.0 + 0.35 * u;
            chevron(ui, [after, centre_y], 0.12 * u, view.menu == Some(i), 1.4 * px, INK_DIM);
        }
        // The page as a whole: sliding from where it was, or growing in as a new one.
        let middle = [(thumb[0] + thumb[2]) / 2.0, (thumb[1] + thumb[3]) / 2.0];
        ui.apply(mark, Xform { origin: middle, scale: 1.0 - 0.4 * motion.grow, shift: motion.shift, alpha: 1.0 - motion.grow });
    }
    // A thin bar at the edge when there is more than fits.
    let max = l.max_scroll(count);
    if max > 0.0 {
        let (top, bottom) = (l.grid[1] + 0.5 * u, l.grid[3] - 0.5 * u);
        let track = bottom - top;
        let content = track + max;
        let (len, at) = ((track * track / content).max(1.2 * u), top + (track - (track * track / content).max(1.2 * u)) * (view.scroll / max));
        let x = l.grid[2] - 0.35 * u;
        ui.line([x, at], [x, at + len], 0.22 * u, [20, 24, 40, 70]);
    }
}

/// The bar of what can be done to the chosen pages.
fn bar(view: &OverviewView<'_>, l: &OverviewLayout, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let Some(bar) = l.bar else { return };
    ui.line([bar[0], bar[1]], [bar[2], bar[1]], px, SEPARATOR);
    for (i, action) in Action::ALL.into_iter().enumerate() {
        let r = l.action(action);
        let enabled = view.enabled[i];
        let hovered = view.hover == Some(Hover::Action(action));
        let wash = if view.pressed == Some(Hover::Action(action)) && enabled {
            Some(PRESSED_WASH)
        } else if hovered && enabled {
            Some(HOVER_WASH)
        } else {
            None
        };
        let radius = (r[3] - r[1]) / 2.0;
        // The pill that chooses everything has a hairline round it, the round buttons only a wash.
        if action == Action::SelectAll {
            ui.panels.push(crate::scene::Panel::solid(r, radius, SEPARATOR));
            ui.panels.push(crate::scene::Panel::solid([r[0] + px, r[1] + px, r[2] - px, r[3] - px], radius, [255, 255, 255, 255]));
        }
        if let Some(color) = wash {
            ui.panels.push(crate::scene::Panel::solid(r, radius, color));
        }
        let color = if enabled || action == Action::SelectAll { INK } else { INK_DIM };
        let centre = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let icon = match action {
            Action::SelectAll => {
                let all = !view.layout.is_empty() && view.selected.len() == view.layout.len();
                let text = if all { "Select none" } else { "Select all" };
                ui.labels.push(Label { text: text.to_string(), size: 0.5 * u, color, at: centre, align: Align::Centre, alpha: 1.0 });
                continue;
            }
            Action::Duplicate => &icons().duplicate,
            Action::Earlier => &icons().earlier,
            Action::Later => &icons().later,
            Action::Delete => &icons().trash,
        };
        let tint = if action == Action::Delete && enabled { DANGER } else { color };
        icon.draw(&mut ui.lines, centre[0], centre[1], 0.42 * u, 1.7 * px, tint);
    }
}

/// The pages being dragged, as a faded picture at the pointer, and where they would land.
fn dragging(view: &OverviewView<'_>, l: &OverviewLayout, drag: &DragView<'_>, ui: &mut Ui) {
    let (u, px) = (l.unit(), l.px());
    let marker = l.slot_marker(drag.slot, view.layout.len(), view.scroll);
    if let Some(shown) = clip(marker, l.grid) {
        ui.line([(shown[0] + shown[2]) / 2.0, shown[1]], [(shown[0] + shown[2]) / 2.0, shown[3]], shown[2] - shown[0], ACCENT);
    }
    let Some(&first) = drag.pages.first() else { return };
    let Some(page) = view.layout.page(first) else { return };
    let cell = l.thumb(first, 0.0, [page.w, page.h]);
    let (w, h) = (cell[2] - cell[0], cell[3] - cell[1]);
    let rect = [drag.pos[0] - w / 2.0, drag.pos[1] - h / 2.0, drag.pos[0] + w / 2.0, drag.pos[1] + h / 2.0];
    ui.panels.push(crate::scene::Panel::new([rect[0], rect[1] + 3.0 * px, rect[2], rect[3] + 3.0 * px], 2.0 * px, 10.0 * px, [16, 20, 44, 70]));
    ui.panels.push(crate::scene::Panel::solid(rect, 1.5 * px, PAPER));
    ui.images.push(Image { page: first as u32, full: rect, shown: rect, alpha: 0.85 });
    outline_rect(ui, rect, 2.0 * px, ACCENT);
    if drag.pages.len() > 1 {
        let badge = [rect[2], rect[1]];
        ui.disc(badge, 0.9 * u, ACCENT);
        ui.labels.push(Label {
            text: drag.pages.len().to_string(),
            size: 0.46 * u,
            color: [255, 255, 255, 255],
            at: badge,
            align: Align::Centre,
            alpha: 1.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(n: usize) -> Layout {
        Layout::new(&vec![[612.0, 792.0]; n])
    }

    fn view<'a>(pages: &'a Layout, outline: &'a [OutlineItem], rows: &'a [TocRow], selected: &'a [usize]) -> OverviewView<'a> {
        OverviewView {
            layout: pages,
            scroll: 0.0,
            current: 0,
            selected,
            edit: false,
            can_undo: false,
            can_redo: false,
            outline,
            rows,
            toc_scroll: 0.0,
            section: None,
            folded: &|_| false,
            hover: None,
            pressed: None,
            drag: None,
            enabled: [true; 5],
            menu: None,
            menu_enabled: [true; 4],
            motion: &|_| PageMotion::default(),
        }
    }

    fn item(title: &str, depth: usize, page: Option<usize>, has_children: bool) -> OutlineItem {
        OutlineItem { title: title.into(), depth, page, open: true, has_children }
    }

    fn rows_of(items: &[OutlineItem]) -> Vec<TocRow> {
        (0..items.len()).map(|i| TocRow { item: i, depth: items[i].depth, foldable: items[i].has_children }).collect()
    }

    fn drawn(v: &OverviewView<'_>, viewport: [f32; 2], dpr: f32) -> (Ui, OverviewLayout) {
        let l = OverviewLayout::new(viewport, dpr, !v.outline.is_empty(), v.edit);
        let mut ui = Ui::default();
        draw(v, &l, &mut ui);
        (ui, l)
    }

    #[test]
    fn the_overview_is_a_card_over_a_scrim_with_a_picture_and_a_number_for_every_visible_page() {
        let pages = layout(5);
        let (ui, l) = drawn(&view(&pages, &[], &[], &[]), [1280.0, 720.0], 1.0);
        assert_eq!(ui.panels[0].rect, [0.0, 0.0, 1280.0, 720.0], "the scrim covers the window");
        assert_eq!(ui.images.iter().map(|i| i.page).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        let numbers: Vec<&str> = ui.labels.iter().map(|x| x.text.as_str()).filter(|t| t.chars().all(|c| c.is_ascii_digit())).collect();
        assert_eq!(numbers, ["1", "2", "3", "4", "5"]);
        assert!(ui.labels.iter().any(|x| x.text == "All pages (5)"), "the title says how many");
        for image in &ui.images {
            assert_eq!(image.full, image.shown, "nothing is cut off at the top of the grid");
            assert!(image.shown[0] >= l.grid[0] && image.shown[2] <= l.grid[2]);
        }
    }

    #[test]
    fn what_is_scrolled_partly_out_of_the_grid_is_cut_at_its_edge_and_never_drawn_over_the_header() {
        let pages = layout(60);
        let mut v = view(&pages, &[], &[], &[]);
        let l = OverviewLayout::new([1280.0, 720.0], 1.0, false, false);
        v.scroll = l.cell(0, 0.0)[1] - l.grid[1] + 100.0; // into the first row
        let (ui, l) = drawn(&v, [1280.0, 720.0], 1.0);
        let cut = ui.images.iter().filter(|i| i.full != i.shown).count();
        assert!(cut > 0, "a row of pictures is cut by the header edge");
        for image in &ui.images {
            assert!(image.shown[1] >= l.grid[1] - 0.01 && image.shown[3] <= l.grid[3] + 0.01, "{image:?} stays in the grid");
        }
        for label in &ui.labels {
            if label.text.chars().all(|c| c.is_ascii_digit()) {
                assert!(label.at[1] >= l.grid[1] && label.at[1] <= l.grid[3], "a number is drawn only where it is in the grid: {label:?}");
            }
        }
        for panel in &ui.panels[1..] {
            let r = panel.rect;
            assert!(r.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn the_page_being_looked_at_and_the_chosen_pages_are_ringed_and_edit_mode_shows_the_circles() {
        let pages = layout(6);
        let chosen = [2, 4];
        let plain = drawn(&view(&pages, &[], &[], &[]), [1280.0, 720.0], 1.0).0;
        let mut v = view(&pages, &[], &[], &chosen);
        v.current = 1;
        let ringed = drawn(&v, [1280.0, 720.0], 1.0).0;
        assert!(ringed.lines.len() >= plain.lines.len() + 4 * 2, "rings for the current page and each chosen page, instead of the first page alone");
        v.edit = true;
        let editing = drawn(&v, [1280.0, 720.0], 1.0).0;
        assert!(editing.lines.len() > ringed.lines.len(), "a circle on every page, ticked on the chosen ones");
        let accent = |ui: &Ui| {
            ui.labels.iter().filter(|l| l.color == ACCENT && l.text.chars().all(|c| c.is_ascii_digit())).map(|l| l.text.clone()).collect::<Vec<_>>()
        };
        assert_eq!(accent(&editing), ["2", "3", "5"], "the numbers of the current and the chosen pages are in the accent colour");
    }

    #[test]
    fn the_table_of_contents_shows_its_rows_folds_and_the_section_being_read() {
        let pages = layout(4);
        let outline = vec![
            item("Chapter 1", 0, Some(0), true),
            item("1.1 Basics", 1, Some(1), false),
            item("Chapter 2", 0, Some(3), false),
            item("Appendix", 0, None, false),
        ];
        let rows = rows_of(&outline);
        let mut v = view(&pages, &outline, &rows, &[]);
        v.section = Some(1);
        let (ui, l) = drawn(&v, [1280.0, 720.0], 1.0);
        let titles: Vec<&str> = ui.labels.iter().map(|x| x.text.as_str()).collect();
        for t in ["Contents", "Chapter 1", "1.1 Basics", "Chapter 2", "Appendix"] {
            assert!(titles.contains(&t), "{t} is shown: {titles:?}");
        }
        let toc = l.toc.expect("a table of contents");
        for label in ui.labels.iter().filter(|x| x.at[0] < toc[2] && x.text != "Contents" && x.at[1] > toc[1]) {
            assert!(label.at[0] >= toc[0] && label.at[0] < toc[2], "{label:?} is in the column");
        }
        let section = ui.labels.iter().find(|x| x.text == "1.1 Basics").expect("label");
        assert_eq!(section.color, ACCENT, "the section being read stands out");
        let dim = ui.labels.iter().find(|x| x.text == "Appendix").expect("label");
        assert_eq!(dim.color, INK_DIM, "an entry that goes nowhere is dim");
        let with = ui.lines.len();
        let folded = OverviewView { folded: &|i| i == 0, ..view(&pages, &outline, &rows, &[]) };
        let (closed, _) = drawn(&folded, [1280.0, 720.0], 1.0);
        assert_eq!(closed.lines.len(), with, "the arrow is drawn pointing the other way, with as many lines");
        // Without an outline there is no column at all.
        let (none, l) = drawn(&view(&pages, &[], &[], &[]), [1280.0, 720.0], 1.0);
        assert!(l.toc.is_none() && none.labels.iter().all(|x| x.text != "Contents"));
    }

    #[test]
    fn long_titles_are_shortened_with_dots_to_fit_and_everything_is_finite_at_every_size() {
        let fitted = fit("A very long chapter title that cannot possibly fit in a narrow column", 18.0, 120.0);
        assert!(fitted.ends_with("...") && text::measure(&fitted, 18.0)[0] <= 120.0, "{fitted}");
        assert_eq!(fit("Short", 18.0, 500.0), "Short");
        assert_eq!(fit("Anything", 18.0, 1.0), "");
        let pages = layout(300);
        let outline = vec![item("Chapter 1", 0, Some(0), true); 50];
        let rows = rows_of(&outline);
        for ([w, h], dpr) in [([2000.0, 1070.0], 1.0), ([360.0, 640.0], 1.0), ([300.0, 260.0], 3.0), ([3840.0, 2054.0], 1.5)] {
            let mut v = view(&pages, &outline, &rows, &[1, 2]);
            v.edit = true;
            v.hover = Some(Hover::Page(2));
            v.pressed = Some(Hover::Control(Control::Add));
            let l = OverviewLayout::new([w, h], dpr, true, false);
            v.scroll = l.max_scroll(300) / 2.0;
            let (ui, _) = drawn(&v, [w, h], dpr);
            assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite()) && c.width.is_finite()), "{w}x{h}");
            assert!(ui.panels.iter().all(|p| p.rect.iter().all(|v| v.is_finite())));
            assert!(ui.labels.iter().all(|x| x.at.iter().all(|v| v.is_finite()) && x.size > 0.0));
            assert!(ui.images.len() < 200, "only what is in view: {}", ui.images.len());
        }
    }
}
