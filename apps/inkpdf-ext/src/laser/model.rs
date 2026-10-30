//! The laser pointer: a dot that follows the pen or the mouse and, while it is pressed, a trail
//! that fades away. Nothing here is ink: it is never saved and never changes the document.
//!
//! Positions are in document space, so a trail stays on the page if the page is scrolled; times are
//! in milliseconds on the pen's clock (the pen's samples are stamped with them). Pure logic with no window
//! or GPU types, so it is unit-tested.

use inkpdf_api::P;

/// How long a point of the trail lasts, in milliseconds.
pub const FADE_MS: f64 = 1100.0;
/// Most points kept in all, so a pen held down for ever cannot grow without bound.
const MAX_POINTS: usize = 4096;

#[derive(Clone, Debug, Default)]
pub struct Laser {
    /// Where the dot is, while the pen or mouse is over the page.
    head: Option<P>,
    /// The trails, oldest first, each point with the time it was made. The last trail is still
    /// growing while `pressed`.
    trails: Vec<Vec<(P, f64)>>,
    pressed: bool,
}

fn finite(p: P) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

impl Laser {
    pub fn head(&self) -> Option<P> {
        self.head
    }

    /// Whether a trail is still fading (the screen must keep drawing until it is gone).
    pub fn is_fading(&self) -> bool {
        !self.trails.is_empty()
    }

    /// Move the dot without drawing, or hide it (`None`).
    pub fn show(&mut self, at: Option<P>) {
        self.head = at.filter(|&p| finite(p));
    }

    /// The pointer went down at `p`: a new trail begins.
    pub fn press(&mut self, p: P, t_ms: f64) {
        if !finite(p) {
            return;
        }
        self.release();
        self.pressed = true;
        self.head = Some(p);
        self.trails.push(vec![(p, t_ms)]);
    }

    /// The pointer moved to `p` while down.
    pub fn drag(&mut self, p: P, t_ms: f64) {
        if !finite(p) {
            return;
        }
        self.head = Some(p);
        if !self.pressed {
            return;
        }
        if self.points() >= MAX_POINTS {
            self.drop_oldest();
        }
        if let Some(trail) = self.trails.last_mut() {
            trail.push((p, t_ms));
        }
    }

    /// The pointer came up: the trail stops growing and fades.
    pub fn release(&mut self) {
        self.pressed = false;
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Drop what has faded by `now_ms`. Returns whether a trail is still fading.
    pub fn prune(&mut self, now_ms: f64) -> bool {
        let (open, last) = (self.pressed, self.trails.len().wrapping_sub(1));
        for (i, trail) in self.trails.iter_mut().enumerate() {
            // A held pen keeps the end of its trail, so the next movement continues from there.
            let held = if open && i == last { trail.last().copied() } else { None };
            trail.retain(|&(_, t)| now_ms - t < FADE_MS);
            if let (true, Some(end)) = (trail.is_empty(), held) {
                trail.push(end);
            }
        }
        let mut index = 0;
        self.trails.retain(|trail| {
            let keep = trail.len() > 1 || (open && index == last);
            index += 1;
            keep
        });
        self.is_fading()
    }

    /// The pieces of the trail still showing at `now_ms`: both ends and how much of its life is
    /// left (1 just made, towards 0 as it fades).
    pub fn pieces(&self, now_ms: f64) -> impl Iterator<Item = (P, P, f32)> + '_ {
        self.trails.iter().flat_map(move |trail| {
            trail.windows(2).filter_map(move |w| {
                let life = (1.0 - (now_ms - w[1].1).max(0.0) / FADE_MS) as f32;
                (life > 0.0).then_some((w[0].0, w[1].0, life.min(1.0)))
            })
        })
    }

    fn points(&self) -> usize {
        self.trails.iter().map(Vec::len).sum()
    }

