//! Laying the pages out for a frame: paper, shadow, preview and tiles as quads.

use std::collections::HashSet;

use crate::gpu::Quad;
use crate::tiles::{self, Key, TILE};

use super::{Paint, UiLayers};

/// What `State::scene` produces for one frame.
pub(super) struct Scene {
    pub quads: Vec<Quad>,
    /// The textures the quads use, so they count as recently drawn.
    pub drawn: Vec<Key>,
    /// How many visible pages have nothing to show yet.
    pub blank_pages: usize,
}

impl UiLayers {
    /// The quads to draw, the textures they use, and how many visible pages have nothing to show.
    pub(super) fn scene(&self, cx: &Paint) -> Scene {
        let mut scene = Scene { quads: Vec::new(), drawn: Vec::new(), blank_pages: 0 };
        let Some(doc) = cx.doc else { return scene };
        let view = &cx.ed.view;
        let zoom = view.zoom;
        let level = tiles::level_for(zoom);
        let (top, bottom) = view.visible_y();
        let resident: HashSet<Key> = cx.gpu.keys().collect();
        // (The paper under a page that has not been drawn yet, and under a page that was added: white, or the dark paper.)
        let paper = if cx.ed.dark_pages_active() {
            let [r, g, b] = crate::palette::DARK_PAPER.map(|v| f32::from(v) / 255.0);
            [r, g, b, 1.0]
        } else {
            [1.0; 4]
        };
        for page in cx.ed.layout.pages_in(top, bottom) {
            let Some(r) = cx.ed.layout.page(page) else { continue };
            // Snap the page to whole pixels: its edges stay sharp, and a tile rendered at the
            // current zoom lands exactly on the pixel grid.
            let o = view.doc_to_screen([r.x, r.y]);
            let (ox, oy) = (o[0].round(), o[1].round());
            let (w, h) = ((r.w * zoom).round(), (r.h * zoom).round());
            // A soft shadow: a few translucent copies of the page, growing and fading.
            for (grow, alpha) in [(6.0, 0.025), (3.5, 0.035), (1.5, 0.05)] {
                scene.quads.push(Quad::solid([ox - grow, oy - grow + 2.0, ox + w + grow, oy + h + grow + 2.0], [0.0, 0.0, 0.0, alpha]));
            }
            scene.quads.push(Quad::solid([ox, oy, ox + w, oy + h], paper));
            let mut showing = false;
            // When sharp tiles cover everything of this page that is on screen, nothing beneath
            // them is visible: skip the preview and the other zoom levels (less to draw, less
            // power).
            // (Its pictures are those of the page of the file that draws it; a page that was added has none.)
            let drawn_from = doc.render_page(page);
            let mut wanted = self.visible.iter().filter(|k| Some(k.page()) == drawn_from).peekable();
            let covered = wanted.peek().is_some() && wanted.all(|k| resident.contains(k));
            let preview = Key::Preview { page: drawn_from.unwrap_or(u32::MAX) };
            if !covered && resident.contains(&preview) {
                scene.quads.push(Quad::page(preview, [ox, oy, ox + w, oy + h]));
                scene.drawn.push(preview);
                showing = true;
            }
            // Resident tiles of this page at any zoom level, the levels furthest from the current
            // one first so the sharpest is drawn on top.
            let mut tiles: Vec<(i32, Key)> = resident
                .iter()
                .filter_map(|&k| match k {
                    Key::Tile { page: p, level: l, .. } if Some(p) == drawn_from && (!covered || l == level) => {
                        Some(((i32::from(l) - i32::from(level)).abs(), k))
                    }
                    _ => None,
                })
                .collect();
            tiles.sort_by_key(|&(distance, _)| std::cmp::Reverse(distance));
            for (_, key) in tiles {
                let Key::Tile { level: l, tx, ty, .. } = key else { continue };
                let Some([tw, th]) = cx.gpu.texture_size(key) else { continue };
                let scale = tiles::render_scale([r.w, r.h], l);
                let (px, py) = (u32::from(tx) * TILE, u32::from(ty) * TILE);
                let to_screen = |px: u32| (px as f32 / scale * zoom).round();
                let rect = [ox + to_screen(px), oy + to_screen(py), (ox + to_screen(px + tw)).min(ox + w), (oy + to_screen(py + th)).min(oy + h)];
                if rect[2] <= 0.0 || rect[3] <= 0.0 || rect[0] >= view.viewport[0] || rect[1] >= view.viewport[1] {
                    continue;
                }
                scene.quads.push(Quad::page(key, rect));
                scene.drawn.push(key);
                showing = true;
            }
            if !showing {
                scene.blank_pages += 1;
            }
            self.text_quads(cx, page, &mut scene.quads);
        }
        scene
    }
}
