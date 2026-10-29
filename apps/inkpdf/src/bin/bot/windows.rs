//! A test driver: sends real pen and touch input to a running InkPDF window through Windows'
//! synthetic pointer devices, so the pointer layer is tested end to end without hardware.
//!
//! `inkpdf-bot TITLE_PART op [args] [op [args] ...]` — TITLE_PART is any part of the window's
//! title. Positions are fractions (0 to 1) of the window's client area, so a script works at any
//! size or DPI.
//!
//! Ops:
//! * `size` — print the client size in pixels
//! * `wait MS`
//! * `flag eraser|barrel on|off` — set a pen state for the ops that follow
//! * `jitter PX` — shake every following pen point by up to PX pixels, like a tremor
//! * `hover FX FY` — the pen above the screen
//! * `stroke FX0 FY0 FX1 FY1 AMP CYCLES STEPS` — a pen stroke along a wave
//! * `line FX0 FY0 FX1 FY1 STEPS` — a pen stroke along a straight line
//! * `rect FX0 FY0 FX1 FY1 STEPS` — a pen stroke around a rectangle (STEPS per side)
//! * `circle FCX FCY FR STEPS` — a pen stroke around a circle (FR is a fraction of the width)
//! * `tap FX FY` — a pen tap
//! * `finger down|move|up ID FX FY`
//! * `drag FX0 FY0 FX1 FY1 STEPS` — one finger dragging
//! * `pinch FCX FCY FR0 FR1 STEPS` — two fingers moving apart (or together)

use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE, POINTER_FEEDBACK_NONE, POINTER_TYPE_INFO,
    POINTER_TYPE_INFO_0,
};
use windows_sys::Win32::UI::Input::Pointer::{
    InjectSyntheticPointerInput, POINTER_FLAG_DOWN, POINTER_FLAG_FIFTHBUTTON, POINTER_FLAG_FIRSTBUTTON, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE,
    POINTER_FLAG_SECONDBUTTON, POINTER_FLAG_UP, POINTER_FLAG_UPDATE, POINTER_PEN_INFO, POINTER_TOUCH_INFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowTextW, IsWindowVisible, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PEN_MASK_PRESSURE, PT_PEN,
    PT_TOUCH, SetProcessDPIAware, TOUCH_MASK_CONTACTAREA, TOUCH_MASK_PRESSURE,
};

struct Finder {
    needle: String,
    found: Option<HWND>,
}

unsafe extern "system" fn each_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `&mut Finder` passed to EnumWindows below, alive for the call.
    let finder = unsafe { &mut *(lparam as *mut Finder) };
    // SAFETY: plain queries on a window handle EnumWindows gave us.
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let mut buf = [0_u16; 256];
    // SAFETY: `buf` holds 256 UTF-16 units, as passed.
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    if title.contains(&finder.needle) {
        finder.found = Some(hwnd);
        return 0;
    }
    1
}

struct Bot {
    origin: POINT,
    size: [f32; 2],
    pen: HSYNTHETICPOINTERDEVICE,
    touch: HSYNTHETICPOINTERDEVICE,
    eraser: bool,
    barrel: bool,
    /// Pixels of random tremor added to every pen point (0 for none).
    jitter: f32,
    noise_state: std::cell::Cell<u32>,
}

impl Bot {
    /// A point nudged by up to `jitter` pixels each way, from a repeatable noise stream.
    fn shaken(&self, p: POINT) -> POINT {
        if self.jitter <= 0.0 {
            return p;
        }
        let next = || {
            let mut x = self.noise_state.get();
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.noise_state.set(x);
            (x % 2001) as f32 / 1000.0 - 1.0
        };
        POINT { x: p.x + (next() * self.jitter).round() as i32, y: p.y + (next() * self.jitter).round() as i32 }
    }

    fn px(&self, fx: f32, fy: f32) -> POINT {
        POINT { x: self.origin.x + (fx * self.size[0]).round() as i32, y: self.origin.y + (fy * self.size[1]).round() as i32 }
    }

