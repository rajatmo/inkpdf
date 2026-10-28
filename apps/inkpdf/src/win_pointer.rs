//! Pen and touch input straight from Windows pointer messages.
//!
//! winit 0.30 reports a pen and a finger as the same kind of touch and drops everything but
//! pressure: no eraser end, no barrel button, no tilt, and only one point per frame. Windows
//! batches the points a pen produces between messages, so reading `GetPointerPenInfoHistory`
//! is what gives smooth strokes at 200–1000 Hz.
//!
//! The window is subclassed: pen and touch pointer messages are read here and swallowed (so
//! winit never sees them and Windows makes no mouse input from them); everything else,
//! including the real mouse, goes on to winit. Events are queued for the app to [`drain`].
//!
//! This is the one place the app uses `unsafe`: calling Win32 and being called back by it. A
//! panic must never unwind into Windows, so the callback catches them.

use std::cell::RefCell;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::UI::Controls::{
    FEEDBACK_GESTURE_PRESSANDTAP, FEEDBACK_PEN_BARRELVISUALIZATION, FEEDBACK_PEN_DOUBLETAP, FEEDBACK_PEN_PRESSANDHOLD, FEEDBACK_PEN_RIGHTTAP,
    FEEDBACK_PEN_TAP, FEEDBACK_TOUCH_CONTACTVISUALIZATION, FEEDBACK_TOUCH_DOUBLETAP, FEEDBACK_TOUCH_PRESSANDHOLD, FEEDBACK_TOUCH_RIGHTTAP,
    FEEDBACK_TOUCH_TAP, SetWindowFeedbackSetting,
};
use windows_sys::Win32::UI::Input::Pointer::{
    GetPointerDeviceRects, GetPointerPenInfo, GetPointerPenInfoHistory, GetPointerTouchInfo, GetPointerTouchInfoHistory, GetPointerType,
    POINTER_FLAG_FIFTHBUTTON, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_INFO, POINTER_PEN_INFO, POINTER_TOUCH_INFO,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PEN_MASK_PRESSURE, PM_REMOVE, PT_PEN, PT_TOUCH, PeekMessageW,
    WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERENTER, WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::input::{Device, PenButtons, Phase, PointerEvent, Sample};

const SUBCLASS_ID: usize = 0x494e_4b50; // "INKP"
/// Most history entries read per message; Windows keeps up to 64.
const MAX_HISTORY: usize = 64;

thread_local! {
    /// The app window's handle (0 before `install`), for `pump`.
    static HWND_OF_APP: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    static QUEUE: RefCell<Vec<PointerEvent>> = const { RefCell::new(Vec::new()) };
    static WAKE: RefCell<Option<Arc<Window>>> = const { RefCell::new(None) };
}

/// Whether `INKPDF_TRACE` is set: print the raw pen flags Windows delivers, for diagnosing a
/// pen or driver that reports its eraser or button differently.
fn trace_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("INKPDF_TRACE").is_some())
}

fn qpc_frequency() -> f64 {
    static FREQ: OnceLock<f64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0_i64;
        // SAFETY: `f` is a valid out-pointer for the call.
        unsafe { QueryPerformanceFrequency(&mut f) };
        if f > 0 { f as f64 } else { 1.0 }
    })
}

/// Milliseconds on the clock pointer timestamps use (the performance counter).
pub fn now_ms() -> f64 {
    let mut c = 0_i64;
    // SAFETY: `c` is a valid out-pointer for the call.
    unsafe { QueryPerformanceCounter(&mut c) };
    c as f64 * 1000.0 / qpc_frequency()
}

/// Start receiving pen and touch for `window`. Fails if the window has no Win32 handle.
pub fn install(window: &Arc<Window>) -> Result<(), String> {
    let hwnd = hwnd_of(window)?;
    HWND_OF_APP.with(|h| h.set(hwnd as isize));
    WAKE.with(|w| *w.borrow_mut() = Some(window.clone()));
    // SAFETY: `hwnd` is this thread's live window; `subclass_proc` has the required signature
    // and stays valid for the life of the process.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) };
    if ok == 0 {
        return Err("could not subclass the window for pen input".into());
    }
    // No press-and-hold rings, tap feedback or right-tap emulation: they delay and interfere
    // with drawing.
    let off: i32 = 0;
    for feedback in [
        FEEDBACK_TOUCH_CONTACTVISUALIZATION,
        FEEDBACK_PEN_BARRELVISUALIZATION,
        FEEDBACK_PEN_TAP,
        FEEDBACK_PEN_DOUBLETAP,
        FEEDBACK_PEN_PRESSANDHOLD,
        FEEDBACK_PEN_RIGHTTAP,
        FEEDBACK_TOUCH_TAP,
        FEEDBACK_TOUCH_DOUBLETAP,
        FEEDBACK_TOUCH_PRESSANDHOLD,
        FEEDBACK_TOUCH_RIGHTTAP,
        FEEDBACK_GESTURE_PRESSANDTAP,
    ] {
        // SAFETY: `off` outlives the call and the size matches the BOOL pointed to.
        unsafe { SetWindowFeedbackSetting(hwnd, feedback, 0, std::mem::size_of::<i32>() as u32, (&off as *const i32).cast::<c_void>()) };
    }
    Ok(())
}

