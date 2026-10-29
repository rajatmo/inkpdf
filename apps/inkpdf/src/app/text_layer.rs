//! Typed text on the GPU: each visible text box is drawn to a bitmap (see [`crate::text`]) at the
//! zoom it is shown at, uploaded once, and drawn as a quad over its page. A bitmap is kept under a
//! hash of what it shows, so a box that is not edited, moved or zoomed is never drawn again.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::editor::Editor;
use crate::gpu::{Gpu, Quad};
use crate::ink::P;
use crate::text;
use crate::tiles;
use crate::toolbar::Image;

use super::State;

/// Text bitmaps kept on the GPU beyond the ones in view.
const KEEP_BITMAPS: usize = 256;

/// A text box to draw this frame.
pub(super) struct TextDraw {
    pub page: u32,
    key: u64,
    /// Top-left corner, in document space.
    top_left: P,
    /// The box's size in points.
    size: [f32; 2],
}

/// The text boxes of the ink, found again only when the ink has changed.
#[derive(Default)]
pub(super) struct TextIndex {
    /// The ink's revision and stroke count this was built from.
    built_from: Option<(u64, usize)>,
    /// Positions in the ink's strokes.
    at: Vec<usize>,
}

impl TextIndex {
    fn refresh(&mut self, ed: &Editor) {
        let now = (ed.ink.revision(), ed.ink.strokes().len());
        if self.built_from != Some(now) {
            self.built_from = Some(now);
            self.at = ed.ink.strokes().iter().enumerate().filter(|(_, s)| s.text.is_some()).map(|(i, _)| i).collect();
        }
    }
}

fn key_of(text: &str, size: f32, rgba: [u8; 4], level: i16) -> u64 {
    let mut hasher = DefaultHasher::new();
    (text, size.to_bits(), rgba, level).hash(&mut hasher);
    hasher.finish()
}

/// Make sure the bitmap of `text` is on the GPU at `level`; the key it is under.
fn ensure(gpu: &mut Gpu, text: &str, size: f32, rgba: [u8; 4], level: i16) -> u64 {
    let key = key_of(text, size, rgba, level);
    if gpu.text_size(key).is_none()
        && let Some(bitmap) = text::raster(text, size, tiles::level_scale(level), rgba)
    {
        gpu.upload_text(key, bitmap.width, bitmap.height, &bitmap.pixels);
    }
    key
}

impl State {
    /// Work out which text boxes to draw this frame and have their bitmaps ready.
    pub(super) fn sync_texts(&mut self) {
        self.layers.text_draws.clear();
        let ed = &self.ed;
        self.layers.text_index.refresh(ed);
        if self.layers.text_index.at.is_empty() && ed.text_edit().is_none() {
            // No text boxes, but the pages overview's text is bitmaps too (a page number each): kept only as long as it is shown.
            self.gpu.trim_texts(&self.layers.overview.keys, KEEP_BITMAPS);
            return;
        }
        let (top, bottom) = ed.view.visible_y();
        let visible = ed.layout.pages_in(top, bottom);
        let level = tiles::level_for(ed.view.zoom);
        // A box being dragged with the selection moves with it; one being edited is drawn from the
        // edit, one being erased not at all.
        let shift = ed.drag_offset();
        let editing = ed.text_edit().and_then(|e| e.target);
        let mut used = HashSet::new();
        let strokes = ed.ink.strokes();
        for s in self.layers.text_index.at.iter().filter_map(|&i| strokes.get(i)) {
            let (Some(block), Some(&corner)) = (&s.text, s.pts.first()) else { continue };
            if !visible.contains(&(s.page as usize)) || Some(s.id) == editing || ed.erasing.contains(&s.id) {
                continue;
            }
            let moved = if ed.selection.contains(&s.id) { shift } else { [0.0, 0.0] };
            let key = ensure(&mut self.gpu, &block.text, block.size, s.style.rgba, level);
            used.insert(key);
            self.layers.text_draws.push(TextDraw {
                page: s.page,
                key,
                top_left: [corner[0] + moved[0], corner[1] + moved[1]],
                size: text::measure(&block.text, block.size),
            });
        }
        if let Some(edit) = ed.text_edit()
            && visible.contains(&(edit.page as usize))
        {
            let key = ensure(&mut self.gpu, &edit.text, edit.size, edit.rgba, level);
            used.insert(key);
            self.layers.text_draws.push(TextDraw { page: edit.page, key, top_left: edit.top_left, size: text::measure(&edit.text, edit.size) });
        }
        used.extend(self.layers.overview.keys.iter().copied());
        self.gpu.trim_texts(&used, KEEP_BITMAPS);
    }

