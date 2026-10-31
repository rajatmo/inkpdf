//! The pages overview's input: a press, a drag, the wheel, the keys and the shortcuts, while it is up.

use super::*;

impl Editor {
    /// A pen or touch event while the overview is up.
    pub(in crate::editor) fn overview_event(&mut self, ev: &PointerEvent) {
        if ev.device == Device::Touch && !self.palm.admit_touch(ev, crate::editor::touch::PALM_PX * self.ws.dpr) {
            return;
        }
        let sample = if ev.phase == Phase::Down { ev.samples.first() } else { ev.samples.last() };
        if let Some(s) = sample {
            self.overview_pointer(ev.device, ev.id, ev.phase, s.pos);
        }
    }

    /// One pointer's position and phase.
    pub(in crate::editor) fn overview_pointer(&mut self, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        self.ws.redraw = true;
        let layout = self.overview_layout();
        let hit = self.overview_hit(pos);
        let over_toc = layout.toc.is_some_and(|t| crate::ink::rect_contains(t, pos));
        let count = self.ws.layout.len();
        // (Pages are not carried about while only some of them are shown: where would they be dropped?)
        let filtered = self.only_flagged().is_some();
        let dpr = self.ws.dpr;
        let Some(state) = &mut self.chrome.overview else { return };
        let mine = state.press.is_some_and(|p| p.dev == dev && p.id == id);
        match phase {
            Phase::Down => {
                if state.press.is_none() {
                    state.press = Some(Press { dev, id, on: hit, in_toc: over_toc, last: pos, start: pos, dragged: false });
                }
                state.hover = hit;
            }
            Phase::Move | Phase::Hover => {
                state.hover = hit;
                if mine && let Some(press) = state.press.as_mut() {
                    let travelled = ((pos[0] - press.start[0]).powi(2) + (pos[1] - press.start[1]).powi(2)).sqrt();
                    if travelled > DRAG_PX * dpr {
                        press.dragged = true;
                    }
                    let dy = pos[1] - press.last[1];
                    press.last = pos;
                    let (on, in_toc, dragged) = (press.on, press.in_toc, press.dragged);
                    match on {
                        // A page pulled by a pen or a mouse is carried to another place; a finger pulls the pages past it.
                        Some(Hover::Page(i)) if dragged && dev != Device::Touch && !filtered => {
                            if state.drag.is_none() {
                                let pages =
                                    if state.edit && state.selected.contains(&i) { state.selected.iter().copied().collect() } else { vec![i] };
                                state.drag = Some(Drag { pages, pos, slot: layout.slot_at(pos, count, state.scroll) });
                            }
                            self.carry_pages(pos, &layout);
                        }
                        Some(Hover::Control(_) | Hover::Action(_)) => {}
                        _ if dragged => self.scroll_overview_by(-dy, in_toc),
                        _ => {}
                    }
                }
            }
            Phase::Up => {
                if mine && let Some(press) = state.press.take() {
                    state.hover = hit;
                    if state.menu.is_some() {
                        // An item of the menu is done if it is pressed and let go; anywhere else puts the menu away.
                        let item = press.on.filter(|on| Some(*on) == hit);
                        match item {
                            Some(Hover::Menu(item)) if !press.dragged => self.do_menu_item(item),
                            _ => state.menu = None,
                        }
                    } else if state.drag.is_some() {
                        let dropped = state.drag.take();
                        if let Some(drag) = dropped {
                            self.drop_pages(&drag);
                        }
                    } else if !press.dragged {
                        match (press.on, hit) {
                            (Some(a), Some(b)) if a == b => self.overview_activate(a),
                            // A tap on the dimmed document round the card puts the overview away.
                            (None, None) if !layout.in_card(pos) && !layout.in_card(press.start) => self.close_overview(),
                            _ => {}
                        }
                    }
                }
            }
            Phase::Cancel | Phase::Leave => {
                if mine || phase == Phase::Leave {
                    state.press = None;
                    state.drag = None;
                }
                state.hover = None;
            }
        }
    }

    /// The dragged pages follow the pointer: where they would land is found, and the pages scroll when it is held near
    /// the top or the bottom of them.
    pub(super) fn carry_pages(&mut self, pos: [f32; 2], layout: &OverviewLayout) {
        let (count, edge, speed) = (self.ws.layout.len(), EDGE_UNITS * layout.unit(), EDGE_SPEED_UNITS * layout.unit());
        let toward = if pos[1] < layout.grid[1] + edge {
            -((layout.grid[1] + edge - pos[1]) / edge).min(1.5)
        } else if pos[1] > layout.grid[3] - edge {
            ((pos[1] - (layout.grid[3] - edge)) / edge).min(1.5)
        } else {
            0.0
        };
        if let Some(state) = &mut self.chrome.overview {
            state.goal = None;
            state.scroll = layout.clamp_scroll(state.scroll + toward * speed, count);
            let slot = layout.slot_at(pos, count, state.scroll);
            if let Some(drag) = &mut state.drag {
                drag.pos = pos;
                drag.slot = slot;
            }
        }
    }

