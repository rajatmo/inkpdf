//! The select tool: selects the text a PDF has (not ink, not text boxes: those have their own tools). A press on a letter and a drag take the
//! letters between, a double press a word, a triple press a line; a handle at each end of what is selected moves it (for a stylus or a
//! finger-less tablet that is the only way to stretch a selection); Ctrl+A takes the page's text, Ctrl+C copies it, Esc lets go. What is
//! selected can be marked (highlighted, underlined, struck out): that makes a markup (see [`crate::ink::MarkupKind`]) from the boxes of
//! the lines selected. A press on a markup selects it, to move or delete it. The text comes from the document's [`DocText`](crate::doctext),
//! as far as it has been read.

use std::sync::Arc;

use crate::ink::{MarkupKind, P, Style};
use crate::input::{Device, Mode, POINTER, Phase};
use crate::pagetext::PageTextView;
use crate::tool::{ToolAction, markup_colour};
use crate::workspace::Workspace;

use super::paint::Painter;
use super::shortcuts::selection_key;
use super::{InputPolicy, Key, PageEvent, Tool};

/// How near, in logical pixels, a press must be to a letter to be on it.
const LETTER_SLACK_PX: f32 = 5.0;
/// How near, in logical pixels, a press must be to a handle to take it.
const HANDLE_REACH_PX: f32 = 16.0;
/// A press that moves less than this many logical pixels is a tap.
const TAP_PX: f32 = 4.0;
/// The marks on the three buttons of the bar.
const MARKS: [ToolAction; 3] =
    [ToolAction::Mark(MarkupKind::Highlight), ToolAction::Mark(MarkupKind::Underline), ToolAction::Mark(MarkupKind::StrikeOut)];

/// Text selected on one page: the letters from `anchor` to `head` (either may come first), both included.
struct Chosen {
    page: u32,
    /// The page's text as it was when this was chosen: if the page now has another (the pages were edited), this is let go.
    text: Arc<dyn PageTextView>,
    anchor: usize,
    head: usize,
}

impl Chosen {
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head) + 1
    }
}

/// A press under way.
struct Press {
    dev: Device,
    id: u32,
    /// Where it began, on the screen.
    from: [f32; 2],
    /// Whether it has been dragged past a tap.
    moved: bool,
    /// Whether the drag stretches the selection (it began on a letter or a handle).
    stretches: bool,
    /// A markup that was under it, to select if it turns out to be a tap.
    markup: Option<u64>,
    /// Whether the drag moves the page instead (it began on nothing to read, or with the middle button of a mouse): the reader's own way,
    /// and where it last was, on the screen.
    pans: bool,
    at: [f32; 2],
}

#[derive(Default)]
pub struct SelectTool {
    chosen: Option<Chosen>,
    press: Option<Press>,
    /// The last press, for a second or a third: when, where (screen) and how many in a row.
    last: Option<(f64, [f32; 2], u8)>,
}

impl SelectTool {
    /// What is selected, if the page still has the text it was chosen from (the pages may have been edited since).
    fn live(&self, ws: &Workspace) -> Option<&Chosen> {
        self.chosen.as_ref().filter(|c| ws.text.page(c.page as usize).is_some_and(|t| Arc::ptr_eq(t, &c.text)))
    }

    /// Whether any text is selected.
    pub fn has_selection(&self, ws: &Workspace) -> bool {
        self.live(ws).is_some()
    }

    /// The selected text, as a reader would copy it.
    pub fn text(&self, ws: &Workspace) -> Option<String> {
        let c = self.live(ws)?;
        let glyphs: Vec<usize> = c.range().collect();
        Some(c.text.column_text(&glyphs)).filter(|t| !t.is_empty())
    }

    /// Put the selected text on the clipboard. `true` if there was some.
    pub fn copy_text(&mut self, ws: &mut Workspace) -> bool {
        let Some(text) = self.text(ws) else { return false };
        ws.clip.text.set(&text);
        true
    }

    /// Let go of the selected text.
    pub fn clear(&mut self, ws: &mut Workspace) {
        if self.chosen.take().is_some() {
            ws.redraw = true;
        }
    }

