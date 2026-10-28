//! Page layout, viewport and smooth scrolling. Pure math: no window or GPU types, so it is
//! unit-tested.
//!
//! Document space is PDF points with y down, pages stacked top to bottom and centred. Screen
//! space is device pixels. `zoom` is device pixels per point.

use std::ops::Range;

/// Gap between pages and margin around the document, in points.
pub const PAGE_GAP: f32 = 12.0;
pub const DOC_MARGIN: f32 = 12.0;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;

/// What a page of unusable size (zero, negative, NaN, absurdly large) is laid out as.
const FALLBACK_PAGE: [f32; 2] = [612.0, 792.0];
const MAX_PAGE_SIDE: f32 = 14_400.0;

/// Time constants of the glide towards a scroll or zoom target, in seconds.
const SCROLL_TAU: f32 = 0.055;
const ZOOM_TAU: f32 = 0.06;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Where each page sits in document space.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pages: Vec<Rect>,
    width: f32,
    height: f32,
}

impl Layout {
    pub fn new(sizes: &[[f32; 2]]) -> Self {
        let sane = |s: [f32; 2]| {
            let ok = |v: f32| v.is_finite() && v > 0.0;
            if ok(s[0]) && ok(s[1]) { [s[0].min(MAX_PAGE_SIDE), s[1].min(MAX_PAGE_SIDE)] } else { FALLBACK_PAGE }
        };
        let sizes: Vec<[f32; 2]> = sizes.iter().map(|&s| sane(s)).collect();
        let widest = sizes.iter().fold(0.0_f32, |m, s| m.max(s[0]));
        let width = widest + 2.0 * DOC_MARGIN;
        let mut y = DOC_MARGIN;
        let mut pages = Vec::with_capacity(sizes.len());
        for s in &sizes {
            pages.push(Rect { x: (width - s[0]) / 2.0, y, w: s[0], h: s[1] });
            y += s[1] + PAGE_GAP;
        }
        // The last page has a margin below it, not a gap.
        let height = if pages.is_empty() { 2.0 * DOC_MARGIN } else { y - PAGE_GAP + DOC_MARGIN };
        Self { pages, width, height }
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn page(&self, i: usize) -> Option<Rect> {
        self.pages.get(i).copied()
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    /// The pages whose rectangles overlap the document-space band `top..bottom`.
    pub fn pages_in(&self, top: f32, bottom: f32) -> Range<usize> {
        let first = self.pages.partition_point(|p| p.bottom() < top);
        let end = self.pages.partition_point(|p| p.y <= bottom);
        first..end.max(first)
    }

    /// The page nearest to document-space `y`: the one containing it, or the one above when it is
    /// in a gap.
    pub fn page_at(&self, y: f32) -> usize {
        if self.pages.is_empty() {
            return 0;
        }
        self.pages.partition_point(|p| p.y <= y).saturating_sub(1)
    }
}

/// The viewport: what part of the document is on screen and how it is animating.
#[derive(Clone, Debug)]
pub struct View {
    pub zoom: f32,
    /// Document point at the viewport's top-left corner.
    pub scroll: [f32; 2],
    /// Viewport size in pixels.
    pub viewport: [f32; 2],
    scroll_target: [f32; 2],
    zoom_target: f32,
    /// Screen point that stays fixed while the zoom glides.
    zoom_anchor: [f32; 2],
}

impl View {
    pub fn new(viewport: [f32; 2]) -> Self {
        Self { zoom: 1.0, scroll: [0.0; 2], viewport, scroll_target: [0.0; 2], zoom_target: 1.0, zoom_anchor: [viewport[0] / 2.0, viewport[1] / 2.0] }
    }

    pub fn doc_to_screen(&self, p: [f32; 2]) -> [f32; 2] {
        [(p[0] - self.scroll[0]) * self.zoom, (p[1] - self.scroll[1]) * self.zoom]
    }

    pub fn screen_to_doc(&self, p: [f32; 2]) -> [f32; 2] {
        [self.scroll[0] + p[0] / self.zoom, self.scroll[1] + p[1] / self.zoom]
    }

    /// The document-space band of the viewport, `(top, bottom)`.
    pub fn visible_y(&self) -> (f32, f32) {
        (self.scroll[1], self.scroll[1] + self.viewport[1] / self.zoom)
    }

    pub fn set_viewport(&mut self, layout: &Layout, size: [f32; 2]) {
        self.viewport = [size[0].max(1.0), size[1].max(1.0)];
        self.clamp(layout);
    }

    /// The zoom at which the widest page, with its margin, exactly fills the viewport width.
    pub fn fit_width_zoom(&self, layout: &Layout) -> f32 {
        (self.viewport[0] / layout.width().max(1.0)).clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// Jump (no animation) to fit-width at the top of the document.
    pub fn fit_width(&mut self, layout: &Layout) {
        let z = self.fit_width_zoom(layout);
        self.zoom = z;
        self.zoom_target = z;
        self.scroll = [0.0, self.scroll[1]];
        self.scroll_target = self.scroll;
        self.clamp(layout);
    }

    /// Keep the document on screen: centred along an axis where it is smaller than the viewport,
    /// otherwise not scrolled past its edges. Applies to the glide target too.
    pub fn clamp(&mut self, layout: &Layout) {
        let (vw, vh) = (self.viewport[0] / self.zoom, self.viewport[1] / self.zoom);
        // A non-finite position (from a bad input event) is put back at the start rather than kept.
        let fit = |pos: f32, doc: f32, vis: f32| {
            if doc <= vis {
                (doc - vis) / 2.0
            } else if pos.is_finite() {
                pos.clamp(0.0, doc - vis)
            } else {
                0.0
            }
        };
        let (dw, dh) = (layout.width(), layout.height());
        self.scroll = [fit(self.scroll[0], dw, vw), fit(self.scroll[1], dh, vh)];
        self.scroll_target = [fit(self.scroll_target[0], dw, vw), fit(self.scroll_target[1], dh, vh)];
    }

    /// Set the zoom immediately, keeping the document point under `anchor` (screen pixels) fixed.
    pub fn zoom_about(&mut self, layout: &Layout, zoom: f32, anchor: [f32; 2]) {
        let zoom = if zoom.is_finite() { zoom.clamp(MIN_ZOOM, MAX_ZOOM) } else { self.zoom };
        let p = self.screen_to_doc(anchor);
        let before = self.scroll;
        self.zoom = zoom;
        self.scroll = [p[0] - anchor[0] / zoom, p[1] - anchor[1] / zoom];
        // A pending glide keeps its remaining distance.
        self.scroll_target = [self.scroll_target[0] + self.scroll[0] - before[0], self.scroll_target[1] + self.scroll[1] - before[1]];
        self.clamp(layout);
    }

    /// Direct manipulation (drag, touch, precision touchpad): moves at once and cancels any glide.
    pub fn pan_px(&mut self, layout: &Layout, dx: f32, dy: f32) {
        self.scroll = [self.scroll[0] - dx / self.zoom, self.scroll[1] - dy / self.zoom];
        self.scroll_target = self.scroll;
        self.zoom_target = self.zoom;
        self.clamp(layout);
    }

    /// Mouse wheel: glide towards a target `dx`, `dy` pixels further along.
    pub fn scroll_by_px(&mut self, layout: &Layout, dx: f32, dy: f32) {
        self.scroll_target = [self.scroll_target[0] - dx / self.zoom, self.scroll_target[1] - dy / self.zoom];
        self.clamp(layout);
    }

    /// Glide to the top of a page.
    pub fn scroll_to_page(&mut self, layout: &Layout, page: usize) {
        if let Some(p) = layout.page(page) {
            self.scroll_target[1] = p.y - DOC_MARGIN;
            self.clamp(layout);
        }
    }

    /// Glide the zoom to `zoom` about `anchor`.
    pub fn zoom_to(&mut self, zoom: f32, anchor: [f32; 2]) {
        if zoom.is_finite() {
            self.zoom_target = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
            self.zoom_anchor = anchor;
        }
    }

    /// Cancel gliding where it stands (a finger or pen went down).
    pub fn stop(&mut self) {
        self.scroll_target = self.scroll;
        self.zoom_target = self.zoom;
    }

    /// Whether a glide is under way.
    pub fn animating(&self) -> bool {
        self.zoom_animating() || self.scroll_animating(&self.scroll_target)
    }

    pub fn zoom_animating(&self) -> bool {
        (self.zoom_target / self.zoom).ln().abs() > 1e-3
    }

    fn scroll_animating(&self, target: &[f32; 2]) -> bool {
        let eps = 0.25 / self.zoom;
        (target[0] - self.scroll[0]).abs() > eps || (target[1] - self.scroll[1]).abs() > eps
    }

    /// Advance the glide by `dt` seconds. Returns whether it is still moving.
    pub fn step(&mut self, layout: &Layout, dt: f32) -> bool {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.05) } else { 0.0 };
        if self.zoom_animating() {
            let f = 1.0 - (-dt / ZOOM_TAU).exp();
            let z = self.zoom * (self.zoom_target / self.zoom).powf(f);
            let anchor = self.zoom_anchor;
            let target = self.zoom_target;
            self.zoom_about(layout, z, anchor);
            if !self.zoom_animating() {
                self.zoom_about(layout, target, anchor);
            }
        }
        let target = self.scroll_target;
        if self.scroll_animating(&target) {
            let f = 1.0 - (-dt / SCROLL_TAU).exp();
            self.scroll = [self.scroll[0] + (target[0] - self.scroll[0]) * f, self.scroll[1] + (target[1] - self.scroll[1]) * f];
            if !self.scroll_animating(&target) {
                self.scroll = target;
            }
        }
        self.clamp(layout);
        self.animating()
    }

    /// Whether the view shows the bottom of the document (or the whole of it).
    pub fn at_end(&self, layout: &Layout) -> bool {
        let visible = self.viewport[1] / self.zoom;
        layout.height() <= visible || self.scroll[1] >= layout.height() - visible - 0.5 / self.zoom
    }

    /// The page under the middle of the viewport.
    pub fn current_page(&self, layout: &Layout) -> usize {
        let (top, bottom) = self.visible_y();
        layout.page_at((top + bottom) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_pages() -> Layout {
        Layout::new(&[[600.0, 800.0], [600.0, 800.0], [400.0, 500.0]])
    }

    #[test]
    fn layout_stacks_and_centres_pages() {
        let l = three_pages();
        assert_eq!(l.len(), 3);
        assert_eq!(l.width(), 600.0 + 2.0 * DOC_MARGIN);
        let p0 = l.page(0).expect("page 0");
        let p2 = l.page(2).expect("page 2");
        assert_eq!(p0.y, DOC_MARGIN);
        assert_eq!(l.page(1).expect("page 1").y, DOC_MARGIN + 800.0 + PAGE_GAP);
        // The narrower page is centred.
        assert!((p2.x - (l.width() - 400.0) / 2.0).abs() < 1e-3);
        assert!((l.height() - (p2.bottom() + DOC_MARGIN)).abs() < 1e-3);
    }

    #[test]
    fn unusable_page_sizes_fall_back() {
        let l = Layout::new(&[[0.0, 10.0], [f32::NAN, 5.0], [-3.0, 3.0], [1e9, 1e9]]);
        assert_eq!(l.page(0).map(|p| (p.w, p.h)), Some((612.0, 792.0)));
        assert_eq!(l.page(1).map(|p| (p.w, p.h)), Some((612.0, 792.0)));
        assert_eq!(l.page(2).map(|p| (p.w, p.h)), Some((612.0, 792.0)));
        assert_eq!(l.page(3).map(|p| (p.w, p.h)), Some((MAX_PAGE_SIDE, MAX_PAGE_SIDE)));
        assert!(l.height().is_finite());
        assert!(Layout::new(&[]).is_empty());
    }

    #[test]
    fn pages_in_band() {
        let l = three_pages();
        assert_eq!(l.pages_in(0.0, 100.0), 0..1);
        assert_eq!(l.pages_in(700.0, 900.0), 0..2);
        // Entirely inside the gap between pages 0 and 1.
        let gap_mid = DOC_MARGIN + 800.0 + PAGE_GAP / 2.0;
        assert!(l.pages_in(gap_mid, gap_mid).is_empty());
        assert_eq!(l.pages_in(0.0, l.height()), 0..3);
        assert_eq!(l.pages_in(-500.0, -100.0), 0..0);
        assert_eq!(l.pages_in(l.height() + 10.0, l.height() + 50.0), 3..3);
    }

    #[test]
    fn page_at_picks_containing_or_previous() {
        let l = three_pages();
        assert_eq!(l.page_at(-50.0), 0);
        assert_eq!(l.page_at(100.0), 0);
        assert_eq!(l.page_at(DOC_MARGIN + 800.0 + PAGE_GAP + 1.0), 1);
        assert_eq!(l.page_at(1e9), 2);
        assert_eq!(Layout::new(&[]).page_at(5.0), 0);
    }

    #[test]
    fn zoom_about_keeps_the_anchor_fixed() {
        let l = three_pages();
        let mut v = View::new([800.0, 600.0]);
        v.zoom = 1.0;
        v.scroll = [0.0, 300.0];
        v.scroll_target = v.scroll;
        let anchor = [400.0, 250.0];
        let before = v.screen_to_doc(anchor);
        v.zoom_about(&l, 2.5, anchor);
        let after = v.screen_to_doc(anchor);
        assert!((before[0] - after[0]).abs() < 1e-3 && (before[1] - after[1]).abs() < 1e-3, "{before:?} vs {after:?}");
        assert_eq!(v.zoom, 2.5);
    }

    #[test]
    fn zoom_is_clamped_and_nan_ignored() {
        let l = three_pages();
        let mut v = View::new([800.0, 600.0]);
        v.zoom_about(&l, 1e6, [0.0, 0.0]);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_about(&l, f32::NAN, [0.0, 0.0]);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_about(&l, 0.0, [0.0, 0.0]);
        assert_eq!(v.zoom, MIN_ZOOM);
    }

    #[test]
    fn a_document_smaller_than_the_viewport_is_centred() {
        let l = Layout::new(&[[200.0, 200.0]]);
        let mut v = View::new([1000.0, 1000.0]);
        v.zoom = 1.0;
        v.clamp(&l);
        let (dw, dh) = (l.width(), l.height());
        assert!((v.scroll[0] - (dw - 1000.0) / 2.0).abs() < 1e-3);
        assert!((v.scroll[1] - (dh - 1000.0) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn scroll_is_clamped_to_the_document() {
        let l = three_pages();
        let mut v = View::new([700.0, 500.0]);
        v.zoom = 1.0;
        v.pan_px(&l, 0.0, 1e7);
        assert_eq!(v.scroll[1], 0.0);
        v.pan_px(&l, 0.0, -1e7);
        assert!((v.scroll[1] - (l.height() - 500.0)).abs() < 1e-3);
    }

    #[test]
    fn wheel_glide_converges_on_its_target() {
        let l = three_pages();
        let mut v = View::new([700.0, 500.0]);
        v.zoom = 1.0;
        v.scroll_by_px(&l, 0.0, -300.0);
        assert!(v.animating());
        let mut frames = 0;
        while v.step(&l, 1.0 / 120.0) {
            frames += 1;
            assert!(frames < 600, "glide never settled");
        }
        assert!((v.scroll[1] - 300.0).abs() < 1.0, "{}", v.scroll[1]);
        // It is monotonic and does not overshoot.
        assert!(v.scroll[1] <= 300.0 + 1e-3);
    }

    #[test]
    fn zoom_glide_converges_and_keeps_the_anchor() {
        let l = three_pages();
        let mut v = View::new([800.0, 600.0]);
        v.zoom = 1.0;
        v.zoom_target = 1.0;
        v.scroll = [0.0, 400.0];
        v.scroll_target = v.scroll;
        let anchor = [300.0, 200.0];
        let doc_point = v.screen_to_doc(anchor);
        v.zoom_to(2.0, anchor);
        let mut frames = 0;
        while v.step(&l, 1.0 / 120.0) {
            frames += 1;
            assert!(frames < 600, "zoom never settled");
        }
        assert!((v.zoom - 2.0).abs() < 1e-3, "{}", v.zoom);
        let after = v.screen_to_doc(anchor);
        assert!((after[1] - doc_point[1]).abs() < 0.5, "{after:?} vs {doc_point:?}");
    }

    #[test]
    fn step_tolerates_bad_dt() {
        let l = three_pages();
        let mut v = View::new([700.0, 500.0]);
        v.scroll_by_px(&l, 0.0, -100.0);
        v.step(&l, f32::NAN);
        v.step(&l, -1.0);
        v.step(&l, 1e9);
        assert!(v.scroll[1].is_finite() && v.zoom.is_finite());
    }

    #[test]
    fn fit_width_fills_the_width() {
        let l = three_pages();
        let mut v = View::new([1224.0, 800.0]);
        v.fit_width(&l);
        assert!((v.zoom - 1224.0 / l.width()).abs() < 1e-4);
        assert!((v.scroll[0]).abs() < 1e-3);
    }

    #[test]
    fn current_page_follows_the_scroll() {
        let l = three_pages();
        let mut v = View::new([700.0, 400.0]);
        v.zoom = 1.0;
        assert_eq!(v.current_page(&l), 0);
        v.pan_px(&l, 0.0, -(DOC_MARGIN + 800.0 + PAGE_GAP + 100.0));
        assert_eq!(v.current_page(&l), 1);
    }
}
