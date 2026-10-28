//! Converting between what the viewer shows and PDF user space.
//!
//! *View space* is a page as displayed: points, origin at the top-left, y down, after `/Rotate`.
//! *User space* is where annotations are stored: the crop box's coordinates, y up, unrotated.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeom {
    /// Displayed size in points.
    pub width: f32,
    pub height: f32,
    /// Crop box `[x0, y0, x1, y1]` in user space.
    pub crop: [f32; 4],
    /// Clockwise rotation in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
}

impl PageGeom {
    pub fn from_info(info: &pdfcraft_render::PageInfo) -> Self {
        Self { width: info.width, height: info.height, crop: info.crop, rotation: info.rotation }
    }

    /// A point in view space to user space.
    pub fn view_to_user(&self, p: [f32; 2]) -> [f32; 2] {
        let [cx0, cy0, cx1, cy1] = self.crop;
        let (a, b) = (p[0] / self.width.max(1e-3), p[1] / self.height.max(1e-3));
        let (u, v) = match self.rotation {
            90 => (b, 1.0 - a),
            180 => (1.0 - a, 1.0 - b),
            270 => (1.0 - b, a),
            _ => (a, b),
        };
        [cx0 + u * (cx1 - cx0), cy1 - v * (cy1 - cy0)]
    }

    /// A point in user space to view space; the inverse of [`Self::view_to_user`].
    pub fn user_to_view(&self, p: [f32; 2]) -> [f32; 2] {
        let [cx0, cy0, cx1, cy1] = self.crop;
        let u = (p[0] - cx0) / (cx1 - cx0).abs().max(1e-3) * (cx1 - cx0).signum();
        let v = (cy1 - p[1]) / (cy1 - cy0).abs().max(1e-3) * (cy1 - cy0).signum();
        let (a, b) = match self.rotation {
            90 => (1.0 - v, u),
            180 => (1.0 - u, 1.0 - v),
            270 => (v, 1.0 - u),
            _ => (u, v),
        };
        [a * self.width, b * self.height]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(rotation: u16) -> PageGeom {
        // A 200 x 100 crop box that does not start at the origin.
        let (w, h) = if rotation.is_multiple_of(180) { (200.0, 100.0) } else { (100.0, 200.0) };
        PageGeom { width: w, height: h, crop: [10.0, 20.0, 210.0, 120.0], rotation }
    }

    #[test]
    fn round_trips_for_every_rotation() {
        for rotation in [0, 90, 180, 270] {
            let g = geom(rotation);
            for p in [[0.0, 0.0], [g.width, g.height], [13.5, 77.25], [g.width / 2.0, 1.0]] {
                let back = g.user_to_view(g.view_to_user(p));
                assert!((back[0] - p[0]).abs() < 1e-3 && (back[1] - p[1]).abs() < 1e-3, "rotation {rotation}: {p:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn unrotated_page_flips_y_about_the_crop_box() {
        let g = geom(0);
        // Top-left of the view is the crop box's top-left corner (x0, y1) in user space.
        assert_eq!(g.view_to_user([0.0, 0.0]), [10.0, 120.0]);
        assert_eq!(g.view_to_user([200.0, 100.0]), [210.0, 20.0]);
    }

    #[test]
    fn rotated_pages_map_corners_to_corners() {
        // At 90 degrees the view's top-left shows the crop box's bottom-left.
        let g = geom(90);
        assert_eq!(g.view_to_user([0.0, 0.0]), [10.0, 20.0]);
        let g = geom(180);
        assert_eq!(g.view_to_user([0.0, 0.0]), [210.0, 20.0]);
        let g = geom(270);
        assert_eq!(g.view_to_user([0.0, 0.0]), [210.0, 120.0]);
    }

    #[test]
    fn a_degenerate_crop_box_does_not_produce_nan() {
        let g = PageGeom { width: 100.0, height: 100.0, crop: [5.0, 5.0, 5.0, 5.0], rotation: 0 };
        let p = g.user_to_view([5.0, 5.0]);
        assert!(p[0].is_finite() && p[1].is_finite());
        let q = g.view_to_user([50.0, 50.0]);
        assert!(q[0].is_finite() && q[1].is_finite());
    }
}
