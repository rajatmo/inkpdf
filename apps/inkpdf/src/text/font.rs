//! The font: the system's if the shell offered one, else the built-in Liberation Sans; its glyph outlines and advances.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::*;

use crate::plane::P;

/// Liberation Sans Regular (SIL Open Font License; see `assets/fonts/LICENSE-LiberationFonts.txt`), the font when the system gives none.
const BUNDLED_FONT: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");

/// A segment of an outline, in em units (y up).
#[derive(Clone, Copy, Debug)]
pub(super) enum Seg {
    Line(P),
    Quad(P, P),
    Cubic(P, P, P),
}

#[derive(Clone, Debug, Default)]
pub(super) struct Glyph {
    /// Advance in ems.
    pub(super) advance: f32,
    /// Each contour: where it starts and its segments.
    pub(super) contours: Vec<(P, Vec<Seg>)>,
}

struct Pen {
    scale: f32,
    contours: Vec<(P, Vec<Seg>)>,
}

impl Pen {
    fn at(&self, x: f32, y: f32) -> P {
        [x * self.scale, y * self.scale]
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.contours.push((p, Vec::new()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        if let Some(c) = self.contours.last_mut() {
            c.1.push(Seg::Line(p));
        }
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (c, p) = (self.at(cx, cy), self.at(x, y));
        if let Some(contour) = self.contours.last_mut() {
            contour.1.push(Seg::Quad(c, p));
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (a, b, p) = (self.at(c0x, c0y), self.at(c1x, c1y), self.at(x, y));
        if let Some(contour) = self.contours.last_mut() {
            contour.1.push(Seg::Cubic(a, b, p));
        }
    }

    fn close(&mut self) {}
}

pub(super) struct Face {
    font: FontRef<'static>,
    upem: f32,
    /// Ascent and descent in ems (descent positive).
    pub(super) ascent: f32,
    pub(super) descent: f32,
    glyphs: Mutex<HashMap<char, Glyph>>,
}

/// The font the system offered, if any (see [`set_font`]).
static PREFERRED: OnceLock<&'static [u8]> = OnceLock::new();

/// Offer a font to use instead of the built-in one, as the bytes of a TrueType file (the shell finds it with the platform's help).
/// Heard only before any text is first measured or drawn, and only if it can be read; returns whether it will be used.
pub fn set_font(bytes: Vec<u8>) -> bool {
    if FACE.get().is_some() || FontRef::new(&bytes).is_err() {
        return false;
    }
    // The bytes live as long as the program: the parser borrows them.
    PREFERRED.set(Box::leak(bytes.into_boxed_slice())).is_ok()
}

fn face_from(bytes: &'static [u8]) -> Option<Face> {
    let font = FontRef::new(bytes).ok()?;
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em).max(1.0);
    Some(Face { font, upem, ascent: metrics.ascent / upem, descent: -metrics.descent / upem, glyphs: Mutex::new(HashMap::new()) })
}

static FACE: OnceLock<Face> = OnceLock::new();

pub(super) fn face() -> &'static Face {
    FACE.get_or_init(|| {
        PREFERRED.get().and_then(|bytes| face_from(bytes)).or_else(|| face_from(BUNDLED_FONT)).expect("the built-in font is a valid font")
    })
}

impl Face {
    pub(super) fn glyph(&self, ch: char) -> Glyph {
        let mut cache = self.glyphs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(g) = cache.get(&ch) {
            return g.clone();
        }
        let g = self.load(ch).unwrap_or_else(|| self.load('?').unwrap_or_default());
        cache.insert(ch, g.clone());
        g
    }

    fn load(&self, ch: char) -> Option<Glyph> {
        let id = self.font.charmap().map(ch)?;
        let advance = self.font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(id)? / self.upem;
        let mut pen = Pen { scale: 1.0 / self.upem, contours: Vec::new() };
        if let Some(outline) = self.font.outline_glyphs().get(id) {
            outline.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen).ok()?;
        }
        Some(Glyph { advance, contours: pen.contours })
    }
}

/// A character's advance in ems.
pub(super) fn advance(ch: char) -> f32 {
    face().glyph(ch).advance
}
