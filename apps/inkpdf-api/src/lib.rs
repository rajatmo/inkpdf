//! The extension API of InkPDF: everything an extension may use of the program, and nothing else.
//!
//! An extension is a tool: it is handed what the pen, the mouse and the keyboard do on the page, may draw over the page, and says whether it is
//! still moving. The program implements [`Context`] for it (what the tool may ask of the document and the window) and calls its
//! [`Tool`]; the extension depends on this crate and on nothing of the program, so it cannot reach into the program's insides.
//!
//! The API is versioned ([`API_VERSION`], semantic versioning): an extension names the version it was written for in its
//! [`Extension`], and the program refuses one it cannot serve. Breaking this crate breaks every extension, so it changes rarely and
//! by adding. Everything here is plain data (no borrowed state is kept between calls) so that the same API could one day be served to
//! an extension that runs somewhere else.

#![forbid(unsafe_code)]

/// The version of this API. An extension's [`Extension::api`] must equal the major version the program serves.
pub const API_VERSION: u32 = 1;

/// A point, `[x, y]`: in document space (points, the pages stacked down from the top) or on the screen (pixels), as said where it is used.
pub type P = [f32; 2];

/// An RGBA colour, straight (not premultiplied).
pub type Rgba = [u8; 4];

/// What gave a pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    /// A mouse (in the desktop mode a tool is handed it as a stylus: see [`MouseUse`]).
    Mouse,
    /// A finger. Fingers pan and zoom the page and never reach a tool.
    Touch,
    /// A stylus.
    Pen,
}

/// Where in its life a pointer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Pressed: the tip touched, or a button went down.
    Down,
    /// Moved while pressed.
    Move,
    /// Lifted.
    Up,
    /// The system took it away (a palm, a call): nothing it did is to be kept.
    Cancel,
    /// Moved above the screen (a stylus in range), or the mouse with no button down.
    Hover,
    /// Gone from above the screen.
    Leave,
}

/// One position of a pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// On the screen, in pixels.
    pub pos: P,
    /// 0 to 1.
    pub pressure: f32,
    /// Milliseconds on a clock that only goes forward.
    pub t_ms: f64,
}

/// What a stylus is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PenButtons {
    /// Near enough to the screen to be seen.
    pub in_range: bool,
    /// Touching it.
    pub contact: bool,
    /// The eraser end is the one in use.
    pub eraser: bool,
    /// The side button is held.
    pub barrel: bool,
}

/// A stylus or mouse message on the page.
#[derive(Clone, Copy, Debug)]
pub struct PointerEvent<'a> {
    /// What gave it (always a stylus or a mouse).
    pub device: Device,
    /// Which pointer, for a device that has several.
    pub id: u32,
    /// Where in its life it is.
    pub phase: Phase,
    /// The positions since the last event, oldest first (a stylus sends several at a time).
    pub samples: &'a [Sample],
    /// What the stylus is doing.
    pub pen: PenButtons,
}

/// A direction of an arrow key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Left.
    Left,
    /// Right.
    Right,
    /// Up.
    Up,
    /// Down.
    Down,
}

/// A key, if the tool's [`Policy`] lets the keyboard through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    /// Characters typed (a new line is `"\n"`).
    Text(&'a str),
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Escape.
    Escape,
    /// An arrow, with whether Shift is held.
    Arrow(Dir, bool),
    /// Home, with whether Shift is held.
    Home(bool),
    /// End, with whether Shift is held.
    End(bool),
    /// A letter pressed with Ctrl.
    Chord(char),
}

/// What the mouse's buttons do on the page with a tool. (The wheel always scrolls.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseUse {
    /// Nothing: the mouse only scrolls.
    Scrolls,
    /// Its presses reach the tool as a mouse's.
    Itself,
    /// It stands in for a stylus: the left button is the tip, the right the eraser end, the middle the barrel button.
    AsStylus,
}

/// Which devices a tool listens to on the page. A finger never reaches a tool; every device can still press the buttons of the toolbars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// The stylus: tip, eraser end and barrel button.
    pub stylus: bool,
    /// The mouse.
    pub mouse: MouseUse,
    /// The keyboard.
    pub keyboard: bool,
}

impl Policy {
    /// A tool the stylus drives: with the desktop mode's mouse it can too, a tablet's mouse only scrolls; no keyboard.
    pub fn stylus_tool(desktop: bool) -> Policy {
        Policy { stylus: true, mouse: if desktop { MouseUse::AsStylus } else { MouseUse::Scrolls }, keyboard: false }
    }
}

