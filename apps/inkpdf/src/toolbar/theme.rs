//! The look of the interface: a light, floating style (white rounded panels with soft shadows,
//! slate icons, one blue accent), and the small drawing helpers the toolbar and pop-ups share.

use crate::scene::{Capsule, Panel};

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

/// What the toolbar and its pop-ups are made of: rounded panels, drawn first, then pictures and text over them, then
/// lines over those.
#[derive(Default)]
pub struct Ui {
    pub panels: Vec<Panel>,
    pub lines: Vec<Capsule>,
    pub images: Vec<Image>,
    pub labels: Vec<Label>,
}

impl Ui {
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
