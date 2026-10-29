//! Which page tiles to render for a viewport, and bookkeeping for the textures kept for them.
//! Pure logic: no GPU types, so it is unit-tested.
//!
//! A page is drawn from a whole-page low-resolution *preview* (always resident for pages near the
//! viewport, so scrolling never shows a blank page) with sharp 512-pixel *tiles* on top, rendered
//! at a quantised zoom *level* so a small zoom change does not invalidate the cache.

use std::collections::HashMap;

use crate::pdf::{RenderRequest, Tile, device_pixels, effective_scale};

use crate::view::{Layout, View};

pub const TILE: u32 = 512;
/// Zoom levels per doubling: neighbouring levels differ by 2.9%, so a texture is never drawn more
/// than 1.5% off its rendered size.
const LEVELS_PER_OCTAVE: f32 = 24.0;
/// The longest side of a page preview, in pixels.
const PREVIEW_SIDE: f32 = 1024.0;
/// The longest side of a page thumbnail, in pixels.
const THUMB_SIDE: f32 = 360.0;
/// Pages kept as previews beyond the visible ones.
pub const PREVIEW_REACH: usize = 3;
/// Never ask for more tiles than this for one view (a guard against a degenerate viewport).
const MAX_TILES: usize = 400;

/// Highest page number a request tag can carry.
pub const MAX_PAGES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Preview {
        page: u32,
    },
    /// A small picture of a whole page, for the pages overview (see [`THUMB_SIDE`]).
    Thumb {
        page: u32,
    },
    Tile {
        page: u32,
        level: i16,
        tx: u16,
        ty: u16,
    },
}

impl Key {
    /// The same key for another page.
    pub fn with_page(self, page: u32) -> Key {
        match self {
            Key::Preview { .. } => Key::Preview { page },
            Key::Thumb { .. } => Key::Thumb { page },
            Key::Tile { level, tx, ty, .. } => Key::Tile { page, level, tx, ty },
        }
    }

    pub fn page(self) -> u32 {
        match self {
            Key::Preview { page } | Key::Thumb { page } | Key::Tile { page, .. } => page,
        }
    }

    /// Pack into a request tag, bit 60 (the renderer's print tag) kept clear.
    pub fn to_tag(self) -> u64 {
        match self {
            Key::Preview { page } => (1 << 62) | (u64::from(page) << 40),
            Key::Thumb { page } => (1 << 61) | (u64::from(page) << 40),
            Key::Tile { page, level, tx, ty } => {
                (u64::from(page) << 40) | (u64::from((level as i32 + 32768) as u16) << 24) | (u64::from(tx & 0xfff) << 12) | u64::from(ty & 0xfff)
            }
        }
    }

    pub fn from_tag(tag: u64) -> Key {
        let page = ((tag >> 40) & 0xf_ffff) as u32;
        if tag & (1 << 62) != 0 {
            return Key::Preview { page };
        }
        if tag & (1 << 61) != 0 {
            return Key::Thumb { page };
        }
        Key::Tile { page, level: (((tag >> 24) & 0xffff) as i32 - 32768) as i16, tx: ((tag >> 12) & 0xfff) as u16, ty: (tag & 0xfff) as u16 }
    }
}

/// The zoom level a zoom is rendered at.
pub fn level_for(zoom: f32) -> i16 {
    (zoom.max(1e-3).log2() * LEVELS_PER_OCTAVE).round().clamp(-2000.0, 2000.0) as i16
}

pub fn level_scale(level: i16) -> f32 {
    (f32::from(level) / LEVELS_PER_OCTAVE).exp2()
}

/// Device pixels per point a page is actually rendered at for `level`: the renderer caps the size
/// of a page, so a huge page at a high zoom is rendered smaller and drawn scaled up.
pub fn render_scale(page_pt: [f32; 2], level: i16) -> f32 {
    effective_scale(page_pt[0], page_pt[1], level_scale(level))
}

fn preview_scale(page_pt: [f32; 2]) -> f32 {
    (PREVIEW_SIDE / page_pt[0].max(page_pt[1]).max(1.0)).min(2.0)
}

