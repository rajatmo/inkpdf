//! A stroke: its points, style and bounding box, and the hit tests that need only the stroke.

use crate::figure::Figure;

use super::P;
use crate::plane::{dist2, dist2_point_segment, dist2_segments, point_in_polygon};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub rgba: [u8; 4],
    /// Width in points.
    pub width: f32,
}

/// Typed text, for a stroke that is a text box: what was typed and how big, in points. The stroke's
/// points are the outline of the box (top-left corner first, clockwise, closed); its style's colour
/// is the text's colour.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub text: String,
    pub size: f32,
}

/// How a text markup looks: a highlighter over the words, a line under them, a line through them, or a wavy line under them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

#[allow(dead_code, reason = "the select tool's action card and the settings use them (X4)")]
impl MarkupKind {
    pub const ALL: [MarkupKind; 4] = [MarkupKind::Highlight, MarkupKind::Underline, MarkupKind::StrikeOut, MarkupKind::Squiggly];

    /// The name the settings and the saved file use for it.
    pub fn name(self) -> &'static str {
        match self {
            MarkupKind::Highlight => "highlight",
            MarkupKind::Underline => "underline",
            MarkupKind::StrikeOut => "strikeout",
            MarkupKind::Squiggly => "squiggly",
        }
    }

    pub fn from_name(name: &str) -> Option<MarkupKind> {
        MarkupKind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// What to change about strokes that already exist: their colour, and their thickness (a text box's size).
/// A part left `None` stays as it is on every stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Restyle {
    pub rgba: Option<[u8; 4]>,
    pub width: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub id: u64,
    pub page: u32,
    pub style: Style,
    pub pts: Vec<P>,
    /// What the points were drawn from, for a shape: its anchors can be edited. `pts` is always
    /// what the figure draws.
    pub figure: Option<Figure>,
    /// For a text box, the text (see [`TextBlock`]); the box is drawn from this, not from `pts`.
    pub text: Option<TextBlock>,
    /// For a text markup, how it looks. Its `pts` are then the boxes it marks, two points (opposite corners) for each, and its style's
    /// colour (with its opacity) is the marker's.
    pub markup: Option<MarkupKind>,
    /// `[x0, y0, x1, y1]` including half the width.
    bbox: [f32; 4],
}

impl Stroke {
    pub(super) fn new(
        id: u64,
        page: u32,
        style: Style,
        pts: Vec<P>,
        figure: Option<Figure>,
        text: Option<TextBlock>,
        markup: Option<MarkupKind>,
    ) -> Self {
        let mut s = Self { id, page, style, pts, figure, text, markup, bbox: [0.0; 4] };
        s.refresh_bbox();
        s
    }

    fn refresh_bbox(&mut self) {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in &self.pts {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        let pad = self.style.width / 2.0;
        self.bbox = [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad];
    }

    /// The boxes a text markup marks (`[x0, y0, x1, y1]`, in document space); none for any other stroke.
    pub fn quads(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        let marks = self.markup.is_some();
        self.pts.as_chunks::<2>().0.iter().filter(move |_| marks).map(|&[a, b]| [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
    }

    pub fn bbox(&self) -> [f32; 4] {
        self.bbox
    }

    /// A copy moved by `(dx, dy)` onto `page`, still with this stroke's id (the store gives a copy that it
    /// adds an id of its own).
    pub fn moved(&self, dx: f32, dy: f32, page: u32) -> Stroke {
        let mut copy = self.clone();
        copy.translate(dx, dy);
        copy.page = page;
        copy
    }

    pub(super) fn translate(&mut self, dx: f32, dy: f32) {
        for p in &mut self.pts {
            p[0] += dx;
            p[1] += dy;
        }
        if let Some(figure) = &mut self.figure {
            figure.translate(dx, dy);
        }
        self.refresh_bbox();
    }

    /// Whether the stroke (with its width) comes within `radius` of `c`.
    pub fn touches_circle(&self, c: P, radius: f32) -> bool {
        let reach = radius + self.style.width / 2.0;
        let b = self.bbox;
        if c[0] < b[0] - radius || c[0] > b[2] + radius || c[1] < b[1] - radius || c[1] > b[3] + radius {
            return false;
        }
        // A text box is hit anywhere inside it, not only on its outline; a markup on any box it marks.
        if self.text.is_some() {
            return true;
        }
        if self.markup.is_some() {
            return self.quads().any(|q| c[0] >= q[0] - radius && c[0] <= q[2] + radius && c[1] >= q[1] - radius && c[1] <= q[3] + radius);
        }
        match self.pts.as_slice() {
            [] => false,
            [p] => dist2(*p, c) <= reach * reach,
            pts => pts.windows(2).any(|w| dist2_point_segment(c, w[0], w[1]) <= reach * reach),
        }
    }

    /// Whether the stroke comes within `radius` of the segment `a`–`b`.
    pub fn touches_segment(&self, a: P, b: P, radius: f32) -> bool {
        let reach = radius + self.style.width / 2.0;
        let sb = self.bbox;
        if a[0].max(b[0]) < sb[0] - radius || a[0].min(b[0]) > sb[2] + radius || a[1].max(b[1]) < sb[1] - radius || a[1].min(b[1]) > sb[3] + radius {
            return false;
        }
        if self.text.is_some() {
            return true;
        }
        if self.markup.is_some() {
            return self.quads().any(|q| segment_meets_box(a, b, [q[0] - radius, q[1] - radius, q[2] + radius, q[3] + radius]));
        }
        match self.pts.as_slice() {
            [] => false,
            [p] => dist2_point_segment(*p, a, b) <= reach * reach,
            pts => pts.windows(2).any(|w| dist2_segments(w[0], w[1], a, b) <= reach * reach),
        }
    }

    /// The fraction of the stroke's points inside `poly` (even-odd).
    pub fn fraction_inside(&self, poly: &[P]) -> f32 {
        if self.pts.is_empty() || poly.len() < 3 {
            return 0.0;
        }
        if self.markup.is_some() {
            let centres: Vec<P> = self.quads().map(|q| [(q[0] + q[2]) / 2.0, (q[1] + q[3]) / 2.0]).collect();
            return centres.iter().filter(|&&p| point_in_polygon(p, poly)).count() as f32 / centres.len().max(1) as f32;
        }
        self.pts.iter().filter(|&&p| point_in_polygon(p, poly)).count() as f32 / self.pts.len() as f32
    }
}

/// Whether the segment `a`–`b` meets the box `[x0, y0, x1, y1]` (Liang-Barsky).
fn segment_meets_box(a: P, b: P, r: [f32; 4]) -> bool {
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    for (p, q) in [(-dx, a[0] - r[0]), (dx, r[2] - a[0]), (-dy, a[1] - r[1]), (dy, r[3] - a[1])] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 <= t1
}
