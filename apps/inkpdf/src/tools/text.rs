//! The text tool: the mouse's and the keyboard's at a desk, the stylus's and the keyboard's (the physical one or
//! Windows' touch keyboard) as a tablet. A click on empty page starts a new text box there; a click on a text box
//! selects it and edits it; a drag that starts on a text box moves it. The keyboard types into the box being
//! edited: characters, Enter for a new line, Backspace and Delete, the arrows, Home and End (with Shift they
//! select), and Ctrl+A, C, X and V with the system clipboard. A press inside the box being edited puts the
//! caret there, and a drag across it selects.
//!
//! What is typed lives in [`TextEdit`] until the edit ends: a press outside the box, a change of tool, saving,
//! closing, a new page. Then it becomes one undo step: a new text box, or the old one changed (or removed,
//! if it was emptied). Undo while typing throws the typing away.

use crate::field::{Edited, Field, Rules};
use crate::ink::{P, Style, TextBlock};
use crate::input::{Device, Mode, Phase, Role, Sample};
use crate::text;
use crate::workspace::Workspace;

use super::gestures::{Mover, Pick};
use super::paint::{Painter, polyline};
use super::shortcuts::selection_key;
use super::{Dir, InputPolicy, Key, MouseUse, PageEvent, TAP_SELECT_PX, Tool};

/// A text box being typed into: a field (its text, caret and selection: see [`crate::field`]) with a place on a page. It reads as its field.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    /// The stroke being edited; `None` for a new text box.
    pub target: Option<u64>,
    pub page: u32,
    /// The box's top-left corner, in document space.
    pub top_left: P,
    field: Field,
    /// Points, and colour.
    pub size: f32,
    pub rgba: [u8; 4],
    /// How far along its line, in points, the caret was before it was moved up or down, so that it comes back to
    /// the same place after a short line.
    goal_x: Option<f32>,
}

impl std::ops::Deref for TextEdit {
    type Target = Field;

    fn deref(&self) -> &Field {
        &self.field
    }
}

impl std::ops::DerefMut for TextEdit {
    fn deref_mut(&mut self) -> &mut Field {
        &mut self.field
    }
}

impl TextEdit {
    fn new(target: Option<u64>, page: u32, top_left: P, text: String, size: f32, rgba: [u8; 4]) -> Self {
        Self { target, page, top_left, field: Field::new(text), size, rgba, goal_x: None }
    }

    /// The box as it is now: its rectangle (see [`text::outline`]).
    pub fn outline(&self) -> Vec<P> {
        text::outline(self.top_left, &self.text, self.size)
    }
}

/// What a text box takes: a page of text, with lines.
const RULES: Rules = Rules { max_chars: text::MAX_CHARS, multiline: true };
/// A press of the mouse, until it is let go.
#[derive(Clone, Debug, Default, PartialEq)]
enum Press {
    #[default]
    None,
    /// On empty page: a text box starts there when the button is let go.
    Place { dev: Device, id: u32, page: u32, at: P },
    /// On a text box or a selection: a drag moves it, a click edits it.
    Grab(Mover),
    /// Erasing or selecting was chosen (or the eraser end or barrel button is used): see [`Pick`].
    Pick(Pick),
    /// In the box being edited: the caret went where it was pressed (character `from`), and a drag selects from there.
    Caret { dev: Device, id: u32, from: usize },
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

