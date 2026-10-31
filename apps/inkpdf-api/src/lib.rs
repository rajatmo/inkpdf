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

/// The name of an item in the drawing, for as long as the document is open.
pub type ItemId = u64;

/// One message of a thread: who said it, when (a date in the file's own form, as [`Who::date`] gives it), and what.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    /// Who wrote it.
    pub author: String,
    /// When, as the file writes a date.
    pub date: String,
    /// What was written.
    pub text: String,
}

/// An item as a PDF has it: a note, which is a text annotation (its contents, who wrote it and when) with replies to it. Every reader shows one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    /// Who made it.
    pub author: String,
    /// When, as the file writes a date.
    pub date: String,
    /// Its contents.
    pub text: String,
    /// The replies, oldest first.
    pub replies: Vec<Message>,
    /// Whether a person has marked the thread done (a reader shows it as "Completed").
    pub closed: bool,
}

/// What an [`ItemKind`] is asked to draw: one item of its kind.
#[derive(Clone, Copy, Debug)]
pub struct ItemView<'a> {
    /// What the item holds (see [`ItemKind`]).
    pub data: &'a str,
    /// Where it is, `[x0, y0, x1, y1]` in document space.
    pub bounds: [f32; 4],
    /// Its colour, which a person can change like any item's.
    pub colour: Rgba,
    /// Whether it is selected now.
    pub selected: bool,
}

/// Who is acting, and when: given to an item kind when a person writes in its card.
#[derive(Clone, Copy, Debug)]
pub struct Who<'a> {
    /// The name written on what they add.
    pub author: &'a str,
    /// Now, as the file writes a date.
    pub date: &'a str,
}

/// One row of a [`Card`]: a line of small print over some text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardRow {
    /// The small line (who and when).
    pub head: String,
    /// The text.
    pub body: String,
}

/// A button of a [`Card`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardAction {
    /// What [`ItemKind::act`] is told when it is pressed.
    pub id: &'static str,
    /// What it says.
    pub label: String,
}

/// What the program shows when a person opens an item: rows of text, a field to write in, and buttons. The program draws it the same way for
/// every kind, and it works with the left mouse button and the keyboard, or with the stylus tip alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    /// Its title.
    pub title: String,
    /// The text it shows, oldest first.
    pub rows: Vec<CardRow>,
    /// A field to write in, with what it says while empty; `None`: no field.
    pub field: Option<String>,
    /// Its buttons, besides the program's own (delete).
    pub actions: Vec<CardAction>,
}

/// A kind of item that an extension makes: something on a page that is kept with the document, so that **undo, moving, copying, pasting,
/// erasing and the page edits follow it without the extension doing anything**. It is a box on a page that holds `data`, a text the kind alone
/// reads and writes (a thread, a name); the program keeps and moves it. The kind says how it is drawn, how it is written into a PDF (as a
/// [`Note`]) and read back, what text in it a search finds, and what its card shows.
pub trait ItemKind: Send + Sync {
    /// Its name: lower case letters, digits and `-`, unique among the kinds. It is written into the file, so it never changes.
    fn id(&self) -> &'static str;

    /// The colour a new item has (a person can change it like any item's).
    fn colour(&self) -> Rgba {
        [255, 204, 0, 255]
    }

    /// Draw one item over its page, in document space.
    fn paint(&self, item: &ItemView<'_>, out: &mut dyn Painter);

    /// What the item looks like in a PDF.
    fn write(&self, data: &str) -> Note;

    /// The data of an item of this kind that a file holds as `note`; `None` if it is not one this kind can use.
    fn read(&self, note: &Note) -> Option<String>;

    /// The text of the item that a search can find.
    fn text(&self, _data: &str) -> Option<String> {
        None
    }

    /// What its card shows; `None`: the kind has no card.
    fn card(&self, _data: &str) -> Option<Card> {
        None
    }

    /// A person pressed `action` of the card, or sent `input` from its field (`action` is then empty). The new data, or `None` if nothing changes.
    /// The change is one undo step.
    fn act(&self, _data: &str, _action: &str, _input: &str, _who: &Who<'_>) -> Option<String> {
        None
    }
}

/// What a [`LinkHandler`] asks the program to do when a button of its card is pressed (an extension touches neither the system nor the network: the
/// program does what is asked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAct {
    /// Nothing.
    Nothing,
    /// Put the card away.
    Close,
    /// Hand the address to the system, to open in its own browser, and put the card away.
    Open,
    /// Put the address on the clipboard.
    Copy,
}

/// What an extension does with a link that leaves the document (an address on the web): it may show a card for it, instead of the address going
/// straight to the system's browser. It is asked for every such link, in the order the extensions are listed; the first that has a card takes the link.
pub trait LinkHandler: Send + Sync {
    /// Its name (lower case letters, digits and `-`).
    fn id(&self) -> &'static str;

    /// The card for the address `url` (the program has checked that it is a web address it may open), or `None` if this handler does not take it.
    /// A card with no field: its buttons are what a person presses, and Enter is the first of them.
    fn card(&self, url: &str) -> Option<Card>;

    /// A button of the card pressed (its [`CardAction::id`]).
    fn act(&self, url: &str, action: &str) -> LinkAct;
}

/// What a tool may ask of the document and the window, on every call it is made.
pub trait Context {
    /// Add an item of the kind named `kind` (one of this extension's: see [`Tool::item_kinds`]) holding `data`, in the box `bounds` (document
    /// space), as one undo step. The box is put on the page it is mostly on. `None` if it cannot be (no such kind, no page there).
    fn add_item(&mut self, _kind: &str, _bounds: [f32; 4], _data: String) -> Option<ItemId> {
        None
    }
    /// The topmost item within `radius` document points of `at`, and its kind's name.
    fn item_at(&self, _at: P, _radius: f32) -> Option<(ItemId, String)> {
        None
    }
    /// Show the card of an item (see [`ItemKind::card`]).
    fn open_item(&mut self, _id: ItemId) {}
    /// Move an item by `(dx, dy)` document points, as one undo step; it may end on another page.
    fn move_item(&mut self, _id: ItemId, _dx: f32, _dy: f32) -> bool {
        false
    }
    /// Take an item away, as one undo step.
    fn remove_item(&mut self, _id: ItemId) -> bool {
        false
    }
    /// The page under a document point, as `[x0, y0, x1, y1]` in document space.
    fn page_at(&self, _at: P) -> Option<[f32; 4]> {
        None
    }
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

    /// What the extension does with links that leave the document, if it does anything (asked once, when the editor is made).
    fn link_handlers(&self) -> Vec<Box<dyn LinkHandler>> {
        Vec::new()
    }

    /// The kinds of item the extension makes, if it makes any (asked once, when the program starts: the program keeps them, and a file that holds
    /// one is read through it, whichever tool is in use).
    fn item_kinds(&self) -> Vec<Box<dyn ItemKind>> {
        Vec::new()
    }

    /// Whether using the tool adds to the document (ink, a note, text): such a tool is not offered for a PDF whose owner has not allowed that. A tool
    /// that only shows something (a pointer) says `false`. Every tool is taken to edit unless it says it does not.
    fn edits(&self) -> bool {
        true
    }
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
        // (An extension with no icon has no button on the toolbar: it is not a tool to choose, only what its item kinds and link handlers give.)
        if !self.icon_svg.is_empty() && !self.icon_svg.contains("<path") {
            return Some(format!("{} has an icon that is not an SVG of paths", self.id));
        }
        None
    }
}
