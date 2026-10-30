//! The panel under a tool's button: a preview of what it will draw, the tool's own settings, the
//! thickness, the colours, and (opened by tapping the colour in use) a hue / saturation /
//! brightness editor for it. The pen's own setting is its smoothing; the shape tool's is which
//! shape. Like everything here it is rounded panels and lines, with no text: the sliders carry
//! small pictures at their ends instead (a small and a big dot; a jagged and a smooth line), and
//! the shapes are drawn on their buttons.

use std::f32::consts::TAU;

use crate::colour::{self, Hsv};
use crate::figure::shape_points;
use crate::geometry::{SHAPES, Shape};
use crate::ink::rect_contains;

use super::theme::Ui;
use super::{Tool, Toolbar, position_in, width_in, width_range};

/// What is open in the panel beyond its basics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelState {
    /// The colour slot whose editor is open.
    pub editing: Option<usize>,
}

/// Everything the panel's look depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelView {
    /// Which tool's panel it is: the pen or the shape tool.
    pub tool: Tool,
    pub editing: Option<usize>,
    pub width: f32,
    pub smoothing: f32,
    /// The shape the shape tool makes.
    pub shape: Shape,
    /// The colour slot in use.
    pub colour: usize,
    pub palette: [[u8; 4]; 5],
    /// The colour being made in the editor (kept as hue, saturation and brightness, so a grey does
    /// not forget its hue).
    pub hsv: Hsv,
    /// Pixels per point on the page, for a preview stroke the size it will really be.
    pub zoom: f32,
}

/// One of the three sliders of the colour editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Hue,
    Saturation,
    Value,
}

/// A slider that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    Width,
    Smoothing,
    Channel(Channel),
}

const CHANNELS: [Channel; 3] = [Channel::Hue, Channel::Saturation, Channel::Value];
/// Pieces a gradient track is drawn in; neighbouring pieces overlap, so it reads as smooth.
const GRADIENT_PIECES: usize = 36;
/// Points a preview stroke is made of.
const PREVIEW_POINTS: usize = 48;

/// The colour editor's place in the panel.
#[derive(Clone, Copy, Debug)]
struct EditorRows {
    track_x: [f32; 2],
    rows: [f32; 3],
    swatch: [f32; 2],
    divider: f32,
}

/// Where everything is, for a window of a given width and the tool button's place.
#[derive(Clone, Copy, Debug)]
pub struct ToolPanel {
    tool: Tool,
    /// `[x0, y0, x1, y1]` in pixels.
    pub rect: [f32; 4],
    unit: f32,
    px: f32,
    preview: [f32; 4],
    /// The row of shapes (the shape tool only).
    shapes_y: Option<f32>,
    shape_x: [f32; SHAPES.len()],
    width_y: f32,
    /// The smoothing slider (the pen only).
    smoothing_y: Option<f32>,
    track_x: [f32; 2],
    swatch_y: f32,
    swatch_x: [f32; 5],
    editor: Option<EditorRows>,
}