pub fn uninstall(window: &Arc<Window>) {
    if let Ok(hwnd) = hwnd_of(window) {
        // SAFETY: removes the subclass installed by `install`.
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
    }
}

fn hwnd_of(window: &Window) -> Result<HWND, String> {
    match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Win32(h) => Ok(h.hwnd.get() as HWND),
        _ => Err("not a Win32 window".into()),
    }
}

/// The pointer messages, from `WM_POINTERUPDATE` to `WM_POINTERROUTEDTO`.
const POINTER_MSG_LAST: u32 = 0x0251;

/// Process the pen and touch messages already waiting in this window's queue, right now.
///
/// While the app waits for the display's refresh it is not pumping messages, so a pen sample
/// that arrives in that wait would otherwise sit until after the frame. Calling this once the
/// wait is over brings the newest samples into the frame about to be drawn. Only pointer
/// messages are taken (they go to the subclass procedure, which swallows them); everything else
/// is left for winit.
pub fn pump() {
    let hwnd = HWND_OF_APP.with(std::cell::Cell::get);
    if hwnd == 0 {
        return;
    }
    // SAFETY: MSG is plain data, for which all zeros is a valid value.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // Bounded, so a flood of input cannot hold the frame.
    for _ in 0..512 {
        // SAFETY: `msg` is a valid out-pointer and `hwnd` is this thread's window.
        let got = unsafe { PeekMessageW(&mut msg, hwnd as HWND, WM_POINTERUPDATE, POINTER_MSG_LAST, PM_REMOVE) };
        if got == 0 {
            break;
        }
        // SAFETY: dispatches a message this thread just removed from its own queue.
        unsafe { DispatchMessageW(&msg) };
    }
}

/// Whether any events are waiting to be taken.
pub fn pending() -> bool {
    QUEUE.with(|q| !q.borrow().is_empty())
}

/// Take the events received since the last call, oldest first.
pub fn drain() -> Vec<PointerEvent> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if matches!(msg, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP | WM_POINTERENTER | WM_POINTERLEAVE | WM_POINTERCAPTURECHANGED) {
        let handled = catch_unwind(AssertUnwindSafe(|| handle_pointer(hwnd, msg, wparam))).unwrap_or(false);
        if handled {
            return 0;
        }
    }
    // SAFETY: forwards the same message to the next handler in the chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Returns whether the message was a pen or touch one we took.
fn handle_pointer(hwnd: HWND, msg: u32, wparam: WPARAM) -> bool {
    let id = (wparam & 0xffff) as u32;
    let mut kind = 0;
    // SAFETY: `kind` is a valid out-pointer.
    if unsafe { GetPointerType(id, &mut kind) } == 0 {
        return false;
    }
    let event = match kind {
        PT_PEN => pen_event(hwnd, msg, id),
        PT_TOUCH => touch_event(hwnd, msg, id),
        _ => return false,
    };
    if let Some(event) = event {
        QUEUE.with(|q| q.borrow_mut().push(event));
        WAKE.with(|w| {
            if let Some(window) = w.borrow().as_ref() {
                window.request_redraw();
            }
        });
    }
    true
}

fn phase_of(msg: u32, in_contact: bool) -> Phase {
    match msg {
        WM_POINTERDOWN => Phase::Down,
        WM_POINTERUP => Phase::Up,
        WM_POINTERCAPTURECHANGED => Phase::Cancel,
        WM_POINTERLEAVE => Phase::Leave,
        WM_POINTERENTER => Phase::Hover,
        _ if in_contact => Phase::Move,
        _ => Phase::Hover,
    }
}

/// A pointer's position in the window's client area, in device pixels. A pen reports where it is
/// on its own, finer, grid: map that onto the display for sub-pixel positions.
fn client_pos(hwnd: HWND, info: &POINTER_INFO, fine: bool) -> [f32; 2] {
    let mut screen = [info.ptPixelLocationRaw.x as f64, info.ptPixelLocationRaw.y as f64];
    if fine {
        let (mut device, mut display) = (RECT { left: 0, top: 0, right: 0, bottom: 0 }, RECT { left: 0, top: 0, right: 0, bottom: 0 });
        // SAFETY: both RECTs are valid out-pointers.
        let ok = unsafe { GetPointerDeviceRects(info.sourceDevice, &mut device, &mut display) };
        if ok != 0 && device.right > device.left && device.bottom > device.top {
            let fx = f64::from(info.ptHimetricLocationRaw.x - device.left) / f64::from(device.right - device.left);
            let fy = f64::from(info.ptHimetricLocationRaw.y - device.top) / f64::from(device.bottom - device.top);
            screen = [
                f64::from(display.left) + fx * f64::from(display.right - display.left),
                f64::from(display.top) + fy * f64::from(display.bottom - display.top),
            ];
        }
    }
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: `origin` is a valid out-pointer.
    unsafe { ClientToScreen(hwnd, &mut origin) };
    [(screen[0] - f64::from(origin.x)) as f32, (screen[1] - f64::from(origin.y)) as f32]
}