    /// Select all the text of the page in view.
    fn select_all(&mut self, ws: &mut Workspace) -> bool {
        let page = ws.view.current_page(&ws.layout);
        let Some(text) = ws.text.page(page).cloned() else { return false };
        let last = text.glyph_count().saturating_sub(1);
        self.chosen = Some(Chosen { page: page as u32, text, anchor: 0, head: last });
        ws.selection.clear();
        ws.redraw = true;
        true
    }

    /// The selection, if the page still has the text it was chosen from.
    fn valid(&mut self, ws: &Workspace) -> Option<&Chosen> {
        if self.live(ws).is_none() {
            self.chosen = None;
        }
        self.chosen.as_ref()
    }

    /// The marks of the selected lines, in document space (one box per line).
    fn boxes(ws: &Workspace, c: &Chosen) -> Vec<[f32; 4]> {
        let Some(r) = ws.layout.page(c.page as usize) else { return Vec::new() };
        c.text.line_rects(c.range()).into_iter().map(|b| [b[0] + r.x, b[1] + r.y, b[2] + r.x, b[3] + r.y]).collect()
    }

    /// The two handles, where the selection begins and ends (the bottom left of its first box, the bottom right of its last), in document space.
    fn handles(ws: &Workspace, c: &Chosen) -> Option<[P; 2]> {
        let boxes = Self::boxes(ws, c);
        let (first, last) = (boxes.first()?, boxes.last()?);
        Some([[first[0], first[3]], [last[2], last[3]]])
    }

    /// The letter at a point of the screen: its page, text and number. Only on a letter (a little beyond it), not near one.
    fn letter_at(ws: &Workspace, screen: [f32; 2]) -> Option<(u32, Arc<dyn PageTextView>, usize)> {
        let doc = ws.to_doc(screen);
        let page = ws.page_hit(doc)?;
        let text = ws.text.page(page as usize)?.clone();
        let r = ws.layout.page(page as usize)?;
        let at = [doc[0] - r.x, doc[1] - r.y];
        let glyph = text.nearest(at[0], at[1])?;
        let b = text.glyph_rects(&[glyph]).first().copied()?;
        let slack = LETTER_SLACK_PX * ws.dpr / ws.view.zoom;
        let on = at[0] >= b[0] - slack && at[0] <= b[2] + slack && at[1] >= b[1] - slack && at[1] <= b[3] + slack;
        on.then_some((page, text, glyph))
    }

    /// The letter nearest a point of the screen on `page`, wherever the point is (a drag past the edge of the text still has an end).
    fn nearest_on(ws: &Workspace, page: u32, screen: [f32; 2]) -> Option<usize> {
        let doc = ws.to_doc(screen);
        let r = ws.layout.page(page as usize)?;
        ws.text.page(page as usize)?.nearest(doc[0] - r.x, doc[1] - r.y)
    }

    /// A markup under a point of the screen, if one is on top there.
    fn markup_at(ws: &Workspace, screen: [f32; 2]) -> Option<u64> {
        let slack = 3.0 * ws.dpr / ws.view.zoom;
        let at = ws.to_doc(screen);
        // (The one on top, if it is a markup: a stroke of ink drawn over it is the one that is meant.)
        ws.ink.strokes().iter().rev().find(|s| s.touches_circle(at, slack)).filter(|s| s.markup.is_some()).map(|s| s.id)
    }

    /// Whether `screen` is on one of the two handles of the selection: which one (0 begins, 1 ends).
    fn handle_at(&mut self, ws: &Workspace, screen: [f32; 2]) -> Option<usize> {
        let c = self.valid(ws)?;
        let handles = Self::handles(ws, c)?;
        let reach = HANDLE_REACH_PX * ws.dpr;
        let near = |h: P| {
            let s = ws.view.doc_to_screen(h);
            (s[0] - screen[0]).hypot(s[1] - screen[1]) <= reach
        };
        // (The nearer, if both are: a selection of one letter has them close together.)
        let d = |h: P| {
            let s = ws.view.doc_to_screen(h);
            (s[0] - screen[0]).hypot(s[1] - screen[1])
        };
        match (near(handles[0]), near(handles[1])) {
            (true, true) => Some(usize::from(d(handles[1]) < d(handles[0]))),
            (true, false) => Some(0),
            (false, true) => Some(1),
            (false, false) => None,
        }
    }