    /// The keyboard on the text box being typed into: the field's own keys (see [`Field::key`]), and the arrows that go up and down a line.
    fn edit_key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        let Some(edit) = &mut self.edit else { return false };
        let edited = match key {
            Key::Arrow(dir @ (Dir::Up | Dir::Down), extend) => {
                // The caret goes to the same place along the line above or below, also after a short line.
                let goal = edit.goal_x.unwrap_or_else(|| text::caret_at(&edit.text, edit.size, edit.caret)[0]);
                let to = text::vertical(&edit.text, edit.size, edit.caret, dir == Dir::Down, goal);
                edit.move_to(to, extend);
                edit.goal_x = Some(goal);
                Edited::Moved
            }
            _ => {
                let edited = edit.field.key(key, &mut *ws.clip.text, RULES);
                edit.goal_x = None;
                if matches!(key, Key::Chord('c' | 'x')) {
                    ws.clip.echo = None;
                }
                edited
            }
        };
        ws.redraw = true;
        if edited == Edited::Changed {
            self.fit_width(ws);
        }
        edited != Edited::Unused
    }
    /// Keep the box being typed into within its page's right edge: lines that would run off it are broken
    /// (see [`text::wrap`]), and a box too near the edge for that to leave a readable width is moved left. The
    /// caret and the selection stay among the same characters.
    fn fit_width(&mut self, ws: &Workspace) {
        let Some(edit) = &mut self.edit else { return };
        let Some(rect) = ws.page_rect(edit.page) else { return };
        let (size, page_width) = (edit.size, rect[2] - rect[0]);
        let least = (0.35 * page_width).min(14.0 * size);
        let left = edit.top_left[0].min(rect[2] - least).max(rect[0]);
        edit.top_left[0] = left;
        let wrapped = text::wrap(&edit.text, size, rect[2] - left);
        edit.caret = text::follow_wrap(&edit.text, &wrapped, edit.caret);
        edit.anchor = edit.anchor.map(|a| text::follow_wrap(&edit.text, &wrapped, a));
        edit.text = wrapped;
    }

    /// A press at `p` inside the box being edited puts the caret there (and a drag selects). `true` if it did.
    fn press_caret(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>, p: P) -> bool {
        let Some(edit) = &mut self.edit else { return false };
        if ws.role(&ev.pen) != Role::Draw {
            return false;
        }
        let slack = TAP_SELECT_PX * ws.dpr / ws.view.zoom;
        let [x0, y0] = edit.top_left;
        let [w, h] = text::measure(&edit.text, edit.size);
        if p[0] < x0 - slack || p[0] > x0 + w + slack || p[1] < y0 - slack || p[1] > y0 + h + slack {
            return false;
        }
        let at = text::index_at(&edit.text, edit.size, [p[0] - x0, p[1] - y0]);
        edit.caret = at;
        edit.anchor = None;
        edit.goal_x = None;
        self.press = Press::Caret { dev: ev.dev, id: ev.id, from: at };
        ws.redraw = true;
        true
    }

    /// The pointer moved through `samples` with a press under way.
    fn follow(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        match &mut self.press {
            Press::Grab(m) => {
                if let Some(s) = samples.last() {
                    m.drag(ws, s.pos);
                }
            }
            Press::Pick(p) => p.moved(ws, samples),
            // A drag in the box selects from where it began.
            Press::Caret { from, .. } => {
                let from = *from;
                if let (Some(edit), Some(s)) = (&mut self.edit, samples.last()) {
                    let p = ws.to_doc(s.pos);
                    let at = text::index_at(&edit.text, edit.size, [p[0] - edit.top_left[0], p[1] - edit.top_left[1]]);
                    edit.anchor = Some(from);
                    edit.caret = at;
                    edit.goal_x = None;
                }
            }
            Press::None | Press::Place { .. } => {}
        }
    }

    /// The mouse pressed at `p` (any press has already ended the typing). On a text box it selects it, and
    /// a drag moves it (a click without a drag edits it); a selection made with the lasso moves as it does
    /// with any tool; on empty page, a click starts a new box there.
    fn press_at(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>, p: P) {
        let (dev, id) = (ev.dev, ev.id);
        let grab = |start| Press::Grab(Mover::begin(dev, id, start));
        if let Some(pick) = Pick::begin(ws, ev, p, false) {
            self.press = Press::Pick(pick);
            return;
        }
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
        self.edit = Some(TextEdit::new(Some(id), s.page, top_left, block.text.clone(), block.size, s.style.rgba));
        // A box that was moved partly off its page is brought back in as the edit begins.
        self.fit_width(ws);
        ws.redraw = true;
    }

    /// Start a new text box with its top-left corner at `at` on `page`, kept on the page.
    fn begin_text(&mut self, ws: &mut Workspace, page: u32, at: P) {
        let Some(rect) = ws.page_rect(page) else { return };
        if !at.iter().all(|c| c.is_finite()) {
            return;
        }
        let top_left = crate::plane::clamp_into(rect, at);
        let (rgba, size) = ws.current_style();
        self.edit = Some(TextEdit::new(None, page, top_left, String::new(), size, rgba));
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
        let block = TextBlock { text: edit.field.text, size: edit.size };
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
                if let Some(first) = ev.samples.first() {
                    let p = ws.to_doc(first.pos);
                    // A press in the box being edited is for the caret; anywhere else it ends the edit first.
                    if !self.press_caret(ws, ev, p) {
                        self.finish_text(ws);
                        self.press_at(ws, ev, p);
                    }
                } else {
                    self.finish_text(ws);
                }
            }
            Phase::Move if self.owns(ev.dev, ev.id) => self.follow(ws, ev.samples),
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                self.follow(ws, ev.samples);
                self.end(ws);
            }
            _ => {}
        }
        ws.redraw = true;
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        match &self.press {
            Press::None => false,
            Press::Place { dev: d, id: i, .. } => *d == dev && *i == id,
            Press::Grab(m) => m.dev == dev && m.id == id,
            Press::Pick(p) => p.owns(dev, id),
            Press::Caret { dev: d, id: i, .. } => *d == dev && *i == id,
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
            Press::Pick(p) => p.end(ws, |_, _| false),
            Press::Caret { .. } => {}
        }
    }

    /// Typing becomes part of the document; a press under way is left to finish by itself.
    fn settle(&mut self, ws: &mut Workspace) {
        self.finish_text(ws);
    }

    fn cancel(&mut self, ws: &mut Workspace) {
        // What the eraser had touched is not erased.
        ws.erasing.clear();
        self.press = Press::None;
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        if self.edit.is_none() {
            return selection_key(ws, key);
        }
        match key {
            Key::Escape => self.finish_text(ws),
            // (A box being typed into has no use for the page keys; the document scrolls.)
            Key::Page(_) | Key::Trail(_) => return false,
            _ => return self.edit_key(ws, key),
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
        match &self.press {
            Press::Grab(m) => m.offset(),
            Press::Pick(p) => p.offset(),
            _ => [0.0, 0.0],
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let accent = ws.look.theme.selection;
        if let Press::Pick(p) = &self.press {
            p.paint(ws, out);
        }
        let Some(edit) = &self.edit else { return };
        let px = ws.dpr / ws.view.zoom;
        let [left, top_edge] = edit.top_left;
        // The box being typed into; what is selected in it, as a wash; and the caret.
        polyline(out, &edit.outline(), 1.5 * px, accent);
        if let Some((lo, hi)) = edit.selection() {
            for [x0, y0, x1, y1] in text::selection_rects(&edit.text, edit.size, lo, hi) {
                // A pill as tall as the line, drawn as one thick line so that the wash does not show seams.
                let height = (y1 - y0).max(px);
                let (r, middle) = (height.min(x1 - x0).max(px) / 2.0, top_edge + (y0 + y1) / 2.0);
                out.doc_line([left + x0 + r, middle], [(left + x1 - r).max(left + x0 + r), middle], 2.0 * r, [accent[0], accent[1], accent[2], 70]);
            }
        }
        let caret = text::caret_at(&edit.text, edit.size, edit.caret);
        let x = left + caret[0] + px;
        let top = top_edge + caret[1];
        out.doc_line([x, top], [x, top + edit.size * text::LINE_HEIGHT], 1.4 * px, ws.on_page(edit.rgba));
    }
}
