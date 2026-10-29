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

use super::State;

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
}

fn label_key(label: &Label) -> u64 {
    let mut hasher = DefaultHasher::new();
    (&label.text, label.size.to_bits(), label.color, "ui").hash(&mut hasher);
    hasher.finish()
}

impl State {
    /// Lay the overview out while it is up (every frame: its look follows the pointer and the scroll), or clear what it left.
    /// `true` if it is up and has taken the interface's place.
    pub(super) fn sync_overview_ui(&mut self) -> bool {
        if !self.ed.overview_open() {
            if !self.overview.images.is_empty() || !self.overview.labels.is_empty() {
                self.overview.images.clear();
                self.overview.labels.clear();
                self.overview.keys.clear();
                self.ui_built = None;
            }
            return false;
        }
        let mut ui = Ui::default();
        self.ed.draw_overview(&mut ui);
        self.gpu.set_ui(&ui.panels, &ui.lines);
        self.overview.images = ui.images;
        self.overview.labels = ui.labels;
        // What the interface was built from is gone from the GPU: it is built afresh when the overview leaves.
        self.ui_built = None;
        true
    }

    /// The quads of the overview's text and pictures, and the ink in the pictures, for this frame; the bitmaps and the
    /// small renders they need are made sure of first.
    pub(super) fn overview_frame(&mut self) {
        self.overview.quads.clear();
        self.overview.thumbs.clear();
        if !self.ed.overview_open() {
            return;
        }
        let viewport = self.ed.view.viewport;
        let mut used = HashSet::new();
        for label in &self.overview.labels {
            let key = label_key(label);
            if self.gpu.text_size(key).is_none()
                && let Some(bitmap) = text::raster(&label.text, label.size, 1.0, label.color)
            {
                self.gpu.upload_text(key, bitmap.width, bitmap.height, &bitmap.pixels);
            }
            let Some([bw, bh]) = self.gpu.text_size(key) else { continue };
            used.insert(key);
            // The line's middle is where the label says; the bitmap has a margin round the text.
            let [width, height] = text::measure(&label.text, label.size);
            let left = match label.align {
                Align::Left => label.at[0],
                Align::Centre => label.at[0] - width / 2.0,
            };
            let (x, y) = ((left - text::PAD).round(), (label.at[1] - height / 2.0 - text::PAD).round());
            self.overview.quads.push(Quad::text(key, [x, y, x + bw as f32, y + bh as f32]));
        }
        self.overview.keys = used;
        for image in &self.overview.images {
            // (The picture is kept under the page of the file that draws the page.)
            let Some(file_page) = self.doc.as_ref().and_then(|d| d.render_page(image.page as usize)) else { continue };
            let key = Key::Thumb { page: file_page };
            if !self.gpu.has(key) {
                continue;
            }
            self.gpu.touch(key, self.frame);
            let (full, shown) = (image.full, image.shown);
            let (w, h) = ((full[2] - full[0]).max(1.0), (full[3] - full[1]).max(1.0));
            let uv = [(shown[0] - full[0]) / w, (shown[1] - full[1]) / h, (shown[2] - full[0]) / w, (shown[3] - full[1]) / h];
            let a = image.alpha.clamp(0.0, 1.0);
            self.overview.quads.push(Quad { uv, color: [a; 4], ..Quad::page(key, shown) });
            let ranges: Vec<_> = self.ink_pages.iter().filter(|(p, _)| *p == image.page).map(|(_, r)| r.clone()).collect();
            let Some(page) = self.ed.layout.page(image.page as usize) else { continue };
            if ranges.is_empty() || page.w <= 0.0 {
                continue;
            }
            let scale = w / page.w;
            let clip_x0 = shown[0].floor().max(0.0);
            let clip_y0 = shown[1].floor().max(0.0);
            let clip = [clip_x0, clip_y0, shown[2].ceil().min(viewport[0]) - clip_x0, shown[3].ceil().min(viewport[1]) - clip_y0];
            if clip[2] > 0.0 && clip[3] > 0.0 {
                self.overview.thumbs.push(InkThumb {
                    ranges,
                    origin: [page.x - full[0] / scale, page.y - full[1] / scale],
                    scale,
                    clip: clip.map(|v| v as u32),
                });
            }
        }
    }
}