    /// Whether a point of the screen is on one of the two handles of the selection (for the cursor, which only looks).
    pub fn over_handle(&self, ws: &Workspace, screen: [f32; 2]) -> bool {
        let Some(handles) = self.live(ws).and_then(|c| Self::handles(ws, c)) else { return false };
        let reach = HANDLE_REACH_PX * ws.dpr;
        handles.iter().any(|&h| {
            let s = ws.view.doc_to_screen(h);
            (s[0] - screen[0]).hypot(s[1] - screen[1]) <= reach
        })
    }

    fn begin(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        let Some(first) = ev.samples.first() else { return };
        let at = first.pos;
        let press = |moved, stretches, markup, pans| Press { dev: ev.dev, id: ev.id, from: at, moved, stretches, markup, pans, at };
        // The middle button of a mouse pans, as in any reader (a stylus's barrel button loops, as it does with every tool).
        if ev.pen.barrel && ev.id == crate::input::MOUSE_STYLUS_ID {
            self.press = Some(press(true, false, None, true));
            self.last = None;
            return;
        }
        // With Shift held, a press on a letter of the page the selection is on stretches it to there (as in every reader), and a drag goes on stretching.
        if ws.shift
            && let Some((page, _, glyph)) = Self::letter_at(ws, at)
            && let Some(c) = self.valid(ws)
            && c.page == page
        {
            if let Some(c) = &mut self.chosen {
                c.head = glyph;
            }
            self.press = Some(press(true, true, None, false));
            self.last = None;
            ws.redraw = true;
            return;
        }
        // A handle takes the end it is on and leaves the other where it is.
        if let Some(end) = self.handle_at(ws, at) {
            if let Some(c) = &mut self.chosen {
                let (lo, hi) = (c.anchor.min(c.head), c.anchor.max(c.head));
                (c.anchor, c.head) = if end == 0 { (hi, lo) } else { (lo, hi) };
            }
            self.press = Some(press(true, true, None, false));
            self.last = None;
            return;
        }
        let markup = Self::markup_at(ws, at);
        let Some((page, text, glyph)) = Self::letter_at(ws, at) else {
            // Nothing to read here: a markup is selected, or what was selected is let go; and a drag from empty page pans it.
            self.chosen = None;
            ws.selection.clear();
            if let Some(id) = markup {
                ws.selection.push(id);
            }
            self.press = Some(press(false, false, None, markup.is_none()));
            self.last = None;
            ws.redraw = true;
            return;
        };
        ws.selection.clear();
        let count = match self.last {
            // (A second or third press, as near and as soon as a double click is, makes a word or a line.)
            Some((t, p, n)) if POINTER.is_double(first.t_ms - t, (p[0] - at[0]).hypot(p[1] - at[1]) / ws.dpr) => (n + 1).min(3),
            _ => 1,
        };
        self.last = Some((first.t_ms, at, count));
        let (anchor, head) = match count {
            2 => text.word_at(glyph).unwrap_or((glyph, glyph)),
            3 => text.line_at(glyph).unwrap_or((glyph, glyph)),
            _ => (glyph, glyph),
        };
        self.chosen = Some(Chosen { page, text, anchor, head });
        // A press that is a word or a line is not stretched by a drag (it would only be unsettling); a first one is.
        self.press = Some(press(count > 1, count == 1, markup, false));
        ws.redraw = true;
    }

    fn drag(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        let (Some(press), Some(last)) = (&mut self.press, ev.samples.last()) else { return };
        if !press.moved && (last.pos[0] - press.from[0]).hypot(last.pos[1] - press.from[1]) > TAP_PX * ws.dpr {
            press.moved = true;
        }
        if press.pans {
            // (Only once it is a drag: a tap on empty page lets go of the selection, which is all it does.)
            if press.moved {
                ws.pan_px(last.pos[0] - press.at[0], last.pos[1] - press.at[1]);
            }
            press.at = last.pos;
            return;
        }
        if !(press.stretches && press.moved) {
            return;
        }
        let Some(page) = self.chosen.as_ref().map(|c| c.page) else { return };
        if let Some(glyph) = Self::nearest_on(ws, page, last.pos)
            && let Some(c) = &mut self.chosen
            && c.head != glyph
        {
            c.head = glyph;
            ws.redraw = true;
        }
    }

