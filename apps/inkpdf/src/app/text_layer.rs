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
        self.text_draws.clear();
        let ed = &self.ed;
        self.text_index.refresh(ed);
        if !text::available() || (self.text_index.at.is_empty() && ed.text_edit().is_none()) {
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
        for s in self.text_index.at.iter().filter_map(|&i| strokes.get(i)) {
            let (Some(block), Some(&corner)) = (&s.text, s.pts.first()) else { continue };
            if !visible.contains(&(s.page as usize)) || Some(s.id) == editing || ed.erasing.contains(&s.id) {
                continue;
            }
            let moved = if ed.selection.contains(&s.id) { shift } else { [0.0, 0.0] };
            let key = ensure(&mut self.gpu, &block.text, block.size, s.style.rgba, level);
            used.insert(key);
            self.text_draws.push(TextDraw {
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
            self.text_draws.push(TextDraw { page: edit.page, key, top_left: edit.top_left, size: text::measure(&edit.text, edit.size) });
        }
        self.gpu.trim_texts(&used, KEEP_BITMAPS);
    }

    /// The quads of the text boxes on `page`.
    pub(super) fn text_quads(&self, page: usize, out: &mut Vec<Quad>) {
        for d in self.text_draws.iter().filter(|d| d.page as usize == page) {
            if let Some(rect) = self.text_rect(d) {
                out.push(Quad::text(d.key, rect));
            }
        }
    }

    /// Where on the screen a text box's bitmap goes: the box, and the margin the bitmap has around it.
    fn text_rect(&self, d: &TextDraw) -> Option<[f32; 4]> {
        let [bw, bh] = self.gpu.text_size(d.key)?;
        let inner = |side: u32| side as f32 - 2.0 * text::PAD;
        let [w, h] = d.size;
        if inner(bw) < 1.0 || inner(bh) < 1.0 || w <= 0.0 || h <= 0.0 {
            return None;
        }
        // The margin in points: the bitmap's own pixels are `w / inner(bw)` points wide.
        let (mx, my) = (text::PAD * w / inner(bw), text::PAD * h / inner(bh));
        let view = &self.ed.view;
        let [x0, y0] = view.doc_to_screen([d.top_left[0] - mx, d.top_left[1] - my]);
        let [x1, y1] = view.doc_to_screen([d.top_left[0] + w + mx, d.top_left[1] + h + my]);
        Some([x0, y0, x1, y1])
    }
}