impl Toolbar {
    /// The panel of `tool` (the pen, the shape tool or the text tool; the pointer has none), unfolded from the bar
    /// under the tools and centred like it, kept on screen; `editing` makes room for the colour editor. (The card
    /// behind it is the bar's: see [`super::ContextBar::draw`].)
    pub fn tool_panel(&self, tool: Tool, viewport_width: f32, editing: bool) -> Option<ToolPanel> {
        if !tool.makes_items() {
            return None;
        }
        let (u, centre) = (self.unit, viewport_width / 2.0);
        let width = (8.4 * u).min(viewport_width - 0.4 * u).max(5.0 * u);
        let left = (centre - width / 2.0).clamp(0.2 * u, (viewport_width - width - 0.2 * u).max(0.2 * u));
        let right = left + width;
        // Just under the bar that is under the tools.
        let top = self.bottom() + self.context_drop() + 0.15 * u;
        // The text tool has only a size and the colours, so its panel is shorter.
        let text = tool == Tool::Text;
        let base = top + if text { 5.3 } else { 6.3 } * u;
        let editor = editing.then_some(EditorRows {
            track_x: [left + 0.8 * u, right - 2.2 * u],
            rows: [base + 0.55 * u, base + 1.3 * u, base + 2.05 * u],
            swatch: [right - 1.1 * u, base + 1.3 * u],
            divider: base - 0.1 * u,
        });
        let pitch = (width - 0.4 * u) / 5.0;
        let shape_pitch = (width - 0.4 * u) / SHAPES.len() as f32;
        // The first of the two rows under the preview is the tool's own setting, the second the
        // thickness; the pen has its thickness first and its smoothing after.
        let (first, second) = (top + 3.2 * u, top + 4.25 * u);
        let shapes = tool == Tool::Geometry;
        Some(ToolPanel {
            tool,
            rect: [left, top, right, if editing { base + 2.6 * u } else { base }],
            unit: u,
            px: self.dpr,
            preview: [left + 0.5 * u, top + 0.4 * u, right - 0.5 * u, top + 2.4 * u],
            shapes_y: shapes.then_some(first),
            shape_x: std::array::from_fn(|i| left + 0.2 * u + shape_pitch * (i as f32 + 0.5)),
            width_y: if shapes { second } else { first },
            smoothing_y: (tool == Tool::Pen).then_some(second),
            track_x: [left + 1.3 * u, right - 1.3 * u],
            swatch_y: top + if text { 4.4 } else { 5.4 } * u,
            swatch_x: std::array::from_fn(|i| left + 0.2 * u + pitch * (i as f32 + 0.5)),
            editor,
        })
    }
}

/// A white disc for a thumb, with a rim so it shows on the white panel.
fn white_thumb(ui: &mut Ui, at: [f32; 2], diameter: f32) {
    ui.disc(at, diameter, ui.pal.rim);
    ui.disc(at, diameter - 0.07 * diameter.max(8.0), ui.pal.white);
}

