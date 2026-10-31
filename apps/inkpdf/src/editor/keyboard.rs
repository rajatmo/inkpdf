//! The on-screen keyboard of a phone or tablet: it covers the lowest part of the window without the window knowing, so what is being typed (a text box, a
//! field of a form) can be under it. The shell says how much of the window it covers; while something is typed the view goes up just far enough to bring it
//! above the keyboard (the end of the document too: the scroll may go past it by the room the keyboard takes), and comes back to where it was when the
//! keyboard goes. It moves only when what is typed moves or the keyboard comes or goes, so a person who scrolls with the keyboard up is not pulled back.

use super::Editor;

#[derive(Default)]
pub struct Keyboard {
    /// How much of the window, in pixels from its bottom, the keyboard covers.
    covered: f32,
    /// What the view was last made clear for: the box typed in, and how much was covered then.
    stamp: Option<([u32; 4], u32)>,
    /// Where the view was heading before it was moved to clear what is typed, and where it was moved to.
    before: Option<(f32, f32)>,
}

impl Editor {
    /// The shell says how much of the window the keyboard covers now (0 when there is none).
    pub fn set_keyboard_covered(&mut self, px: f32) {
        let px = if px.is_finite() { px.clamp(0.0, self.ws.view.viewport[1]) } else { 0.0 };
        if px == self.keyboard.covered {
            return;
        }
        self.keyboard.covered = px;
        self.ws.view.set_bottom_room(&self.ws.layout, px);
        self.ws.redraw = true;
    }

    /// How much of the window the keyboard covers.
    pub fn keyboard_covered(&self) -> f32 {
        self.keyboard.covered
    }

    /// The box being typed into (a text box, a field of a form), in document space.
    fn typed_rect(&self) -> Option<[f32; 4]> {
        if let Some(edit) = self.text_edit() {
            let pts = edit.outline();
            let fold = |f: fn(f32, f32) -> f32, i: usize, start: f32| pts.iter().map(|p| p[i]).fold(start, f);
            return (!pts.is_empty())
                .then(|| [fold(f32::min, 0, f32::MAX), fold(f32::min, 1, f32::MAX), fold(f32::max, 0, f32::MIN), fold(f32::max, 1, f32::MIN)]);
        }
        self.typing_rect()
    }

    /// Keep what is typed above the keyboard, and put the view back when the keyboard goes (called every frame: it does something only when
    /// what it depends on has changed).
    pub(super) fn keep_typing_clear(&mut self) {
        let covered = self.keyboard.covered;
        let target = if covered > 0.0 { self.typed_rect() } else { None };
        let Some(rect) = target else {
            self.keyboard.stamp = None;
            if covered == 0.0
                && let Some((before, set)) = self.keyboard.before.take()
                // (Not if the person scrolled meanwhile: then it is theirs.)
                && (self.ws.view.scroll_target_y() - set).abs() < 1.0
            {
                self.ws.view.glide_to_y(&self.ws.layout, before);
                self.ws.redraw = true;
            }
            return;
        };
        let stamp = (rect.map(f32::to_bits), covered.to_bits());
        if self.keyboard.stamp == Some(stamp) {
            return;
        }
        self.keyboard.stamp = Some(stamp);
        let bars = self.context_bar().map_or(self.toolbar().bottom(), |c| c.rect[3]) + 8.0 * self.ws.dpr;
        let was = self.ws.view.scroll_target_y();
        self.ws.view.scroll_to_clear(&self.ws.layout, rect, covered, bars);
        let now = self.ws.view.scroll_target_y();
        if now != was {
            let first = self.keyboard.before.map_or(was, |(before, _)| before);
            self.keyboard.before = Some((first, now));
            self.ws.redraw = true;
        }
    }
}
