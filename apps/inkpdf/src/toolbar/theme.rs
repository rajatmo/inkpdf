//! The look of the interface: a light, floating style (white rounded panels with soft shadows,
//! slate icons, one blue accent), and the small drawing helpers the toolbar and pop-ups share.

use crate::scene::{Capsule, Panel, Xform};

/// Panels: plain white, so page text never ghosts through them.
pub(super) const PANEL: [u8; 4] = [255, 255, 255, 255];
pub(super) const PANEL_EDGE: [u8; 4] = [20, 24, 40, 26];
/// Icons and text-weight lines.
pub(super) const INK: [u8; 4] = [64, 69, 82, 255];
pub(super) const INK_DIM: [u8; 4] = [176, 181, 192, 255];
pub(super) const ACCENT: [u8; 4] = [47, 128, 237, 255];
/// The wash behind an active button, and behind one the mouse is over.
pub(super) const ACCENT_WASH: [u8; 4] = [47, 128, 237, 58];
pub(super) const HOVER_WASH: [u8; 4] = [20, 24, 40, 16];
pub(super) const SEPARATOR: [u8; 4] = [20, 24, 40, 24];
/// The empty part of a slider.
pub(super) const TRACK: [u8; 4] = [20, 24, 40, 34];
/// The rim of a colour dot or a slider thumb, so a white one still shows on a white panel.
pub(super) const RIM: [u8; 4] = [20, 24, 40, 56];

/// Which side of its position a label sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// The position is the label's left edge.
    Left,
    /// The position is the label's middle.
    Centre,
}

/// A line of text, in pixels: drawn by the app from the font it has, not made of lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    /// The size of the text, in pixels.
    pub size: f32,
    pub color: [u8; 4],
    /// Where it is: `x` by `align`, `y` the middle of the line.
    pub at: [f32; 2],
    pub align: Align,
    /// How opaque it is, 0 to 1 (not in what the bitmap is made of: a fade does not make a bitmap for every step of it).
    pub alpha: f32,
}

/// A picture of a page, with its ink, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Image {
    pub page: u32,
    /// Where the whole picture would be.
    pub full: [f32; 4],
    /// The part of it that is shown (it can be cut off where it leaves the area it is in).
    pub shown: [f32; 4],
    /// How opaque it is, 0 to 1 (a page being dragged is see-through).
    pub alpha: f32,
}

/// What of `items` is from index `from` on (none if there are not that many).
fn tail<T>(items: &mut [T], from: usize) -> &mut [T] {
    let from = from.min(items.len());
    &mut items[from..]
}

/// A place in what is drawn (see [`Ui::mark`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    panels: usize,
    lines: usize,
    images: usize,
    labels: usize,
}

/// What the toolbar and its pop-ups are made of: rounded panels, drawn first, then pictures and text over them, then
/// lines over those.
#[derive(Clone, Default)]
pub struct Ui {
    pub panels: Vec<Panel>,
    pub lines: Vec<Capsule>,
    pub images: Vec<Image>,
    pub labels: Vec<Label>,
}

impl Ui {
    /// Where what is drawn so far ends: what is drawn after it can be given a change of place or opacity on its own
    /// (see [`Ui::apply`]).
    pub fn mark(&self) -> Mark {
        Mark { panels: self.panels.len(), lines: self.lines.len(), images: self.images.len(), labels: self.labels.len() }
    }

    /// Put `x` over everything drawn since `since`: the interface coming into view, going out, growing from a button.
    pub fn apply(&mut self, since: Mark, x: Xform) {
        if x.is_identity() {
            return;
        }
        let alpha = x.alpha.clamp(0.0, 1.0);
        for p in tail(&mut self.panels, since.panels) {
            *p = p.transformed(&x);
        }
        for c in tail(&mut self.lines, since.lines) {
            *c = c.transformed(&x);
        }
        for i in tail(&mut self.images, since.images) {
            i.full = x.rect(i.full);
            i.shown = x.rect(i.shown);
            i.alpha *= alpha;
        }
        for l in tail(&mut self.labels, since.labels) {
            // (The size of the text stays what it was drawn at; the position and the opacity follow.)
            l.at = x.point(l.at);
            l.alpha *= alpha;
        }
    }

    /// Everything drawn `f` (0 to 1) as opaque as it was: the interface coming into view.
    pub fn fade(&mut self, f: f32) {
        self.apply(Mark::default(), Xform::fade(f));
    }

    /// A filled circle of `diameter`.
    pub fn disc(&mut self, centre: [f32; 2], diameter: f32, color: [u8; 4]) {
        self.lines.push(Capsule::new(centre, centre, diameter, color));
    }

    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 4]) {
        self.lines.push(Capsule::new(a, b, width, color));
    }

    pub fn panel(&mut self, rect: [f32; 4], radius: f32, color: [u8; 4]) {
        self.panels.push(Panel::solid(rect, radius, color));
    }

    /// A white panel that floats: a wide soft shadow, a tight one, a hairline edge, the fill.
    /// `px` is one interface pixel at the current scale.
    pub fn floating(&mut self, rect: [f32; 4], radius: f32, px: f32) {
        let shifted = |dy: f32, grow: f32| [rect[0] - grow, rect[1] + dy - grow, rect[2] + grow, rect[3] + dy + grow];
        self.panels.push(Panel::new(shifted(8.0 * px, 0.0), radius, 28.0 * px, [16, 20, 44, 46]));
        self.panels.push(Panel::new(shifted(2.0 * px, 0.0), radius, 6.0 * px, [16, 20, 44, 40]));
        self.panels.push(Panel::solid(shifted(0.0, 0.5 * px), radius + 0.5 * px, PANEL_EDGE));
        self.panels.push(Panel::solid(rect, radius, PANEL));
    }
}
