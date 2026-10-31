//! Showing the document as slides: one page at a time, whole, as big as the window lets it be, over black, with nothing of the program's own on it but, for
//! a moment after anything is done, how far through it is and a button to leave. It is a modal layer, as the pages overview is: every device talks to it and to
//! nothing else, and what it takes is only what moves between the pages. With the left mouse button and the keyboard, or with the stylus's tip alone, or with a
//! finger, all of it can be done:
//!
//! * next page: a click, a tap of the tip or of a finger on the right of the window, or a swipe of a finger to the left; the right and down arrows, Page Down, the
//!   space bar, Enter; a turn of the wheel down;
//! * the page before: a click or a tap on the left third of the window, a swipe to the right; the left and up arrows, Page Up, Backspace; a turn of the wheel up;
//! * the first and the last page: Home and End; leave: Escape, or the button that shows in a corner while the pointer moves or a tap has just been made, or Ctrl+L
//!   again.
//!
//! The window is made full screen for it (and put back as it was), and the view is put back, on the page that was last shown, when it is left.

use crate::input::{Device, Dir, Key, Phase, PointerEvent, Wheel};
use crate::requests::Request;
use crate::toolbar::{PresentationLayout, PresentationView};

use super::{Act, Editor};

/// How long the counter and the button to leave stay after anything is done, in seconds.
const HINT_SECONDS: f32 = 2.5;
/// How far a press may move and still be a tap, and how far a finger must go to be a swipe, in logical pixels.
const TAP_PX: f32 = 12.0;
const SWIPE_PX: f32 = 56.0;
/// What a touchpad's pixels of scrolling make a page turn, in pixels.
const WHEEL_PAGE_PX: f32 = 100.0;
/// A tap on the left of the window (this much of its width) goes back a page; anywhere else, on.
const LEFT_ZONE: f32 = 0.3;

/// A press under way: which pointer, where it began, and whether the button to leave was showing when it did.
#[derive(Clone, Copy, Debug)]
struct Press {
    dev: Device,
    id: u32,
    start: [f32; 2],
    exit_showing: bool,
}

/// The slides, while they are shown.
pub(super) struct Presentation {
    /// The page shown (0-based).
    page: usize,
    /// Whether the window filled the screen before, so that it is given back as it was.
    was_fullscreen: bool,
    /// Whether the zoom followed the width, to be so again.
    fit_mode: bool,
    /// How much longer the counter and the button to leave are shown, in seconds.
    hint: f32,
    press: Option<Press>,
    /// A touchpad's scrolling not yet enough for a page.
    wheel: f32,
}

impl Editor {
    /// Whether the document is being shown as slides.
    pub fn presenting(&self) -> bool {
        self.presentation.is_some()
    }

    /// Begin showing the document as slides, from the page in view. (Not with a prompt up, and not without a document.)
    pub fn start_presentation(&mut self) {
        if self.presentation.is_some() || !self.ws.has_doc || self.ws.layout.is_empty() || self.chrome.prompt.is_some() {
            return;
        }
        // (What was being typed becomes part of the document; what was under way ends; every card of the window is put away.)
        self.settle();
        self.tools.end_all(&mut self.ws);
        self.cancel_touches();
        self.mouse_press = None;
        self.chrome.act = Act::Idle;
        self.chrome.overview = None;
        self.chrome.panel = None;
        self.chrome.pill = None;
        self.chrome.theme_menu = false;
        self.chrome.bar_open = false;
        self.search.close();
        self.close_item_card();
        self.close_link_card();
        self.close_error();
        self.ws.selection.clear();
        let page = self.ws.view.current_page(&self.ws.layout);
        let was_fullscreen = self.chrome.fullscreen;
        if !was_fullscreen {
            self.ws.requests.push(Request::Fullscreen);
        }
        self.presentation = Some(Presentation { page, was_fullscreen, fit_mode: self.ws.fit_mode, hint: HINT_SECONDS, press: None, wheel: 0.0 });
        self.show_slide(page);
    }

    /// Leave the slides: the window is put back as it was, and the page that was last shown is where the document is.
    pub fn stop_presentation(&mut self) {
        let Some(shown) = self.presentation.take() else { return };
        self.ws.view.show_normally(&self.ws.layout);
        self.ws.fit_mode = shown.fit_mode;
        if shown.fit_mode {
            self.ws.view.fit_width(&self.ws.layout);
        }
        self.ws.scroll_to_page(shown.page);
        if !shown.was_fullscreen && self.chrome.fullscreen {
            self.ws.requests.push(Request::Fullscreen);
        }
        self.ws.redraw = true;
    }

    /// Show page `page` (0-based, kept within the document).
    fn show_slide(&mut self, page: usize) {
        let last = self.ws.layout.len().saturating_sub(1);
        let page = page.min(last);
        if let Some(rect) = self.ws.layout.page(page) {
            self.ws.fit_mode = false;
            self.ws.view.show_whole(rect);
        }
        if let Some(shown) = &mut self.presentation {
            shown.page = page;
            shown.hint = HINT_SECONDS;
        }
        self.ws.redraw = true;
    }

    /// The page after (`forward`) or before the one shown, if there is one.
    fn turn(&mut self, forward: bool) {
        let Some(page) = self.presentation.as_ref().map(|p| p.page) else { return };
        let to = if forward { page + 1 } else { page.saturating_sub(1) };
        // (The last page stays, with its counter shown: there is nothing after it.)
        self.show_slide(to);
    }

    /// The window changed size while the slides are shown: the page is put in it again.
    pub(super) fn reshow_slide(&mut self) {
        if let Some(page) = self.presentation.as_ref().map(|p| p.page) {
            self.show_slide(page);
        }
    }