    fn finish(&mut self, ws: &mut Workspace) {
        let Some(press) = self.press.take() else { return };
        // A tap on a letter selects no more than the letter it was on: it lets go (and selects the markup that was there, if one was).
        if !press.moved {
            self.chosen = None;
            if let Some(id) = press.markup {
                ws.selection.clear();
                ws.selection.push(id);
            }
            ws.redraw = true;
        }
    }

    /// Make a markup of `kind` from the lines selected, and let go of the selection (the markup is what shows). `true` if it made one.
    fn mark(&mut self, ws: &mut Workspace, kind: MarkupKind) -> bool {
        let Some(c) = self.valid(ws) else { return false };
        let (page, boxes) = (c.page, Self::boxes(ws, c));
        let made = ws.ink.add_markup(page, Style { rgba: markup_colour(kind), width: 1.0 }, kind, &boxes);
        self.chosen = None;
        ws.redraw = true;
        made.is_some()
    }
}

impl Tool for SelectTool {
    fn policy(&self, mode: Mode) -> InputPolicy {
        InputPolicy::editing_tool(mode)
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        match ev.phase {
            Phase::Down => self.begin(ws, ev),
            Phase::Move if self.owns(ev.dev, ev.id) => self.drag(ws, ev),
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                self.drag(ws, ev);
                if ev.phase == Phase::Cancel {
                    self.press = None;
                } else {
                    self.finish(ws);
                }
            }
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        self.press.as_ref().is_some_and(|p| p.dev == dev && p.id == id)
    }

    fn busy(&self) -> bool {
        self.press.is_some()
    }

    fn end(&mut self, ws: &mut Workspace) {
        self.finish(ws);
    }

    fn leave(&mut self, ws: &mut Workspace) {
        self.press = None;
        self.last = None;
        self.clear(ws);
    }

    fn cancel(&mut self, _ws: &mut Workspace) {
        self.press = None;
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        match key {
            Key::Chord('a') => self.select_all(ws),
            Key::Chord('c') => self.copy_text(ws),
            Key::Escape if self.chosen.is_some() => {
                self.clear(ws);
                true
            }
            // (Delete, Escape and the arrows are for what is selected of the markups.)
            Key::Delete | Key::Backspace | Key::Escape | Key::Arrow(..) => selection_key(ws, key),
            _ => false,
        }
    }

    fn actions(&self, _ws: &Workspace) -> &'static [ToolAction] {
        &MARKS
    }

    fn action_enabled(&self, ws: &Workspace, action: ToolAction) -> bool {
        matches!(action, ToolAction::Mark(_)) && self.has_selection(ws)
    }

    fn act(&mut self, ws: &mut Workspace, action: ToolAction) -> bool {
        match action {
            ToolAction::Mark(kind) => self.mark(ws, kind),
            ToolAction::Node(_) => false,
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let Some(c) = self.live(ws) else { return };
        let px = ws.dpr / ws.view.zoom;
        let [r, g, b, _] = ws.look.theme.selection;
        for q in Self::boxes(ws, c) {
            let h = q[3] - q[1];
            let ends = (h / 2.0).min((q[2] - q[0]) / 2.0);
            out.doc_line([q[0] + ends, q[1] + h / 2.0], [q[2] - ends, q[1] + h / 2.0], h, [r, g, b, 90]);
        }
        // A handle at each end: a ring with a dot, on the line's foot.
        if let Some(handles) = Self::handles(ws, c) {
            for h in handles {
                out.doc_line(h, h, 11.0 * px, ws.look.theme.selection);
                out.doc_line(h, h, 7.5 * px, ws.look.theme.white);
            }
        }
    }
}