fn thumb_scale(page_pt: [f32; 2]) -> f32 {
    (THUMB_SIDE / page_pt[0].max(page_pt[1]).max(1.0)).min(2.0)
}

/// The render request for a key.
pub fn request(key: Key, page_pt: [f32; 2]) -> RenderRequest {
    let page = key.page() as usize;
    match key {
        Key::Preview { .. } => RenderRequest { page, scale: preview_scale(page_pt), tag: key.to_tag(), preview: true, ..RenderRequest::default() },
        Key::Thumb { .. } => RenderRequest { page, scale: thumb_scale(page_pt), tag: key.to_tag(), preview: true, ..RenderRequest::default() },
        Key::Tile { level, tx, ty, .. } => {
            let scale = render_scale(page_pt, level);
            let (pw, ph) = (device_pixels(page_pt[0], scale), device_pixels(page_pt[1], scale));
            let (x, y) = (u32::from(tx) * TILE, u32::from(ty) * TILE);
            let tile = Tile { x, y, w: TILE.min(pw.saturating_sub(x)).max(1), h: TILE.min(ph.saturating_sub(y)).max(1) };
            RenderRequest { page, scale, tag: key.to_tag(), tile: Some(tile), ..RenderRequest::default() }
        }
    }
}

/// What to render for the current view, most urgent first within each list.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Previews of the visible pages.
    pub previews: Vec<Key>,
    /// Sharp tiles covering the viewport.
    pub visible: Vec<Key>,
    /// Previews of pages just outside the viewport.
    pub near_previews: Vec<Key>,
    /// Tiles just outside the viewport, rendered while idle so a scroll finds them ready.
    pub prefetch: Vec<Key>,
}

impl Plan {
    /// The plan with every key pointed at the page `to` says (the page of the file it is drawn from), and those of pages that
    /// are drawn from none (a blank page) left out.
    pub fn mapped(&self, to: impl Fn(u32) -> Option<u32>) -> Plan {
        let map = |keys: &[Key]| keys.iter().filter_map(|k| to(k.page()).map(|p| k.with_page(p))).collect::<Vec<_>>();
        Plan { previews: map(&self.previews), visible: map(&self.visible), near_previews: map(&self.near_previews), prefetch: map(&self.prefetch) }
    }

    /// Every wanted key in render order, without those for which `have` is true.
    pub fn queue(&self, mut have: impl FnMut(Key) -> bool) -> Vec<Key> {
        self.previews.iter().chain(&self.visible).chain(&self.near_previews).chain(&self.prefetch).copied().filter(|&k| !have(k)).collect()
    }
}

