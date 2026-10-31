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
/// The same for the glide to a page that a pull past the end has just added (slower, so that the page is seen coming), and for the
/// document coming back down when the pull is let go.
const PAGE_TAU: f32 = 0.12;
const LIFT_TAU: f32 = 0.09;

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
    /// How far a finger pulling past the end of the document has lifted it, in pixels (see [`View::pull`]), how far that is on its way
    /// to what the pull is for (0 to 1), and whether the finger still holds it there (else it comes back down).
    lift: f32,
    lift_ring: f32,
    lift_held: bool,
    /// The time constant the scroll glides with now.
    scroll_tau: f32,
    /// Room below the end of the document that the scroll may use, in pixels: what an on-screen keyboard covers, so that the last lines of a
    /// document can be brought above it.
    bottom_room: f32,
}

impl View {
    pub fn new(viewport: [f32; 2]) -> Self {
        Self {
            zoom: 1.0,
            scroll: [0.0; 2],
            viewport,
            scroll_target: [0.0; 2],
            zoom_target: 1.0,
            zoom_anchor: [viewport[0] / 2.0, viewport[1] / 2.0],
            lift: 0.0,
            lift_ring: 0.0,
            lift_held: false,
            scroll_tau: SCROLL_TAU,
            bottom_room: 0.0,
        }
    }

    /// Where the document is seen from: the scroll, and further down by how far it is lifted. (What is drawn is placed by this, so
    /// that a pull past the end moves all of it, the pages and the ink, together.)
    pub fn eye(&self) -> [f32; 2] {
        [self.scroll[0], self.scroll[1] + self.lift / self.zoom]
    }

    pub fn doc_to_screen(&self, p: [f32; 2]) -> [f32; 2] {
        let eye = self.eye();
        [(p[0] - eye[0]) * self.zoom, (p[1] - eye[1]) * self.zoom]
    }

    pub fn screen_to_doc(&self, p: [f32; 2]) -> [f32; 2] {
        let eye = self.eye();
        [eye[0] + p[0] / self.zoom, eye[1] + p[1] / self.zoom]
    }

    /// The document-space band of the viewport, `(top, bottom)`.
    pub fn visible_y(&self) -> (f32, f32) {
        let top = self.eye()[1];
        (top, top + self.viewport[1] / self.zoom)
    }

    /// A finger pulling up past the end of the document holds it `lift` pixels up (what is under the end shows), with `ring` (0 to 1)
    /// of the way to what the pull is for.
    pub fn pull(&mut self, lift: f32, ring: f32) {
        self.lift = if lift.is_finite() { lift.max(0.0) } else { 0.0 };
        self.lift_ring = if ring.is_finite() { ring.clamp(0.0, 1.0) } else { 0.0 };
        self.lift_held = self.lift > 0.0;
    }

    /// The pull is let go (or given up): the document comes back down over a moment.
    pub fn let_go(&mut self) {
        self.lift_held = false;
    }

    /// How far the document is lifted, in pixels, and how far that is on its way to what the pull is for.
    pub fn lift(&self) -> f32 {
        self.lift
    }

    pub fn lift_ring(&self) -> f32 {
        self.lift_ring
    }

    /// Let the glide that is under way (or about to be) take its time: the scroll to a page that was just added at the end.
    pub fn glide_slowly(&mut self) {
        self.scroll_tau = PAGE_TAU;
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
        let (dw, dh) = (layout.width(), layout.height() + self.bottom_room / self.zoom);
        self.scroll = [fit(self.scroll[0], dw, vw), fit(self.scroll[1], dh, vh)];
        self.scroll_target = [fit(self.scroll_target[0], dw, vw), fit(self.scroll_target[1], dh, vh)];
    }

    /// Let the scroll go `px` pixels past the end of the document (what a keyboard covers), or take that back (0): the view is then put back
    /// within the document if it was in the room.
    pub fn set_bottom_room(&mut self, layout: &Layout, px: f32) {
        self.bottom_room = if px.is_finite() { px.max(0.0) } else { 0.0 };
        self.clamp(layout);
    }

