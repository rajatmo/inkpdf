//! Filling in a form. A field's boxes are links on the page (`Target::Field`), so what finds a link finds a box: a finger's tap with every tool,
//! and the stylus's tip and the left mouse button with the Select tool (the same click that follows a link). A check box turns on or off, a
//! radio button is chosen and a choice goes to its next option at once; a text field starts being typed into (at its end, as a text box is),
//! until Esc, Enter (in a field of one line) or a press elsewhere. What is put in is a change to the file (it is saved with it), not a step of the
//! history: pressing a box again is how it is undone.

use crate::field::{Edited, Field, Rules};
use crate::form::Kind;
use crate::input::Dir;
use crate::links::{self, Target};
use crate::tools::Key;

use super::Editor;

/// The field being typed into, which of its boxes was pressed, and the text being edited (see [`crate::field`]): it is put in the form as it changes.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Typing {
    pub field: usize,
    pub widget: usize,
    edit: Field,
}

impl Typing {
    fn is(&self, field: usize, widget: usize) -> bool {
        (self.field, self.widget) == (field, widget)
    }
}

/// How a box of a field is drawn: where it is, what the person put in it and where its text goes.
pub struct BoxDraw {
    pub page: usize,
    /// The box, in document space.
    pub rect: [f32; 4],
    pub kind: Kind,
    /// The person has changed the field: the program draws it, over what the file's page shows.
    pub changed: bool,
    /// A check box or radio button that is on.
    pub on: bool,
    /// A text or choice's text, the size it is set at and where its top-left corner is (a long text of one line is shown by its end).
    pub text: String,
    pub size: f32,
    pub top_left: [f32; 2],
    /// Where the caret is, if this box is the one being typed into.
    pub caret: Option<[f32; 4]>,
}

/// The size a text is set at in a box: that of its lines, but not more than the box has room for.
fn text_size(rect: [f32; 4], multiline: bool) -> f32 {
    if multiline { 11.0 } else { ((rect[3] - rect[1]) * 0.62).clamp(6.0, 18.0) }
}

impl Editor {
    /// The boxes of the form on `pages`, as they are to be drawn. (Nothing for a document without a form.)
    pub fn form_boxes(&self, pages: &std::ops::Range<usize>) -> Vec<BoxDraw> {
        if self.form.fields.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (field, widget, rect) in self.field_places() {
            let page = self.ws.layout.page_at((rect[1] + rect[3]) / 2.0);
            let Some(f) = self.form.fields.get(field).filter(|_| pages.contains(&page)) else { continue };
            let text = f.shown();
            let size = text_size(rect, f.multiline);
            let (width, height) = (rect[2] - rect[0] - 4.0, rect[3] - rect[1]);
            let mut top_left = [rect[0] + 2.0, if f.multiline { rect[1] + 2.0 } else { rect[1] + (height - size * crate::text::LINE_HEIGHT) / 2.0 }];
            let typing = self.form_typing.as_ref().filter(|t| t.is(field, widget));
            let at = typing.map(|t| crate::text::caret_at(&text, size, t.edit.caret));
            // (A line longer than the box is shown as far as the caret.)
            if let Some(at) = at.filter(|_| !f.multiline) {
                top_left[0] -= (at[0] - width).max(0.0);
            }
            let caret =
                at.map(|at| [top_left[0] + at[0], top_left[1] + at[1], top_left[0] + at[0], top_left[1] + at[1] + size * crate::text::LINE_HEIGHT]);
            out.push(BoxDraw { page, rect, kind: f.kind, changed: f.changed(), on: f.is_on(widget), text, size, top_left, caret });
        }
        out
    }

    /// The field being typed into, if one is.
    #[cfg(test)]
    pub fn form_typing(&self) -> Option<usize> {
        self.form_typing.as_ref().map(|t| t.field)
    }

