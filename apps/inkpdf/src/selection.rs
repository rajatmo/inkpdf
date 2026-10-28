//! What every tool can do to the items it made, with whatever device is at hand: select them, move them, delete,
//! duplicate, copy and paste them, and change their colour and thickness. The devices differ (a key, a button of
//! the toolbar, a loop of the pen), the operations do not, so they live here once, on the workspace, and each is
//! one undo step.
//!
//! A tool works on its own kind of item: the pen on handwriting, the shape tool on shapes, the text tool on text
//! boxes (see [`Kind`]). Selecting, erasing, pasting and selecting all leave the others alone, so a loop drawn
//! round a shape with the pen does not pick it up.

use crate::ink::{P, Replacement, Restyle, Stroke, Style, TextBlock};
use crate::plane::clamp_into;
use crate::text;
use crate::tool::{Kind, MAX_WIDTH, MIN_WIDTH, TEXT_SIZES};
use crate::workspace::Workspace;

/// How far, in points, a duplicate or a repeated paste lands from what it copies, so that it can be seen.
const COPY_OFFSET: f32 = 14.0;

/// How far the arrow keys nudge the selection, in points; with Shift, the larger.
pub const NUDGE: f32 = 2.0;
pub const NUDGE_BIG: f32 = 20.0;

impl Workspace {
    /// The kind of item the tool in use works on.
    pub fn kind(&self) -> Option<Kind> {
        self.tool.kind()
    }

    /// Whether the tool in use works on `stroke`.
    pub fn is_own(&self, stroke: &Stroke) -> bool {
        self.kind().is_some_and(|k| Kind::of(stroke) == k)
    }

    /// `ids` without the items the tool in use does not work on.
    pub fn keep_own(&self, ids: Vec<u64>) -> Vec<u64> {
        ids.into_iter().filter(|&id| self.ink.get(id).is_some_and(|s| self.is_own(s))).collect()
    }

    /// The topmost of the tool's own items within `radius` of `p`.
    pub fn hit_own(&self, p: P, radius: f32) -> Option<u64> {
        self.ink.strokes().iter().rev().find(|s| self.is_own(s) && s.touches_circle(p, radius)).map(|s| s.id)
    }

    /// The page being looked at.
    pub fn current_page(&self) -> u32 {
        self.view.current_page(&self.layout) as u32
    }

    /// Select every item of the tool's kind on the page being looked at.
    pub fn select_all(&mut self) {
        let page = self.current_page();
        self.selection = self.ink.strokes().iter().filter(|s| s.page == page && self.is_own(s)).map(|s| s.id).collect();
        self.redraw = true;
    }