    /// Where the glide is heading, as a document y (what is scrolled to when it ends).
    pub fn scroll_target_y(&self) -> f32 {
        self.scroll_target[1]
    }

    /// Glide to the document y `y` at the top of the window.
    pub fn glide_to_y(&mut self, layout: &Layout, y: f32) {
        if y.is_finite() {
            self.scroll_target[1] = y;
            self.clamp(layout);
        }
    }

    /// Glide so that `rect` (document space) is above the lowest `covered_px` pixels of the viewport (what an on-screen keyboard covers) with a
    /// little to spare, and not under the top `bars_px` pixels (the toolbars); a rect too tall for the room between has its top kept in view. Nothing
    /// moves if it is there already.
    pub fn scroll_to_clear(&mut self, layout: &Layout, rect: [f32; 4], covered_px: f32, bars_px: f32) {
        if !rect.iter().all(|v| v.is_finite()) || !covered_px.is_finite() || !bars_px.is_finite() {
            return;
        }
        let zoom = self.zoom;
        let room = ((self.viewport[1] - covered_px.max(0.0)) / zoom).max(1.0);
        let (pad, bars) = (12.0 / zoom, bars_px.max(0.0) / zoom);
        let top = self.scroll_target[1];
        let mut want = top;
        if rect[3] + pad > top + room {
            want = rect[3] + pad - room;
        }
        if rect[1] - bars < want {
            want = rect[1] - bars;
        }
        self.scroll_target[1] = want;
        self.clamp(layout);
    }

    /// Set the zoom immediately, keeping the document point under `anchor` (screen pixels) fixed.
    pub fn zoom_about(&mut self, layout: &Layout, zoom: f32, anchor: [f32; 2]) {
        let zoom = if zoom.is_finite() { zoom.clamp(MIN_ZOOM, MAX_ZOOM) } else { self.zoom };
        // (A zoom is of the document at rest: whatever a pull had lifted it is put down first.)
        self.lift = 0.0;
        self.lift_ring = 0.0;
        self.lift_held = false;
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
        self.scroll_tau = SCROLL_TAU;
        self.zoom_target = self.zoom;
        self.clamp(layout);
    }

