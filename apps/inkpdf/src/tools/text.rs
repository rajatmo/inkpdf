//! The text tool: the mouse's and the keyboard's (the stylus does nothing to the page with it). A click of
//! the mouse on empty page starts a new text box there; a click on a text box selects it and edits it; a
//! drag that starts on a text box moves it. The keyboard types into the box being edited (and is heard by
//! nothing else): characters, Enter for a new line, Backspace. The caret is always at the end of the text.
//!
//! What is typed lives in [`TextEdit`] until the edit ends: another click, a change of tool, saving,
//! closing, a new page. Then it becomes one undo step: a new text box, or the old one changed (or removed,
//! if it was emptied). Undo while typing throws the typing away.

use crate::ink::{P, Style, TextBlock};
use crate::input::{Device, Mode, Phase};
use crate::text;
use crate::workspace::Workspace;

use super::gestures::Mover;
use super::paint::{ACCENT, Painter, polyline};
use super::{InputPolicy, Key, MouseUse, PageEvent, TAP_SELECT_PX, Tool};

/// A text box being typed into.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    /// The stroke being edited; `None` for a new text box.
    pub target: Option<u64>,
    pub page: u32,
    /// The box's top-left corner, in document space.
    pub top_left: P,
    pub text: String,
    /// Points, and colour.
    pub size: f32,
    pub rgba: [u8; 4],
}

impl TextEdit {
    /// The box as it is now: its rectangle (see [`text::outline`]).
    pub fn outline(&self) -> Vec<P> {
        text::outline(self.top_left, &self.text, self.size)
    }
}

/// A press of the mouse, until it is let go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Press {
    #[default]
    None,
    /// On empty page: a text box starts there when the button is let go.
    Place { dev: Device, id: u32, page: u32, at: P },
    /// On a text box or a selection: a drag moves it, a click edits it.
    Grab(Mover),
}

#[derive(Default)]
pub struct TextTool {
    edit: Option<TextEdit>,
    press: Press,
}

impl TextTool {
    /// The text box being typed into, for drawing it.
    pub fn edit(&self) -> Option<&TextEdit> {
        self.edit.as_ref()
    }

    pub fn is_typing(&self) -> bool {
        self.edit.is_some()
    }

    /// Type `typed` at the end of the text (see [`text::clean`]).
    pub fn type_text(&mut self, ws: &mut Workspace, typed: &str) {
        if let Some(edit) = &mut self.edit {
            let cleaned = text::clean(typed);
            let room = text::MAX_CHARS.saturating_sub(edit.text.chars().count());
            edit.text.extend(cleaned.chars().take(room));
            ws.redraw = true;
        }
        self.fit_width(ws);
    }

    /// Take the last character away (a new line goes whole).
    pub fn backspace(&mut self, ws: &mut Workspace) {
        if let Some(edit) = &mut self.edit {
            edit.text.pop();
            ws.redraw = true;
        }
    }

    /// Keep the box being typed into within its page's right edge: lines that would run off it are broken
    /// (see [`text::wrap`]), and a box too near the edge for that to leave a readable width is moved left.
    fn fit_width(&mut self, ws: &Workspace) {
        let Some(edit) = &mut self.edit else { return };
        let Some(rect) = ws.page_rect(edit.page) else { return };
        let (size, page_width) = (edit.size, rect[2] - rect[0]);
        let least = (0.35 * page_width).min(14.0 * size);
        let left = edit.top_left[0].min(rect[2] - least).max(rect[0]);
        edit.top_left[0] = left;
        edit.text = text::wrap(&edit.text, size, rect[2] - left);
    }