    /// The pages that were being dragged are dropped at the place they were carried to.
    pub(super) fn drop_pages(&mut self, drag: &Drag) {
        let mut pages = drag.pages.clone();
        pages.sort_unstable();
        // `to` counts the pages that stay: the ones before the place, but not the moving ones among them.
        let to = (0..drag.slot).filter(|p| !pages.contains(p)).count();
        if self.edit_pages(PageEdit::Move { pages: pages.clone(), to }) {
            let first = to;
            self.overview_show_page(first);
        }
    }

    /// Scroll the pages (or the table of contents) by `dy` pixels.
    pub(super) fn scroll_overview_by(&mut self, dy: f32, toc: bool) {
        let layout = self.overview_layout();
        let (pages, rows) = (self.overview_count(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.menu = None;
            if toc {
                state.toc_goal = None;
                state.toc_scroll = layout.clamp_toc_scroll(state.toc_scroll + dy, rows);
            } else {
                state.goal = None;
                state.scroll = layout.clamp_scroll(state.scroll + dy, pages);
            }
        }
        self.ws.redraw = true;
    }

    /// Send the pages (or the table of contents) `dy` pixels further than they are going: they glide there.
    pub(super) fn glide_overview_by(&mut self, dy: f32, toc: bool) {
        // (With motion reduced the pages go where they are sent at once.)
        if self.motions.prefs().reduced {
            return self.scroll_overview_by(dy, toc);
        }
        let layout = self.overview_layout();
        let (pages, rows) = (self.overview_count(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.menu = None;
            if toc {
                let to = layout.clamp_toc_scroll(state.toc_goal.unwrap_or(state.toc_scroll) + dy, rows);
                state.toc_goal = Some(to);
            } else {
                let to = layout.clamp_scroll(state.goal.unwrap_or(state.scroll) + dy, pages);
                state.goal = Some(to);
            }
        }
        self.ws.redraw = true;
    }

    /// The wheel scrolls the pages, or the table of contents when the pointer is over it.
    pub(in crate::editor) fn overview_wheel(&mut self, delta: Wheel) {
        let unit = self.overview_layout().unit();
        let dy = match delta {
            Wheel::Lines(_, y) => -y * WHEEL_UNITS * unit,
            Wheel::Pixels(_, y) => -y,
        };
        let over_toc = self.overview_layout().toc.is_some_and(|t| crate::ink::rect_contains(t, self.cursor));
        self.glide_overview_by(dy, over_toc);
    }

    /// The mouse left the window: nothing is under it.
    pub(in crate::editor) fn overview_pointer_left(&mut self) {
        if let Some(state) = &mut self.chrome.overview {
            state.hover = None;
            self.ws.redraw = true;
        }
    }

    /// The window lost the keyboard focus: a button let go of elsewhere never reaches us, so a press or a drag under way
    /// is given up.
    pub(in crate::editor) fn overview_gestures_lost(&mut self) {
        if let Some(state) = &mut self.chrome.overview {
            state.press = None;
            state.drag = None;
            state.hover = None;
            self.ws.redraw = true;
        }
    }

    /// A key while the overview is up. `true` if it was used.
    pub(in crate::editor) fn overview_key(&mut self, key: Key<'_>) -> bool {
        let layout = self.overview_layout();
        let page = layout.grid[3] - layout.grid[1];
        match key {
            Key::Escape => {
                // Out of what is under way, a step at a time: a drag, the pages chosen, edit mode, the overview.
                match &mut self.chrome.overview {
                    Some(state) if state.menu.is_some() => state.menu = None,
                    Some(state) if state.drag.is_some() => {
                        state.drag = None;
                        state.press = None;
                    }
                    Some(state) if !state.selected.is_empty() => state.selected.clear(),
                    Some(state) if state.edit => state.edit = false,
                    _ => self.close_overview(),
                }
                self.overview_settled();
            }
            Key::Delete => {
                if !self.apply_action(Action::Delete) {
                    return false;
                }
            }
            Key::Home(_) => self.glide_overview_by(f32::MIN / 2.0, false),
            Key::End(_) => self.glide_overview_by(f32::MAX / 2.0, false),
            Key::Arrow(dir, _) => {
                let step = match dir {
                    crate::tools::Dir::Up => -0.5,
                    crate::tools::Dir::Down => 0.5,
                    _ => return false,
                };
                self.glide_overview_by(step * page, false);
            }
            _ => return false,
        }
        self.ws.redraw = true;
        true
    }

    /// A shortcut with Ctrl while the overview is up (desktop mode): `a` chooses every page, `d` duplicates the chosen ones.
    /// Undo, redo and save are the document's, and are not asked of this.
    pub(in crate::editor) fn overview_shortcut(&mut self, letter: &str) -> bool {
        let done = match letter {
            "a" => {
                if let Some(state) = &mut self.chrome.overview {
                    state.edit = true;
                    state.filter = false;
                }
                if let Some(state) = &mut self.chrome.overview {
                    state.selected = (0..self.ws.layout.len()).collect();
                }
                true
            }
            "d" => self.apply_action(Action::Duplicate),
            _ => false,
        };
        self.overview_settled();
        self.ws.redraw = true;
        done
    }
}