impl ToolPanel {
    /// The size of a button, which the panel is laid out in.
    pub fn unit(&self) -> f32 {
        self.unit
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.rect, p)
    }

    /// A point on `slider`, `fraction` (0 to 1) of the way along it, for tests that press it.
    #[cfg(test)]
    pub fn point_on(&self, slider: Slider, fraction: f32) -> [f32; 2] {
        let [x0, x1] = self.track_of(slider);
        let y = match (slider, &self.editor) {
            (Slider::Width, _) => self.width_y,
            (Slider::Smoothing, _) => self.smoothing_y.unwrap_or(f32::NAN),
            (Slider::Channel(c), Some(e)) => e.rows[CHANNELS.iter().position(|x| *x == c).unwrap_or(0)],
            (Slider::Channel(_), None) => f32::NAN,
        };
        [x0 + (x1 - x0) * fraction, y]
    }

    /// The middle of colour slot `slot`, for tests that press it.
    #[cfg(test)]
    pub fn swatch_centre(&self, slot: usize) -> [f32; 2] {
        [self.swatch_x[slot], self.swatch_y]
    }

    /// The middle of `shape`'s button, for tests that press it.
    #[cfg(test)]
    pub fn shape_centre(&self, shape: Shape) -> [f32; 2] {
        let i = SHAPES.iter().position(|s| *s == shape).unwrap_or(0);
        [self.shape_x[i], self.shapes_y.unwrap_or(f32::NAN)]
    }

    /// The slider a press at `p` lands on: its row, with a little slack above and below, and the
    /// little pictures at its ends count as part of it.
    pub fn slider_at(&self, p: [f32; 2]) -> Option<Slider> {
        let near = |y: f32, reach: f32| (p[1] - y).abs() <= reach * self.unit;
        if let Some(e) = &self.editor
            && p[0] >= e.track_x[0] - 0.4 * self.unit
            && p[0] <= e.track_x[1] + 0.4 * self.unit
            // The colour rows are close together: less slack, so a press between two is neither.
            && let Some((channel, _)) = CHANNELS.iter().zip(e.rows).find(|(_, y)| near(*y, 0.36))
        {
            return Some(Slider::Channel(*channel));
        }
        let across = p[0] >= self.rect[0] + 0.3 * self.unit && p[0] <= self.rect[2] - 0.3 * self.unit;
        if !across {
            return None;
        }
        if near(self.width_y, 0.42) {
            Some(Slider::Width)
        } else if self.smoothing_y.is_some_and(|y| near(y, 0.42)) {
            Some(Slider::Smoothing)
        } else {
            None
        }
    }

    /// The shape under a press at `p`, on the shape tool's row of shapes.
    pub fn shape_at(&self, p: [f32; 2]) -> Option<Shape> {
        let y = self.shapes_y?;
        let half = (self.shape_x[1] - self.shape_x[0]) / 2.0;
        if (p[1] - y).abs() > 0.62 * self.unit {
            return None;
        }
        self.shape_x.iter().position(|x| (p[0] - x).abs() <= half).map(|i| SHAPES[i])
    }

    /// The colour slot under a press at `p`.
    pub fn swatch_at(&self, p: [f32; 2]) -> Option<usize> {
        let half = (self.swatch_x[1] - self.swatch_x[0]) / 2.0;
        if (p[1] - self.swatch_y).abs() > 0.6 * self.unit {
            return None;
        }
        self.swatch_x.iter().position(|x| (p[0] - x).abs() <= half)
    }

    fn track_of(&self, slider: Slider) -> [f32; 2] {
        match (slider, &self.editor) {
            (Slider::Channel(_), Some(e)) => e.track_x,
            _ => self.track_x,
        }
    }

    /// What a pointer at `x` sets `slider` to: a width in points, a smoothing from 0 to 1, or a
    /// hue in degrees, a saturation or a brightness. The ends of the track give the extremes.
    pub fn value_at(&self, slider: Slider, x: f32) -> f32 {
        let [x0, x1] = self.track_of(slider);
        let t = ((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
        match slider {
            Slider::Width => width_in(width_range(self.tool), t),
            Slider::Smoothing | Slider::Channel(Channel::Saturation | Channel::Value) => t,
            Slider::Channel(Channel::Hue) => t * 359.99,
        }
    }

    pub fn draw(&self, view: &PanelView, ui: &mut Ui) {
        let u = self.unit;
        self.draw_preview(view, ui);
        let line = (0.04 * u).max(1.4 * self.px);
        // Thickness: a small dot to a big one.
        self.draw_track(self.width_y, position_in(width_range(self.tool), view.width), ui);
        ui.disc([self.rect[0] + 0.7 * u, self.width_y], 0.14 * u, ui.pal.ink);
        ui.disc([self.rect[2] - 0.7 * u, self.width_y], 0.5 * u, ui.pal.ink);
        if let Some(y) = self.smoothing_y {
            // Smoothing: a jagged line to a smooth one.
            self.draw_track(y, view.smoothing.clamp(0.0, 1.0), ui);
            let (left, right) = (self.rect[0] + 0.7 * u, self.rect[2] - 0.7 * u);
            let wave = |centre: f32, smooth: bool| -> Vec<[f32; 2]> {
                (0..=12)
                    .map(|i| {
                        let t = i as f32 / 12.0;
                        let wiggle = if smooth {
                            (t * TAU).sin() * 0.14
                        } else if i % 2 == 0 {
                            0.14
                        } else {
                            -0.14
                        };
                        [centre + (t - 0.5) * 0.7 * u, y + wiggle * u]
                    })
                    .collect()
            };
            for (centre, smooth) in [(left, false), (right, true)] {
                for w in wave(centre, smooth).windows(2) {
                    ui.line(w[0], w[1], line, ui.pal.ink);
                }
            }
        }
        if let Some(y) = self.shapes_y {
            // The shapes, each drawn on its button; the one in use is washed blue.
            for (&x, shape) in self.shape_x.iter().zip(SHAPES) {
                let on = shape == view.shape;
                if on {
                    ui.panel([x - 0.62 * u, y - 0.58 * u, x + 0.62 * u, y + 0.58 * u], 0.3 * u, ui.pal.accent_wash);
                }
                let icon = shape_points(shape, [x - 0.36 * u, y + 0.3 * u], [x + 0.36 * u, y - 0.3 * u]);
                for w in icon.windows(2) {
                    ui.line(w[0], w[1], line * 1.15, if on { ui.pal.accent } else { ui.pal.ink });
                }
            }
        }
        // The colours.
        for (i, (&x, &colour)) in self.swatch_x.iter().zip(&view.palette).enumerate() {
            let at = [x, self.swatch_y];
            if i == view.colour {
                // A ring: the accent, a white gap, then the colour.
                ui.disc(at, 0.86 * u, ui.pal.accent);
                ui.disc(at, 0.76 * u, ui.pal.white);
                ui.disc(at, 0.64 * u, colour);
            } else {
                ui.disc(at, 0.66 * u, ui.pal.rim);
                ui.disc(at, 0.6 * u, colour);
            }
        }
        if let Some(e) = &self.editor {
            self.draw_editor(e, view.hsv, ui);
        }
    }

    /// A slider's rail with the part up to the thumb filled, and the thumb.
    fn draw_track(&self, y: f32, t: f32, ui: &mut Ui) {
        let u = self.unit;
        let [x0, x1] = self.track_x;
        let thumb = x0 + (x1 - x0) * t;
        ui.line([x0, y], [x1, y], 0.12 * u, ui.pal.track);
        ui.line([x0, y], [thumb, y], 0.12 * u, ui.pal.accent);
        white_thumb(ui, [thumb, y], 0.56 * u);
    }

    /// What the tool will draw, in its colour and thickness, as it will look on the page (up to
    /// what fits): a curve for the pen, the chosen shape for the shape tool.
    fn draw_preview(&self, view: &PanelView, ui: &mut Ui) {
        let u = self.unit;
        ui.panel(self.preview, 0.3 * u, ui.pal.wash(10));
        let colour = view.palette[view.colour % view.palette.len()];
        let thickness = (view.width * view.zoom).clamp(1.0, 0.8 * u);
        let [x0, y0, x1, y1] = self.preview;
        let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0 - 0.7 * u, (y1 - y0) / 2.0 - 0.6 * u);
        if self.tool == Tool::Text {
            // Lines of text, as bars as thick as the letters will be.
            let bar = (view.width * view.zoom * 0.55).clamp(2.0, 0.55 * u);
            let gap = (bar * 1.7).min(0.9 * u);
            for (i, k) in [0.9, 0.62].into_iter().enumerate() {
                let y = cy - gap / 2.0 + i as f32 * gap;
                ui.line([x0 + 0.9 * u, y], [x0 + 0.9 * u + (x1 - x0 - 1.8 * u) * k, y], bar, colour);
            }
            return;
        }
        let points: Vec<[f32; 2]> = if self.tool == Tool::Geometry {
            shape_points(view.shape, [cx - rx * 0.8, cy + ry], [cx + rx * 0.8, cy - ry])
        } else {
            (0..PREVIEW_POINTS)
                .map(|i| {
                    let t = i as f32 / (PREVIEW_POINTS - 1) as f32;
                    [cx + rx * (2.0 * t - 1.0), cy - ry * (t * TAU * 1.25).sin()]
                })
                .collect()
        };
        for w in points.windows(2) {
            ui.line(w[0], w[1], thickness, colour);
        }
    }

    /// Hue, saturation and brightness sliders drawn as the gradients they change, and a swatch of
    /// the colour being made.
    fn draw_editor(&self, e: &EditorRows, hsv: Hsv, ui: &mut Ui) {
        let u = self.unit;
        ui.line([self.rect[0] + 0.5 * u, e.divider], [self.rect[2] - 0.5 * u, e.divider], self.px.max(1.0), ui.pal.separator);
        let [x0, x1] = e.track_x;
        for (channel, y) in CHANNELS.into_iter().zip(e.rows) {
            // The track: flat pieces side by side (a hair of overlap hides the seams), then a round
            // cap at each end in the end colour.
            let at = |t: f32| x0 + (x1 - x0) * t;
            let thickness = 0.24 * u;
            for piece in 0..GRADIENT_PIECES {
                let (t0, t1) = (piece as f32 / GRADIENT_PIECES as f32, (piece + 1) as f32 / GRADIENT_PIECES as f32);
                let colour = colour_along(channel, hsv, (t0 + t1) / 2.0);
                ui.panel([at(t0) - 0.5, y - thickness / 2.0, at(t1) + 0.5, y + thickness / 2.0], 0.0, colour);
            }
            for end in [0.0, 1.0] {
                ui.disc([at(end), y], thickness, colour_along(channel, hsv, end));
            }
            let position = position_of(channel, hsv);
            white_thumb(ui, [at(position), y], 0.54 * u);
            ui.disc([at(position), y], 0.34 * u, colour_along(channel, hsv, position));
        }
        let rgb = colour::hsv_to_rgb(hsv);
        ui.disc(e.swatch, 1.06 * u, ui.pal.rim);
        ui.disc(e.swatch, 1.0 * u, [rgb[0], rgb[1], rgb[2], 255]);
    }
}