    fn drop_oldest(&mut self) {
        if let Some(first) = self.trails.first_mut() {
            let cut = (first.len() / 2).max(1).min(first.len());
            first.drain(..cut);
        }
        self.trails.retain(|t| !t.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether anything is showing: the dot, or a trail that has not faded yet.
    fn is_visible(laser: &Laser) -> bool {
        laser.head().is_some() || laser.is_fading()
    }

    #[test]
    fn a_new_laser_shows_nothing() {
        let laser = Laser::default();
        assert!(!is_visible(&laser));
        assert_eq!(laser.pieces(0.0).count(), 0);
    }

    #[test]
    fn the_dot_follows_the_pointer_without_leaving_a_trail() {
        let mut laser = Laser::default();
        laser.show(Some([10.0, 20.0]));
        assert_eq!(laser.head(), Some([10.0, 20.0]));
        laser.drag([30.0, 40.0], 5.0);
        assert_eq!(laser.head(), Some([30.0, 40.0]));
        assert!(!laser.is_fading(), "moving without pressing draws no trail");
        laser.show(None);
        assert!(!is_visible(&laser));
    }

    #[test]
    fn a_pressed_drag_leaves_a_trail_that_fades_and_is_then_forgotten() {
        let mut laser = Laser::default();
        laser.press([0.0, 0.0], 0.0);
        for i in 1..=10 {
            laser.drag([i as f32 * 10.0, 0.0], i as f64 * 10.0);
        }
        laser.release();
        assert_eq!(laser.pieces(100.0).count(), 10);
        let fresh: Vec<f32> = laser.pieces(100.0).map(|p| p.2).collect();
        assert!(fresh.windows(2).all(|w| w[0] <= w[1]), "older pieces are fainter: {fresh:?}");
        // Half way through the fade every piece is dimmer; after it, none is left.
        let later: Vec<f32> = laser.pieces(100.0 + FADE_MS / 2.0).map(|p| p.2).collect();
        assert!(later.iter().zip(&fresh).all(|(l, f)| l < f));
        assert!(laser.prune(100.0 + FADE_MS / 2.0), "still fading");
        assert!(!laser.prune(100.0 + FADE_MS + 1.0), "faded away");
        assert_eq!(laser.pieces(100.0 + FADE_MS + 1.0).count(), 0);
    }

    #[test]
    fn two_presses_make_two_separate_trails() {
        let mut laser = Laser::default();
        laser.press([0.0, 0.0], 0.0);
        laser.drag([10.0, 0.0], 5.0);
        laser.release();
        laser.press([500.0, 500.0], 20.0);
        laser.drag([510.0, 500.0], 25.0);
        laser.release();
        let pieces: Vec<_> = laser.pieces(30.0).collect();
        assert_eq!(pieces.len(), 2, "no piece joins the two trails");
        assert_eq!(pieces[0].1, [10.0, 0.0]);
        assert_eq!(pieces[1].0, [500.0, 500.0]);
    }

    #[test]
    fn a_pressed_pen_that_stands_still_keeps_its_place_in_a_trail() {
        let mut laser = Laser::default();
        laser.press([5.0, 5.0], 0.0);
        assert!(laser.prune(10.0 * FADE_MS), "the open trail is not dropped from under a held pen");
        laser.drag([6.0, 5.0], 10.0 * FADE_MS + 1.0);
        assert_eq!(laser.pieces(10.0 * FADE_MS + 1.0).count(), 1);
    }

    #[test]
    fn nonsense_input_is_ignored_and_the_trail_is_bounded() {
        let mut laser = Laser::default();
        laser.press([f32::NAN, 0.0], 0.0);
        laser.show(Some([f32::INFINITY, 1.0]));
        laser.drag([0.0, f32::NEG_INFINITY], 1.0);
        assert!(!is_visible(&laser));
        laser.press([0.0, 0.0], 0.0);
        for i in 0..(3 * MAX_POINTS) {
            laser.drag([i as f32, 0.0], i as f64 * 0.001);
        }
        assert!(laser.points() <= MAX_POINTS + 1, "{} points", laser.points());
        // Times that go backwards or are not numbers do not break the arithmetic.
        laser.drag([1.0, 1.0], f64::NAN);
        assert!(laser.pieces(f64::NAN).all(|p| p.2.is_finite()));
        laser.clear();
        assert!(!is_visible(&laser));
    }
}
