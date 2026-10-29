//! The Save As dialog, from the editor's side. While it is up every device talks to it and to nothing else, as with the
//! prompt: the pen does not draw, a finger does not pan, no shortcut of the page fires. What it holds is [`SaveAsState`]: the
//! folder you are in, what is in it, the name being typed and what the person has pressed. It does nothing to the file
//! system: it asks the shell, through [`Request`]s, to list a folder (`ListFolder`, answered by [`Editor::folder_listed`]) and
//! to write the file (`SaveTo`, answered by [`Editor::save_as_done`]), or says it was put away (`SaveAsCancelled`).
//!
//! Every device can finish it: the mouse and the stylus's tip press the places, the folders and the buttons, the wheel and a
//! drag of any device scroll the list, and the name that is offered (`Untitled.pdf`, in Documents) can be saved as it is, so a
//! tablet with no keyboard needs only the tip. A keyboard types the name (Enter saves, Escape cancels).
//!
//! How it looks and where everything is: see `toolbar::save_as`.

use std::path::{Path, PathBuf};

use crate::browse::{self, Entry, Place};
use crate::input::{Device, Phase, PointerEvent, Wheel};
use crate::requests::Request;
use crate::toolbar::{ListingView, SaveAsHit, SaveAsLayout, SaveAsView, Ui};
use crate::tools::{Dir, Key};

use super::Editor;

/// How far a pointer must move before a press on the list is a scroll, not a tap, in pixels.
const DRAG_PX: f32 = 6.0;
/// How many rows one notch of a wheel scrolls.
const WHEEL_ROWS: f32 = 3.0;

/// How the folder's listing stands.
#[derive(Clone, Debug, PartialEq)]
enum Listing {
    Loading,
    Ready,
    Failed(String),
}

/// A press under way.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    dev: Device,
    id: u32,
    /// What it began on.
    on: Option<SaveAsHit>,
    start: [f32; 2],
    last: [f32; 2],
    /// It has moved far enough to be a scroll.
    dragged: bool,
}

/// What a key or a press asks of the editor beyond changing the state.
#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    Nothing,
    Ask(Request),
    Cancel,
}

/// The dialog's state, while it is open.
pub struct SaveAsState {
    dir: PathBuf,
    places: Vec<Place>,
    entries: Vec<Entry>,
    listing: Listing,
    /// How far the list is scrolled, in pixels (it is kept in range when it is used: the window can change size).
    scroll: f32,
    name: String,
    /// Where the caret is in the name, as a byte index on a character.
    caret: usize,
    /// The name without its extension is selected, as it is when a name is offered: what is typed first replaces it.
    stem_selected: bool,
    message: Option<String>,
    /// The file name Save was pressed on while it was taken: pressing it again replaces the file.
    replace: Option<String>,
    /// The file is being written: nothing more is asked until it is said how that went.
    busy: bool,
    hover: Option<SaveAsHit>,
    press: Option<Press>,
}

impl SaveAsState {
    /// A dialog in `dir`, offering `name`, whose stem (the part before `.pdf`) is selected: typing replaces it.
    pub(super) fn new(dir: PathBuf, name: &str, places: Vec<Place>) -> Self {
        // (Whatever it is called, no more of it than a name can be.)
        let mut name: String = name.chars().filter(|c| !c.is_control()).collect();
        while name.len() > browse::MAX_NAME_BYTES {
            name.pop();
        }
        let caret = name.strip_suffix(".pdf").map_or(name.len(), str::len);
        Self {
            dir,
            places,
            entries: Vec::new(),
            listing: Listing::Loading,
            scroll: 0.0,
            name,
            caret,
            stem_selected: true,
            message: None,
            replace: None,
            busy: false,
            hover: None,
            press: None,
        }
    }

    /// What must be true whatever has been done to it (for the tests that do hostile things).
    #[cfg(test)]
    pub(super) fn assert_sound(&self) {
        assert!(self.name.len() <= browse::MAX_NAME_BYTES, "a name no longer than a name can be");
        assert!(
            self.name.is_char_boundary(self.caret) && self.caret <= self.name.len(),
            "the caret is on a character of the name: {} in {:?}",
            self.caret,
            self.name
        );
        assert!(self.scroll.is_finite() && self.scroll >= 0.0, "the scroll is a position: {}", self.scroll);
        assert!(self.entries.len() <= browse::MAX_ENTRIES);
        assert!(!self.busy || self.message.is_none(), "while it is written there is nothing to say");
        assert!(self.replace.is_none() || self.message.is_some(), "a replace is only asked for after saying so");
    }

