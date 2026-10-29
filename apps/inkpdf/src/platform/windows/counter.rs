//! The performance counter, the clock Windows stamps pointer messages with, converted to the app's clock (`crate::clock`).
//! The two differ by a constant (Rust's `Instant` is the counter on Windows), measured once with two readings taken back to
//! back, so a pen sample's time is on the same base as `clock::now_ms` to within the microseconds between those readings.

use std::sync::OnceLock;

use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use crate::clock::now_ms;

fn frequency() -> f64 {
    static FREQ: OnceLock<f64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0_i64;
        // SAFETY: `f` is a valid out-pointer for the call.
        unsafe { QueryPerformanceFrequency(&mut f) };
        if f > 0 { f as f64 } else { 1.0 }
    })
}

fn to_ms(count: i64) -> f64 {
    count as f64 * 1000.0 / frequency()
}

/// What to add to the counter's milliseconds to get the app clock's.
fn offset_ms() -> f64 {
    static OFFSET: OnceLock<f64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let before = now_ms();
        let mut c = 0_i64;
        // SAFETY: `c` is a valid out-pointer for the call.
        unsafe { QueryPerformanceCounter(&mut c) };
        let after = now_ms();
        (before + after) / 2.0 - to_ms(c)
    })
}

/// Measure the offset now (at start), so the first sample does not pay for it.
pub fn calibrate() {
    offset_ms();
}

/// A reading of the performance counter (as in a pointer message), in milliseconds of the app's clock.
pub fn counter_ms(count: u64) -> f64 {
    to_ms(count as i64) + offset_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_of_the_performance_counter_convert_the_same_way_and_agree_with_the_app_clock() {
        assert!((counter_ms(3_000_000) - counter_ms(1_000_000) - 2.0 * (counter_ms(2_000_000) - counter_ms(1_000_000))).abs() < 1e-6);
        let mut c = 0_i64;
        // SAFETY: `c` is a valid out-pointer for the call.
        unsafe { QueryPerformanceCounter(&mut c) };
        assert!((counter_ms(c as u64) - now_ms()).abs() < 5.0);
    }
}