    /// Take the selection away from the document. `false` if nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        let ids = std::mem::take(&mut self.selection);
        self.redraw = true;
        self.ink.remove(&ids) > 0
    }

    /// Put the selection on the clipboard (the items themselves, whatever the tool: it pastes its own kind). The
    /// words of text boxes go on the system clipboard too, for other programs.
    pub fn copy_selection(&mut self) -> bool {
        let copied: Vec<Stroke> = self.selection.iter().filter_map(|&id| self.ink.get(id)).cloned().collect();
        if copied.is_empty() {
            return false;
        }
        let words: Vec<&str> = copied.iter().filter_map(|s| s.text.as_ref()).map(|t| t.text.as_str()).collect();
        self.clipboard_echo = (!words.is_empty()).then(|| words.join("\n"));
        if let Some(echo) = &self.clipboard_echo {
            self.text_clipboard.set(echo);
        }
        self.clipboard = copied;
        self.pasted = 0;
        true
    }

    /// Copy the selection, then delete it.
    pub fn cut_selection(&mut self) -> bool {
        self.copy_selection() && self.delete_selection()
    }

    /// Whether there may be something to paste: items of the tool's kind on the clipboard, or, for the text tool,
    /// words (which might be on the system clipboard from anywhere; it is not asked every time the bar is drawn).
    pub fn can_paste(&self) -> bool {
        self.kind() == Some(Kind::Text) || self.clipboard.iter().any(|s| self.is_own(s))
    }

    /// Paste into the middle of what is in view, and select what was pasted: the tool's own items from the
    /// clipboard (each paste a little further along than the last, so copies can be told apart) or, with the text
    /// tool and words from some other program on the system clipboard, a new text box with them.
    pub fn paste(&mut self) -> bool {
        if self.kind() == Some(Kind::Text) {
            let from_elsewhere = self.text_clipboard.get().filter(|t| !t.trim().is_empty() && self.clipboard_echo.as_ref() != Some(t));
            if let Some(words) = from_elsewhere {
                return self.paste_words(&words);
            }
        }
        let clip: Vec<Stroke> = self.clipboard.iter().filter(|s| self.is_own(s)).cloned().collect();
        let Some(b) = union(clip.iter().map(Stroke::bbox)) else { return false };
        let page = self.current_page();
        let Some(rect) = self.page_rect(page) else { return false };
        let shown = self.to_doc([self.view.viewport[0] / 2.0, self.view.viewport[1] / 2.0]);
        let step = COPY_OFFSET * (self.pasted % 6) as f32;
        let at = clamp_into(rect, [shown[0] + step, shown[1] + step]);
        let (dx, dy) = keep_inside(rect, b, at[0] - (b[0] + b[2]) / 2.0, at[1] - (b[1] + b[3]) / 2.0);
        self.pasted += 1;
        self.place(clip, dx, dy, page)
    }

    /// A new text box with `words` in the middle of what is in view, in the text tool's colour and size, selected.
    fn paste_words(&mut self, words: &str) -> bool {
        let page = self.current_page();
        let Some(rect) = self.page_rect(page) else { return false };
        let (rgba, size) = self.current_style();
        let shown = self.to_doc([self.view.viewport[0] / 2.0, self.view.viewport[1] / 2.0]);
        let step = COPY_OFFSET * (self.pasted % 6) as f32;
        self.pasted += 1;
        let at = clamp_into(rect, [shown[0] - 0.15 * (rect[2] - rect[0]) + step, shown[1] + step]);
        let (wrapped, outline) = text::fit_box(Some(rect), at, &text::clean(words), size);
        if wrapped.trim().is_empty() {
            return false;
        }
        let id = self.ink.add_text(page, Style { rgba, width: 0.0 }, TextBlock { text: wrapped, size }, outline);
        self.redraw = true;
        self.selection = id.into_iter().collect();
        id.is_some()
    }

    /// Copy the selection a little way over, and select the copies.
    pub fn duplicate_selection(&mut self) -> bool {
        let copies: Vec<Stroke> = self.selection.iter().filter_map(|&id| self.ink.get(id)).cloned().collect();
        let Some(b) = union(copies.iter().map(Stroke::bbox)) else { return false };
        let page = self.page_hit([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]).unwrap_or_else(|| self.current_page());
        // Towards the page's far corner, unless the copy would leave the page that way.
        let rect = self.page_rect(page);
        let away = |hi: f32, limit: f32| if hi + COPY_OFFSET > limit { -COPY_OFFSET } else { COPY_OFFSET };
        let (dx, dy) = rect.map_or((COPY_OFFSET, COPY_OFFSET), |r| keep_inside(r, b, away(b[2], r[2]), away(b[3], r[3])));
        self.place(copies, dx, dy, page)
    }

    /// Add `strokes` moved by `(dx, dy)`, each on the page it lands on (or `page`), and select them.
    fn place(&mut self, strokes: Vec<Stroke>, dx: f32, dy: f32, page: u32) -> bool {
        let moved: Vec<Stroke> = strokes
            .iter()
            .map(|s| {
                let b = s.bbox();
                let at = self.page_hit([(b[0] + b[2]) / 2.0 + dx, (b[1] + b[3]) / 2.0 + dy]).unwrap_or(page);
                s.moved(dx, dy, at)
            })
            .collect();
        let ids = self.ink.add_copies(moved);
        self.redraw = true;
        let placed = !ids.is_empty();
        if placed {
            self.selection = ids;
        }
        placed
    }

    /// Move the selection by `(dx, dy)` points. What lies wholly on its page stays on it (a drag is how to take
    /// something off a page, or onto another).
    pub fn nudge_selection(&mut self, dx: f32, dy: f32) -> bool {
        let ids = self.selection.clone();
        let Some(b) = self.ink.bounds_of(&ids) else { return false };
        let page_of_it = self.page_hit([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]).and_then(|p| self.page_rect(p));
        let (dx, dy) = match page_of_it {
            Some(r) if b[0] >= r[0] && b[1] >= r[1] && b[2] <= r[2] && b[3] <= r[3] => keep_inside(r, b, dx, dy),
            _ => (dx, dy),
        };
        let to_page = self.page_hit([(b[0] + b[2]) / 2.0 + dx, (b[1] + b[3]) / 2.0 + dy]);
        self.redraw = true;
        self.ink.move_by(&ids, dx, dy, to_page) > 0
    }

    /// Give the selection a new colour or thickness (a text box's size), as one undo step. With `coalesce`, a run
    /// of these (a slider being dragged) is one step.
    pub fn restyle_selection(&mut self, change: Restyle, coalesce: bool) -> bool {
        let items: Vec<Replacement> = self.selection.iter().filter_map(|&id| self.ink.get(id)).map(|s| self.restyled(s, change)).collect();
        let changed = self.ink.replace_many(items, coalesce);
        self.redraw |= changed;
        changed
    }

    fn restyled(&self, s: &Stroke, change: Restyle) -> Replacement {
        let mut style = s.style;
        if let Some(rgba) = change.rgba {
            style.rgba = rgba;
        }
        match &s.text {
            Some(block) => {
                let size = change.width.map_or(block.size, |w| w.clamp(TEXT_SIZES.0, TEXT_SIZES.1));
                let top_left = s.pts.first().copied().unwrap_or([0.0; 2]);
                let (text, outline) = text::fit_box(self.page_rect(s.page), top_left, &block.text, size);
                Replacement { id: s.id, style, pts: outline, figure: None, text: Some(TextBlock { text, size }) }
            }
            None => {
                if let Some(w) = change.width {
                    style.width = w.clamp(MIN_WIDTH, MAX_WIDTH);
                }
                Replacement { id: s.id, style, pts: s.pts.clone(), figure: s.figure.clone(), text: None }
            }
        }
    }
}