    /// Every box of a field on a page, in document space: `(field, widget, rect)`.
    pub fn field_places(&self) -> impl Iterator<Item = (usize, usize, [f32; 4])> + '_ {
        self.links.iter().filter_map(|l| match l.target {
            Target::Field { field, widget } => Some((field, widget, l.rect)),
            _ => None,
        })
    }

    /// A press on box `widget` of field `field`, which is on `rect`: it is filled as its kind is.
    pub(super) fn press_field(&mut self, field: usize, widget: usize, rect: [f32; 4]) {
        // (Another field that was being typed into is let go of: its text stays.)
        if self.form_typing.as_ref().is_some_and(|t| !t.is(field, widget)) {
            self.end_form_typing();
        }
        let Some(f) = self.form.fields.get_mut(field) else { return };
        if f.kind == Kind::Text {
            // (The caret is at the end of what the field holds, as in a text box.)
            self.form_typing.get_or_insert_with(|| Typing { field, widget, edit: Field::new(f.shown()) });
            self.ws.redraw = true;
        } else if f.press(widget) {
            self.form_changed(rect);
        }
    }

    /// Something was put in a field whose box is at `rect`: the page counts as changed (there is something to save) and is drawn again.
    fn form_changed(&mut self, rect: [f32; 4]) {
        let page = self.ws.layout.page_at((rect[1] + rect[3]) / 2.0);
        self.ws.ink.mark_dirty(page as u32);
        self.ws.redraw = true;
    }

    /// The box the field being typed into is pressed on, in document space.
    fn typing_rect(&self) -> Option<[f32; 4]> {
        let t = self.form_typing.as_ref()?;
        self.field_places().find(|&(f, w, _)| f == t.field && w == t.widget).map(|(_, _, r)| r)
    }

    /// Stop typing into a field (what was typed stays).
    pub(super) fn end_form_typing(&mut self) {
        if self.form_typing.take().is_some() {
            self.ws.redraw = true;
        }
    }

    /// A press at `pos` (the window's) that is not on the box being typed into lets go of it.
    pub(super) fn leave_field(&mut self, pos: [f32; 2]) {
        let at = self.to_doc(pos);
        if self.form_typing.is_some() && !self.typing_rect().is_some_and(|r| crate::ink::rect_contains(r, at)) {
            self.end_form_typing();
        }
    }

    /// The keyboard, while a field is being typed into: the text is edited as any field is (see [`Field::key`]); Enter ends a field of one line,
    /// as Esc ends any. `true` if it was heard.
    pub(super) fn form_key(&mut self, key: Key<'_>) -> bool {
        let Some(typing) = &mut self.form_typing else { return false };
        let Some(f) = self.form.fields.get_mut(typing.field) else { return false };
        let (max_chars, multiline) = f.rules();
        match key {
            Key::Escape => {
                self.end_form_typing();
                return true;
            }
            Key::Text("\n") if !multiline => {
                self.end_form_typing();
                return true;
            }
            // (What scrolls the page, and the arrows that go up and down, are left to scroll it.)
            Key::Arrow(Dir::Up | Dir::Down, _) | Key::Page(_) => return false,
            _ => {}
        }
        let edited = typing.edit.key(key, &mut *self.ws.clip.text, Rules { max_chars, multiline });
        if edited == Edited::Changed
            && f.set_text(typing.edit.text.clone())
            && let Some(rect) = self.typing_rect()
        {
            self.form_changed(rect);
        }
        self.ws.redraw = true;
        edited != Edited::Unused
    }
    /// Fill the field under `pos`, if there is one (a tap): `true` if there was.
    pub(super) fn fill_field_at(&mut self, pos: [f32; 2]) -> bool {
        let at = self.to_doc(pos);
        let Some(link) = links::hit(&self.links, at) else { return false };
        let Target::Field { field, widget } = link.target else { return false };
        let rect = link.rect;
        self.press_field(field, widget, rect);
        true
    }
}
