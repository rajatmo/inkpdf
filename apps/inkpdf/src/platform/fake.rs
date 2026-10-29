//! A pen and touch source that is scripted: what a test "does to the screen" is put in its OS queue with [`Injector`], and the
//! source brings it in as a real one does (on [`PointerSource::pump`], a bounded number at a time). It exists to hold the contract
//! (see `contract.rs`) and to feed the editor events as a platform would, with no platform.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use super::{MAX_EVENTS_PER_PUMP, PointerSource};
use crate::input::PointerEvent;

/// What the OS has queued and the source has not yet taken.
#[derive(Default)]
struct Os {
    queue: VecDeque<PointerEvent>,
    posture_changed: bool,
}

/// How a test touches the "screen": it puts events in the OS queue.
#[derive(Clone, Default)]
pub struct Injector(Rc<RefCell<Os>>);

impl Injector {
    pub fn push_all(&self, events: impl IntoIterator<Item = PointerEvent>) {
        self.0.borrow_mut().queue.extend(events);
    }

    /// The machine was turned into a tablet or back.
    pub fn posture_changed(&self) {
        self.0.borrow_mut().posture_changed = true;
    }

    /// How many events the OS holds that the source has not taken.
    pub fn waiting(&self) -> usize {
        self.0.borrow().queue.len()
    }
}

/// The source: what is taken from the OS waits to be drained.
pub struct FakeSource {
    os: Rc<RefCell<Os>>,
    ready: Vec<PointerEvent>,
    installed: bool,
}

impl FakeSource {
    /// A source and the way to touch its screen.
    pub fn new() -> (Self, Injector) {
        let injector = Injector::default();
        (Self { os: injector.0.clone(), ready: Vec::new(), installed: true }, injector)
    }
}

impl PointerSource for FakeSource {
    fn pump(&mut self) {
        if !self.installed {
            return;
        }
        let mut os = self.os.borrow_mut();
        let take = os.queue.len().min(MAX_EVENTS_PER_PUMP);
        self.ready.extend(os.queue.drain(..take));
    }

    fn pending(&self) -> bool {
        !self.ready.is_empty()
    }

    fn drain(&mut self) -> Vec<PointerEvent> {
        std::mem::take(&mut self.ready)
    }

    fn take_posture_change(&mut self) -> bool {
        std::mem::take(&mut self.os.borrow_mut().posture_changed)
    }

    fn uninstall(&mut self) {
        self.installed = false;
        self.ready.clear();
        self.os.borrow_mut().queue.clear();
    }
}