    /// The mouse pressed at `p` (any press has already ended the typing). On a text box it selects it, and
    /// a drag moves it (a click without a drag edits it); a selection made with the lasso moves as it does
    /// with any tool; on empty page, a click starts a new box there.
    fn press_at(&mut self, ws: &mut Workspace, dev: Device, id: u32, p: P) {
        let grab = |start| Press::Grab(Mover::begin(dev, id, start));
        self.press = if ws.selection.len() > 1 && ws.selection_hit(p) {
            grab(p)
        } else if let Some(stroke) = self.text_at(ws, p) {
            ws.selection = vec![stroke];
            grab(p)
        } else if ws.selection_hit(p) {
            grab(p)
        } else {
            ws.selection.clear();
            match ws.page_hit(p) {
                Some(page) => Press::Place { dev, id, page, at: p },
                None => Press::None,
            }
        };
    }

    /// The topmost text box with `p` in it, or within a finger's width of it (so a click on the very corner
    /// a box was placed at, or a hair outside it, still finds it).
    fn text_at(&self, ws: &Workspace, p: P) -> Option<u64> {
        let slack = TAP_SELECT_PX * ws.dpr / ws.view.zoom;
        ws.ink.strokes().iter().rev().find(|s| s.text.is_some() && s.touches_circle(p, slack)).map(|s| s.id)
    }

    /// Throw away what was typed (a box begun, or changes to one): `true` if there was any.
    fn discard_typing(&mut self, ws: &mut Workspace) -> bool {
        let Some(edit) = self.edit.take() else { return false };
        ws.redraw = true;
        let before = edit.target.and_then(|id| ws.ink.get(id)).and_then(|s| s.text.as_ref()).map_or("", |t| t.text.as_str());
        edit.text != before
    }

    /// Start typing into the text box `id`.
    fn begin_edit(&mut self, ws: &mut Workspace, id: u64) {
        let Some(s) = ws.ink.get(id) else { return };
        let Some(block) = &s.text else { return };
        let top_left = s.pts.first().copied().unwrap_or([0.0; 2]);
        self.edit = Some(TextEdit { target: Some(id), page: s.page, top_left, text: block.text.clone(), size: block.size, rgba: s.style.rgba });
        ws.redraw = true;
    }

    /// Start a new text box with its top-left corner at `at` on `page`, kept on the page.
    fn begin_text(&mut self, ws: &mut Workspace, page: u32, at: P) {
        let Some(rect) = ws.page_rect(page) else { return };
        if !at.iter().all(|c| c.is_finite()) {
            return;
        }
        let top_left = crate::geometry::clamp_into(rect, at);
        let (rgba, size) = ws.current_style();
        self.edit = Some(TextEdit { target: None, page, top_left, text: String::new(), size, rgba });
        self.fit_width(ws);
        ws.redraw = true;
    }

    /// End the edit: the typing becomes an undo step of its own.
    pub fn finish_text(&mut self, ws: &mut Workspace) {
        let Some(edit) = self.edit.take() else { return };
        ws.redraw = true;
        let style = Style { rgba: edit.rgba, width: 0.0 };
        let empty = edit.text.trim().is_empty();
        let outline = keep_on_page(ws, edit.page, edit.outline());
        let block = TextBlock { text: edit.text, size: edit.size };
        match edit.target {
            Some(id) if empty => {
                ws.ink.remove(&[id]);
                ws.selection.retain(|&s| s != id);
            }
            Some(id) => {
                ws.ink.edit_text(id, style, block, outline);
            }
            None if empty => {}
            None => {
                if let Some(id) = ws.ink.add_text(edit.page, style, block, outline) {
                    ws.selection = vec![id];
                }
            }
        }
    }
}

/// `outline` (a text box's rectangle) moved just far enough to lie inside `page`.
fn keep_on_page(ws: &Workspace, page: u32, mut outline: Vec<P>) -> Vec<P> {
    let (Some(rect), Some(top_left), Some(bottom_right)) = (ws.page_rect(page), outline.first().copied(), outline.get(2).copied()) else {
        return outline;
    };
    let dx = (rect[2] - bottom_right[0]).min(0.0).max(rect[0] - top_left[0]);
    let dy = (rect[3] - bottom_right[1]).min(0.0).max(rect[1] - top_left[1]);
    for p in &mut outline {
        *p = [p[0] + dx, p[1] + dy];
    }
    outline
}