    #[cfg(test)]
    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Go to `dir`: what it holds is asked for, and until it is told the list is empty.
    fn go(&mut self, dir: PathBuf) -> Request {
        self.dir = dir.clone();
        self.entries.clear();
        self.listing = Listing::Loading;
        self.scroll = 0.0;
        self.message = None;
        self.replace = None;
        Request::ListFolder(dir)
    }

    /// What was pressed (and let go on) does its work.
    fn activate(&mut self, hit: SaveAsHit) -> Outcome {
        if self.busy && hit != SaveAsHit::Cancel {
            return Outcome::Nothing;
        }
        match hit {
            SaveAsHit::Place(i) => match self.places.get(i) {
                Some(place) => Outcome::Ask(self.go(place.path.clone())),
                None => Outcome::Nothing,
            },
            SaveAsHit::Up => match browse::parent_of(&self.dir) {
                Some(parent) => Outcome::Ask(self.go(parent)),
                None => Outcome::Nothing,
            },
            SaveAsHit::Row(i) => match self.entries.get(i).cloned() {
                Some(Entry { name, folder: true }) => Outcome::Ask(self.go(self.dir.join(name))),
                Some(Entry { name, folder: false }) => {
                    self.name = name;
                    self.stem_selected = true;
                    self.caret = self.stem_len();
                    self.edited();
                    Outcome::Nothing
                }
                None => Outcome::Nothing,
            },
            SaveAsHit::Name => {
                self.stem_selected = false;
                self.caret = self.name.len();
                Outcome::Nothing
            }
            SaveAsHit::Save => self.confirm(),
            SaveAsHit::Cancel => Outcome::Cancel,
            SaveAsHit::List => Outcome::Nothing,
        }
    }

    /// Save was pressed, or Enter: the name is checked, a file that is there is only replaced when it was asked twice, and
    /// then the shell is asked to write it.
    fn confirm(&mut self) -> Outcome {
        if self.busy {
            return Outcome::Nothing;
        }
        let file = match browse::clean_name(&self.name) {
            Ok(file) => file,
            Err(e) => {
                self.message = Some(e.to_string());
                self.replace = None;
                return Outcome::Nothing;
            }
        };
        if self.listing == Listing::Loading {
            // (What is in the folder is not known yet, so a file of that name could be there.)
            self.message = Some("Still reading the folder".to_string());
            return Outcome::Nothing;
        }
        match browse::taken(&self.entries, &file) {
            Some(Entry { folder: true, .. }) => {
                self.message = Some("A folder has that name".to_string());
                return Outcome::Nothing;
            }
            Some(_) if self.replace.as_deref() != Some(file.as_str()) => {
                self.message = Some("A file with that name is here: Replace overwrites it".to_string());
                self.replace = Some(file);
                return Outcome::Nothing;
            }
            _ => {}
        }
        self.busy = true;
        self.message = None;
        Outcome::Ask(Request::SaveTo(self.dir.join(file)))
    }

    /// How much of the name is its stem: all of it but a `.pdf`.
    fn stem_len(&self) -> usize {
        if browse::has_extension(&self.name) { self.name.len() - browse::EXTENSION.len() - 1 } else { self.name.len() }
    }

    /// The name is about to be changed by the keyboard: a selected stem goes first.
    fn replace_selection(&mut self) {
        if std::mem::take(&mut self.stem_selected) {
            let stem = self.stem_len();
            self.name.replace_range(..stem, "");
            self.caret = 0;
        }
    }

    /// The name was changed: what was said about the old one is no longer so.
    fn edited(&mut self) {
        self.message = None;
        self.replace = None;
    }