/// The box that holds all of `boxes`.
fn union(boxes: impl Iterator<Item = [f32; 4]>) -> Option<[f32; 4]> {
    boxes.reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

/// `(dx, dy)` changed as little as it takes for `b` moved by it to lie inside `rect` (as far as it fits).
fn keep_inside(rect: [f32; 4], b: [f32; 4], dx: f32, dy: f32) -> (f32, f32) {
    let fit = |lo: f32, hi: f32, from: f32, to: f32, d: f32| {
        let d = d - (to + d - hi).max(0.0);
        d + (lo - (from + d)).max(0.0)
    };
    (fit(rect[0], rect[2], b[0], b[2], dx), fit(rect[1], rect[3], b[1], b[3], dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_is_kept_inside_the_page_as_far_as_it_fits() {
        let page = [0.0, 0.0, 100.0, 100.0];
        let item = [10.0, 10.0, 30.0, 30.0];
        assert_eq!(keep_inside(page, item, 5.0, 5.0), (5.0, 5.0));
        assert_eq!(keep_inside(page, item, 200.0, -200.0), (70.0, -10.0));
        // Larger than the page: the left edge wins.
        assert_eq!(keep_inside([0.0, 0.0, 10.0, 10.0], item, 0.0, 0.0), (-10.0, -10.0));
    }
}
