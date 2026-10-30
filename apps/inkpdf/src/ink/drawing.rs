//! What is drawn: the strokes, in drawing order, and the pages that have changed since the file was written. This is the one thing a step of
//! the history changes (see [`super::history`]); what is asked of it (hit tests, bounds) is in [`super::query`].

use std::collections::{BTreeSet, HashSet};

use crate::pages::Remap;

use super::Stroke;

#[derive(Default)]
pub(super) struct Drawing {
    pub strokes: Vec<Stroke>,
    pub dirty: BTreeSet<u32>,
}

impl Drawing {
    /// `with` takes the place of the stroke that has its id, and its page is changed.
    pub fn replace(&mut self, with: &Stroke) {
        if let Some(slot) = self.strokes.iter_mut().find(|s| s.id == with.id) {
            *slot = with.clone();
        }
        self.dirty.insert(with.page);
    }

    /// Put `stroke` last in the drawing order.
    pub fn push(&mut self, stroke: Stroke) {
        self.dirty.insert(stroke.page);
        self.strokes.push(stroke);
    }

    /// The strokes with these ids come out; their pages are changed.
    pub fn remove_ids(&mut self, ids: &HashSet<u64>) {
        for s in self.strokes.iter().filter(|s| ids.contains(&s.id)) {
            self.dirty.insert(s.page);
        }
        self.strokes.retain(|s| !ids.contains(&s.id));
    }

    /// Put strokes back, each where it was in the drawing order (`removed` is by ascending position, so each insert lands where it was).
    pub fn put_back(&mut self, removed: &[(usize, Stroke)]) {
        for (i, s) in removed {
            self.dirty.insert(s.page);
            self.strokes.insert((*i).min(self.strokes.len()), s.clone());
        }
    }

    /// The strokes on pages that `remap` drops come out (with their places in the drawing order) and the others take the pages' new numbers and
    /// places; the dirty pages follow, and the pages the edit touches are dirty.
    pub fn remap(&mut self, remap: &Remap) -> Vec<(usize, Stroke)> {
        let mut buried = Vec::new();
        for (i, mut s) in std::mem::take(&mut self.strokes).into_iter().enumerate() {
            match remap.moved(s.page as usize) {
                Some(m) => {
                    s.page = m.page as u32;
                    s.translate(m.dx, m.dy);
                    self.strokes.push(s);
                }
                None => buried.push((i, s)),
            }
        }
        self.dirty = self.dirty.iter().filter_map(|&p| remap.page(p as usize)).map(|p| p as u32).collect();
        self.dirty.extend(remap.touched().into_iter().map(|p| p as u32));
        buried
    }

    /// Move the strokes with these ids by `(dx, dy)`, and (if `to_page` says) onto a page; both pages are changed.
    pub fn shift(&mut self, ids: &HashSet<u64>, dx: f32, dy: f32, to_page: impl Fn(&Stroke) -> Option<u32>) {
        for s in self.strokes.iter_mut().filter(|s| ids.contains(&s.id)) {
            self.dirty.insert(s.page);
            s.translate(dx, dy);
            if let Some(page) = to_page(s) {
                s.page = page;
            }
            self.dirty.insert(s.page);
        }
    }
}