    /// A key (the arrows up and down scroll the list a row).
    pub(super) fn key(&mut self, key: Key<'_>, layout: &SaveAsLayout) -> Outcome {
        if self.busy {
            return if key == Key::Escape { Outcome::Cancel } else { Outcome::Nothing };
        }
        match key {
            Key::Escape => return Outcome::Cancel,
            Key::Text(typed) => {
                if typed.chars().any(|c| !c.is_control()) {
                    self.replace_selection();
                }
                for c in typed.chars() {
                    if c == '\n' || c == '\r' {
                        return self.confirm();
                    }
                    if !c.is_control() && self.name.len() + c.len_utf8() <= browse::MAX_NAME_BYTES {
                        self.name.insert(self.caret, c);
                        self.caret += c.len_utf8();
                    }
                }
                self.edited();
            }
            Key::Backspace | Key::Delete if self.stem_selected => {
                self.replace_selection();
                self.edited();
            }
            Key::Backspace => {
                if let Some(c) = self.name[..self.caret].chars().next_back() {
                    self.caret -= c.len_utf8();
                    self.name.remove(self.caret);
                    self.edited();
                }
            }
            Key::Delete => {
                if self.caret < self.name.len() {
                    self.name.remove(self.caret);
                    self.edited();
                }
            }
            Key::Arrow(Dir::Left, _) => {
                self.stem_selected = false;
                if let Some(c) = self.name[..self.caret].chars().next_back() {
                    self.caret -= c.len_utf8();
                }
            }
            Key::Arrow(Dir::Right, _) => {
                self.stem_selected = false;
                if let Some(c) = self.name[self.caret..].chars().next() {
                    self.caret += c.len_utf8();
                }
            }
            Key::Arrow(Dir::Up, _) => self.scroll_to(layout, self.scroll_in(layout) - layout.row_height()),
            Key::Arrow(Dir::Down, _) => self.scroll_to(layout, self.scroll_in(layout) + layout.row_height()),
            Key::Home(_) => {
                self.stem_selected = false;
                self.caret = 0;
            }
            Key::End(_) => {
                self.stem_selected = false;
                self.caret = self.name.len();
            }
            Key::Chord(_) => {}
        }
        Outcome::Nothing
    }

    /// The scroll, kept between the top and the last row.
    fn scroll_in(&self, layout: &SaveAsLayout) -> f32 {
        let max = layout.max_scroll(self.entries.len());
        if self.scroll.is_finite() { self.scroll.clamp(0.0, max) } else { 0.0 }
    }

    /// Scroll to `to`, as far as the list goes: asking for more than it has leaves nothing owing (an arrow pressed at the
    /// top is not a press to be made up for at the next one).
    fn scroll_to(&mut self, layout: &SaveAsLayout, to: f32) {
        let max = layout.max_scroll(self.entries.len());
        self.scroll = if to.is_finite() { to.clamp(0.0, max) } else { 0.0 };
    }

    /// What the folder holds, if it is still the one asked about; the answer to [`Request::ListFolder`].
    fn listed(&mut self, dir: &Path, result: Result<Vec<Entry>, String>) {
        if dir != self.dir {
            return;
        }
        match result {
            Ok(mut entries) => {
                entries.truncate(browse::MAX_ENTRIES);
                browse::sort(&mut entries);
                self.entries = entries;
                self.listing = Listing::Ready;
            }
            Err(why) => {
                self.entries.clear();
                self.listing = Listing::Failed(why);
            }
        }
        self.scroll = 0.0;
        // (A name that was found to be taken in the old listing is not known to be taken now.)
        self.replace = None;
    }

    /// One pointer's position and phase. Returns what to do beyond the state: a press that ends on what it began on does that
    /// thing.
    fn pointer(&mut self, layout: &SaveAsLayout, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) -> Outcome {
        let scroll = self.scroll_in(layout);
        let over = layout.hit(pos, scroll, self.entries.len());
        let mine = self.press.is_some_and(|p| p.dev == dev && p.id == id);
        let mut outcome = Outcome::Nothing;
        match phase {
            Phase::Down => {
                if self.press.is_none() {
                    self.press = Some(Press { dev, id, on: over, start: pos, last: pos, dragged: false });
                }
            }
            Phase::Move => {
                if let Some(press) = self.press.as_mut().filter(|_| mine) {
                    let on_list = matches!(press.on, Some(SaveAsHit::Row(_) | SaveAsHit::List));
                    if on_list && (press.dragged || (pos[0] - press.start[0]).hypot(pos[1] - press.start[1]) > DRAG_PX) {
                        press.dragged = true;
                        let to = scroll - (pos[1] - press.last[1]);
                        press.last = pos;
                        self.scroll_to(layout, to);
                        return Outcome::Nothing;
                    }
                    press.last = pos;
                }
            }
            Phase::Up => {
                if mine
                    && let Some(press) = self.press.take()
                    && let Some(hit) = over.filter(|h| !press.dragged && press.on == Some(*h))
                {
                    outcome = self.activate(hit);
                }
            }
            Phase::Cancel => {
                if mine {
                    self.press = None;
                }
            }
            Phase::Hover | Phase::Leave => {}
        }
        // (A finger that has been lifted does not hover over anything.)
        self.hover = if phase == Phase::Leave || (dev == Device::Touch && matches!(phase, Phase::Up | Phase::Cancel)) { None } else { over };
        outcome
    }

