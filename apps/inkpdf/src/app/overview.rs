//! The pages overview on the GPU: its panels and lines go where the interface's do; its text is drawn to bitmaps (as
//! typed text is) and its pictures are small renders of the pages, both drawn as quads over its panels; and the ink on a
//! page is drawn into its picture from the same buffer the document's ink is drawn from, put where the picture is.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::gpu::{InkThumb, Quad};
use crate::text;
use crate::tiles::Key;
use crate::toolbar::{Align, Image, Label, Ui};

use super::{Paint, UiLayers};

/// What the overview needs drawn this frame, kept between frames so that its room is kept.
#[derive(Default)]
pub(super) struct Shown {
    /// The pictures and text of the interface, as built when it was last laid out.
    images: Vec<Image>,
    labels: Vec<Label>,
    /// The quads that draw them (text and pictures), and the ink in the pictures.
    pub(super) quads: Vec<Quad>,
    pub(super) thumbs: Vec<InkThumb>,
    /// The bitmaps of the text being shown: kept on the GPU however many other bitmaps there are.
    pub(super) keys: HashSet<u64>,
    /// The overview as it was drawn last, kept while it goes out over a moment after it is put away.
    ghost: Option<Ui>,
}

fn label_key(label: &Label) -> u64 {
    let mut hasher = DefaultHasher::new();
    (&label.text, label.size.to_bits(), label.color, "ui").hash(&mut hasher);
    hasher.finish()
}

impl UiLayers {
    /// Lay the overview out while it is up (every frame: its look follows the pointer and the scroll), or clear what it left.
    /// `true` if it is up and has taken the interface's place.
    pub(super) fn sync_overview_ui(&mut self, cx: &mut Paint) -> bool {
        if !cx.ed.overview_open() {
            // Just put away: it goes out, drawn as it was, a little clearer each frame.
            let leaving = cx.ed.overview_leaving();
            if leaving > 0.0
                && let Some(ghost) = &self.overview.ghost
            {
                let mut ui = ghost.clone();
                ui.fade(leaving);
                cx.gpu.set_ui(&ui.panels, &ui.lines);
                self.overview.images = ui.images;
                self.overview.labels = ui.labels;
                self.ui_built = None;
                return true;
            }
            self.overview.ghost = None;
            if !self.overview.images.is_empty() || !self.overview.labels.is_empty() {
                self.overview.images.clear();
                self.overview.labels.clear();
                self.overview.keys.clear();
                self.ui_built = None;
            }
            return false;
        }
        let mut ui = Ui::default();
        cx.ed.draw_overview(&mut ui);
        cx.gpu.set_ui(&ui.panels, &ui.lines);
        self.overview.ghost = Some(ui.clone());
        self.overview.images = ui.images;
        self.overview.labels = ui.labels;
        // What the interface was built from is gone from the GPU: it is built afresh when the overview leaves.
        self.ui_built = None;
        true
    }

    /// The Save As dialog takes the window as the overview does: its panels and lines go where the interface's do, and its text
    /// is drawn to bitmaps by [`Self::overview_frame`] (the dialog has no pictures). `true` if it is up.
    pub(super) fn sync_save_as_ui(&mut self, cx: &mut Paint) -> bool {
        if !cx.ed.save_as_open() {
            return false;
        }
        let mut ui = Ui::default();
        cx.ed.draw_save_as(&mut ui);
        cx.gpu.set_ui(&ui.panels, &ui.lines);
        self.overview.images.clear();
        self.overview.labels = ui.labels;
        // What the interface was built from is gone from the GPU: it is built afresh when the dialog leaves.
        self.ui_built = None;
        true
    }