    /// The quads of the text boxes on `page`.
    pub(super) fn text_quads(&self, page: usize, out: &mut Vec<Quad>) {
        for d in self.layers.text_draws.iter().filter(|d| d.page as usize == page) {
            if let Some(rect) = self.text_rect(d) {
                out.push(Quad::text(d.key, rect));
            }
        }
    }

    /// Where on the screen a text box's bitmap goes: the box, and the margin the bitmap has around it.
    fn text_rect(&self, d: &TextDraw) -> Option<[f32; 4]> {
        let view = &self.ed.view;
        bitmap_rect(self.gpu.text_size(d.key)?, d.size, d.top_left, |p| view.doc_to_screen(p))
    }

    /// The quads of the text boxes on the page of `image`, its small picture in the pages overview, as that shows them:
    /// `scale` is its pixels to a point, and what is outside the part of the picture that is in view is cut off. The keys
    /// of the bitmaps used are added to `keys`, to be kept.
    pub(super) fn thumb_text_quads(&mut self, image: &Image, scale: f32, keys: &mut HashSet<u64>, out: &mut Vec<Quad>) {
        if self.layers.text_index.at.is_empty() {
            return;
        }
        let (page, origin, shown, alpha) = (image.page, [image.full[0], image.full[1]], image.shown, image.alpha);
        let Some(at) = self.ed.layout.page(page as usize).map(|r| [r.x, r.y]) else { return };
        // The bitmaps are made at the size they are shown, so that they are crisp and small.
        let level = tiles::level_for(scale);
        let strokes = self.ed.ink.strokes();
        for s in self.layers.text_index.at.iter().filter_map(|&i| strokes.get(i)) {
            let (Some(block), Some(&corner)) = (&s.text, s.pts.first()) else { continue };
            if s.page != page {
                continue;
            }
            let key = ensure(&mut self.gpu, &block.text, block.size, s.style.rgba, level);
            keys.insert(key);
            let Some(size) = self.gpu.text_size(key) else { continue };
            let map = |p: P| [origin[0] + (p[0] - at[0]) * scale, origin[1] + (p[1] - at[1]) * scale];
            if let Some(rect) = bitmap_rect(size, text::measure(&block.text, block.size), corner, map)
                && let Some(quad) = clipped(key, rect, shown, alpha)
            {
                out.push(quad);
            }
        }
    }
}

/// Where a text bitmap of `bitmap` pixels goes for a box of `size` points at `top_left`: the box and the margin the
/// bitmap has around it, put where `map` takes document points to.
fn bitmap_rect(bitmap: [u32; 2], size: [f32; 2], top_left: P, map: impl Fn(P) -> [f32; 2]) -> Option<[f32; 4]> {
    let inner = |side: u32| side as f32 - 2.0 * text::PAD;
    let [w, h] = size;
    if inner(bitmap[0]) < 1.0 || inner(bitmap[1]) < 1.0 || w <= 0.0 || h <= 0.0 {
        return None;
    }
    // The margin in points: the bitmap's own pixels are `w / inner(bw)` points wide.
    let (mx, my) = (text::PAD * w / inner(bitmap[0]), text::PAD * h / inner(bitmap[1]));
    let [x0, y0] = map([top_left[0] - mx, top_left[1] - my]);
    let [x1, y1] = map([top_left[0] + w + mx, top_left[1] + h + my]);
    Some([x0, y0, x1, y1])
}

/// The quad of a text bitmap in `rect`, cut off at `bounds` (the part of the bitmap that is left is what it shows), `alpha`
/// as opaque; none when nothing is left.
fn clipped(key: u64, rect: [f32; 4], bounds: [f32; 4], alpha: f32) -> Option<Quad> {
    let cut = [rect[0].max(bounds[0]), rect[1].max(bounds[1]), rect[2].min(bounds[2]), rect[3].min(bounds[3])];
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    if cut[2] <= cut[0] || cut[3] <= cut[1] || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let uv = [(cut[0] - rect[0]) / w, (cut[1] - rect[1]) / h, (cut[2] - rect[0]) / w, (cut[3] - rect[1]) / h];
    let a = alpha.clamp(0.0, 1.0);
    Some(Quad { uv, color: [a; 4], ..Quad::text(key, cut) })
}
