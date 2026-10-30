//! The keys and the wheel of a reader: what a person at a desk expects of any PDF program, with every tool (none of them is a key a text box
//! being typed into could do without: they are only heard when nothing takes the key). Ctrl and the wheel zoom about the pointer; Ctrl and
//! `+`, `-`, `0` and `1` zoom, fit the width and show the page at its size; Page Up and Page Down (and the space bar) scroll a screen;
//! Home and End go to the top and the bottom of the document; the arrows scroll a little, when nothing is selected for them to nudge.

use crate::input::{Dir, Key, Mode};

use super::Editor;

/// How much one notch of the wheel with Ctrl zooms: a tenth.
const ZOOM_PER_NOTCH: f32 = 1.1;
/// How far an arrow key scrolls, in logical pixels, and how much of the window a page key does.
const ARROW_PX: f32 = 60.0;
const PAGE_FRACTION: f32 = 0.9;

impl Editor {
    /// The wheel turned with Ctrl held: zoom about the pointer, by `steps` notches (positive is in).
    pub(super) fn wheel_zoom(&mut self, steps: f32) {
        if self.modal().is_some() || !steps.is_finite() {
            return;
        }
        self.zoom_by(ZOOM_PER_NOTCH.powf(steps), self.cursor);
    }

    /// Zoom by `factor` about the screen point `about`; the page stops following the width of the window until it is asked to.
    pub(super) fn zoom_by(&mut self, factor: f32, about: [f32; 2]) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        self.ws.fit_mode = false;
        let zoom = self.ws.view.zoom * factor;
        self.ws.zoom_about(zoom, about);
        self.ws.redraw = true;
    }

    /// The middle of the window, which the keys zoom about.
    fn window_centre(&self) -> [f32; 2] {
        [self.ws.view.viewport[0] / 2.0, self.ws.view.viewport[1] / 2.0]
    }

    /// Ctrl and a zoom key (`letter` is what the key types): `true` if it is one.
    pub(super) fn zoom_shortcut(&mut self, letter: &str) -> bool {
        match letter {
            "+" | "=" => self.zoom_by(ZOOM_PER_NOTCH, self.window_centre()),
            "-" | "_" => self.zoom_by(1.0 / ZOOM_PER_NOTCH, self.window_centre()),
            // The width of the window, as the page opens.
            "0" => {
                self.ws.fit_mode = true;
                self.ws.fit_width();
                self.ws.redraw = true;
            }
            // The page at its size: a point is a pixel at the window's scale.
            "1" => {
                self.ws.fit_mode = false;
                let (dpr, centre) = (self.ws.dpr, self.window_centre());
                self.ws.zoom_about(dpr, centre);
                self.ws.redraw = true;
            }
            _ => return false,
        }
        true
    }

    /// A key that no tool wanted, in the desktop mode, with nothing being typed: scroll.
    pub(super) fn reader_key(&mut self, key: Key<'_>) -> bool {
        if self.mode != Mode::Desktop || self.is_typing() {
            return false;
        }
        let (dpr, height) = (self.ws.dpr, self.ws.view.viewport[1]);
        let by = |ed: &mut Editor, dx: f32, dy: f32| {
            ed.ws.scroll_by_px(dx, dy);
            ed.ws.redraw = true;
        };
        match key {
            Key::Page(down) => by(self, 0.0, if down { -PAGE_FRACTION * height } else { PAGE_FRACTION * height }),
            Key::Arrow(Dir::Up, _) => by(self, 0.0, ARROW_PX * dpr),
            Key::Arrow(Dir::Down, _) => by(self, 0.0, -ARROW_PX * dpr),
            Key::Arrow(Dir::Left, _) => by(self, ARROW_PX * dpr, 0.0),
            Key::Arrow(Dir::Right, _) => by(self, -ARROW_PX * dpr, 0.0),
            // (A very long way: the view stops at the end.)
            Key::Home(_) => by(self, 0.0, 1.0e7),
            Key::End(_) => by(self, 0.0, -1.0e7),
            _ => return false,
        }
        true
    }
}