    /// Let the counter and the button to leave be seen for a moment.
    fn hint(&mut self) {
        if let Some(shown) = &mut self.presentation {
            if shown.hint <= 0.0 {
                self.ws.redraw = true;
            }
            shown.hint = HINT_SECONDS;
        }
    }

    /// One frame of the slides' own motion: the counter and the button to leave go after a while. Whether it is still moving.
    pub(in crate::editor) fn step_presentation(&mut self, dt: f32) -> bool {
        let Some(shown) = &mut self.presentation else { return false };
        if shown.hint <= 0.0 {
            return false;
        }
        shown.hint -= dt.max(0.0);
        if shown.hint <= 0.0 {
            shown.hint = 0.0;
            self.ws.redraw = true;
            return false;
        }
        true
    }

    /// What is drawn over the page: where the page is, and what is shown of the rest.
    pub fn presentation_view(&self) -> Option<PresentationView> {
        let shown = self.presentation.as_ref()?;
        let rect = self.ws.layout.page(shown.page)?;
        let (a, b) = (self.ws.view.doc_to_screen([rect.x, rect.y]), self.ws.view.doc_to_screen([rect.x + rect.w, rect.y + rect.h]));
        Some(PresentationView {
            page: shown.page,
            pages: self.ws.layout.len(),
            hint: shown.hint > 0.0,
            page_rect: [a[0], a[1], b[0], b[1]].map(f32::to_bits),
        })
    }

    pub fn presentation_layout(&self) -> Option<PresentationLayout> {
        self.presentation.as_ref().map(|_| PresentationLayout::new(self.ws.view.viewport, self.ws.dpr))
    }

    /// A pen or touch event while the slides are shown.
    pub(in crate::editor) fn presentation_event(&mut self, ev: &PointerEvent) {
        if ev.device == Device::Touch && !self.palm.admit_touch(ev, super::touch::PALM_PX * self.ws.dpr) {
            return;
        }
        let sample = if ev.phase == Phase::Down { ev.samples.first() } else { ev.samples.last() };
        if let Some(s) = sample {
            self.presentation_pointer(ev.device, ev.id, ev.phase, s.pos);
        }
    }

    /// One pointer's position and phase. A press that goes nowhere is a tap: on the button to leave if that was showing when it began, else on the left
    /// of the window (back) or elsewhere (on); a finger that goes far across is a swipe.
    pub(in crate::editor) fn presentation_pointer(&mut self, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        let (viewport, dpr) = (self.ws.view.viewport, self.ws.dpr);
        let layout = PresentationLayout::new(viewport, dpr);
        // (Whether the button was showing when the press began: this very press shows it, and is not on it for that.)
        let showing = self.presentation.as_ref().is_some_and(|p| p.hint > 0.0);
        self.hint();
        let Some(shown) = &mut self.presentation else { return };
        match phase {
            Phase::Down => {
                if shown.press.is_none() {
                    shown.press = Some(Press { dev, id, start: pos, exit_showing: showing });
                }
            }
            Phase::Up => {
                let Some(press) = shown.press.take().filter(|p| p.dev == dev && p.id == id) else { return };
                let (dx, dy) = (pos[0] - press.start[0], pos[1] - press.start[1]);
                let far = dx.hypot(dy);
                if far <= TAP_PX * dpr {
                    if press.exit_showing && layout.on_exit(press.start) && layout.on_exit(pos) {
                        self.stop_presentation();
                    } else {
                        self.turn(press.start[0] >= LEFT_ZONE * viewport[0]);
                    }
                } else if dev == Device::Touch && dx.abs() >= SWIPE_PX * dpr && dx.abs() > 1.5 * dy.abs() {
                    self.turn(dx < 0.0);
                }
            }
            Phase::Cancel | Phase::Leave => {
                if shown.press.is_some_and(|p| p.dev == dev && p.id == id) || phase == Phase::Leave {
                    shown.press = None;
                }
            }
            Phase::Move | Phase::Hover => {}
        }
        self.ws.redraw = true;
    }

    /// The mouse left the window, or the window lost the keyboard: a press under way is given up.
    pub(in crate::editor) fn presentation_gestures_lost(&mut self) {
        if let Some(shown) = &mut self.presentation {
            shown.press = None;
        }
    }

    /// A key while the slides are shown. `true` if it was used.
    pub(in crate::editor) fn presentation_key(&mut self, key: Key<'_>) -> bool {
        self.hint();
        let last = self.ws.layout.len().saturating_sub(1);
        match key {
            Key::Escape => self.stop_presentation(),
            Key::Page(true) | Key::Arrow(Dir::Right | Dir::Down, _) | Key::Text(" " | "\n") | Key::Trail(true) => self.turn(true),
            Key::Page(false) | Key::Arrow(Dir::Left | Dir::Up, _) | Key::Backspace | Key::Trail(false) => self.turn(false),
            Key::Home(_) => self.show_slide(0),
            Key::End(_) => self.show_slide(last),
            _ => return false,
        }
        self.ws.redraw = true;
        true
    }

    /// The wheel turns the pages: a notch a page, and a touchpad's scrolling a page for each so many pixels.
    pub(in crate::editor) fn presentation_wheel(&mut self, delta: Wheel) {
        self.hint();
        match delta {
            Wheel::Lines(_, y) if y.is_finite() && y.abs() >= 0.5 => self.turn(y > 0.0),
            Wheel::Pixels(_, y) if y.is_finite() => {
                let Some(shown) = &mut self.presentation else { return };
                shown.wheel += y;
                let turns = (shown.wheel / WHEEL_PAGE_PX).trunc();
                shown.wheel -= turns * WHEEL_PAGE_PX;
                for _ in 0..(turns.abs() as usize).min(5) {
                    self.turn(turns > 0.0);
                }
            }
            _ => {}
        }
    }
}