fn timestamp(info: &POINTER_INFO) -> f64 {
    if info.PerformanceCount > 0 { info.PerformanceCount as f64 * 1000.0 / qpc_frequency() } else { now_ms() }
}

fn pen_event(hwnd: HWND, msg: u32, id: u32) -> Option<PointerEvent> {
    let mut count = 0_u32;
    // SAFETY: asking only for the number of history entries.
    let have_history = unsafe { GetPointerPenInfoHistory(id, &mut count, std::ptr::null_mut()) } != 0 && count > 0;
    let mut entries: Vec<POINTER_PEN_INFO> = Vec::new();
    if have_history {
        let n = (count as usize).min(MAX_HISTORY);
        let mut got = n as u32;
        entries.resize(n, POINTER_PEN_INFO::default());
        // SAFETY: `entries` has room for `got` entries, as the call requires.
        if unsafe { GetPointerPenInfoHistory(id, &mut got, entries.as_mut_ptr()) } != 0 {
            entries.truncate(got as usize);
        } else {
            entries.clear();
        }
    }
    if entries.is_empty() {
        let mut one = POINTER_PEN_INFO::default();
        // SAFETY: `one` is a valid out-pointer.
        if unsafe { GetPointerPenInfo(id, &mut one) } == 0 {
            return None;
        }
        entries.push(one);
    }
    // Windows lists the newest first.
    entries.reverse();
    let newest = entries.last()?;
    let flags = newest.pointerInfo.pointerFlags;
    let pen = PenButtons {
        in_range: flags & POINTER_FLAG_INRANGE != 0,
        contact: flags & POINTER_FLAG_INCONTACT != 0,
        // Drivers differ: the eraser end can show as pen flags, or as the pen's fifth button.
        eraser: newest.penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0 || flags & POINTER_FLAG_FIFTHBUTTON != 0,
        barrel: newest.penFlags & PEN_FLAG_BARREL != 0,
    };
    if trace_enabled() {
        eprintln!(
            "pen msg {msg:#x} pointerFlags={:#x} penFlags={:#x} penMask={:#x} pressure={} -> contact={} eraser={} barrel={} ({} samples)",
            flags,
            newest.penFlags,
            newest.penMask,
            newest.pressure,
            pen.contact,
            pen.eraser,
            pen.barrel,
            entries.len()
        );
    }
    let samples = entries
        .iter()
        .map(|e| Sample {
            pos: client_pos(hwnd, &e.pointerInfo, true),
            pressure: if e.penMask & PEN_MASK_PRESSURE != 0 { (e.pressure as f32 / 1024.0).clamp(0.0, 1.0) } else { 0.5 },
            t_ms: timestamp(&e.pointerInfo),
        })
        .collect();
    Some(PointerEvent { device: Device::Pen, id, phase: phase_of(msg, pen.contact), samples, pen, contact_px: 0.0 })
}

fn touch_event(hwnd: HWND, msg: u32, id: u32) -> Option<PointerEvent> {
    let mut count = 0_u32;
    // SAFETY: asking only for the number of history entries.
    let have_history = unsafe { GetPointerTouchInfoHistory(id, &mut count, std::ptr::null_mut()) } != 0 && count > 0;
    let mut entries: Vec<POINTER_TOUCH_INFO> = Vec::new();
    if have_history {
        let n = (count as usize).min(MAX_HISTORY);
        let mut got = n as u32;
        entries.resize(n, POINTER_TOUCH_INFO::default());
        // SAFETY: `entries` has room for `got` entries.
        if unsafe { GetPointerTouchInfoHistory(id, &mut got, entries.as_mut_ptr()) } != 0 {
            entries.truncate(got as usize);
        } else {
            entries.clear();
        }
    }
    if entries.is_empty() {
        let mut one = POINTER_TOUCH_INFO::default();
        // SAFETY: `one` is a valid out-pointer.
        if unsafe { GetPointerTouchInfo(id, &mut one) } == 0 {
            return None;
        }
        entries.push(one);
    }
    entries.reverse();
    let newest = entries.last()?;
    let in_contact = newest.pointerInfo.pointerFlags & POINTER_FLAG_INCONTACT != 0;
    let r = newest.rcContact;
    let contact_px = ((r.right - r.left).max(r.bottom - r.top)).max(0) as f32;
    let samples =
        entries.iter().map(|e| Sample { pos: client_pos(hwnd, &e.pointerInfo, false), pressure: 1.0, t_ms: timestamp(&e.pointerInfo) }).collect();
    Some(PointerEvent { device: Device::Touch, id, phase: phase_of(msg, in_contact), samples, pen: PenButtons::default(), contact_px })
}
