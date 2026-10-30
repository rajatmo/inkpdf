//! What a click is, for every device, in one place. A press that lets go within `slop_px` (logical pixels: a hand's wander, not the screen's
//! density, which is what made fingers fail to tap on dense screens before 1.3.3) and `max_ms` of where it began is a click; two clicks within
//! `double_ms` and `double_px` of each other are a double. A finger's limits are looser than a mouse's or a stylus's, which are exact. The
//! recognisers that use these (a finger's tap, a link followed by a click, a word or a line selected by two or three presses, the double
//! tap that fits the page to the width) all read them from here, so no two of them can disagree.

/// The limits of a click for one kind of device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// How far the pointer may wander and still be clicking, in logical pixels.
    pub slop_px: f32,
    /// How long it may stay down, in milliseconds.
    pub max_ms: f64,
    /// How soon a second click must follow the first to make a double, and how near it must be, in milliseconds and logical pixels.
    pub double_ms: f64,
    pub double_px: f32,
}

/// A finger (Android's own touch slop is 8 dp, and a finger on a tablet wanders more).
pub const FINGER: Limits = Limits { slop_px: 12.0, max_ms: 350.0, double_ms: 300.0, double_px: 24.0 };

/// A mouse or a stylus: exact, and a click is slower.
pub const POINTER: Limits = Limits { slop_px: 6.0, max_ms: 800.0, double_ms: 450.0, double_px: 12.0 };

// (A finger wanders further and is quicker than a mouse or a stylus, which are exact and slower.)
const _: () = assert!(FINGER.slop_px > POINTER.slop_px && FINGER.max_ms < POINTER.max_ms);

impl Limits {
    /// Whether a press that wandered `moved_px` (logical pixels) and was down `held_ms` was a click.
    pub fn is_click(&self, moved_px: f32, held_ms: f64) -> bool {
        moved_px <= self.slop_px && held_ms <= self.max_ms
    }

    /// Whether a click `gap_ms` after the one before, `apart_px` (logical pixels) from it, makes a double.
    pub fn is_double(&self, gap_ms: f64, apart_px: f32) -> bool {
        gap_ms <= self.double_ms && apart_px <= self.double_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finger_may_wander_further_and_is_quicker_than_a_mouse_and_both_are_judged_in_logical_pixels() {
        assert!(FINGER.is_click(11.0, 300.0) && !FINGER.is_click(13.0, 300.0) && !FINGER.is_click(5.0, 400.0));
        assert!(POINTER.is_click(5.0, 700.0) && !POINTER.is_click(7.0, 100.0));
    }

    #[test]
    fn a_double_needs_the_second_click_soon_and_near() {
        assert!(FINGER.is_double(250.0, 20.0) && !FINGER.is_double(350.0, 20.0) && !FINGER.is_double(250.0, 30.0));
        assert!(POINTER.is_double(450.0, 12.0) && !POINTER.is_double(500.0, 1.0));
    }
}