pub fn plan(layout: &Layout, view: &View) -> Plan {
    let mut plan = Plan::default();
    if layout.is_empty() {
        return plan;
    }
    let (top, bottom) = view.visible_y();
    let left = view.scroll[0];
    let right = left + view.viewport[0] / view.zoom;
    let level = level_for(view.zoom);
    let centre = [(left + right) / 2.0, (top + bottom) / 2.0];
    // Prefetch half a viewport beyond each edge.
    let (pad_x, pad_y) = ((right - left) / 2.0, (bottom - top) / 2.0);
    let visible_pages = layout.pages_in(top, bottom);

    let mut tiles: Vec<(f32, Key, bool)> = Vec::new();
    for page in layout.pages_in(top - pad_y, bottom + pad_y) {
        let Some(rect) = layout.page(page) else { continue };
        let on_screen = visible_pages.contains(&page);
        let page_u32 = page as u32;
        if on_screen {
            plan.previews.push(Key::Preview { page: page_u32 });
        }
        let scale = render_scale([rect.w, rect.h], level);
        let (wpx, hpx) = (device_pixels(rect.w, scale), device_pixels(rect.h, scale));
        let (tiles_x, tiles_y) = (wpx.div_ceil(TILE), hpx.div_ceil(TILE));
        // The tile range overlapping the viewport grown by the prefetch pad, in page pixels.
        let px = |doc: f32, origin: f32| ((doc - origin) * scale).max(0.0);
        let (x0, x1) = (px(left - pad_x, rect.x), px(right + pad_x, rect.x));
        let (y0, y1) = (px(top - pad_y, rect.y), px(bottom + pad_y, rect.y));
        let range = |lo: f32, hi: f32, count: u32| -> std::ops::Range<u32> {
            let first = ((lo / TILE as f32).floor() as u32).min(count);
            let last = ((hi / TILE as f32).floor() as u32 + 1).min(count);
            first..last.max(first)
        };
        for ty in range(y0, y1, tiles_y) {
            for tx in range(x0, x1, tiles_x) {
                let (Ok(tx16), Ok(ty16)) = (u16::try_from(tx), u16::try_from(ty)) else { continue };
                // Is this tile in the viewport itself, not just the prefetch pad?
                let tile_doc = [rect.x + (tx * TILE) as f32 / scale, rect.y + (ty * TILE) as f32 / scale];
                let tile_size = TILE as f32 / scale;
                let seen = tile_doc[0] < right && tile_doc[0] + tile_size > left && tile_doc[1] < bottom && tile_doc[1] + tile_size > top;
                let mid = [tile_doc[0] + tile_size / 2.0, tile_doc[1] + tile_size / 2.0];
                let dist = (mid[0] - centre[0]).powi(2) + (mid[1] - centre[1]).powi(2);
                tiles.push((dist, Key::Tile { page: page_u32, level, tx: tx16, ty: ty16 }, seen));
            }
        }
        if !on_screen {
            plan.near_previews.push(Key::Preview { page: page_u32 });
        }
    }
    // Previews a little further out, so a fast scroll finds something to draw.
    let near = layout.pages_in(top - pad_y, bottom + pad_y);
    let (lo, hi) = (near.start.saturating_sub(PREVIEW_REACH), (near.end + PREVIEW_REACH).min(layout.len()));
    for page in lo..hi {
        let key = Key::Preview { page: page as u32 };
        if !plan.previews.contains(&key) && !plan.near_previews.contains(&key) {
            plan.near_previews.push(key);
        }
    }
    tiles.sort_by(|a, b| a.0.total_cmp(&b.0));
    tiles.truncate(MAX_TILES);
    for (_, key, seen) in tiles {
        if seen { plan.visible.push(key) } else { plan.prefetch.push(key) }
    }
    plan
}

/// Which textures are resident and how recently they were drawn, for eviction.
#[derive(Debug, Default)]
pub struct Residency {
    entries: HashMap<Key, Entry>,
    bytes: usize,
}

#[derive(Debug)]
struct Entry {
    bytes: usize,
    last_used: u64,
}

