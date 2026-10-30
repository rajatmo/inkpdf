//! The lasso and the eraser: the two tools that make nothing. The lasso's tip loops (or taps) to select items of any kind, and drags what is
//! selected where a press lands on it; the eraser's tip erases whatever it passes over. Both are the same machinery as the pen's barrel button and
//! eraser end (see [`Pick`]), which still do the same in any other tool, and `Workspace::role` says which of the two a press is.
//! The mouse scrolls in the tablet mode, a finger pans and zooms, and at a desk the keyboard works on the selection (Delete, Esc, the arrows).

use crate::input::{Device, Mode, Phase};
use crate::workspace::Workspace;

use super::gestures::Pick;
use super::paint::Painter;
use super::shortcuts::selection_key;
use super::{InputPolicy, Key, PageEvent, Tool};

#[derive(Default)]
enum Gesture {
    #[default]
    None,
    /// Looping, erasing or dragging the selection.
    Pick(Pick),
    /// A press that only let go of the selection; it does nothing else.
    Swallow(u32),
}

#[derive(Default)]
pub struct PickTool {
    gesture: Gesture,
}

impl PickTool {
    fn id(&self) -> Option<u32> {
        match &self.gesture {
            Gesture::None => None,
            Gesture::Pick(p) => Some(p.id()),
            Gesture::Swallow(id) => Some(*id),
        }
    }

    fn down(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        let Some(first) = ev.samples.first() else { return };
        let p = ws.to_doc(first.pos);
        let on_selection = ws.selection_hit(p);
        self.gesture = match Pick::begin(ws, ev, p, on_selection) {
            Some(pick) => Gesture::Pick(pick),
            None => Gesture::Swallow(ev.id),
        };
    }
}

impl Tool for PickTool {
    fn policy(&self, mode: Mode) -> InputPolicy {
        InputPolicy::editing_tool(mode)
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        selection_key(ws, key)
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        match ev.phase {
            Phase::Down => {
                self.end(ws);
                self.down(ws, ev);
                ws.redraw = true;
            }
            Phase::Move if self.owns(ev.dev, ev.id) => {
                if let Gesture::Pick(p) = &mut self.gesture {
                    p.moved(ws, ev.samples);
                }
                ws.redraw = true;
            }
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                if let Gesture::Pick(p) = &mut self.gesture {
                    p.moved(ws, ev.samples);
                }
                self.end(ws);
            }
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        dev == Device::Pen && self.id() == Some(id)
    }

    fn busy(&self) -> bool {
        self.id().is_some()
    }

    fn end(&mut self, ws: &mut Workspace) {
        match std::mem::take(&mut self.gesture) {
            Gesture::None | Gesture::Swallow(_) => return,
            Gesture::Pick(p) => p.end(ws, |_, _| false),
        }
        ws.redraw = true;
    }

    fn cancel(&mut self, ws: &mut Workspace) {
        // What the eraser had touched is not erased.
        ws.erasing.clear();
        self.gesture = Gesture::None;
    }

    fn drag_offset(&self) -> [f32; 2] {
        match &self.gesture {
            Gesture::Pick(p) => p.offset(),
            _ => [0.0; 2],
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        if let Gesture::Pick(p) = &self.gesture {
            p.paint(ws, out);
        }
    }
}
