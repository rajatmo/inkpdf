//! A compositor of our own for the tests of the pen reader: it serves a surface and a tablet, and says what a pen would, over a socket pair. The reader
//! under test is the real one, on a real Wayland connection (`wayland-server` on the other end); what is not real is the pen, which is a script.

use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_compositor as client_compositor, wl_registry, wl_surface as client_surface};
use wayland_client::{Connection, Dispatch as ClientDispatch, Proxy as ClientProxy, QueueHandle, globals::GlobalListContents};
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_manager_v2::{self, ZwpTabletManagerV2};
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_seat_v2::{self, ZwpTabletSeatV2};
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_tool_v2::{self, ZwpTabletToolV2};
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_v2::{self, ZwpTabletV2};
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use wayland_server::protocol::{wl_compositor, wl_seat, wl_surface};
use wayland_server::{Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New, Resource};
use winit::raw_window_handle::{
    DisplayHandle as WinitDisplay, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle,
    WaylandWindowHandle, WindowHandle,
};

use super::super::super::PointerSource;
use crate::input::{self, Phase, PointerEvent};

/// What the scripted pen does, one thing at a time.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// A pen (`eraser: false`) or its eraser end comes into being, as a tool of the tablet.
    Appear {
        eraser: bool,
    },
    /// It comes near our window, or near a window that is not ours.
    NearOurs,
    NearOther,
    Away,
    Down,
    Up,
    Move(f64, f64),
    Pressure(u32),
    Side(bool),
    Frame(u32),
}

struct Clients;

impl ClientData for Clients {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
}

/// What the compositor holds: the surfaces clients made, and the tablet and tool it made for them.
#[derive(Default)]
struct Server {
    surfaces: Vec<wl_surface::WlSurface>,
    seat: Option<ZwpTabletSeatV2>,
    tablet: Option<ZwpTabletV2>,
    tool: Option<ZwpTabletToolV2>,
    serial: u32,
}