    /// Mouse wheel: glide towards a target `dx`, `dy` pixels further along.
    pub fn scroll_by_px(&mut self, layout: &Layout, dx: f32, dy: f32) {
        self.scroll_tau = SCROLL_TAU;
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

    /// Glide so that `rect` (`[x0, y0, x1, y1]`, in document space) is in view, if it is not: a rectangle that is shown (below `top_px`
    /// pixels of the window, which toolbars cover) is left where it is; one that is not is brought to about a third of the way down.
    pub fn scroll_to_show(&mut self, layout: &Layout, rect: [f32; 4], top_px: f32) {
        if !rect.iter().all(|v| v.is_finite()) {
            return;
        }
        let (view_w, view_h) = (self.viewport[0] / self.zoom, self.viewport[1] / self.zoom);
        let top = self.scroll_target[1] + top_px / self.zoom;
        if rect[1] < top || rect[3] > self.scroll_target[1] + view_h {
            self.scroll_target[1] = rect[1] - view_h * 0.33;
        }
        if rect[0] < self.scroll_target[0] || rect[2] > self.scroll_target[0] + view_w {
            self.scroll_target[0] = (rect[0] + rect[2]) / 2.0 - view_w / 2.0;
        }
        self.clamp(layout);
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
        self.scroll_tau = SCROLL_TAU;
        self.zoom_target = self.zoom;
    }

    /// Whether a glide is under way (the zoom's, the scroll's, or the document coming back down after a pull).
    pub fn animating(&self) -> bool {
        self.zoom_animating() || self.scroll_animating(&self.scroll_target) || (self.lift > 0.0 && !self.lift_held)
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
            let f = 1.0 - (-dt / self.scroll_tau).exp();
            self.scroll = [self.scroll[0] + (target[0] - self.scroll[0]) * f, self.scroll[1] + (target[1] - self.scroll[1]) * f];
            if !self.scroll_animating(&target) {
                self.scroll = target;
                self.scroll_tau = SCROLL_TAU;
            }
        }
        // A pull that is let go comes back down, quickly at first and then softly.
        if self.lift > 0.0 && !self.lift_held {
            self.lift *= (-dt / LIFT_TAU).exp();
            if self.lift < 0.25 {
                self.lift = 0.0;
                self.lift_ring = 0.0;
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
    fn showing_a_rectangle_moves_the_view_only_when_it_is_not_in_view() {
        let layout = three_pages();
        let mut v = View::new([800.0, 600.0]);
        v.scroll_to_show(&layout, [100.0, 100.0, 200.0, 120.0], 100.0);
        assert!(!v.animating(), "a rectangle in view is left");
        v.scroll_to_show(&layout, [100.0, 2000.0, 200.0, 2020.0], 100.0);
        assert!(v.animating(), "one below is brought up");
        for _ in 0..400 {
            v.step(&layout, 0.016);
        }
        let on = v.doc_to_screen([150.0, 2010.0]);
        assert!(on[1] > 100.0 && on[1] < 600.0, "it ends in view, below the toolbars: {on:?}");
        v.scroll_to_show(&layout, [f32::NAN, 0.0, 1.0, 1.0], 100.0);
        assert!(v.scroll.iter().all(|c| c.is_finite()));
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

    #[test]
    fn a_lifted_document_is_drawn_and_pressed_where_it_is_and_comes_back_down_when_let_go() {
        let l = three_pages();
        let mut v = View::new([800.0, 600.0]);
        let (before, band) = (v.doc_to_screen([100.0, 500.0]), v.visible_y());
        v.pull(80.0, 0.5);
        let after = v.doc_to_screen([100.0, 500.0]);
        assert!((before[1] - after[1] - 80.0).abs() < 1e-3 && before[0] == after[0], "up by the lift: {before:?} {after:?}");
        assert!((v.screen_to_doc(after)[1] - 500.0).abs() < 1e-3);
        assert!((v.visible_y().0 - band.0 - 80.0 / v.zoom).abs() < 1e-3 && v.eye()[1] == v.scroll[1] + 80.0 / v.zoom);
        assert!(!v.animating(), "held by the finger, it stays");
        v.let_go();
        assert!(v.animating());
        let mut last = v.lift();
        while v.step(&l, 1.0 / 60.0) {
            assert!(v.lift() < last, "it comes down, a bit each frame");
            last = v.lift();
        }
        assert_eq!((v.lift(), v.lift_ring()), (0.0, 0.0));
        assert_eq!(
            v.doc_to_screen([100.0, 500.0])[1],
            before[1],
            "down where it was (the glide's clamp centres it sideways, which is not the lift's)"
        );
        // Nonsense does not lift it, and zooming puts it down.
        v.pull(f32::NAN, f32::NAN);
        assert_eq!((v.lift(), v.lift_ring()), (0.0, 0.0));
        v.pull(50.0, 2.0);
        assert_eq!(v.lift_ring(), 1.0);
        v.zoom_about(&l, 2.0, [400.0, 300.0]);
        assert_eq!(v.lift(), 0.0);
    }

    #[test]
    fn the_glide_to_a_page_that_was_just_added_is_slower_than_a_scroll_and_a_scroll_after_it_is_quick_again() {
        let l = three_pages();
        let travelled = |slowly: bool| {
            let mut v = View::new([800.0, 600.0]);
            v.scroll_to_page(&l, 2);
            if slowly {
                v.glide_slowly();
            }
            for _ in 0..6 {
                v.step(&l, 1.0 / 60.0);
            }
            v.scroll[1]
        };
        assert!(travelled(true) < travelled(false), "{} {}", travelled(true), travelled(false));
        let mut v = View::new([800.0, 600.0]);
        v.scroll_to_page(&l, 1);
        v.glide_slowly();
        while v.step(&l, 1.0 / 60.0) {}
        v.scroll_to_page(&l, 2);
        let mut quick = View::new([800.0, 600.0]);
        quick.scroll[1] = v.scroll[1];
        quick.scroll_target = v.scroll_target;
        for _ in 0..4 {
            v.step(&l, 1.0 / 60.0);
            quick.step(&l, 1.0 / 60.0);
        }
        assert_eq!(v.scroll, quick.scroll, "after it arrived the glide is as quick as ever");
    }

    #[test]
    fn a_rect_under_the_keyboard_is_brought_above_it_and_one_already_clear_is_left_alone() {
        let layout = Layout::new(&[[612.0, 792.0], [612.0, 792.0]]);
        let mut view = View::new([600.0, 900.0]);
        view.zoom = 1.0;
        view.clamp(&layout);
        // A box low in the window (its bottom at 800 of 900), the keyboard covering the lowest 400 pixels.
        let rect = [100.0, 740.0, 300.0, 800.0];
        let before = view.scroll_target_y();
        view.scroll_to_clear(&layout, rect, 400.0, 100.0);
        let moved = view.scroll_target_y() - before;
        assert!(moved > 0.0, "scrolled down by {moved}");
        assert!(rect[3] + 12.0 <= view.scroll_target_y() + (900.0 - 400.0) + 0.01, "its bottom is above the keyboard");
        assert!(rect[1] - 100.0 >= view.scroll_target_y() - 0.01, "and its top is not under the bars");
        // Asked again, it is where it should be: nothing moves.
        let at = view.scroll_target_y();
        view.scroll_to_clear(&layout, rect, 400.0, 100.0);
        assert_eq!(view.scroll_target_y(), at);
        // A box that is clear already stays where it is.
        let mut still = View::new([600.0, 900.0]);
        still.clamp(&layout);
        still.scroll_to_clear(&layout, [100.0, 200.0, 300.0, 260.0], 400.0, 100.0);
        assert_eq!(still.scroll_target_y(), 0.0);
        // A box taller than the room has its top kept in view, under the bars.
        let mut tall = View::new([600.0, 900.0]);
        tall.clamp(&layout);
        tall.scroll_to_clear(&layout, [100.0, 300.0, 300.0, 1000.0], 400.0, 100.0);
        assert!((tall.scroll_target_y() - 200.0).abs() < 0.01, "{}", tall.scroll_target_y());
    }

    #[test]
    fn the_end_of_the_document_can_be_scrolled_up_by_the_room_a_keyboard_takes_and_comes_back_when_it_goes() {
        let layout = Layout::new(&[[612.0, 792.0]]);
        let mut view = View::new([600.0, 700.0]);
        view.clamp(&layout);
        view.glide_to_y(&layout, 1.0e6);
        let end = view.scroll_target_y();
        assert!((end - 116.0).abs() < 0.01, "the end of the document: {end}");
        // The last lines of the page are at the bottom: with no room they cannot go higher.
        view.scroll_to_clear(&layout, [100.0, 700.0, 300.0, 790.0], 400.0, 100.0);
        assert!(view.scroll_target_y() <= end + 0.01, "no room past the end");
        view.set_bottom_room(&layout, 400.0);
        view.scroll_to_clear(&layout, [100.0, 700.0, 300.0, 790.0], 400.0, 100.0);
        assert!(view.scroll_target_y() > end, "with the keyboard's room it goes up: {} > {end}", view.scroll_target_y());
        view.set_bottom_room(&layout, 0.0);
        assert!(view.scroll_target_y() <= end + 0.01, "the room is taken back with the keyboard");
        for bad in [f32::NAN, f32::INFINITY, -5.0] {
            view.set_bottom_room(&layout, bad);
            view.scroll_to_clear(&layout, [f32::NAN, 0.0, 1.0, 1.0], bad, bad);
            view.glide_to_y(&layout, bad);
            assert!(view.scroll_target_y().is_finite());
        }
    }
}