/// Where along its slider `hsv` puts `channel`, 0 to 1.
fn position_of(channel: Channel, hsv: Hsv) -> f32 {
    match channel {
        Channel::Hue => hsv[0] / 360.0,
        Channel::Saturation => hsv[1],
        Channel::Value => hsv[2],
    }
    .clamp(0.0, 1.0)
}

/// The colour the slider shows at position `t` along `channel`, given the others as they are.
fn colour_along(channel: Channel, hsv: Hsv, t: f32) -> [u8; 4] {
    let rgb = match channel {
        Channel::Hue => colour::hsv_to_rgb([t * 360.0, 1.0, 1.0]),
        Channel::Saturation => colour::hsv_to_rgb([hsv[0], t, hsv[2].max(0.25)]),
        Channel::Value => colour::hsv_to_rgb([hsv[0], hsv[1], t]),
    };
    [rgb[0], rgb[1], rgb[2], 255]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Panel;
    use crate::tool::{MAX_WIDTH, MIN_WIDTH, PALETTE, TEXT_SIZES};
    use crate::toolbar::{position_in, slider_position};

    const WINDOWS: [(f32, f32); 5] = [(480.0, 1.0), (800.0, 1.0), (1100.0, 1.5), (1650.0, 1.5), (3840.0, 2.0)];

    fn view() -> PanelView {
        PanelView {
            tool: Tool::Pen,
            shape: Shape::Line,
            editing: None,
            width: 2.5,
            smoothing: 0.5,
            colour: 2,
            palette: PALETTE,
            hsv: colour::rgb_to_hsv([215, 40, 40]),
            zoom: 2.0,
        }
    }

    fn panel(editing: bool) -> ToolPanel {
        Toolbar::layout([1650.0, 1350.0], 1.5).tool_panel(Tool::Pen, 1650.0, editing).expect("panel")
    }

    #[test]
    fn the_panel_sits_under_the_pen_button_and_stays_on_screen() {
        for (w, dpr) in WINDOWS {
            let bar = Toolbar::layout([w, 900.0], dpr);
            for editing in [false, true] {
                let p = bar.tool_panel(Tool::Pen, w, editing).expect("panel");
                assert!(p.rect[1] > bar.bottom(), "below the bar at {w}");
                assert!(p.rect[0] >= 0.0 && p.rect[2] <= w + 0.5, "on screen at {w}: {:?}", p.rect);
            }
        }
    }

    #[test]
    fn the_editor_makes_the_panel_taller_and_nothing_else_moves() {
        let (plain, tall) = (panel(false), panel(true));
        assert!(tall.rect[3] > plain.rect[3]);
        assert_eq!(&tall.rect[..3], &plain.rect[..3]);
        assert_eq!((tall.width_y, tall.smoothing_y, tall.swatch_y), (plain.width_y, plain.smoothing_y, plain.swatch_y));
    }

    #[test]
    fn a_press_finds_the_slider_or_swatch_it_is_on() {
        let p = panel(false);
        let mid = (p.rect[0] + p.rect[2]) / 2.0;
        assert_eq!(p.slider_at([mid, p.width_y]), Some(Slider::Width));
        let smoothing_y = p.smoothing_y.expect("the pen has smoothing");
        assert_eq!(p.slider_at([mid, smoothing_y]), Some(Slider::Smoothing));
        assert_eq!(p.slider_at([mid, (p.width_y + smoothing_y) / 2.0 + 0.1]), None, "between two rows");
        assert_eq!(p.slider_at([p.rect[0] - 5.0, p.width_y]), None, "outside the panel");
        assert_eq!(p.slider_at([mid, p.swatch_y]), None, "a swatch is not a slider");
        for (i, x) in p.swatch_x.iter().enumerate() {
            assert_eq!(p.swatch_at([*x, p.swatch_y]), Some(i));
            assert!(p.contains([*x, p.swatch_y]));
        }
        assert_eq!(p.swatch_at([mid, p.width_y]), None);
        assert_eq!(p.swatch_at([p.rect[0] - 40.0, p.swatch_y]), None);
    }

    #[test]
    fn the_colour_sliders_exist_only_while_the_editor_is_open() {
        let (closed, open) = (panel(false), panel(true));
        let e = open.editor.expect("editor");
        let x = (e.track_x[0] + e.track_x[1]) / 2.0;
        assert_eq!(open.slider_at([x, e.rows[0]]), Some(Slider::Channel(Channel::Hue)));
        assert_eq!(open.slider_at([x, e.rows[1]]), Some(Slider::Channel(Channel::Saturation)));
        assert_eq!(open.slider_at([x, e.rows[2]]), Some(Slider::Channel(Channel::Value)));
        assert_eq!(open.slider_at([x, (e.rows[0] + e.rows[1]) / 2.0]), None, "midway between two rows");
        assert_eq!(open.slider_at([e.swatch[0], e.swatch[1]]), None, "the swatch is not a slider");
        assert_eq!(closed.slider_at([x, e.rows[0]]), None);
        // The thickness and smoothing sliders still work with the editor open, right above it.
        assert_eq!(open.slider_at([x, open.width_y]), Some(Slider::Width));
        assert_eq!(open.slider_at([x, open.smoothing_y.expect("smoothing")]), Some(Slider::Smoothing));
    }

    #[test]
    fn dragging_along_the_width_slider_runs_from_thinnest_to_thickest() {
        let p = panel(false);
        assert_eq!(p.value_at(Slider::Width, p.rect[0] - 500.0), MIN_WIDTH, "off the left end is the thinnest");
        assert!((p.value_at(Slider::Width, p.rect[2] + 500.0) - MAX_WIDTH).abs() < 1e-3, "off the right end is the thickest");
        let mut last = 0.0;
        for i in 0..=40 {
            let w = p.value_at(Slider::Width, p.rect[0] + (p.rect[2] - p.rect[0]) * i as f32 / 40.0);
            assert!(w >= last, "monotonic");
            last = w;
        }
    }

    #[test]
    fn smoothing_and_the_colour_sliders_run_their_full_range() {
        let p = panel(true);
        let [a, b] = p.track_x;
        assert_eq!(p.value_at(Slider::Smoothing, a - 99.0), 0.0);
        assert_eq!(p.value_at(Slider::Smoothing, b + 99.0), 1.0);
        assert!((p.value_at(Slider::Smoothing, (a + b) / 2.0) - 0.5).abs() < 1e-3);
        let [c, d] = p.editor.expect("editor").track_x;
        assert_eq!(p.value_at(Slider::Channel(Channel::Hue), c - 99.0), 0.0);
        assert!((p.value_at(Slider::Channel(Channel::Hue), d + 99.0) - 359.99).abs() < 1e-3, "the hue stops just short of a full turn");
        assert_eq!(p.value_at(Slider::Channel(Channel::Saturation), c - 5.0), 0.0);
        assert_eq!(p.value_at(Slider::Channel(Channel::Value), d + 5.0), 1.0);
    }

    #[test]
    fn the_thumbs_are_drawn_where_the_values_say() {
        let p = panel(false);
        let mut ui = Ui::default();
        p.draw(&PanelView { width: 5.0, smoothing: 0.25, ..view() }, &mut ui);
        let thumb = |y: f32, t: f32| {
            p.track_x[0] + (p.track_x[1] - p.track_x[0]) * t
                == ui
                    .lines
                    .iter()
                    .find(|c| c.a == c.b && c.a[1] == y && (c.a[0] - (p.track_x[0] + (p.track_x[1] - p.track_x[0]) * t)).abs() < 0.01)
                    .map_or(f32::NAN, |c| c.a[0])
        };
        assert!(thumb(p.width_y, slider_position(5.0)), "the width thumb");
        assert!(thumb(p.smoothing_y.expect("smoothing"), 0.25), "the smoothing thumb");
    }

    #[test]
    fn the_preview_is_in_the_pens_colour_and_grows_with_its_width() {
        let p = panel(false);
        let strokes = |width: f32| -> Vec<f32> {
            let mut ui = Ui::default();
            p.draw(&PanelView { width, ..view() }, &mut ui);
            ui.lines.iter().filter(|c| c.color == PALETTE[2] && c.a != c.b && c.a[1] < p.rect[1] + 3.0 * p.unit).map(|c| c.width).collect()
        };
        let (thin, thick) = (strokes(0.5), strokes(8.0));
        assert!(!thin.is_empty() && thin.len() == thick.len());
        assert!(thick[0] > thin[0] * 3.0, "{} vs {}", thin[0], thick[0]);
        // Whatever the width, the preview fits inside the panel.
        assert!(strokes(MAX_WIDTH)[0] <= 0.8 * p.unit + 1e-3);
    }

    #[test]
    fn the_colour_in_use_wears_a_ring_and_the_editor_draws_gradients_and_the_swatch() {
        let p = panel(true);
        let mut ui = Ui::default();
        p.draw(&PanelView { editing: Some(2), ..view() }, &mut ui);
        let ring = ui.lines.iter().filter(|c| c.a == [p.swatch_x[2], p.swatch_y] && c.color == crate::palette::LIGHT.accent).count();
        assert_eq!(ring, 1, "the colour in use has an accent ring");
        assert_eq!(ui.lines.iter().filter(|c| c.a == [p.swatch_x[1], p.swatch_y] && c.color == crate::palette::LIGHT.accent).count(), 0);
        let e = p.editor.expect("editor");
        let row =
            |y: f32| -> Vec<&Panel> { ui.panels.iter().filter(|q| ((q.rect[1] + q.rect[3]) / 2.0 - y).abs() < 0.01 && q.radius == 0.0).collect() };
        let hue = row(e.rows[0]);
        assert_eq!(hue.len(), GRADIENT_PIECES);
        assert!(hue[0].color[0] > 200 && hue[0].color[1] < 60, "starts red: {:?}", hue[0].color);
        assert!(hue[GRADIENT_PIECES / 3].color[1] > 150, "a third of the way round is green: {:?}", hue[GRADIENT_PIECES / 3].color);
        let value = row(e.rows[2]);
        assert!(value[0].color[0] < 20 && value[GRADIENT_PIECES - 1].color[0] > 200, "brightness runs from black to the colour");
        assert!(ui.lines.iter().any(|c| c.a == e.swatch && c.color == [215, 40, 40, 255]), "the swatch shows the colour being made");
    }

    #[test]
    fn the_text_panel_has_a_size_and_colours_and_no_smoothing() {
        for (w, dpr) in WINDOWS {
            let bar = Toolbar::layout([w, 900.0], dpr);
            let pen = bar.tool_panel(Tool::Pen, w, false).expect("pen panel");
            for editing in [false, true] {
                let p = bar.tool_panel(Tool::Text, w, editing).expect("text panel");
                assert!(p.smoothing_y.is_none() && p.shapes_y.is_none());
                assert!(editing || p.rect[3] < pen.rect[3], "shorter than the pen's at {w}");
                assert!(p.width_y < p.swatch_y, "the size above the colours");
                let mid = (p.rect[0] + p.rect[2]) / 2.0;
                assert_eq!(p.slider_at([mid, p.width_y]), Some(Slider::Width));
                assert_eq!(p.slider_at([mid, p.swatch_y]), None);
                for (i, x) in p.swatch_x.iter().enumerate() {
                    assert_eq!(p.swatch_at([*x, p.swatch_y]), Some(i));
                    assert!(p.contains([*x, p.swatch_y]));
                }
                // The slider runs over text sizes, not pen widths.
                assert_eq!(p.value_at(Slider::Width, p.rect[0] - 500.0), TEXT_SIZES.0);
                assert!((p.value_at(Slider::Width, p.rect[2] + 500.0) - TEXT_SIZES.1).abs() < 1e-2);
            }
        }
    }

    #[test]
    fn the_text_panel_draws_its_size_thumb_where_the_size_says_and_stays_inside() {
        let p = Toolbar::layout([1650.0, 900.0], 1.5).tool_panel(Tool::Text, 1650.0, true).expect("panel");
        for size in [TEXT_SIZES.0, 18.0, TEXT_SIZES.1] {
            let mut ui = Ui::default();
            p.draw(&PanelView { tool: Tool::Text, width: size, editing: Some(2), ..view() }, &mut ui);
            let t = position_in(TEXT_SIZES, size);
            let x = p.track_x[0] + (p.track_x[1] - p.track_x[0]) * t;
            assert!(ui.lines.iter().any(|c| c.a == c.b && c.a[1] == p.width_y && (c.a[0] - x).abs() < 0.01), "thumb for {size}");
            for c in &ui.lines {
                for q in [c.a, c.b] {
                    assert!(q.iter().all(|v| v.is_finite()));
                    assert!(q[0] >= p.rect[0] && q[0] <= p.rect[2] && q[1] >= p.rect[1] && q[1] <= p.rect[3], "{q:?} outside {:?}", p.rect);
                }
            }
        }
    }

    #[test]
    fn everything_drawn_stays_inside_the_panel_and_is_finite() {
        for (w, dpr) in WINDOWS {
            for editing in [false, true] {
                let p = Toolbar::layout([w, 900.0], dpr).tool_panel(Tool::Pen, w, editing).expect("panel");
                for width in [MIN_WIDTH, 2.5, MAX_WIDTH] {
                    let mut ui = Ui::default();
                    p.draw(&PanelView { editing: editing.then_some(2), width, zoom: 40.0, ..view() }, &mut ui);
                    for c in &ui.lines {
                        for q in [c.a, c.b] {
                            assert!(q.iter().all(|v| v.is_finite()));
                            assert!(
                                q[0] >= p.rect[0] && q[0] <= p.rect[2] && q[1] >= p.rect[1] && q[1] <= p.rect[3],
                                "{q:?} outside {:?} at {w}",
                                p.rect
                            );
                        }
                    }
                }
            }
        }
    }
}
