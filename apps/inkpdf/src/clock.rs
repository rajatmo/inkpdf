//! The clock everything that is timed shares: milliseconds on a monotonic clock, counted from the first reading (a little after
//! zero, never zero: a sample stamped 0 means "no timestamp" to the pen readers). The OS's own pen readers convert the stamps
//! their input carries to this clock (on Windows, `platform::windows::counter` does it for the performance counter). The editor
//! takes the clock as a function (see `Editor::set_clock`), so that tests can give it another.

use std::sync::OnceLock;
use std::time::Instant;

fn origin() -> Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    *ORIGIN.get_or_init(|| Instant::now() - std::time::Duration::from_millis(1))
}

/// Milliseconds on the monotonic clock, now.
pub fn now_ms() -> f64 {
    origin().elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_moves_forward() {
        let (a, b) = (now_ms(), now_ms());
        assert!(a > 0.0 && b >= a);
    }
}