    fn wheel(&mut self, layout: &SaveAsLayout, delta: Wheel) {
        let by = match delta {
            Wheel::Lines(_, y) => -y * WHEEL_ROWS * layout.row_height(),
            Wheel::Pixels(_, y) => -y,
        };
        self.scroll_to(layout, self.scroll_in(layout) + by);
    }

    fn view<'a>(&'a self, layout: &SaveAsLayout, dir: &'a str) -> SaveAsView<'a> {
        let pressed = self.press.filter(|p| !p.dragged).and_then(|p| p.on).filter(|on| self.hover == Some(*on));
        SaveAsView {
            places: &self.places,
            place: self.places.iter().position(|p| p.path == self.dir),
            dir,
            has_parent: browse::parent_of(&self.dir).is_some(),
            entries: &self.entries,
            listing: match &self.listing {
                Listing::Loading => ListingView::Loading,
                Listing::Ready => ListingView::Ready,
                Listing::Failed(why) => ListingView::Failed(why),
            },
            scroll: self.scroll_in(layout),
            name: &self.name,
            caret: self.caret,
            stem_selected: self.stem_selected.then(|| self.stem_len()),
            message: self.message.as_deref(),
            replace: self.replace.as_deref().is_some_and(|r| browse::clean_name(&self.name).is_ok_and(|n| n == r)),
            busy: self.busy,
            hover: self.hover,
            pressed,
        }
    }
}

impl Editor {
    /// Put the Save As dialog up, in `dir`, offering `name`, with `places` to go to in one press. Whatever was under way
    /// stops. The folder's contents are asked for (see [`Request::ListFolder`]).
    pub fn ask_save_as(&mut self, dir: PathBuf, name: &str, places: Vec<Place>) {
        self.ws.erasing.clear();
        self.mouse_press = None;
        self.tools.cancel_all(&mut self.ws);
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        self.ws.requests.clear_answer();
        self.chrome.put_up_save_as(SaveAsState::new(dir.clone(), name, places));
        self.ws.requests.push(Request::ListFolder(dir));
        self.ws.redraw = true;
    }

    /// Whether the dialog is up.
    pub fn save_as_open(&self) -> bool {
        self.chrome.save_as.is_some()
    }

    /// Check what must be true of the dialog, if it is up.
    #[cfg(test)]
    pub fn assert_save_as_sound(&self) {
        if let Some(state) = &self.chrome.save_as {
            state.assert_sound();
        }
    }

    /// Put the dialog away without saving (the window is being closed, say). The shell is told, as it is when the person cancels.
    pub fn cancel_save_as(&mut self) {
        if self.chrome.save_as.take().is_some() {
            self.ws.requests.push(Request::SaveAsCancelled);
            self.ws.redraw = true;
        }
    }

    /// What `dir` holds (or why that is not known): the answer to [`Request::ListFolder`]. An answer about a folder the
    /// dialog has left is ignored.
    pub fn folder_listed(&mut self, dir: &Path, result: Result<Vec<Entry>, String>) {
        if let Some(state) = &mut self.chrome.save_as {
            state.listed(dir, result);
            self.ws.redraw = true;
        }
    }

    /// How the write that [`Request::SaveTo`] asked for went. If it worked the dialog goes; if not it stays, with what went
    /// wrong, for another name or another folder.
    pub fn save_as_done(&mut self, result: Result<(), String>) {
        let Some(state) = &mut self.chrome.save_as else { return };
        match result {
            Ok(()) => self.chrome.save_as = None,
            Err(why) => {
                state.busy = false;
                state.message = Some(why);
            }
        }
        self.ws.redraw = true;
    }