impl Tool for TextTool {
    /// At a desk the mouse places, selects and moves text and the stylus does nothing to the page; as a tablet
    /// the stylus does what the mouse does (and the mouse only scrolls). The keyboard types in both: the
    /// physical one, or Windows' touch keyboard.
    fn policy(&self, mode: Mode) -> InputPolicy {
        match mode {
            Mode::Desktop => InputPolicy { stylus: false, mouse: MouseUse::Itself, keyboard: true },
            Mode::Tablet => InputPolicy { stylus: true, mouse: MouseUse::Scrolls, keyboard: true },
        }
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        match ev.phase {
            Phase::Down => {
                self.finish_text(ws);
                if let Some(first) = ev.samples.first() {
                    let p = ws.to_doc(first.pos);
                    self.press_at(ws, ev.dev, ev.id, p);
                }
            }
            Phase::Move if self.owns(ev.dev, ev.id) => {
                if let (Press::Grab(m), Some(s)) = (&mut self.press, ev.samples.last()) {
                    m.drag(ws, s.pos);
                }
            }
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                if let (Press::Grab(m), Some(s)) = (&mut self.press, ev.samples.last()) {
                    m.drag(ws, s.pos);
                }
                self.end(ws);
            }
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        match self.press {
            Press::None => false,
            Press::Place { dev: d, id: i, .. } => d == dev && i == id,
            Press::Grab(m) => m.dev == dev && m.id == id,
        }
    }

    fn busy(&self) -> bool {
        self.press != Press::None
    }

    fn end(&mut self, ws: &mut Workspace) {
        ws.redraw = true;
        match std::mem::take(&mut self.press) {
            Press::None => {}
            Press::Place { page, at, .. } => self.begin_text(ws, page, at),
            Press::Grab(m) => {
                // A click, not a drag, on a text box edits it.
                if !m.finish(ws)
                    && let [id] = ws.selection[..]
                {
                    self.begin_edit(ws, id);
                }
            }
        }
    }

    /// Typing becomes part of the document; a press under way is left to finish by itself.
    fn settle(&mut self, ws: &mut Workspace) {
        self.finish_text(ws);
    }

    fn cancel(&mut self, _ws: &mut Workspace) {
        self.press = Press::None;
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        if self.edit.is_none() {
            return false;
        }
        match key {
            Key::Text(typed) => self.type_text(ws, typed),
            Key::Backspace => self.backspace(ws),
            Key::Escape => self.finish_text(ws),
        }
        true
    }

    fn undo(&mut self, ws: &mut Workspace) -> bool {
        self.discard_typing(ws)
    }

    fn redo(&mut self, ws: &mut Workspace) -> bool {
        self.discard_typing(ws)
    }

    fn restyled(&mut self, ws: &mut Workspace) {
        let (rgba, size) = ws.current_style();
        if let Some(edit) = &mut self.edit {
            edit.rgba = rgba;
            edit.size = size;
        }
        self.fit_width(ws);
    }

    fn drag_offset(&self) -> [f32; 2] {
        match self.press {
            Press::Grab(m) => m.offset(),
            _ => [0.0, 0.0],
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let Some(edit) = &self.edit else { return };
        let px = ws.dpr / ws.view.zoom;
        // The box being typed into, and a caret after its last character.
        polyline(out, &edit.outline(), 1.5 * px, ACCENT);
        let caret = text::caret(&edit.text, edit.size);
        let x = edit.top_left[0] + caret[0] + px;
        let top = edit.top_left[1] + caret[1];
        out.doc_line([x, top], [x, top + edit.size * text::LINE_HEIGHT], 1.4 * px, edit.rgba);
    }
}
