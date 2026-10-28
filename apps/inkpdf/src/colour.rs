//! Converting colours between RGB and hue–saturation–value, which is what the colour sliders
//! edit. Pure functions, unit-tested.

/// `[hue in degrees (0 to 360), saturation (0 to 1), value (0 to 1)]`.
pub type Hsv = [f32; 3];

/// The hue, saturation and value of an RGB colour. Greys have no hue; it is reported as 0.
pub fn rgb_to_hsv(rgb: [u8; 3]) -> Hsv {
    let [r, g, b] = rgb.map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    [hue, if max <= f32::EPSILON { 0.0 } else { delta / max }, max]
}

/// The RGB colour for hue, saturation and value. Out-of-range or non-finite input is brought into
/// range, so every input gives a colour.
pub fn hsv_to_rgb(hsv: Hsv) -> [u8; 3] {
    let clean = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
    let hue = clean(hsv[0], 0.0).rem_euclid(360.0);
    let (s, v) = (clean(hsv[1], 0.0).clamp(0.0, 1.0), clean(hsv[2], 0.0).clamp(0.0, 1.0));
    let chroma = v * s;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - chroma;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    [r, g, b].map(|c| ((c + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_primary_colours_have_the_expected_hues() {
        assert_eq!(rgb_to_hsv([255, 0, 0]), [0.0, 1.0, 1.0]);
        assert_eq!(rgb_to_hsv([0, 255, 0]), [120.0, 1.0, 1.0]);
        assert_eq!(rgb_to_hsv([0, 0, 255]), [240.0, 1.0, 1.0]);
        assert_eq!(rgb_to_hsv([255, 255, 0])[0], 60.0);
        assert_eq!(rgb_to_hsv([0, 255, 255])[0], 180.0);
        assert_eq!(rgb_to_hsv([255, 0, 255])[0], 300.0);
    }

    #[test]
    fn greys_have_no_saturation() {
        assert_eq!(rgb_to_hsv([0, 0, 0]), [0.0, 0.0, 0.0]);
        let [h, s, v] = rgb_to_hsv([128, 128, 128]);
        assert_eq!((h, s), (0.0, 0.0));
        assert!((v - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(hsv_to_rgb([200.0, 0.0, 0.5]), [128, 128, 128]);
    }

    #[test]
    fn every_colour_survives_a_round_trip() {
        // A coarse sweep of the whole RGB cube.
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let back = hsv_to_rgb(rgb_to_hsv([r as u8, g as u8, b as u8]));
                    assert_eq!(back, [r as u8, g as u8, b as u8], "{r},{g},{b}");
                }
            }
        }
    }

    #[test]
    fn hue_wraps_and_bad_input_still_gives_a_colour() {
        assert_eq!(hsv_to_rgb([360.0, 1.0, 1.0]), hsv_to_rgb([0.0, 1.0, 1.0]));
        assert_eq!(hsv_to_rgb([-120.0, 1.0, 1.0]), hsv_to_rgb([240.0, 1.0, 1.0]));
        assert_eq!(hsv_to_rgb([725.0, 2.0, 3.0]), hsv_to_rgb([5.0, 1.0, 1.0]));
        for bad in [[f32::NAN, 0.5, 0.5], [10.0, f32::INFINITY, 0.5], [10.0, 0.5, f32::NEG_INFINITY]] {
            let _ = hsv_to_rgb(bad);
        }
        assert_eq!(hsv_to_rgb([0.0, 1.0, 0.0]), [0, 0, 0], "no brightness is black whatever the hue");
    }
}
