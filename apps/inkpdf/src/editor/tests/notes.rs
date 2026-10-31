//! Sticky notes, the first extension that makes items: in each mode, with only the device that mode guarantees (the left mouse button and the
//! keyboard, or the stylus tip), a note can be put on the page, opened, replied to, marked done, moved, selected, restyled, duplicated, copied, pasted,
//! erased and deleted, each undone; and a finger never makes one. (This is the capability matrix's row for the notes: they are not one of the program's
//! own tools, so they are not in its table.)

use super::*;
use crate::input::Mode;
use crate::toolbar::{CardItem, ContextItem};
use crate::tools::Key;

const NOTES: Tool = Tool::named("notes");
const MODES: [Mode; 2] = [Mode::Desktop, Mode::Tablet];

/// The reply the notes are written with: what the card says after a note has been put on the page and written in.
fn thread_of(ed: &Editor, id: u64) -> crate::ink::Item {
    ed.ink.get(id).and_then(|s| s.item.clone()).expect("a note")
}

struct Rig {
    ed: Editor,
    mode: Mode,
    clock: f64,
}

impl Rig {
    fn new(mode: Mode) -> Rig {
        let notes: Vec<_> = inkpdf_ext::all().into_iter().filter(|e| e.id == "notes").collect();
        assert_eq!(notes.len(), 1, "the extension of the notes is in this build");
        let mut ed = Editor::with_extensions([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED, &notes);
        ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
        ed.fit_mode = false;
        ed.view.zoom = 1.0;
        ed.pan_px(0.0, 0.0);
        ed.set_mode(mode);
        ed.set_tool(NOTES);
        ed.chrome.panel = None;
        Rig { ed, mode, clock: 10_000.0 }
    }

    fn at(&self, x: f32, y: f32) -> [f32; 2] {
        on_page(&self.ed, x, y)
    }

    /// Press, move through the rest and let go, with the mode's pointing device.
    fn drag(&mut self, path: &[[f32; 2]]) {
        match self.mode {
            Mode::Desktop => {
                self.ed.mouse_moved(path[0]);
                self.ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
                for &p in &path[1..] {
                    self.ed.mouse_moved(p);
                }
                self.ed.mouse_button(ButtonState::Released, MouseButton::Left);
            }
            Mode::Tablet => {
                self.clock += 1000.0;
                let at = |phase, pts: &[[f32; 2]]| PointerEvent {
                    device: Device::Pen,
                    id: 1,
                    phase,
                    samples: samples(pts, self.clock),
                    pen: TIP,
                    contact_px: 0.0,
                };
                self.ed.pointer_event(at(Phase::Down, &path[..1]));
                self.ed.pointer_event(at(Phase::Move, &path[1..]));
                self.ed.pointer_event(at(Phase::Up, &path[path.len() - 1..]));
            }
        }
    }

    fn tap(&mut self, at: [f32; 2]) {
        self.drag(&[at, at]);
    }

    fn bar(&mut self, item: ContextItem) {
        let at = self.ed.context_bar().expect("the bar is there").centre_of(item);
        self.tap(at);
    }

    fn choose(&mut self, tool: Tool) {
        let at = self.ed.toolbar().centre_of(Item::Tool(tool));
        self.tap(at);
        assert_eq!(self.ed.tool, tool);
    }

    fn undo(&mut self) {
        let at = self.ed.toolbar().centre_of(Item::Undo);
        self.tap(at);
    }

    fn card(&mut self, item: CardItem) {
        let at = self.ed.item_card_layout().expect("the card is open").centre_of(item);
        self.tap(at);
    }

    /// The notes on the page, in drawing order.
    fn notes(&self) -> Vec<u64> {
        self.ed.ink.strokes().iter().filter(|s| s.item.as_ref().is_some_and(|i| i.kind == "note")).map(|s| s.id).collect()
    }

    /// Where a note put by `put` is: a press there is on it.
    fn on_note(&self) -> [f32; 2] {
        self.at(110.0, 310.0)
    }

    fn label(&self) -> String {
        format!("the notes in the {:?} mode", self.mode)
    }

    /// A note, put where the page is clicked or tapped (its corner there), with its card put away again.
    fn put(&mut self) -> u64 {
        let before = self.notes();
        let at = self.at(100.0, 300.0);
        self.tap(at);
        let made: Vec<u64> = self.notes().into_iter().filter(|id| !before.contains(id)).collect();
        assert_eq!(made.len(), 1, "{}: one note made", self.label());
        assert!(self.ed.item_card_open(), "{}: its thread opens, to write in", self.label());
        self.card(CardItem::Close);
        assert!(!self.ed.item_card_open(), "{}", self.label());
        self.ed.selection.clear();
        made[0]
    }

    fn corner(&self, id: u64) -> [f32; 2] {
        self.ed.ink.get(id).expect("the note").quads().next().map(|q| [q[0], q[1]]).expect("a box")
    }
}

fn for_each_mode(check: impl Fn(&mut Rig)) {
    for mode in MODES {
        check(&mut Rig::new(mode));
    }
}

#[test]
fn a_click_or_a_tap_puts_a_note_on_the_page_and_undo_takes_it_back_and_a_finger_never_makes_one() {
    for_each_mode(|rig| {
        let id = rig.put();
        assert_eq!(rig.notes(), [id], "{}", rig.label());
        let c = rig.corner(id);
        assert!((c[0] - 112.0).abs() < 0.5 && (c[1] - 312.0).abs() < 0.5, "{}: its corner is where the press was: {c:?}", rig.label());
        rig.undo();
        assert!(rig.notes().is_empty(), "{}", rig.label());
    });
    let mut rig = Rig::new(Mode::Desktop);
    let at = rig.at(100.0, 300.0);
    finger_tap(&mut rig.ed, at);
    assert!(rig.notes().is_empty(), "a finger pans and zooms and makes nothing");
}

#[test]
fn a_reply_is_written_in_the_card_and_sent_and_the_thread_is_marked_done_and_each_is_an_undo_step() {
    for_each_mode(|rig| {
        let id = rig.put();
        // A press on the note opens its thread; with the keyboard, or the card's own field and button.
        let on = rig.on_note();
        rig.tap(on);
        assert!(rig.ed.item_card_open(), "{}", rig.label());
        let view = rig.ed.item_card_view().expect("open");
        assert_eq!((view.title.as_str(), view.hint.as_deref(), view.actions.as_slice()), ("Note", Some("Write a note"), &["Done".to_string()][..]));
        rig.ed.card.name = crate::field::Field::new("Ada".into());
        rig.ed.press(Key::Text("Is this right?"));
        rig.card(CardItem::Send);
        let view = rig.ed.item_card_view().expect("open");
        assert_eq!(view.rows.len(), 1, "{}: the note itself", rig.label());
        assert!(view.rows[0].0.starts_with("Ada, "), "{}: who said it, and when: {:?}", rig.label(), view.rows[0].0);
        assert_eq!(view.hint.as_deref(), Some("Reply"));
        rig.ed.press(Key::Text("It is.\n"));
        assert_eq!(rig.ed.item_card_view().expect("open").rows.len(), 2, "{}: Enter sends", rig.label());
        rig.card(CardItem::Action(0));
        assert_eq!(rig.ed.item_card_view().expect("open").title, "Note (done)", "{}", rig.label());
        // Three steps, taken back one by one.
        rig.undo();
        assert_eq!(rig.ed.item_card_view().expect("open").title, "Note", "{}: reopened", rig.label());
        rig.undo();
        assert_eq!(rig.ed.item_card_view().expect("open").rows.len(), 1);
        rig.undo();
        assert_eq!(rig.ed.item_card_view().expect("open").rows.len(), 0, "{}", rig.label());
        assert_eq!(rig.notes(), [id]);
    });
}

#[test]
fn a_note_is_dragged_with_its_own_tool_and_undo_puts_it_back_and_a_drag_on_the_page_makes_nothing() {
    for_each_mode(|rig| {
        let id = rig.put();
        let from = rig.corner(id);
        let on = rig.on_note();
        rig.drag(&[on, [on[0] + 30.0, on[1] + 20.0], [on[0] + 60.0, on[1] + 40.0]]);
        let to = rig.corner(id);
        assert!((to[0] - from[0] - 60.0).abs() < 1.0 && (to[1] - from[1] - 40.0).abs() < 1.0, "{}: moved to {to:?} from {from:?}", rig.label());
        assert!(!rig.ed.item_card_open(), "{}: a drag does not open it", rig.label());
        rig.undo();
        assert_eq!(rig.corner(id), from, "{}: undone", rig.label());
        // A drag that begins on the page is not a note.
        let (a, b) = (rig.at(400.0, 500.0), rig.at(500.0, 600.0));
        rig.drag(&[a, [450.0, 550.0].map(|v| v + 12.0), b]);
        assert_eq!(rig.notes(), [id], "{}", rig.label());
    });
}

#[test]
fn the_lasso_selects_moves_restyles_duplicates_copies_pastes_and_deletes_a_note_and_the_eraser_erases_it_and_each_is_undone() {
    for_each_mode(|rig| {
        let id = rig.put();
        let from = rig.corner(id);
        let colour = rig.ed.ink.get(id).expect("note").style.rgba;
        rig.choose(Tool::Lasso);
        let on = rig.on_note();
        rig.tap(on);
        assert_eq!(rig.ed.selection, [id], "{}: a tap selects it", rig.label());
        // Moved.
        rig.drag(&[on, [on[0] + 30.0, on[1] + 20.0], [on[0] + 60.0, on[1] + 40.0]]);
        assert!((rig.corner(id)[0] - from[0] - 60.0).abs() < 1.0, "{}", rig.label());
        rig.undo();
        assert_eq!(rig.corner(id), from, "{}", rig.label());
        // Restyled: a colour in the bar.
        let want = rig.ed.styles.palette[3];
        rig.bar(ContextItem::Swatch(3));
        assert_eq!(rig.ed.ink.get(id).expect("note").style.rgba, want, "{}: the colour", rig.label());
        assert_eq!(thread_of(&rig.ed, id).kind, "note");
        rig.undo();
        assert_eq!(rig.ed.ink.get(id).expect("note").style.rgba, colour, "{}", rig.label());
        // Duplicated, keeping what it holds.
        rig.bar(ContextItem::Duplicate);
        assert_eq!(rig.notes().len(), 2, "{}", rig.label());
        let copy = rig.notes()[1];
        assert_eq!(thread_of(&rig.ed, copy).data, thread_of(&rig.ed, id).data, "{}", rig.label());
        rig.undo();
        // Copied and pasted, twice.
        rig.tap(on);
        rig.bar(ContextItem::Copy);
        let off = rig.at(500.0, 650.0);
        rig.tap(off);
        rig.bar(ContextItem::Paste);
        rig.bar(ContextItem::Paste);
        assert_eq!(rig.notes().len(), 3, "{}", rig.label());
        rig.undo();
        rig.undo();
        assert_eq!(rig.notes(), [id], "{}", rig.label());
        // Deleted, and brought back.
        rig.tap(on);
        rig.bar(ContextItem::Delete);
        assert!(rig.notes().is_empty(), "{}", rig.label());
        rig.undo();
        assert_eq!(rig.notes(), [id], "{}", rig.label());
        // Erased.
        rig.choose(Tool::Eraser);
        rig.drag(&[rig.at(110.0, 285.0), rig.at(110.0, 310.0), rig.at(110.0, 345.0)]);
        assert!(rig.notes().is_empty(), "{}: the eraser", rig.label());
        rig.undo();
        assert_eq!(rig.notes(), [id], "{}", rig.label());
    });
}

#[test]
fn the_card_deletes_its_note_with_its_button_or_with_delete_in_an_empty_field_and_what_the_note_says_is_found_by_the_search() {
    for_each_mode(|rig| {
        let id = rig.put();
        let on = rig.on_note();
        rig.tap(on);
        rig.ed.press(Key::Text("remember the milk"));
        rig.ed.press(Key::Text("\n"));
        rig.ed.close_item_card();
        rig.ed.open_search();
        rig.ed.press(Key::Text("milk"));
        assert_eq!(rig.ed.search.hits().len(), 1, "{}: what a note says is found", rig.label());
        rig.ed.close_search();
        rig.tap(on);
        rig.card(CardItem::Delete);
        assert!(rig.notes().is_empty() && !rig.ed.item_card_open(), "{}", rig.label());
        rig.undo();
        assert_eq!(rig.notes(), [id], "{}", rig.label());
    });
    // With the keyboard alone: Delete in the empty field.
    let mut rig = Rig::new(Mode::Desktop);
    let id = rig.put();
    let on = rig.on_note();
    rig.tap(on);
    rig.ed.press(Key::Delete);
    assert!(rig.ed.ink.get(id).is_none() && !rig.ed.item_card_open());
}

#[test]
fn a_note_saved_with_its_thread_is_read_back_as_a_note_with_the_same_thread_and_the_renderer_does_not_draw_it_twice() {
    use crate::coords::PageGeom;
    use crate::ink_io::{load, save};
    use std::sync::Arc;
    let mut rig = Rig::new(Mode::Desktop);
    let id = rig.put();
    let on = rig.on_note();
    rig.tap(on);
    rig.ed.card.name = crate::field::Field::new("Ada".into());
    rig.ed.press(Key::Text("Check this\n"));
    rig.ed.press(Key::Text("Agreed\n"));
    rig.card(CardItem::Action(0));
    let sent = thread_of(&rig.ed, id);
    let geom = PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 };
    let bytes = Arc::new(crate::testutil::sample_pdf());
    let layout = Layout::new(&[[612.0, 792.0]]);
    // (The page's origin in the document is where the note is: put it on that page.)
    let mut store = crate::ink::InkStore::new();
    let origin = layout.page(0).map(|r| [r.x, r.y]).expect("a page");
    store.add_item(
        0,
        rig.ed.ink.get(id).expect("note").style,
        "note",
        sent.data.clone(),
        [origin[0] + 100.0, origin[1] + 120.0, origin[0] + 120.0, origin[1] + 140.0],
    );
    let saved = Arc::new(save(&bytes, &store, &[geom], &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
    let loaded = load(&saved, &[geom], &layout).expect("load");
    assert_eq!(loaded.strokes.len(), 1);
    let item = loaded.strokes[0].6.clone().expect("an item");
    assert_eq!(item.kind, "note");
    let (before, after) = (crate::ink::Item { kind: "note".into(), data: sent.data.clone() }, item);
    // The same thread, said by the same people: written as a note and replies and read back (the dates are the file's own).
    let thread = |data: &str| {
        let kind = crate::tools::items::kind("note").expect("the kind");
        let card = kind.card(data).expect("a card");
        (card.title, card.rows.iter().map(|r| r.body.clone()).collect::<Vec<_>>())
    };
    assert_eq!(thread(&after.data), thread(&before.data));
    assert_eq!(thread(&after.data).0, "Note (done)", "marked done: a reader shows it as completed");
}
