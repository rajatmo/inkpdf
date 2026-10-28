//! The machine's posture: is it being used as a desktop (keyboard and mouse) or as a tablet (a slate you
//! touch and write on)? That picks the [`Mode`].
//!
//! Windows reports the posture of a convertible as `SM_CONVERTIBLESLATEMODE` (0 when it is a slate, non-zero
//! when it is a laptop) and announces a change with `WM_SETTINGCHANGE` ("ConvertibleSlateMode"). The number
//! alone cannot be trusted: an ordinary desktop PC reports 0 too (measured on one with no touch screen). So it
//! only counts on a machine that could be a tablet at all: with a touch or pen digitizer, and a platform role
//! of mobile or slate (a desktop with a touch monitor is still a desktop).
//!
//! [`decide`] is the rule, as a pure function with the readings as its input; [`detect`] takes the readings.

use windows_sys::Win32::System::Power::PowerDeterminePlatformRole;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CONVERTIBLESLATEMODE, SM_DIGITIZER, SM_MAXIMUMTOUCHES};

use crate::input::Mode;

/// `SM_DIGITIZER` bits for a touch or pen digitizer, built in or attached.
const DIGITIZER_TOUCH_OR_PEN: i32 = 0x1 | 0x2 | 0x4 | 0x8;
/// `POWER_PLATFORM_ROLE` values of a machine that can be carried and used as a tablet.
const ROLE_MOBILE: i32 = 2;
const ROLE_SLATE: i32 = 8;

/// What Windows said about the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Readings {
    /// `SM_CONVERTIBLESLATEMODE`: 0 means slate (and, on a desktop with no such sensor, means nothing).
    pub slate_mode: i32,
    /// `SM_MAXIMUMTOUCHES`: how many fingers the screen follows at once (0: no touch screen).
    pub touches: i32,
    /// `SM_DIGITIZER`: the kinds of touch and pen input present.
    pub digitizer: i32,
    /// `PowerDeterminePlatformRole`: desktop, mobile, slate, …
    pub role: i32,
}

/// Tablet when the machine is in slate posture *and* is the kind of machine that has one; desktop otherwise.
pub fn decide(r: Readings) -> Mode {
    let has_digitizer = r.touches > 0 || r.digitizer & DIGITIZER_TOUCH_OR_PEN != 0;
    let portable = r.role == ROLE_MOBILE || r.role == ROLE_SLATE;
    if r.slate_mode == 0 && has_digitizer && portable { Mode::Tablet } else { Mode::Desktop }
}

/// Read the machine's posture now.
pub fn read() -> Readings {
    // SAFETY: these take no pointers and have no preconditions.
    unsafe {
        Readings {
            slate_mode: GetSystemMetrics(SM_CONVERTIBLESLATEMODE),
            touches: GetSystemMetrics(SM_MAXIMUMTOUCHES),
            digitizer: GetSystemMetrics(SM_DIGITIZER),
            role: PowerDeterminePlatformRole(),
        }
    }
}

/// The mode the machine's posture calls for.
pub fn detect() -> Mode {
    decide(read())
}

/// Whether a `WM_SETTINGCHANGE` message says the posture changed: its `lparam` points at the name of the
/// setting, as a null-ended wide string.
pub fn is_posture_message(lparam: isize) -> bool {
    const NAME: &str = "ConvertibleSlateMode";
    if lparam == 0 {
        return false;
    }
    let wide = lparam as *const u16;
    for (i, expected) in NAME.encode_utf16().chain(std::iter::once(0)).enumerate() {
        // SAFETY: Windows passes a null-ended string; we stop at the first character that differs from the name
        // (the null at the end of the name included), so we never read past the end of one that is shorter.
        if unsafe { wide.add(i).read() } != expected {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP_PC: Readings = Readings { slate_mode: 0, touches: 0, digitizer: 0, role: 1 };
    const TOUCH_MONITOR_ON_DESKTOP: Readings = Readings { slate_mode: 0, touches: 10, digitizer: 0x81, role: 1 };
    const CONVERTIBLE_AS_LAPTOP: Readings = Readings { slate_mode: 1, touches: 10, digitizer: 0x85, role: 2 };
    const CONVERTIBLE_AS_TABLET: Readings = Readings { slate_mode: 0, touches: 10, digitizer: 0x85, role: 2 };
    const SLATE_WITHOUT_KEYBOARD: Readings = Readings { slate_mode: 0, touches: 10, digitizer: 0x85, role: 8 };

    #[test]
    fn a_desktop_is_a_desktop_even_though_it_reports_slate_mode() {
        // Measured: a desktop PC with no touch screen reports SM_CONVERTIBLESLATEMODE = 0.
        assert_eq!(decide(DESKTOP_PC), Mode::Desktop);
        assert_eq!(decide(TOUCH_MONITOR_ON_DESKTOP), Mode::Desktop);
    }

    #[test]
    fn a_convertible_follows_its_posture() {
        assert_eq!(decide(CONVERTIBLE_AS_LAPTOP), Mode::Desktop);
        assert_eq!(decide(CONVERTIBLE_AS_TABLET), Mode::Tablet);
        assert_eq!(decide(SLATE_WITHOUT_KEYBOARD), Mode::Tablet);
    }

    #[test]
    fn a_portable_in_slate_posture_with_no_touch_or_pen_is_not_a_tablet() {
        let no_digitizer = Readings { touches: 0, digitizer: 0, ..CONVERTIBLE_AS_TABLET };
        assert_eq!(decide(no_digitizer), Mode::Desktop);
        // Any of the digitizer kinds is enough.
        for bits in [0x1, 0x2, 0x4, 0x8] {
            assert_eq!(decide(Readings { touches: 0, digitizer: bits, ..CONVERTIBLE_AS_TABLET }), Mode::Tablet, "{bits:#x}");
        }
    }

    #[test]
    fn only_the_convertible_slate_mode_message_is_a_posture_change() {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let yes = wide("ConvertibleSlateMode");
        assert!(is_posture_message(yes.as_ptr() as isize));
        for other in ["", "Convertible", "ConvertibleSlateModeX", "ImmersiveColorSet", "convertibleslatemode", "Environment"] {
            let w = wide(other);
            assert!(!is_posture_message(w.as_ptr() as isize), "{other:?}");
        }
        assert!(!is_posture_message(0));
    }

    #[test]
    fn what_this_machine_reports_decides_without_crashing() {
        // Whatever the machine is, reading it works and gives one of the two modes.
        let r = read();
        assert!(matches!(decide(r), Mode::Desktop | Mode::Tablet), "{r:?}");
    }
}
