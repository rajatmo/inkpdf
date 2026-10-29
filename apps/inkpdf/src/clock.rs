//! The clock everything that is timed shares: milliseconds on a monotonic clock. On Windows that is the performance
//! counter, the clock Windows stamps pen and touch messages with; elsewhere it is `Instant`, from the first reading,
//! which is the clock the OS's own pen readers convert their timestamps to. The editor takes it as a function (see
//! `Editor::set_clock`), so that tests can give it another.

#[cfg(windows)]
mod counter {
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

    pub fn now_ms() -> f64 {
        let mut c = 0_i64;
        // SAFETY: `c` is a valid out-pointer for the call.
        unsafe { QueryPerformanceCounter(&mut c) };
        c as f64 * 1000.0 / frequency()
    }

    pub fn counter_ms(count: u64) -> f64 {
        count as f64 * 1000.0 / frequency()
    }
}

#[cfg(not(windows))]
mod counter {
    use std::sync::OnceLock;
    use std::time::Instant;

    /// The first reading is a little after zero, never zero: a sample stamped 0 means "no timestamp" to the pen readers.
    fn origin() -> Instant {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        *ORIGIN.get_or_init(|| Instant::now() - std::time::Duration::from_millis(1))
    }

    pub fn now_ms() -> f64 {
        origin().elapsed().as_secs_f64() * 1000.0
    }
}

/// Milliseconds on the monotonic clock, now.
pub fn now_ms() -> f64 {
    counter::now_ms()
}

/// A reading of the performance counter (as in a pointer message), in milliseconds.
#[cfg(windows)]
pub fn counter_ms(count: u64) -> f64 {
    counter::counter_ms(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_moves_forward() {
        let (a, b) = (now_ms(), now_ms());
        assert!(a > 0.0 && b >= a);
    }

    #[cfg(windows)]
    #[test]
    fn readings_of_the_performance_counter_convert_the_same_way() {
        assert!((counter_ms(2_000_000) - 2.0 * counter_ms(1_000_000)).abs() < 1e-6);
    }
}
