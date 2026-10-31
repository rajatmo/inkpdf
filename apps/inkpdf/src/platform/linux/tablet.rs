//! The pen on Wayland: pressure, the eraser end, the side buttons, hover and the points between frames, read from the compositor's tablet protocol
//! (`zwp_tablet_v2`) on the connection winit already has, by a second event queue of our own, so that the pen is read where the program asks for it
//! (right after the display wait, as `platform/windows` reads its message queue) and not when the toolkit's loop gets to it. What the compositor says is
//! handed to [`Frames`](super::super::tablet_frames::Frames), which makes the program's pointer events of it (and is tested without a compositor); this
//! file only binds the protocol, and says so when there is nothing to bind (an X11 session, a compositor with no tablet support): the pen is then read as
//! the mouse, as it was.
//!
//! A touch screen and a mouse are still winit's: the touches it reports are handed through (`late_touch`), the mouse never came this way.
//!
//! **Not tried against a compositor with a tablet on it** (there is no pen hardware where this was written): the translation is tested with scripted
//! frames and the binding is checked to compile and to refuse cleanly; a person with a Wayland tablet is the test of the rest.

use std::collections::HashSet;

use wayland_backend::client::{Backend, ObjectId, WaylandError};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_manager_v2::ZwpTabletManagerV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::{self, ZwpTabletPadV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_seat_v2::{self, ZwpTabletSeatV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_tool_v2::{self, ZwpTabletToolV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_v2::ZwpTabletV2;
use winit::raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use super::super::tablet_frames::{Frames, Raw};
use super::super::{PointerSource, WindowHandles};
use crate::input::PointerEvent;

/// What the protocol's events are handled into: the translator, and which tools are in range over our window.
struct State {
    frames: Frames,
    /// Our window's surface (its address), so that only a tool over our window is read.
    ours: usize,
    /// The tools that are the eraser end of a pen.
    erasers: HashSet<ObjectId>,
    /// The tools that are over our window, and those of them that have said they went (their last frame is still to come).
    over: HashSet<ObjectId>,
    leaving: HashSet<ObjectId>,
}

impl State {
    fn raw(&mut self, raw: Raw) {
        self.frames.raw(raw, crate::clock::now_ms());
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletManagerV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletManagerV2, _: <ZwpTabletManagerV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletSeatV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletSeatV2, _: zwp_tablet_seat_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    // (The seat makes an object for each tablet, tool and pad it tells of: each needs a place to send its events, even the ones nobody reads.)
    event_created_child!(State, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletV2, _: <ZwpTabletV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletPadV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadV2, _: zwp_tablet_pad_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(State, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadGroupV2, _: zwp_tablet_pad_group_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(State, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadRingV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadRingV2, _: <ZwpTabletPadRingV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletPadStripV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadStripV2, _: <ZwpTabletPadStripV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletToolV2, ()> for State {
    fn event(state: &mut Self, tool: &ZwpTabletToolV2, event: zwp_tablet_tool_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwp_tablet_tool_v2::Event;
        let id = tool.id();
        match event {
            Event::Type { tool_type } => {
                if matches!(tool_type, WEnum::Value(zwp_tablet_tool_v2::Type::Eraser)) {
                    state.erasers.insert(id);
                } else {
                    state.erasers.remove(&id);
                }
            }
            Event::ProximityIn { surface, .. } => {
                // (Only a tool over our own window is read: the compositor tells every client that listens.)
                if surface.id().as_ptr() as usize == state.ours {
                    state.over.insert(id.clone());
                    let eraser = state.erasers.contains(&id);
                    state.raw(Raw::ProximityIn { eraser });
                }
            }
            Event::ProximityOut if state.over.contains(&id) => {
                state.leaving.insert(id);
                state.raw(Raw::ProximityOut);
            }
            Event::Removed if state.over.remove(&id) => {
                state.leaving.remove(&id);
                state.raw(Raw::ProximityOut);
                state.raw(Raw::Frame { ms: 0 });
                state.erasers.remove(&id);
            }
            _ if !state.over.contains(&id) => {}
            Event::Down { .. } => state.raw(Raw::Down),
            Event::Up => state.raw(Raw::Up),
            Event::Motion { x, y } => state.raw(Raw::Motion { x: x as f32, y: y as f32 }),
            Event::Pressure { pressure } => state.raw(Raw::Pressure(pressure)),
            Event::Button { button, state: pressed, .. } => {
                state.raw(Raw::Button { code: button, pressed: matches!(pressed, WEnum::Value(zwp_tablet_tool_v2::ButtonState::Pressed)) });
            }
            Event::Frame { time } => {
                state.raw(Raw::Frame { ms: time });
                if state.leaving.remove(&id) {
                    state.over.remove(&id);
                }
            }
            _ => {}
        }
    }
}

/// The pen, read from the compositor.
pub struct TabletSource {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    /// Touches winit reported, handed through (it still reads the touch screen).
    touches: Vec<PointerEvent>,
}

/// Bind the tablet protocol on the connection of `window`. An `Err` says why not (and is not fatal: the pen is the mouse then).
pub fn install(window: &dyn WindowHandles) -> Result<Box<dyn PointerSource>, String> {
    let display = match window.display_handle().map_err(|e| e.to_string())?.as_raw() {
        RawDisplayHandle::Wayland(h) => h.display,
        _ => return Err("this is not a Wayland session: the pen is read as the mouse".into()),
    };
    let surface = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Wayland(h) => h.surface,
        _ => return Err("this window is not a Wayland surface".into()),
    };
    // SAFETY: `display` is the `wl_display` winit connected to the compositor with, which lives as long as the window does (and so as long as this
    // source, which the shell drops first); `from_foreign_display` only borrows it: the connection is not closed when this one is dropped.
    let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("could not list what the compositor offers: {e}"))?;
    let qh = queue.handle();
    let manager = globals
        .bind::<ZwpTabletManagerV2, _, _>(&qh, 1..=1, ())
        .map_err(|_| "the compositor has no tablet protocol: the pen is read as the mouse".to_string())?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=1, ()).map_err(|_| "the compositor has no seat".to_string())?;
    // (What this asks for is told in the events that follow: the tablets, tools and pads there are.)
    let _tablet_seat = manager.get_tablet_seat(&seat, &qh, ());
    let mut state =
        State { frames: Frames::new(1.0), ours: surface.as_ptr() as usize, erasers: HashSet::new(), over: HashSet::new(), leaving: HashSet::new() };
    queue.roundtrip(&mut state).map_err(|e| format!("the compositor did not answer the tablet protocol: {e}"))?;
    Ok(Box::new(TabletSource { conn, queue, state, touches: Vec::new() }))
}

impl PointerSource for TabletSource {
    fn pump(&mut self) {
        let _ = self.conn.flush();
        // (What winit read from the shared socket for our objects is in our queue already; this reads what has come since, without waiting.)
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => eprintln!("could not read the pen from the compositor: {e}"),
            }
        }
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            eprintln!("the compositor's pen events could not be handled: {e}");
        }
    }

    fn pending(&self) -> bool {
        // (A queue with events in it cannot be prepared to read: that is how to ask, without taking them.)
        self.state.frames.pending() || !self.touches.is_empty() || self.queue.prepare_read().is_none()
    }

    fn drain_into(&mut self, out: &mut Vec<PointerEvent>) {
        self.state.frames.drain_into(out);
        out.append(&mut self.touches);
    }

    fn take_posture_change(&mut self) -> bool {
        false
    }

    fn late_touch(&mut self, ev: PointerEvent) {
        self.touches.push(ev);
    }

    fn set_scale(&mut self, scale: f32) {
        self.state.frames.set_scale(scale);
    }

    fn uninstall(&mut self) {
        self.state.frames.finish(crate::clock::now_ms());
    }
}

#[cfg(test)]
mod fake_compositor;