    /// The quads of the overview's text and pictures, and the ink in the pictures, for this frame; the bitmaps and the
    /// small renders they need are made sure of first.
    pub(super) fn overview_frame(&mut self, cx: &mut Paint) {
        self.overview.quads.clear();
        self.overview.thumbs.clear();
        if !cx.ed.overview_open() && self.overview.images.is_empty() && self.overview.labels.is_empty() {
            return;
        }
        let viewport = cx.ed.view.viewport;
        // (The ink in the pictures is drawn once the overview is all there, and not while it goes out.)
        let appear = if cx.ed.overview_open() { cx.ed.overview_appear() } else { 0.0 };
        let mut used = HashSet::new();
        for label in &self.overview.labels {
            let key = label_key(label);
            if cx.gpu.text_size(key).is_none()
                && let Some(bitmap) = text::raster(&label.text, label.size, 1.0, label.color)
            {
                cx.gpu.upload_text(key, bitmap.width, bitmap.height, &bitmap.pixels);
            }
            let Some([bw, bh]) = cx.gpu.text_size(key) else { continue };
            used.insert(key);
            // The line's middle is where the label says; the bitmap has a margin round the text.
            let [width, height] = text::measure(&label.text, label.size);
            let left = match label.align {
                Align::Left => label.at[0],
                Align::Centre => label.at[0] - width / 2.0,
            };
            let (x, y) = ((left - text::PAD).round(), (label.at[1] - height / 2.0 - text::PAD).round());
            self.overview.quads.push(Quad { color: [label.alpha.clamp(0.0, 1.0); 4], ..Quad::text(key, [x, y, x + bw as f32, y + bh as f32]) });
        }
        self.overview.keys = used;
        let mut thumb_keys = HashSet::new();
        let mut thumb_texts = Vec::new();
        // (Taken for the loop, which needs the shell for the bitmaps of the text, and put back.)
        let images = std::mem::take(&mut self.overview.images);
        for image in &images {
            let (full, shown) = (image.full, image.shown);
            let (w, h) = ((full[2] - full[0]).max(1.0), (full[3] - full[1]).max(1.0));
            // The picture of the page the file draws it from, once it is there: not for a page that was added (it is
            // blank), nor yet for one that is still being rendered. What is drawn on the page shows either way.
            // (It is kept under the page of the file that draws the page.)
            let key = cx.doc.and_then(|d| d.render_page(image.page as usize)).map(|p| Key::Thumb { page: p });
            if let Some(key) = key.filter(|&k| cx.gpu.has(k)) {
                cx.gpu.touch(key, cx.frame);
                let uv = [(shown[0] - full[0]) / w, (shown[1] - full[1]) / h, (shown[2] - full[0]) / w, (shown[3] - full[1]) / h];
                let a = image.alpha.clamp(0.0, 1.0);
                self.overview.quads.push(Quad { uv, color: [a; 4], ..Quad::page(key, shown) });
            }
            let Some(page) = cx.ed.layout.page(image.page as usize) else { continue };
            if page.w <= 0.0 {
                continue;
            }
            let scale = w / page.w;
            // The typed text on the page (bitmaps, like the labels), over its picture.
            self.thumb_text_quads(cx, image, scale, &mut thumb_keys, &mut thumb_texts);
            let ranges: Vec<_> = self.ink_pages.iter().filter(|(p, _)| *p == image.page).map(|(_, r)| r.clone()).collect();
            if ranges.is_empty() {
                continue;
            }
            let clip_x0 = shown[0].floor().max(0.0);
            let clip_y0 = shown[1].floor().max(0.0);
            let clip = [clip_x0, clip_y0, shown[2].ceil().min(viewport[0]) - clip_x0, shown[3].ceil().min(viewport[1]) - clip_y0];
            // (The ink in a picture is drawn once the overview is fully there: it cannot be faded with it.)
            if clip[2] > 0.0 && clip[3] > 0.0 && appear >= 1.0 {
                self.overview.thumbs.push(InkThumb {
                    ranges,
                    origin: [page.x - full[0] / scale, page.y - full[1] / scale],
                    scale,
                    clip: clip.map(|v| v as u32),
                });
            }
        }
        self.overview.images = images;
        self.overview.quads.append(&mut thumb_texts);
        self.overview.keys.extend(thumb_keys);
    }
}