    /// The dialog as drawn now: the scrim, the card and what is on it, in `ui`.
    pub fn draw_save_as(&self, ui: &mut Ui) {
        let Some(state) = &self.chrome.save_as else { return };
        let layout = self.save_as_layout(state);
        let dir = state.dir.display().to_string();
        layout.draw(&state.view(&layout, &dir), ui);
    }

    fn save_as_layout(&self, state: &SaveAsState) -> SaveAsLayout {
        SaveAsLayout::new(self.ws.view.viewport, self.ws.dpr, state.places.len())
    }

    /// What `outcome` asks of the editor.
    fn save_as_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Ask(request) => self.ws.requests.push(request),
            Outcome::Cancel => self.cancel_save_as(),
        }
        self.ws.redraw = true;
    }

    /// A pen or touch event while the dialog is up.
    pub(super) fn save_as_event(&mut self, ev: &PointerEvent) {
        let sample = if ev.phase == Phase::Down { ev.samples.first() } else { ev.samples.last() };
        if let Some(s) = sample {
            self.save_as_pointer(ev.device, ev.id, ev.phase, s.pos);
        }
    }

    /// One pointer's position and phase, while the dialog is up.
    pub(super) fn save_as_pointer(&mut self, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        let Some(state) = &self.chrome.save_as else { return };
        let layout = self.save_as_layout(state);
        let outcome = match &mut self.chrome.save_as {
            Some(state) => state.pointer(&layout, dev, id, phase, pos),
            None => Outcome::Nothing,
        };
        self.save_as_outcome(outcome);
    }

    /// A key while the dialog is up: it types the name. `true` if it was used.
    pub(super) fn save_as_key(&mut self, key: Key<'_>) -> bool {
        let Some(state) = &self.chrome.save_as else { return false };
        let layout = self.save_as_layout(state);
        let outcome = match &mut self.chrome.save_as {
            Some(state) => state.key(key, &layout),
            None => Outcome::Nothing,
        };
        self.save_as_outcome(outcome);
        true
    }

    /// The wheel, while the dialog is up: it scrolls the list.
    pub(super) fn save_as_wheel(&mut self, delta: Wheel) {
        let Some(state) = &self.chrome.save_as else { return };
        let layout = self.save_as_layout(state);
        if let Some(state) = &mut self.chrome.save_as {
            state.wheel(&layout, delta);
        }
        self.ws.redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, folder: bool) -> Entry {
        Entry { name: name.to_string(), folder }
    }

    fn layout() -> SaveAsLayout {
        SaveAsLayout::new([1100.0, 900.0], 1.0, 2)
    }

    fn places() -> Vec<Place> {
        vec![Place { label: "Documents".into(), path: "/home/me/Documents".into() }, Place { label: "Home".into(), path: "/home/me".into() }]
    }

    fn ready(entries: Vec<Entry>) -> SaveAsState {
        let mut state = SaveAsState::new("/home/me/Documents".into(), "Untitled.pdf", places());
        state.listed(Path::new("/home/me/Documents"), Ok(entries));
        state
    }

    fn saved_as(outcome: Outcome) -> Option<PathBuf> {
        match outcome {
            Outcome::Ask(Request::SaveTo(path)) => Some(path),
            _ => None,
        }
    }

    #[test]
    fn it_opens_on_the_folder_with_the_name_offered_and_its_stem_selected() {
        let state = SaveAsState::new("/d".into(), "Untitled.pdf", places());
        assert_eq!((state.name.as_str(), state.caret, state.stem_selected, state.stem_len()), ("Untitled.pdf", 8, true, 8));
        assert_eq!(SaveAsState::new("/d".into(), "no extension", places()).stem_len(), "no extension".len());
        assert_eq!(SaveAsState::new("/d".into(), "a\nb.pdf", places()).name, "ab.pdf", "no control characters get in");
    }

    #[test]
    fn the_first_thing_typed_replaces_the_selected_stem_and_the_extension_stays() {
        let mut s = ready(vec![]);
        s.key(Key::Text("Invoice 7"), &layout());
        assert_eq!((s.name.as_str(), s.caret, s.stem_selected), ("Invoice 7.pdf", 9, false));
        s.key(Key::Text("!"), &layout());
        assert_eq!(s.name, "Invoice 7!.pdf", "after that it types where the caret is");
        // Backspace (or Delete) on the selection takes the stem away, and nothing more.
        for key in [Key::Backspace, Key::Delete] {
            let mut s = ready(vec![]);
            s.key(key, &layout());
            assert_eq!((s.name.as_str(), s.caret), (".pdf", 0), "{key:?}");
        }
        // Moving the caret first keeps the stem, and then typing goes in at the caret.
        let mut s = ready(vec![]);
        s.key(Key::Arrow(Dir::Left, false), &layout());
        s.key(Key::Text("X"), &layout());
        assert_eq!(s.name, "UntitleXd.pdf");
        // Control characters alone do not count as typing.
        let mut s = ready(vec![]);
        s.key(Key::Text("\t"), &layout());
        assert_eq!(s.name, "Untitled.pdf");
        assert!(s.stem_selected);
    }

    #[test]
    fn typing_inserts_at_the_caret_and_the_keys_move_and_delete_by_character() {
        let mut s = ready(vec![]);
        s.key(Key::Text("My "), &layout());
        s.key(Key::Text("é文"), &layout());
        assert_eq!(s.name, "My é文.pdf");
        s.key(Key::Backspace, &layout());
        assert_eq!(s.name, "My é.pdf");
        s.key(Key::Arrow(Dir::Left, false), &layout());
        s.key(Key::Delete, &layout());
        assert_eq!(s.name, "My .pdf");
        s.key(Key::Home(false), &layout());
        s.key(Key::Text(">"), &layout());
        assert_eq!(s.name, ">My .pdf");
        s.key(Key::End(false), &layout());
        s.key(Key::Backspace, &layout());
        assert_eq!(s.name, ">My .pd");
        // The ends of the name are ends, not errors.
        let mut s = ready(vec![]);
        s.key(Key::Home(false), &layout());
        s.key(Key::Backspace, &layout());
        s.key(Key::Arrow(Dir::Left, false), &layout());
        s.key(Key::End(false), &layout());
        s.key(Key::Delete, &layout());
        s.key(Key::Arrow(Dir::Right, false), &layout());
        assert_eq!(s.name, "Untitled.pdf");
    }

    #[test]
    fn what_is_typed_is_never_a_control_character_and_never_longer_than_a_name_can_be() {
        let mut s = ready(vec![]);
        s.key(Key::Text("a\tb\u{7f}c"), &layout());
        assert_eq!(s.name, "abc.pdf");
        s.key(Key::Text(&"x".repeat(1000)), &layout());
        assert!(s.name.len() <= browse::MAX_NAME_BYTES);
        s.key(Key::Text(&"é".repeat(1000)), &layout());
        assert!(s.name.len() <= browse::MAX_NAME_BYTES && s.name.is_char_boundary(s.caret));
    }

    #[test]
    fn enter_saves_the_cleaned_name_in_the_folder_and_asks_nothing_twice_while_it_is_written() {
        let mut s = ready(vec![]);
        s.key(Key::Text("  Invoice 7 "), &layout());
        assert_eq!(s.name, "  Invoice 7 .pdf");
        let outcome = s.key(Key::Text("\n"), &layout());
        assert_eq!(saved_as(outcome), Some(PathBuf::from("/home/me/Documents/Invoice 7.pdf")));
        assert!(s.busy);
        assert_eq!(s.key(Key::Text("\n"), &layout()), Outcome::Nothing, "being written");
        assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing);
        assert_eq!(s.activate(SaveAsHit::Row(0)), Outcome::Nothing, "and nothing else is pressed meanwhile");
        assert_eq!(s.key(Key::Escape, &layout()), Outcome::Cancel, "though it can still be put away");
    }

    #[test]
    fn a_name_that_is_wrong_says_so_and_asks_nothing() {
        for (typed, why) in [("", "name"), ("a/b", "/"), ("CON", "reserved"), ("..", "dots")] {
            let mut s = ready(vec![]);
            s.name = typed.to_string();
            s.caret = s.name.len();
            assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing, "{typed:?}");
            let message = s.message.clone().expect("a message");
            assert!(message.to_lowercase().contains(why), "{typed:?}: {message}");
            assert!(!s.busy);
            s.key(Key::Text("x"), &layout());
            assert_eq!(s.message, None, "typing again clears it");
        }
    }

    #[test]
    fn a_file_that_is_there_is_replaced_only_when_asked_twice_and_a_folder_never() {
        let mut s = ready(vec![entry("Untitled.pdf", false), entry("Folder.pdf", true)]);
        assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing);
        assert_eq!(s.replace.as_deref(), Some("Untitled.pdf"));
        assert!(s.view(&SaveAsLayout::new([1100.0, 900.0], 1.0, 2), "/x").replace, "the button says Replace");
        assert_eq!(saved_as(s.activate(SaveAsHit::Save)), Some(PathBuf::from("/home/me/Documents/Untitled.pdf")));
        // Typing another name starts over; the old one asked once is not asked twice for the new one.
        let mut s = ready(vec![entry("Untitled.pdf", false), entry("Other.pdf", false)]);
        s.activate(SaveAsHit::Save);
        s.name = "Other".into();
        s.edited();
        assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing, "asked afresh for the other file");
        assert_eq!(s.replace.as_deref(), Some("Other.pdf"));
        // A folder that has the name.
        let mut s = ready(vec![entry("Untitled.pdf", true)]);
        assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing);
        assert!(s.message.expect("message").contains("folder"));
        assert_eq!(s.replace, None);
    }

    #[test]
    fn nothing_is_saved_before_the_folder_is_known() {
        let mut s = SaveAsState::new("/d".into(), "a.pdf", places());
        assert_eq!(s.activate(SaveAsHit::Save), Outcome::Nothing, "it could be there");
        assert!(s.message.is_some() && !s.busy);
        s.listed(Path::new("/d"), Err("no such folder".into()));
        assert!(saved_as(s.activate(SaveAsHit::Save)).is_some(), "a folder that cannot be read can still be written to");
    }

    #[test]
    fn a_folder_goes_in_a_place_goes_there_up_goes_up_and_a_file_gives_its_name() {
        let mut s = ready(vec![entry("Invoices", true), entry("old.pdf", false)]);
        assert_eq!(s.listing, Listing::Ready);
        assert_eq!(s.activate(SaveAsHit::Row(0)), Outcome::Ask(Request::ListFolder("/home/me/Documents/Invoices".into())));
        assert_eq!((s.listing.clone(), s.entries.len(), s.dir()), (Listing::Loading, 0, Path::new("/home/me/Documents/Invoices")));
        assert_eq!(s.activate(SaveAsHit::Up), Outcome::Ask(Request::ListFolder("/home/me/Documents".into())));
        assert_eq!(s.activate(SaveAsHit::Place(1)), Outcome::Ask(Request::ListFolder("/home/me".into())));
        assert_eq!(s.activate(SaveAsHit::Place(9)), Outcome::Nothing, "no such place");
        let mut s = ready(vec![entry("Invoices", true), entry("old.pdf", false)]);
        assert_eq!(s.activate(SaveAsHit::Row(1)), Outcome::Nothing);
        assert_eq!(
            (s.name.as_str(), s.caret, s.stem_selected),
            ("old.pdf", 3, true),
            "the name of the file picked, its stem selected, to be saved over or changed"
        );
        let mut top = SaveAsState::new("/".into(), "a.pdf", vec![]);
        assert_eq!(top.activate(SaveAsHit::Up), Outcome::Nothing, "nothing above the top");
    }

    #[test]
    fn a_listing_of_a_folder_that_was_left_is_ignored_and_the_entries_are_sorted_and_capped() {
        let mut s = ready(vec![]);
        s.activate(SaveAsHit::Place(1));
        s.listed(Path::new("/home/me/Documents"), Ok(vec![entry("stale", true)]));
        assert!(s.entries.is_empty() && s.listing == Listing::Loading, "the answer for where we were");
        s.listed(Path::new("/home/me"), Ok(vec![entry("b.pdf", false), entry("a", true)]));
        assert_eq!(s.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a", "b.pdf"]);
        let many: Vec<Entry> = (0..browse::MAX_ENTRIES + 50).map(|i| entry(&format!("{i}.pdf"), false)).collect();
        s.listed(Path::new("/home/me"), Ok(many));
        assert_eq!(s.entries.len(), browse::MAX_ENTRIES);
        s.listed(Path::new("/home/me"), Err("denied".into()));
        assert_eq!((s.entries.len(), s.listing.clone()), (0, Listing::Failed("denied".into())));
    }

    #[test]
    fn a_press_does_its_thing_only_when_let_go_on_what_it_began_on_and_a_drag_scrolls_instead() {
        let layout = SaveAsLayout::new([1100.0, 900.0], 1.0, 2);
        let entries: Vec<Entry> = (0..40).map(|i| entry(&format!("folder {i:02}"), true)).collect();
        let mut s = ready(entries);
        let row = |i: usize, s: &SaveAsState| {
            let r = layout.row_rect(i, s.scroll_in(&layout));
            [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
        };
        // A tap on a row goes into the folder.
        let at = row(2, &s);
        s.pointer(&layout, Device::Touch, 1, Phase::Down, at);
        assert_eq!(s.pointer(&layout, Device::Touch, 1, Phase::Up, at), Outcome::Ask(Request::ListFolder("/home/me/Documents/folder 02".into())));
        // Pressed on a row, let go on another: nothing.
        let mut s = ready((0..40).map(|i| entry(&format!("folder {i:02}"), true)).collect());
        let (a, b) = (row(2, &s), row(3, &s));
        s.pointer(&layout, Device::Pen, 1, Phase::Down, a);
        assert_eq!(s.pointer(&layout, Device::Pen, 1, Phase::Up, b), Outcome::Nothing);
        // Dragged: it scrolls, and what it lets go on is not pressed.
        let (from, to) = (row(5, &s), [row(5, &s)[0], row(5, &s)[1] - 3.0 * layout.row_height()]);
        s.pointer(&layout, Device::Touch, 2, Phase::Down, from);
        s.pointer(&layout, Device::Touch, 2, Phase::Move, to);
        assert!((s.scroll - 3.0 * layout.row_height()).abs() < 0.5, "the list followed the finger: {}", s.scroll);
        assert_eq!(s.pointer(&layout, Device::Touch, 2, Phase::Up, to), Outcome::Nothing);
        // A second pointer does not take over a press under way; a cancelled one never acts.
        let mut s = ready(vec![entry("x", true)]);
        let at = row(0, &s);
        s.pointer(&layout, Device::Touch, 1, Phase::Down, at);
        s.pointer(&layout, Device::Touch, 2, Phase::Down, at);
        assert_eq!(s.pointer(&layout, Device::Touch, 2, Phase::Up, at), Outcome::Nothing);
        s.pointer(&layout, Device::Touch, 1, Phase::Cancel, at);
        assert_eq!(s.pointer(&layout, Device::Touch, 1, Phase::Up, at), Outcome::Nothing);
    }

    #[test]
    fn the_wheel_and_the_arrows_scroll_within_the_list_and_never_out_of_it() {
        let layout = SaveAsLayout::new([1100.0, 900.0], 1.0, 2);
        let mut s = ready((0..40).map(|i| entry(&format!("f{i}"), true)).collect());
        s.wheel(&layout, Wheel::Lines(0.0, -1.0));
        assert!((s.scroll_in(&layout) - WHEEL_ROWS * layout.row_height()).abs() < 0.01);
        s.wheel(&layout, Wheel::Lines(0.0, 1000.0));
        assert_eq!(s.scroll_in(&layout), 0.0, "not above the first row");
        s.wheel(&layout, Wheel::Lines(0.0, -1000.0));
        assert_eq!(s.scroll_in(&layout), layout.max_scroll(40), "not past the last");
        s.wheel(&layout, Wheel::Lines(f32::NAN, f32::NAN));
        s.scroll = f32::NAN;
        assert_eq!(s.scroll_in(&layout), 0.0, "never not a number");
        s.key(Key::Arrow(Dir::Down, false), &layout);
        for _ in 0..5 {
            s.key(Key::Arrow(Dir::Up, false), &layout);
        }
        assert_eq!(s.scroll, 0.0, "an arrow at the top leaves nothing owing");
        s.key(Key::Arrow(Dir::Down, false), &layout);
        assert!((s.scroll_in(&layout) - layout.row_height()).abs() < 0.01);
    }

    #[test]
    fn escape_cancels_and_cancel_cancels() {
        let mut s = ready(vec![]);
        assert_eq!(s.key(Key::Escape, &layout()), Outcome::Cancel);
        assert_eq!(s.activate(SaveAsHit::Cancel), Outcome::Cancel);
    }
}
