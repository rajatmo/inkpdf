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

    fn drain_into(&mut self, out: &mut Vec<PointerEvent>) {
        out.append(&mut self.ready);
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

/// A whole platform that does nothing real: folders are fixed, the "pen" is a [`FakeSource`], and what the shell tells the
/// user is kept. With it the shell's platform calls run in a test with no OS and no window.
pub struct FakePlatform {
    pub injector: Injector,
    pub shown: RefCell<Vec<String>>,
    source: RefCell<Option<FakeSource>>,
}

impl FakePlatform {
    pub fn new() -> Self {
        let (source, injector) = FakeSource::new();
        Self { injector, shown: RefCell::new(Vec::new()), source: RefCell::new(Some(source)) }
    }
}

impl super::Platform for FakePlatform {
    fn documents_dir(&self) -> std::path::PathBuf {
        "/fake/Documents".into()
    }

    fn config_dir(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn detect_mode(&self) -> crate::input::Mode {
        crate::input::Mode::Tablet
    }

    fn motion_prefs(&self) -> crate::motion::Prefs {
        crate::motion::Prefs::FULL
    }

    fn file_store(&self) -> std::sync::Arc<dyn crate::store::FileStore> {
        std::sync::Arc::new(crate::path_store::PathStore)
    }

    fn color_scheme(&self) -> Option<crate::palette::Scheme> {
        None
    }

    fn system_font(&self) -> Option<Vec<u8>> {
        None
    }

    fn open_url(&self, url: &str) {
        self.shown.borrow_mut().push(format!("open {url}"));
    }

    fn error_box(&self, _window: &dyn winit::raw_window_handle::HasWindowHandle, text: &str) {
        self.shown.borrow_mut().push(text.to_string());
    }

    fn set_text_input(&self, _active: bool) {}

    fn clipboard(&self, _window: &dyn winit::raw_window_handle::HasWindowHandle) -> Option<Box<dyn crate::clipboard::TextClipboard>> {
        None
    }

    fn install_pointer(&self, _window: &dyn winit::raw_window_handle::HasWindowHandle, wake: super::Waker) -> Result<Box<dyn PointerSource>, String> {
        wake();
        self.source.borrow_mut().take().map(|s| Box::new(s) as Box<dyn PointerSource>).ok_or_else(|| "already installed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle, WebWindowHandle, WindowHandle};

    use super::*;
    use crate::platform::Platform;

    /// A window handle that points at nothing: the fake platform never looks.
    struct NoWindow;

    impl HasWindowHandle for NoWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, winit::raw_window_handle::HandleError> {
            // SAFETY: a web handle is only an id; nothing dereferences it.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Web(WebWindowHandle::new(1))) })
        }
    }

    #[test]
    fn the_shell_can_be_given_a_platform_that_is_not_an_os() {
        let fake = FakePlatform::new();
        let injector = fake.injector.clone();
        let platform: Box<dyn Platform> = Box::new(fake);
        let woken = Arc::new(AtomicUsize::new(0));
        let count = woken.clone();
        let mut pointer = platform
            .install_pointer(
                &NoWindow,
                Arc::new(move || {
                    count.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .expect("a source");
        assert_eq!(woken.load(Ordering::Relaxed), 1, "it can wake the loop");
        assert!(!pointer.pending() && injector.waiting() == 0);
        pointer.uninstall();
        assert!(platform.install_pointer(&NoWindow, Arc::new(|| {})).is_err(), "and there is one reader");
        platform.error_box(&NoWindow, "could not save");
        assert_eq!(platform.documents_dir(), std::path::PathBuf::from("/fake/Documents"));
    }
}