    fn pen_inject(&self, at: POINT, flags: u32, pressure: u32) {
        let mut info = POINTER_PEN_INFO::default();
        info.pointerInfo.pointerType = PT_PEN;
        info.pointerInfo.pointerId = 1;
        info.pointerInfo.ptPixelLocation = at;
        info.pointerInfo.ptPixelLocationRaw = at;
        info.pointerInfo.pointerFlags = flags;
        info.penMask = PEN_MASK_PRESSURE;
        info.pressure = pressure;
        if self.eraser {
            info.penFlags |= PEN_FLAG_INVERTED | PEN_FLAG_ERASER;
        }
        if self.barrel {
            info.penFlags |= PEN_FLAG_BARREL;
        }
        let item = POINTER_TYPE_INFO { r#type: PT_PEN, Anonymous: POINTER_TYPE_INFO_0 { penInfo: info } };
        // SAFETY: `item` is one valid POINTER_TYPE_INFO and the device is live.
        let ok = unsafe { InjectSyntheticPointerInput(self.pen, &item, 1) };
        if ok == 0 {
            eprintln!("pen injection failed");
        }
    }

    /// The tip is the first button, the barrel the second, and the eraser end the fifth.
    fn button_flags(&self) -> u32 {
        if self.eraser { POINTER_FLAG_FIFTHBUTTON } else { POINTER_FLAG_FIRSTBUTTON | if self.barrel { POINTER_FLAG_SECONDBUTTON } else { 0 } }
    }

    fn pen_hover(&self, at: POINT) {
        self.pen_inject(at, POINTER_FLAG_INRANGE | POINTER_FLAG_UPDATE, 0);
    }

    fn pen_down(&self, at: POINT) {
        self.pen_inject(at, POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN | self.button_flags(), 400);
    }

    fn pen_move(&self, at: POINT, pressure: u32) {
        self.pen_inject(at, POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE | self.button_flags(), pressure);
    }

    fn pen_up(&self, at: POINT) {
        self.pen_inject(at, POINTER_FLAG_INRANGE | POINTER_FLAG_UP, 0);
    }

    fn pen_leave(&self, at: POINT) {
        self.pen_inject(at, POINTER_FLAG_UPDATE, 0);
    }

    /// Hover, touch down, follow `path` at about 250 Hz, lift.
    fn pen_path(&self, path: &[POINT]) {
        let shaken: Vec<POINT> = path.iter().map(|&p| self.shaken(p)).collect();
        let Some((&first, rest)) = shaken.split_first() else { return };
        self.pen_hover(first);
        sleep(Duration::from_millis(30));
        self.pen_down(first);
        for (i, &p) in rest.iter().enumerate() {
            sleep(Duration::from_millis(4));
            self.pen_move(p, 300 + (i as u32 * 7) % 500);
        }
        sleep(Duration::from_millis(4));
        let last = shaken.last().copied().unwrap_or(first);
        self.pen_up(last);
        sleep(Duration::from_millis(20));
        self.pen_leave(last);
    }

    fn touch_item(&self, id: u32, at: POINT, flags: u32) -> POINTER_TYPE_INFO {
        let mut info = POINTER_TOUCH_INFO::default();
        info.pointerInfo.pointerType = PT_TOUCH;
        info.pointerInfo.pointerId = id;
        info.pointerInfo.ptPixelLocation = at;
        info.pointerInfo.ptPixelLocationRaw = at;
        info.pointerInfo.pointerFlags = flags;
        info.touchMask = TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE;
        info.rcContact = RECT { left: at.x - 8, top: at.y - 8, right: at.x + 8, bottom: at.y + 8 };
        info.pressure = 512;
        POINTER_TYPE_INFO { r#type: PT_TOUCH, Anonymous: POINTER_TYPE_INFO_0 { touchInfo: info } }
    }

    fn touch_inject(&self, items: &[POINTER_TYPE_INFO]) {
        // SAFETY: `items` is a valid array of the given length and the device is live.
        let ok = unsafe { InjectSyntheticPointerInput(self.touch, items.as_ptr(), items.len() as u32) };
        if ok == 0 {
            eprintln!("touch injection failed");
        }
    }

    fn finger(&self, phase: &str, id: u32, at: POINT) {
        let flags = match phase {
            "down" => POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN | POINTER_FLAG_FIRSTBUTTON,
            "move" => POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE | POINTER_FLAG_FIRSTBUTTON,
            _ => POINTER_FLAG_UP,
        };
        self.touch_inject(&[self.touch_item(id, at, flags)]);
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: inkpdf-bot TITLE_PART op [args] [op [args] ...]   (see the source for the ops)");
        std::process::exit(2);
    }
    // SAFETY: no arguments; makes this process report real pixel coordinates.
    unsafe { SetProcessDPIAware() };
    let mut finder = Finder { needle: args[0].clone(), found: None };
    // SAFETY: the callback only uses `finder`, which outlives the call.
    unsafe { EnumWindows(Some(each_window), (&mut finder as *mut Finder) as LPARAM) };
    let Some(hwnd) = finder.found else {
        eprintln!("no visible window with \"{}\" in its title", args[0]);
        std::process::exit(1);
    };
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: valid out-pointers and a live window handle.
    unsafe {
        GetClientRect(hwnd, &mut client);
        ClientToScreen(hwnd, &mut origin);
    }
    // SAFETY: creating synthetic devices; destroyed before exit.
    let (pen, touch) =
        unsafe { (CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_NONE), CreateSyntheticPointerDevice(PT_TOUCH, 2, POINTER_FEEDBACK_NONE)) };
    if pen.is_null() || touch.is_null() {
        eprintln!("could not create synthetic pointer devices (needs Windows 10 or later)");
        std::process::exit(1);
    }
    let mut bot = Bot {
        origin,
        size: [(client.right - client.left) as f32, (client.bottom - client.top) as f32],
        pen,
        touch,
        eraser: false,
        barrel: false,
        jitter: 0.0,
        noise_state: std::cell::Cell::new(0x9E37_79B9),
    };

    let ops = &args[1..];
    let mut i = 0;
    let num = |k: usize| -> f32 { ops.get(k).and_then(|s| s.parse().ok()).unwrap_or(0.0) };
    while i < ops.len() {
        match ops[i].as_str() {
            "size" => {
                println!("client {}x{} at ({}, {})", bot.size[0], bot.size[1], bot.origin.x, bot.origin.y);
                i += 1;
            }
            "wait" => {
                sleep(Duration::from_millis(num(i + 1) as u64));
                i += 2;
            }
            "flag" => {
                let on = ops.get(i + 2).is_some_and(|s| s == "on");
                match ops.get(i + 1).map(String::as_str) {
                    Some("eraser") => bot.eraser = on,
                    Some("barrel") => bot.barrel = on,
                    _ => {}
                }
                i += 3;
            }
            "jitter" => {
                bot.jitter = num(i + 1);
                i += 2;
            }
            "hover" => {
                bot.pen_hover(bot.px(num(i + 1), num(i + 2)));
                i += 3;
            }
            "tap" => {
                let p = bot.px(num(i + 1), num(i + 2));
                bot.pen_path(&[p, p]);
                i += 3;
            }
            "stroke" => {
                let (x0, y0, x1, y1, amp, cycles, steps) =
                    (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5), num(i + 6), num(i + 7).max(2.0) as usize);
                let path: Vec<POINT> = (0..=steps)
                    .map(|s| {
                        let t = s as f32 / steps as f32;
                        bot.px(lerp(x0, x1, t), lerp(y0, y1, t) + amp * (t * cycles * std::f32::consts::TAU).sin())
                    })
                    .collect();
                bot.pen_path(&path);
                i += 8;
            }
            "line" => {
                let (x0, y0, x1, y1, steps) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5).max(2.0) as usize);
                let path: Vec<POINT> =
                    (0..=steps).map(|s| bot.px(lerp(x0, x1, s as f32 / steps as f32), lerp(y0, y1, s as f32 / steps as f32))).collect();
                bot.pen_path(&path);
                i += 6;
            }
            "rect" => {
                let (x0, y0, x1, y1, steps) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5).max(2.0) as usize);
                let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
                let mut path = Vec::new();
                for side in corners.windows(2) {
                    for s in 0..steps {
                        let t = s as f32 / steps as f32;
                        path.push(bot.px(lerp(side[0].0, side[1].0, t), lerp(side[0].1, side[1].1, t)));
                    }
                }
                path.push(bot.px(x0, y0));
                bot.pen_path(&path);
                i += 6;
            }
            "circle" => {
                let (cx, cy, r, steps) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4).max(8.0) as usize);
                let aspect = bot.size[0] / bot.size[1];
                let path: Vec<POINT> = (0..=steps)
                    .map(|s| {
                        let a = s as f32 / steps as f32 * std::f32::consts::TAU;
                        bot.px(cx + r * a.cos(), cy + r * aspect * a.sin())
                    })
                    .collect();
                bot.pen_path(&path);
                i += 5;
            }
            "finger" => {
                let phase = ops.get(i + 1).cloned().unwrap_or_default();
                bot.finger(&phase, num(i + 2) as u32, bot.px(num(i + 3), num(i + 4)));
                i += 5;
            }
            "drag" => {
                let (x0, y0, x1, y1, steps) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5).max(2.0) as usize);
                bot.finger("down", 0, bot.px(x0, y0));
                for s in 1..=steps {
                    sleep(Duration::from_millis(8));
                    let t = s as f32 / steps as f32;
                    bot.finger("move", 0, bot.px(lerp(x0, x1, t), lerp(y0, y1, t)));
                }
                sleep(Duration::from_millis(8));
                bot.finger("up", 0, bot.px(x1, y1));
                i += 6;
            }
            "pinch" => {
                let (cx, cy, r0, r1, steps) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5).max(2.0) as usize);
                let at = |r: f32| (bot.px(cx - r, cy), bot.px(cx + r, cy));
                let (a, b) = at(r0);
                let down = POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN | POINTER_FLAG_FIRSTBUTTON;
                bot.touch_inject(&[bot.touch_item(0, a, down), bot.touch_item(1, b, down)]);
                for s in 1..=steps {
                    sleep(Duration::from_millis(8));
                    let (a, b) = at(lerp(r0, r1, s as f32 / steps as f32));
                    let flags = POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE | POINTER_FLAG_FIRSTBUTTON;
                    bot.touch_inject(&[bot.touch_item(0, a, flags), bot.touch_item(1, b, flags)]);
                }
                sleep(Duration::from_millis(8));
                let (a, b) = at(r1);
                bot.touch_inject(&[bot.touch_item(0, a, POINTER_FLAG_UP), bot.touch_item(1, b, POINTER_FLAG_UP)]);
                i += 6;
            }
            other => {
                eprintln!("unknown op {other}");
                std::process::exit(2);
            }
        }
    }
    // SAFETY: the devices were created above and are not used again.
    unsafe {
        DestroySyntheticPointerDevice(bot.pen);
        DestroySyntheticPointerDevice(bot.touch);
    }
}
