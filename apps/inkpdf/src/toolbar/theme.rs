//! The look of the interface: a floating style (rounded panels with soft shadows, line icons, one accent), in the colours of the
//! [`Palette`] the [`Ui`] carries, and the small drawing helpers the toolbar and pop-ups share.

use super::icons::Icon;
use crate::palette::Palette;
pub use crate::scene::LineSink;
use crate::scene::{Capsule, Panel, Xform};

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

impl Label {
    /// A label whose position is its left edge, fully opaque.
    pub fn left(text: impl Into<String>, size: f32, color: [u8; 4], at: [f32; 2]) -> Self {
        Self { text: text.into(), size, color, at, align: Align::Left, alpha: 1.0 }
    }

    /// A label whose position is its middle, fully opaque.
    pub fn centre(text: impl Into<String>, size: f32, color: [u8; 4], at: [f32; 2]) -> Self {
        Self { align: Align::Centre, ..Self::left(text, size, color, at) }
    }
}

/// How a round button is (what its wash and its icon's colour are made from).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonLook {
    /// A button that cannot be pressed now is dim and does not light up.
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    /// Switched on: in the accent colour.
    pub on: bool,
    /// It takes something away: its icon is in the danger colour.
    pub danger: bool,
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
    /// The colours it is drawn in (the light scheme unless it was made with [`Ui::with_palette`]).
    pub pal: Palette,
    pub panels: Vec<Panel>,
    pub lines: Vec<Capsule>,
    pub images: Vec<Image>,
    pub labels: Vec<Label>,
}

impl LineSink for Ui {
    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 4]) {
        self.lines.push(Capsule::new(a, b, width, color));
    }
}

impl Ui {
    /// A line, as the interface draws them (the same as through [`LineSink`], which the shapes made of lines are made through).
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 4]) {
        LineSink::line(self, a, b, width, color);
    }

    /// A filled circle of `diameter`.
    pub fn disc(&mut self, centre: [f32; 2], diameter: f32, color: [u8; 4]) {
        LineSink::disc(self, centre, diameter, color);
    }

    /// An empty drawing, to be made in these colours.
    pub fn with_palette(pal: Palette) -> Self {
        Self { pal, ..Self::default() }
    }

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

    pub fn panel(&mut self, rect: [f32; 4], radius: f32, color: [u8; 4]) {
        self.panels.push(Panel::solid(rect, radius, color));
    }

    /// A round button in `rect`: its wash (pressed, switched on or hovered, in that order of say; none while it cannot be pressed) and its `icon`,
    /// `half` across, in the colour of how the button is.
    pub(super) fn round_button(&mut self, rect: [f32; 4], icon: &Icon, look: ButtonLook, half: f32, line: f32) {
        let pal = self.pal;
        let wash = if look.pressed && look.enabled {
            Some(pal.pressed_wash)
        } else if look.on {
            Some(pal.accent_wash)
        } else if look.hovered && look.enabled {
            Some(pal.hover_wash)
        } else {
            None
        };
        if let Some(color) = wash {
            self.panel(rect, (rect[2] - rect[0]).min(rect[3] - rect[1]) / 2.0, color);
        }
        let color = match (look.enabled, look.on, look.danger) {
            (false, _, _) => pal.ink_dim,
            (_, true, _) => pal.accent,
            (_, _, true) => pal.danger,
            _ => pal.ink,
        };
        icon.draw(&mut self.lines, (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0, half, line, color);
    }

    /// A panel that floats: a wide soft shadow, a tight one, a hairline edge, the fill.
    /// `px` is one interface pixel at the current scale.
    pub fn floating(&mut self, rect: [f32; 4], radius: f32, px: f32) {
        let shifted = |dy: f32, grow: f32| [rect[0] - grow, rect[1] + dy - grow, rect[2] + grow, rect[3] + dy + grow];
        let pal = self.pal;
        self.panels.push(Panel::new(shifted(8.0 * px, 0.0), radius, 28.0 * px, pal.shadow(46)));
        self.panels.push(Panel::new(shifted(2.0 * px, 0.0), radius, 6.0 * px, pal.shadow(40)));
        self.panels.push(Panel::solid(shifted(0.0, 0.5 * px), radius + 0.5 * px, pal.panel_edge));
        self.panels.push(Panel::solid(rect, radius, pal.panel));
    }
}
