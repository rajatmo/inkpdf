//! Native Windows pieces: error boxes, opening web addresses, and the text clipboard.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{GlobalFree, HWND};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW, SPI_GETCLIENTAREAANIMATION, SW_SHOWNORMAL, SystemParametersInfoW};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::clipboard::TextClipboard;

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

/// What the user has asked of motion: `INKPDF_MOTION` (`off` or `on`) says it for this run, and otherwise Windows does
/// ("Show animations in Windows", under Accessibility > Visual effects).
pub fn motion_prefs() -> crate::motion::Prefs {
    match std::env::var("INKPDF_MOTION").ok().as_deref() {
        Some("off") => return crate::motion::Prefs::REDUCED,
        Some("on") => return crate::motion::Prefs::FULL,
        _ => {}
    }
    let mut animations: i32 = 1;
    // SAFETY: the value is a BOOL the call fills in, which lives across the call; no flags are set.
    let known = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut animations as *mut i32).cast(), 0) } != 0;
    crate::motion::Prefs { reduced: known && animations == 0 }
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

/// The clipboard format for text in UTF-16 (`CF_UNICODETEXT`).
const CF_UNICODETEXT: u32 = 13;

/// The Windows clipboard, for the text of text boxes: copy in InkPDF, paste in any program, and the other way.
/// (The window owns it: Windows refuses to put anything on the clipboard for a program with no window of its own.)
pub struct SystemClipboard {
    owner: HWND,
}

impl SystemClipboard {
    /// The clipboard as `window` uses it.
    pub fn new(window: &Window) -> Option<Self> {
        Some(Self { owner: hwnd_of(window)? })
    }
}

impl TextClipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        // SAFETY: the clipboard is opened for the duration and closed again on every path; the handle Windows gives
        // is locked while read, and read only as far as the size of the block it names, stopping at the first NUL.
        unsafe {
            if OpenClipboard(self.owner) == 0 {
                return None;
            }
            let handle = GetClipboardData(CF_UNICODETEXT);
            let text = if handle.is_null() {
                None
            } else {
                let units = GlobalLock(handle).cast::<u16>().cast_const();
                if units.is_null() {
                    None
                } else {
                    let all = std::slice::from_raw_parts(units, GlobalSize(handle) / 2);
                    let end = all.iter().position(|&u| u == 0).unwrap_or(all.len());
                    let text = String::from_utf16_lossy(&all[..end]);
                    GlobalUnlock(handle);
                    Some(text)
                }
            };
            CloseClipboard();
            text
        }
    }

    fn set(&mut self, text: &str) {
        let units = wide(text);
        // SAFETY: the clipboard is opened for the duration and closed again on every path; the block is allocated
        // for exactly the text and its NUL, filled while locked, and given to Windows (which owns it from then on)
        // or freed if Windows would not take it.
        unsafe {
            if OpenClipboard(self.owner) == 0 {
                return;
            }
            EmptyClipboard();
            let block = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
            if !block.is_null() {
                let dst = GlobalLock(block).cast::<u16>();
                if dst.is_null() {
                    GlobalFree(block);
                } else {
                    std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
                    GlobalUnlock(block);
                    if SetClipboardData(CF_UNICODETEXT, block).is_null() {
                        GlobalFree(block);
                    }
                }
            }
            CloseClipboard();
        }
    }
}

/// Tell the user something went wrong.
pub fn error_box(window: &Arc<Window>, text: &str) {
    let Some(hwnd) = hwnd_of(window) else { return };
    let (text, caption) = (wide(text), wide("InkPDF"));
    // SAFETY: both strings are NUL-terminated and live across the call.
    unsafe { MessageBoxW(hwnd, text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
}
