//! Native Windows pieces: error boxes and opening web addresses.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW, SW_SHOWNORMAL};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

fn hwnd_of(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
        _ => None,
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether the app was started from a terminal (see [`attach_parent_console`]).
static HAS_CONSOLE: AtomicBool = AtomicBool::new(false);

/// The release build is a Windows-subsystem program, so it opens no console window of its own.
/// When it is started from a terminal, join that terminal so `--list-gpus`, `--bench` and error
/// messages still print there.
pub fn attach_parent_console() {
    // SAFETY: no pointers; fails harmlessly when the parent has no console (started from Explorer).
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    HAS_CONSOLE.store(attached, Ordering::Relaxed);
}

/// Report a problem before there is a window: on the terminal if there is one, otherwise in a box,
/// since a program without a console would fail silently.
pub fn report_fatal(text: &str) {
    eprintln!("{text}");
    if HAS_CONSOLE.load(Ordering::Relaxed) {
        return;
    }
    let (text, caption) = (wide(text), wide("InkPDF"));
    // SAFETY: both strings are NUL-terminated and live across the call; no owner window.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
}

/// Open a web address in the system's default program for it (a browser, or a mail app). The
/// address is checked again here: only a plain http, https or mailto address is ever handed over.
pub fn open_url(url: &str) {
    let Some(url) = crate::links::safe_url(url) else { return };
    let (verb, target) = (wide("open"), wide(&url));
    // SAFETY: both strings are NUL-terminated and live across the call; no owner window or
    // working directory is passed.
    unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
}

/// Tell the user something went wrong.
pub fn error_box(window: &Arc<Window>, text: &str) {
    let Some(hwnd) = hwnd_of(window) else { return };
    let (text, caption) = (wide(text), wide("InkPDF"));
    // SAFETY: both strings are NUL-terminated and live across the call.
    unsafe { MessageBoxW(hwnd, text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
}
