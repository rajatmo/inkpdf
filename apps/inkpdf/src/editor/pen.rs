//! The stylus on the window: its presses on the toolbars and the tool panel. On the page it is the tool's
//! (see [`super::routing`]).

use crate::input::{Device, Phase, PointerEvent};

use super::{Act, Editor};

impl Editor {
    pub(super) fn pen_event(&mut self, ev: &PointerEvent) {
        if ev.pen.contact {
            self.pending_sample_ms = ev.samples.last().map(|s| s.t_ms);
        }
        match ev.phase {
            Phase::Down => self.pen_down(ev),
            Phase::Move | Phase::Up | Phase::Cancel if !self.act_belongs_to(Device::Pen, ev.id) => {
                // Not the window's own (a toolbar press): the tool in use may have it.
                self.route_to_tool(Device::Pen, ev.id, ev.phase, &ev.samples, ev.pen);
            }
            Phase::Move => self.pointer_moved(Device::Pen, ev.id, &ev.samples),
            Phase::Up | Phase::Cancel => self.pen_up(ev),
            Phase::Hover => self.route_hover(ev.samples.last().map(|s| s.pos)),
            Phase::Leave => self.route_hover(None),
        }
    }

    pub(super) fn pen_down(&mut self, ev: &PointerEvent) {
        if !matches!(self.chrome.act, Act::Idle) {
            self.end_act();
        }
        self.tools.end_all(&mut self.ws);
        let Some(first) = ev.samples.first() else { return };
        let pos = first.pos;
        self.dismiss_theme_menu(pos);
        self.ws.view.stop();
        if self.press_panel(Device::Pen, ev.id, pos) {
            return;
        }
        let bar = self.toolbar();
        if bar.contains(pos) {
            self.chrome.act = Act::Toolbar { dev: Device::Pen, id: ev.id, item: bar.hit(pos) };
            return;
        }
        if self.press_context(Device::Pen, ev.id, pos) {
            return;
        }
        if !self.ws.has_doc {
            return;
        }
        // On the page the tool in use takes the press (or, by its policy, ignores the pen).
        self.route_to_tool(Device::Pen, ev.id, Phase::Down, &ev.samples, ev.pen);
    }

    pub(super) fn pen_up(&mut self, ev: &PointerEvent) {
        if !self.act_belongs_to(Device::Pen, ev.id) {
            return;
        }
        self.pointer_moved(Device::Pen, ev.id, &ev.samples);
        let pos = ev.samples.last().map(|s| s.pos);
        self.release_toolbar(Device::Pen, ev.id, pos);
        self.release_context(Device::Pen, ev.id, pos);
        self.end_act();
    }
}