impl Residency {
    #[cfg(test)]
    pub fn contains(&self, key: Key) -> bool {
        self.entries.contains_key(&key)
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn insert(&mut self, key: Key, bytes: usize, frame: u64) {
        if let Some(old) = self.entries.insert(key, Entry { bytes, last_used: frame }) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
    }

    pub fn touch(&mut self, key: Key, frame: u64) {
        if let Some(e) = self.entries.get_mut(&key) {
            e.last_used = frame;
        }
    }

    /// Drop least recently used entries until at most `budget` bytes remain. Entries used in
    /// `frame` are kept even if that exceeds the budget. Returns the dropped keys.
    pub fn evict(&mut self, budget: usize, frame: u64) -> Vec<Key> {
        if self.bytes <= budget {
            return Vec::new();
        }
        let mut old: Vec<(u64, Key)> = self.entries.iter().filter(|(_, e)| e.last_used < frame).map(|(k, e)| (e.last_used, *k)).collect();
        old.sort_unstable_by_key(|&(used, _)| used);
        let mut dropped = Vec::new();
        for (_, key) in old {
            if self.bytes <= budget {
                break;
            }
            if let Some(e) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(e.bytes);
                dropped.push(key);
            }
        }
        dropped
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(pages: usize) -> Layout {
        Layout::new(&vec![[612.0, 792.0]; pages])
    }

    fn view_at(layout: &Layout, zoom: f32, scroll_y: f32) -> View {
        let mut v = View::new([1200.0, 900.0]);
        v.zoom = zoom;
        v.scroll = [0.0, scroll_y];
        v.pan_px(layout, 0.0, 0.0);
        v
    }

    #[test]
    fn tags_round_trip() {
        let keys = [
            Key::Preview { page: 0 },
            Key::Preview { page: 12345 },
            Key::Thumb { page: 0 },
            Key::Thumb { page: 1_048_575 },
            Key::Tile { page: 7, level: -500, tx: 3, ty: 15 },
            Key::Tile { page: 1_048_575, level: 1999, tx: 4095, ty: 4095 },
            Key::Tile { page: 0, level: 0, tx: 0, ty: 0 },
        ];
        for k in keys {
            assert_eq!(Key::from_tag(k.to_tag()), k);
            assert_eq!(k.to_tag() & (1 << 60), 0, "print-tag bit must stay clear for {k:?}");
        }
    }

    #[test]
    fn a_plan_can_be_pointed_at_the_pages_of_the_file_and_leaves_out_the_blank_ones() {
        let layout = Layout::new(&[[612.0, 792.0]; 4]);
        let plan = plan(&layout, &View::new([800.0, 600.0]));
        // Page 1 is blank (added), the others are drawn from the page of the file before them.
        let mapped = plan.mapped(|p| (p != 1).then_some(p.saturating_sub(1)));
        assert!(mapped.previews.iter().all(|k| k.page() != 1 || plan.previews.iter().all(|o| o.page() != 2)));
        assert_eq!(mapped.previews.len(), plan.previews.iter().filter(|k| k.page() != 1).count());
        assert!(mapped.visible.iter().all(|k| matches!(k, Key::Tile { .. })));
        assert_eq!(Key::Tile { page: 3, level: 5, tx: 2, ty: 1 }.with_page(9), Key::Tile { page: 9, level: 5, tx: 2, ty: 1 });
        assert_eq!(Key::Thumb { page: 3 }.with_page(0), Key::Thumb { page: 0 });
    }

    #[test]
    fn a_thumbnail_is_a_small_picture_of_the_whole_page_whatever_its_size() {
        for page in [[612.0, 792.0], [14_400.0, 200.0], [72.0, 72.0], [300.0, 400.0]] {
            let r = request(Key::Thumb { page: 3 }, page);
            assert_eq!((r.page, r.preview, r.tile), (3, true, None));
            assert_eq!(Key::from_tag(r.tag), Key::Thumb { page: 3 });
            let longest = page[0].max(page[1]) * r.scale;
            assert!(longest <= THUMB_SIDE + 1.0, "{page:?} is {longest} px");
            assert!(r.scale <= 2.0, "a tiny page is not blown up beyond twice its size");
        }
        assert_ne!(Key::Thumb { page: 1 }.to_tag(), Key::Preview { page: 1 }.to_tag(), "a thumbnail and a preview of the same page are two textures");
    }

    #[test]
    fn levels_are_monotonic_and_close_to_the_zoom() {
        let mut last = i16::MIN;
        let mut z = 0.1_f32;
        while z < 8.0 {
            let l = level_for(z);
            assert!(l >= last);
            last = l;
            assert!((level_scale(l) / z - 1.0).abs() < 0.016, "zoom {z} level {l}");
            z *= 1.01;
        }
        assert_eq!(level_for(1.0), 0);
        assert_eq!(level_for(f32::NAN.max(0.0)), level_for(1e-3));
    }

    #[test]
    fn request_tiles_stay_inside_the_page() {
        let page = [612.0, 792.0];
        let level = level_for(2.0);
        let scale = render_scale(page, level);
        let (wpx, hpx) = (device_pixels(page[0], scale), device_pixels(page[1], scale));
        for ty in 0..hpx.div_ceil(TILE) {
            for tx in 0..wpx.div_ceil(TILE) {
                let r = request(Key::Tile { page: 0, level, tx: tx as u16, ty: ty as u16 }, page);
                let t = r.tile.expect("tile");
                assert!(t.w >= 1 && t.h >= 1 && t.w <= TILE && t.h <= TILE);
                assert!(t.x + t.w <= wpx && t.y + t.h <= hpx, "tile {tx},{ty} outside {wpx}x{hpx}: {t:?}");
            }
        }
    }

    #[test]
    fn requests_carry_their_key() {
        let r = request(Key::Preview { page: 4 }, [612.0, 792.0]);
        assert!(r.preview && r.tile.is_none());
        assert_eq!(Key::from_tag(r.tag), Key::Preview { page: 4 });
        assert!(r.scale > 0.0);
    }

    #[test]
    fn plan_covers_the_viewport() {
        let l = doc(10);
        let v = view_at(&l, 1.0, 0.0);
        let p = plan(&l, &v);
        assert!(!p.visible.is_empty());
        assert!(p.previews.contains(&Key::Preview { page: 0 }));
        // At zoom 1 a 612x792 page at the top is 2x2 tiles, all visible in a 1200x900 viewport.
        let on_page0 = p.visible.iter().filter(|k| k.page() == 0).count();
        assert_eq!(on_page0, 4, "{:?}", p.visible);
        // Every key is unique.
        let mut all = p.queue(|_| false);
        let n = all.len();
        all.sort_by_key(|k| k.to_tag());
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn plan_orders_the_centre_first() {
        let l = doc(10);
        let v = view_at(&l, 1.5, 3000.0);
        let p = plan(&l, &v);
        assert!(p.visible.len() > 2);
        let centre = [v.scroll[0] + v.viewport[0] / v.zoom / 2.0, v.scroll[1] + v.viewport[1] / v.zoom / 2.0];
        let dist = |k: Key| {
            let Key::Tile { page, level, tx, ty } = k else { return f32::MAX };
            let r = l.page(page as usize).expect("page");
            let s = level_scale(level);
            let mid = [r.x + (tx as f32 + 0.5) * TILE as f32 / s, r.y + (ty as f32 + 0.5) * TILE as f32 / s];
            (mid[0] - centre[0]).powi(2) + (mid[1] - centre[1]).powi(2)
        };
        let ds: Vec<f32> = p.visible.iter().map(|&k| dist(k)).collect();
        assert!(ds.windows(2).all(|w| w[0] <= w[1] * 1.0001), "{ds:?}");
    }

    #[test]
    fn queue_skips_what_is_resident() {
        let l = doc(10);
        let v = view_at(&l, 1.0, 0.0);
        let p = plan(&l, &v);
        let all = p.queue(|_| false);
        let first = all[0];
        let rest = p.queue(|k| k == first);
        assert_eq!(rest.len(), all.len() - 1);
        assert!(!rest.contains(&first));
    }

    #[test]
    fn plan_for_an_empty_document_is_empty() {
        let l = Layout::new(&[]);
        let v = View::new([800.0, 600.0]);
        assert_eq!(plan(&l, &v), Plan::default());
    }

    #[test]
    fn plan_is_bounded_at_extreme_zoom() {
        let l = Layout::new(&[[100_000.0, 100_000.0]]);
        let mut v = View::new([3840.0, 2160.0]);
        v.zoom = 8.0;
        let p = plan(&l, &v);
        assert!(p.visible.len() + p.prefetch.len() <= MAX_TILES);
    }

    #[test]
    fn residency_evicts_oldest_but_never_the_current_frame() {
        let mut r = Residency::default();
        let k = |p: u32| Key::Preview { page: p };
        r.insert(k(0), 100, 1);
        r.insert(k(1), 100, 2);
        r.insert(k(2), 100, 3);
        r.touch(k(0), 4);
        assert_eq!(r.bytes(), 300);
        // Over budget: drop the least recently used that is not from this frame.
        let dropped = r.evict(150, 4);
        assert_eq!(dropped, vec![k(1), k(2)]);
        assert_eq!(r.bytes(), 100);
        assert!(r.contains(k(0)));
        // The current frame's entries survive even when over budget.
        assert!(r.evict(0, 4).is_empty());
        assert!(r.contains(k(0)));
        // Re-inserting replaces the old size.
        r.insert(k(0), 40, 5);
        assert_eq!(r.bytes(), 40);
        r.clear();
        assert_eq!((r.len(), r.bytes()), (0, 0));
    }
}