impl GlobalDispatch<wl_compositor::WlCompositor, ()> for Server {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<wl_compositor::WlCompositor>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wl_compositor::Request::CreateSurface { id } = request {
            state.surfaces.push(init.init(id, ()));
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for Server {
    fn request(_: &mut Self, _: &Client, _: &wl_surface::WlSurface, _: wl_surface::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl GlobalDispatch<wl_seat::WlSeat, ()> for Server {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<wl_seat::WlSeat>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Server {
    fn request(_: &mut Self, _: &Client, _: &wl_seat::WlSeat, _: wl_seat::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl GlobalDispatch<ZwpTabletManagerV2, ()> for Server {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<ZwpTabletManagerV2>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(resource, ());
    }
}

impl Dispatch<ZwpTabletManagerV2, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ZwpTabletManagerV2,
        request: zwp_tablet_manager_v2::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let zwp_tablet_manager_v2::Request::GetTabletSeat { tablet_seat, .. } = request {
            state.seat = Some(init.init(tablet_seat, ()));
        }
    }
}

impl Dispatch<ZwpTabletSeatV2, ()> for Server {
    fn request(_: &mut Self, _: &Client, _: &ZwpTabletSeatV2, _: zwp_tablet_seat_v2::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
    }
}

impl Dispatch<ZwpTabletV2, ()> for Server {
    fn request(_: &mut Self, _: &Client, _: &ZwpTabletV2, _: zwp_tablet_v2::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<ZwpTabletToolV2, ()> for Server {
    fn request(_: &mut Self, _: &Client, _: &ZwpTabletToolV2, _: zwp_tablet_tool_v2::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
    }
}

impl Server {
    /// Do one step of the script, as the compositor would tell it to the client.
    fn step(&mut self, dh: &DisplayHandle, step: Step, ours: usize, other: Option<&wl_surface::WlSurface>) {
        let serial = {
            self.serial += 1;
            self.serial
        };
        match step {
            Step::Appear { eraser } => {
                let seat = self.seat.clone().expect("the client asked for the tablet seat");
                let client = seat.client().expect("a client");
                let tablet = client.create_resource::<ZwpTabletV2, (), Server>(dh, 1, ()).expect("a tablet");
                seat.tablet_added(&tablet);
                tablet.done();
                let tool = client.create_resource::<ZwpTabletToolV2, (), Server>(dh, 1, ()).expect("a tool");
                seat.tool_added(&tool);
                tool._type(if eraser { zwp_tablet_tool_v2::Type::Eraser } else { zwp_tablet_tool_v2::Type::Pen });
                tool.done();
                self.tablet = Some(tablet);
                self.tool = Some(tool);
            }
            _ => {
                let (Some(tool), Some(tablet)) = (&self.tool, &self.tablet) else { return };
                match step {
                    Step::NearOurs => {
                        let surface = self.surfaces.iter().find(|s| s.id().protocol_id() as usize == ours).expect("our surface");
                        tool.proximity_in(serial, tablet, surface);
                    }
                    Step::NearOther => tool.proximity_in(serial, tablet, other.expect("another surface")),
                    Step::Away => tool.proximity_out(),
                    Step::Down => tool.down(serial),
                    Step::Up => tool.up(),
                    Step::Move(x, y) => tool.motion(x, y),
                    Step::Pressure(p) => tool.pressure(p),
                    Step::Side(on) => tool.button(
                        serial,
                        0x14b,
                        if on { zwp_tablet_tool_v2::ButtonState::Pressed } else { zwp_tablet_tool_v2::ButtonState::Released },
                    ),
                    Step::Frame(ms) => tool.frame(ms),
                    Step::Appear { .. } => {}
                }
            }
        }
    }
}

/// What the test's own client of the compositor holds: nothing but the surface it makes.
struct Window;

impl ClientDispatch<wl_registry::WlRegistry, GlobalListContents> for Window {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl ClientDispatch<client_compositor::WlCompositor, ()> for Window {
    fn event(_: &mut Self, _: &client_compositor::WlCompositor, _: client_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl ClientDispatch<client_surface::WlSurface, ()> for Window {
    fn event(_: &mut Self, _: &client_surface::WlSurface, _: client_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

/// The handles of a window, as winit would give them: the display the program is connected with and the surface it draws on.
struct Handles {
    display: std::ptr::NonNull<std::ffi::c_void>,
    surface: std::ptr::NonNull<std::ffi::c_void>,
}

impl HasDisplayHandle for Handles {
    fn display_handle(&self) -> Result<WinitDisplay<'_>, HandleError> {
        // SAFETY: the pointer is the live `wl_display` of the test's connection, which outlives this.
        Ok(unsafe { WinitDisplay::borrow_raw(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(self.display))) })
    }
}

impl HasWindowHandle for Handles {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: as above, for the surface.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Wayland(WaylandWindowHandle::new(self.surface))) })
    }
}

/// A compositor running on its own thread, a window of ours on it, and the reader made for that window.
struct Rig {
    steps: Sender<Step>,
    source: Box<dyn PointerSource>,
    stop: Arc<Mutex<bool>>,
    server: Option<std::thread::JoinHandle<()>>,
    /// (Kept so that the connection, and with it the display the reader borrows, stays up.)
    _conn: Connection,
}

impl Rig {
    fn new() -> Rig {
        let (client_end, server_end) = UnixStream::pair().expect("a socket pair");
        let (steps, wanted): (Sender<Step>, Receiver<Step>) = channel();
        let stop = Arc::new(Mutex::new(false));
        let (told_surface, surface_told) = channel::<usize>();
        let stopping = stop.clone();
        let server = std::thread::spawn(move || {
            let mut display: Display<Server> = Display::new().expect("a display");
            let mut dh = display.handle();
            dh.create_global::<Server, wl_compositor::WlCompositor, ()>(1, ());
            dh.create_global::<Server, wl_seat::WlSeat, ()>(1, ());
            dh.create_global::<Server, ZwpTabletManagerV2, ()>(1, ());
            dh.insert_client(server_end, Arc::new(Clients)).expect("the client");
            let mut state = Server::default();
            let mut ours = None;
            while !*stopping.lock().expect("lock") {
                display.dispatch_clients(&mut state).expect("dispatch");
                if ours.is_none()
                    && let Some(s) = state.surfaces.first()
                {
                    ours = Some(s.id().protocol_id() as usize);
                    told_surface.send(ours.expect("set")).ok();
                }
                while let Ok(step) = wanted.try_recv() {
                    // (A second surface, for a pen that is over some other window.)
                    let other = state.surfaces.get(1).cloned();
                    state.step(&dh, step, ours.unwrap_or(0), other.as_ref());
                }
                display.flush_clients().expect("flush");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let conn = Connection::from_socket(client_end).expect("a connection");
        let (globals, mut queue) = wayland_client::globals::registry_queue_init::<Window>(&conn).expect("globals");
        let qh = queue.handle();
        let compositor = globals.bind::<client_compositor::WlCompositor, _, _>(&qh, 1..=1, ()).expect("a compositor");
        let surface = compositor.create_surface(&qh, ());
        let other = compositor.create_surface(&qh, ());
        let mut window = Window;
        queue.roundtrip(&mut window).expect("a roundtrip");
        // (The server knows our surface by its protocol id, the reader by the client side's address: the same object seen from the two ends.)
        surface_told.recv_timeout(Duration::from_secs(5)).expect("the surface");
        let handles = Handles {
            display: std::ptr::NonNull::new(conn.backend().display_ptr().cast()).expect("a display"),
            surface: std::ptr::NonNull::new(surface.id().as_ptr().cast()).expect("a surface"),
        };
        let source = super::install(&handles).expect("the reader binds the tablet protocol");
        // (The test's own queue is not read again: the objects it made are only there to be named.)
        std::mem::forget(queue);
        std::mem::forget(surface);
        std::mem::forget(other);
        Rig { steps, source, stop, server: Some(server), _conn: conn }
    }

    fn say(&mut self, steps: &[Step]) {
        for step in steps {
            self.steps.send(*step).expect("the compositor is there");
        }
    }

    /// Pump the reader until it has events, or a moment has gone by with none.
    fn events(&mut self) -> Vec<PointerEvent> {
        let until = Instant::now() + Duration::from_millis(600);
        let mut out = Vec::new();
        loop {
            self.source.pump();
            self.source.drain_into(&mut out);
            if (!out.is_empty() && !self.source.pending()) || Instant::now() > until {
                return out;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        *self.stop.lock().expect("lock") = true;
        if let Some(server) = self.server.take() {
            server.join().ok();
        }
    }
}

#[test]
fn a_scripted_pen_session_through_a_real_connection_comes_out_as_hover_press_moves_lift_and_leave() {
    let mut rig = Rig::new();
    rig.source.set_scale(2.0);
    rig.say(&[
        Step::Appear { eraser: false },
        Step::NearOurs,
        Step::Move(10.0, 20.0),
        Step::Frame(100),
        Step::Down,
        Step::Pressure(32_768),
        Step::Move(11.0, 21.0),
        Step::Frame(108),
        Step::Move(12.0, 22.0),
        Step::Pressure(65_535),
        Step::Frame(116),
        Step::Up,
        Step::Frame(124),
        Step::Away,
        Step::Frame(132),
    ]);
    let mut events = Vec::new();
    while events.len() < 5 {
        let more = rig.events();
        if more.is_empty() {
            break;
        }
        events.extend(more);
    }
    let phases: Vec<Phase> = events.iter().map(|e| e.phase).collect();
    assert_eq!(phases, [Phase::Hover, Phase::Down, Phase::Move, Phase::Up, Phase::Leave], "{events:?}");
    assert_eq!(events[0].samples[0].pos, [20.0, 40.0], "the compositor's logical pixels times the window's scale");
    assert!((events[1].samples[0].pressure - 0.5).abs() < 1e-3 && events[1].pen.contact);
    assert_eq!(events[2].samples[0].pressure, 1.0);
    assert_eq!(input::check(&events), Ok(()), "and it is a stream that keeps the rules of a stream");
}

#[test]
fn the_eraser_end_the_side_button_and_a_pen_over_another_window_are_told_apart() {
    let mut rig = Rig::new();
    rig.say(&[
        Step::Appear { eraser: true },
        // (Over another window first: nothing of it is ours to read.)
        Step::NearOther,
        Step::Move(1.0, 1.0),
        Step::Frame(5),
        Step::Down,
        Step::Frame(6),
        Step::Up,
        Step::Away,
        Step::Frame(7),
        Step::NearOurs,
        Step::Move(5.0, 5.0),
        Step::Frame(10),
        Step::Down,
        Step::Side(true),
        Step::Frame(20),
        Step::Up,
        Step::Frame(30),
        Step::Away,
        Step::Frame(40),
    ]);
    let mut events = Vec::new();
    for _ in 0..4 {
        events.extend(rig.events());
        if events.len() >= 4 {
            break;
        }
    }
    assert!(events.iter().take(3).all(|e| e.pen.eraser), "the eraser end: {events:?}");
    assert!(events.iter().any(|e| e.pen.barrel && e.phase == Phase::Move || e.pen.barrel && e.phase == Phase::Down), "the side button: {events:?}");
    assert_eq!(input::check(&events), Ok(()));
}
