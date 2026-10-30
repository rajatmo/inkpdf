//! The undo and redo history: each step is a [`Cmd`] that knows how to do itself to the [`Drawing`] and how to take itself back. The
//! [`History`] keeps the steps, in order, and runs them.

use std::collections::{HashMap, HashSet};

use crate::pages::{PageEdit, Remap};

use super::Stroke;
use super::drawing::Drawing;

/// Undo steps kept.
pub(super) const MAX_HISTORY: usize = 500;

pub(super) enum Cmd {
    Add(Vec<Stroke>),
    /// With each stroke's position, so undo puts it back where it was in the drawing order.
    Remove(Vec<(usize, Stroke)>),
    Move {
        ids: Vec<u64>,
        dx: f32,
        dy: f32,
        from: Vec<(u64, u32)>,
        to_page: Option<u32>,
    },
    /// A shape reshaped by its anchors: the stroke before and after.
    Reshape {
        before: Box<Stroke>,
        after: Box<Stroke>,
    },
    /// Several strokes restyled at once (a new colour, thickness or text size): each before and after.
    Restyle {
        before: Vec<Stroke>,
        after: Vec<Stroke>,
    },
    /// The pages were edited (added, deleted, rearranged).
    Pages(Box<PageStep>),
}

/// An edit of the pages as the history holds it.
pub(super) struct PageStep {
    pub edit: PageEdit,
    /// The edit that takes the pages back.
    pub undo: PageEdit,
    pub forward: Remap,
    pub backward: Remap,
    /// The strokes that were on the pages that went, each with its place in the drawing order.
    pub buried: Vec<(usize, Stroke)>,
    /// The strokes of the pages that were copied, as the copies have them: they come back when the step is redone.
    pub made: Vec<Stroke>,
}

impl Cmd {
    /// Do the step. An edit of the pages is also said in `page_edit`, for the rest of the document to follow.
    pub fn apply(&self, d: &mut Drawing, page_edit: &mut Option<PageEdit>) {
        match self {
            Cmd::Reshape { after, .. } => d.replace(after),
            Cmd::Restyle { after, .. } => after.iter().for_each(|s| d.replace(s)),
            Cmd::Add(strokes) => strokes.iter().for_each(|s| d.push(s.clone())),
            Cmd::Remove(removed) => d.remove_ids(&removed.iter().map(|(_, s)| s.id).collect()),
            Cmd::Pages(step) => {
                d.remap(&step.forward);
                d.strokes.extend(step.made.iter().cloned());
                *page_edit = Some(step.edit.clone());
            }
            Cmd::Move { ids, dx, dy, to_page, .. } => d.shift(&ids.iter().copied().collect(), *dx, *dy, |_| *to_page),
        }
    }

    /// Take the step back.
    pub fn revert(&self, d: &mut Drawing, page_edit: &mut Option<PageEdit>) {
        match self {
            Cmd::Reshape { before, .. } => d.replace(before),
            Cmd::Restyle { before, .. } => before.iter().for_each(|s| d.replace(s)),
            Cmd::Add(strokes) => d.remove_ids(&strokes.iter().map(|s| s.id).collect()),
            Cmd::Remove(removed) => d.put_back(removed),
            Cmd::Pages(step) => {
                d.remap(&step.backward);
                d.put_back(&step.buried);
                *page_edit = Some(step.undo.clone());
            }
            Cmd::Move { ids, dx, dy, from, .. } => {
                let from: HashMap<u64, u32> = from.iter().copied().collect();
                d.shift(&ids.iter().copied().collect::<HashSet<_>>(), -*dx, -*dy, |s| from.get(&s.id).copied());
            }
        }
    }
}

#[derive(Default)]
pub(super) struct History {
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
    /// The graves of deletions whose steps the history let go of, until they are taken (see [`History::take_evicted`]).
    evicted: Vec<u64>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// A step that has been done: it can be undone, and what was undone before it can no longer be redone.
    pub fn record(&mut self, cmd: Cmd) {
        self.undo.push(cmd);
        if self.undo.len() > MAX_HISTORY
            && let Cmd::Pages(step) = self.undo.remove(0)
            && let PageEdit::Delete { grave, .. } = step.edit
            && grave != 0
        {
            // The oldest step goes: a deletion that can no longer be undone has nothing to be kept for.
            self.evicted.push(grave);
        }
        self.redo.clear();
    }

    /// Take the last step back. `false` if there is none.
    pub fn undo(&mut self, d: &mut Drawing, page_edit: &mut Option<PageEdit>) -> bool {
        let Some(cmd) = self.undo.pop() else { return false };
        cmd.revert(d, page_edit);
        self.redo.push(cmd);
        true
    }

    /// Do the last step that was taken back again. `false` if there is none.
    pub fn redo(&mut self, d: &mut Drawing, page_edit: &mut Option<PageEdit>) -> bool {
        let Some(cmd) = self.redo.pop() else { return false };
        cmd.apply(d, page_edit);
        self.undo.push(cmd);
        true
    }

    /// If the last step restyled all of these strokes, `after` is added to it (a slider being dragged is one step) and `true`; the redo steps go.
    pub fn coalesce_restyle(&mut self, after: &[Stroke]) -> bool {
        let Some(Cmd::Restyle { after: last, .. }) = self.undo.last_mut() else { return false };
        if !after.iter().all(|s| last.iter().any(|l| l.id == s.id)) {
            return false;
        }
        for s in after {
            if let Some(slot) = last.iter_mut().find(|l| l.id == s.id) {
                *slot = s.clone();
            }
        }
        self.redo.clear();
        true
    }

    /// Nothing before the last page that was deleted can be undone once the file is written without it: the page cannot be put back into a file
    /// that no longer has it.
    pub fn forget_deleted_pages(&mut self) {
        let deleted = |cmd: &Cmd| matches!(cmd, Cmd::Pages(step) if matches!(step.edit, PageEdit::Delete { .. }));
        if let Some(last) = self.undo.iter().rposition(deleted) {
            self.undo.drain(..=last);
        }
    }

    /// The names of the graves of deletions that can no longer be undone, because the history has let go of their steps. Taken, so it is given once.
    pub fn take_evicted(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.evicted)
    }
}