/// What a tool may draw over the page. Lines are round-ended capsules; a dot is a line from a point to itself.
pub trait Painter {
    /// A line from `a` to `b` in document space, `width` points thick.
    fn doc_line(&mut self, a: P, b: P, width: f32, rgba: Rgba);
    /// A line in screen pixels.
    fn screen_line(&mut self, a: P, b: P, width: f32, rgba: Rgba);
    /// Milliseconds on the clock the pen's samples are stamped with (for what fades).
    fn now_ms(&self) -> f64;
}

/// The colours of the interface now (they follow the look: light or dark), for a tool that wants to match it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Icons and text-weight lines.
    pub ink: Rgba,
    /// The one accent.
    pub accent: Rgba,
    /// What shows a selection on the page.
    pub selection: Rgba,
    /// What can destroy.
    pub danger: Rgba,
    /// A mark on a coloured dot: white in every look.
    pub white: Rgba,
}

/// What a tool may ask of the document and the window, on every call it is made.
pub trait Context {
    /// A point of the screen (pixels) as a point of the document.
    fn to_doc(&self, screen: P) -> P;
    /// A point of the document as a point of the screen.
    fn doc_to_screen(&self, doc: P) -> P;
    /// Screen pixels per document point.
    fn zoom(&self) -> f32;
    /// The scale of the screen (1 at 96 dpi).
    fn dpr(&self) -> f32;
    /// Whether a document is open (with none there is no page to point at).
    fn has_document(&self) -> bool;
    /// Draw the next frame (what a tool shows over the page changed).
    fn request_redraw(&mut self);
    /// The colours of the interface now.
    fn theme(&self) -> Theme;
}

/// A tool. All of its state is its own. Every method has the context of the moment; none may keep it.
pub trait Tool {
    /// Which devices the tool listens to; `desktop` says which mode the machine is in.
    fn policy(&self, desktop: bool) -> Policy;

    /// A stylus or mouse message on the page (only ones the policy lets through).
    fn pointer(&mut self, cx: &mut dyn Context, ev: &PointerEvent<'_>);

    /// Whether a gesture of this pointer is under way (so that its moves and release are the tool's).
    fn owns(&self, device: Device, id: u32) -> bool;

    /// Whether any gesture, or work not yet made part of the document, is under way.
    fn busy(&self) -> bool;

    /// A press elsewhere, or the pen lifting: finish the gesture under way.
    fn end(&mut self, cx: &mut dyn Context);

    /// The gesture under way is called off without being kept (a prompt came up, a palm).
    fn cancel(&mut self, cx: &mut dyn Context);

    /// The tool is being put away: leave nothing of its own to surface later.
    fn leave(&mut self, cx: &mut dyn Context) {
        self.end(cx);
    }

    /// The keyboard, if the policy lets it through. `true` if the key was used.
    fn key(&mut self, _cx: &mut dyn Context, _key: Key<'_>) -> bool {
        false
    }

    /// Time has passed (called every frame, whatever tool is in use): `true` while the tool is still moving by itself (a trail fading),
    /// which keeps the frames coming; an idle window asks for none.
    fn tick(&mut self, _now_ms: f64) -> bool {
        false
    }

    /// The pages were edited (added, deleted, moved): what the tool holds in document space may no longer be where it was.
    fn pages_edited(&mut self, _cx: &mut dyn Context) {}

    /// What the tool shows over the page: its drafts, trails, handles.
    fn paint(&self, _cx: &dyn Context, _out: &mut dyn Painter) {}
}

/// What an extension is: the name the program knows it by, what its toolbar button shows, and how to make it.
#[derive(Clone, Copy)]
pub struct Extension {
    /// The version of this API it was written for ([`API_VERSION`]).
    pub api: u32,
    /// Its name: lower case letters, digits and `-`, unique among the tools. It is what the settings file remembers the tool by.
    pub id: &'static str,
    /// The name a person sees.
    pub label: &'static str,
    /// The icon of its button: an SVG made of stroked paths in the style of the others (see `docs/EXTENSIONS.md`).
    pub icon_svg: &'static str,
    /// Make the tool (called once, when the window starts).
    pub create: fn() -> Box<dyn Tool>,
}

impl Extension {
    /// Whether the program can serve this extension, and if not why not (every extension is checked by a test).
    pub fn problem(&self) -> Option<String> {
        if self.api != API_VERSION {
            return Some(format!("{} was written for API {}, this is {}", self.id, self.api, API_VERSION));
        }
        let well_named = !self.id.is_empty()
            && self.id.len() <= 24
            && self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && self.id.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        if !well_named {
            return Some(format!("the name {:?} is not lower case letters, digits and '-' (24 at most, beginning with a letter)", self.id));
        }
        if self.label.trim().is_empty() {
            return Some(format!("{} has no label", self.id));
        }
        if !self.icon_svg.contains("<path") {
            return Some(format!("{} has no icon (an SVG of paths)", self.id));
        }
        None
    }
}
