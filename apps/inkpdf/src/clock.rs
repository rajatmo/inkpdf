//! The clock everything that is timed shares: milliseconds on the performance counter, the clock Windows stamps pen and
//! touch messages with. The editor takes it as a function (see `Editor::set_clock`), so that tests can give it another.

use std::sync::OnceLock;

use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

fn frequency() -> f64 {
    static FREQ: OnceLock<f64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0_i64;
        // SAFETY: `f` is a valid out-pointer for the call.
        unsafe { QueryPerformanceFrequency(&mut f) };
        if f > 0 { f as f64 } else { 1.0 }
    })
}

/// Milliseconds on the performance counter, now.
pub fn now_ms() -> f64 {
    let mut c = 0_i64;
    // SAFETY: `c` is a valid out-pointer for the call.
    unsafe { QueryPerformanceCounter(&mut c) };
    c as f64 * 1000.0 / frequency()
}

/// A reading of the performance counter (as in a pointer message), in milliseconds.
pub fn counter_ms(count: u64) -> f64 {
    count as f64 * 1000.0 / frequency()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_moves_forward_and_readings_convert_the_same_way() {
        let (a, b) = (now_ms(), now_ms());
        assert!(a > 0.0 && b >= a);
        assert!((counter_ms(2_000_000) - 2.0 * counter_ms(1_000_000)).abs() < 1e-6);
    }
}
