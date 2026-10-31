//! Sticky notes, as an extension: a click of the mouse or a tap of the stylus on the page puts a note there, a click on a note opens its thread
//! (its card: what was said, a field to reply in, a button to mark it done), and a drag of a note moves it. A note is an item the program keeps
//! (see `inkpdf_api::ItemKind`), so undo, copy and paste, the lasso and the eraser, the page edits and search all work on it without a line of
//! this extension's; in a PDF it is a text annotation with replies to it, which every reader shows as a note and its thread.
//!
//! It is written against `inkpdf-api` alone, and with the left mouse button or the stylus's tip alone: nothing needs the barrel button.

use inkpdf_api::{
    API_VERSION, Card, CardAction, CardRow, Context, Device, Extension, ItemId, ItemKind, ItemView, Note, P, Painter, Phase, PointerEvent, Policy, Tool,
    Who,
};

mod model;

use model::{Thread, short_date};

/// What the notes are called in a file (never changes: it is written into every note).
const KIND: &str = "note";
/// A note's side, in document points (a PDF's own note icon is this size too).
const SIDE: f32 = 20.0;
/// How far a press may move, in screen pixels, and still be a click.
const SLOP_PX: f32 = 6.0;

/// The extension, for the list of the program's.
pub fn extension() -> Extension {
    Extension { api: API_VERSION, id: "notes", label: "Sticky notes", icon_svg: include_str!("icon.svg"), create: || Box::new(Notes::default()) }
}

/// The kind of item a note is.
struct Sticky;

impl ItemKind for Sticky {
    fn id(&self) -> &'static str {
        KIND
    }

    fn colour(&self) -> [u8; 4] {
        [255, 204, 0, 255]
    }

    /// A square of the note's colour with a corner turned down and two lines of writing; a thread with replies has a dot for each (up to four) and a
    /// thread that is done a tick over it.
    fn paint(&self, item: &ItemView<'_>, out: &mut dyn Painter) {
        let [x0, y0, x1, y1] = item.bounds;
        if !(x1 - x0 > 0.0 && y1 - y0 > 0.0 && item.bounds.iter().all(|v| v.is_finite())) {
            return;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let thread = Thread::decode(item.data);
        let c = item.colour;
        let done = if thread.closed { 0.45 } else { 1.0 };
        let fill = [c[0], c[1], c[2], (f32::from(c[3]) * done) as u8];
        let ink = [70, 55, 0, if thread.closed { 150 } else { 255 }];
        // The paper: thin rows side by side (a line has round ends, so the thinner it is the squarer the corners).
        let rows = (h / 2.0).ceil().max(1.0) as usize;
        for i in 0..rows {
            let y = y0 + (i as f32 + 0.5) * h / rows as f32;
            out.doc_line([x0 + 1.0, y], [x1 - 1.0, y], h / rows as f32 + 0.3, fill);
        }
        let edge = 1.1;
        let fold = 0.3 * w.min(h);
        let outline = [[x0, y0], [x1, y0], [x1, y1 - fold], [x1 - fold, y1], [x0, y1], [x0, y0]];
        for pair in outline.windows(2) {
            out.doc_line(pair[0], pair[1], edge, ink);
        }
        out.doc_line([x1 - fold, y1], [x1 - fold, y1 - fold], edge, ink);
        out.doc_line([x1 - fold, y1 - fold], [x1, y1 - fold], edge, ink);
        for (i, line) in [0.3_f32, 0.5].into_iter().enumerate() {
            let right = if i == 0 { 0.8 } else { 0.55 };
            out.doc_line([x0 + 0.2 * w, y0 + line * h], [x0 + right * w, y0 + line * h], 1.2, ink);
        }
        for i in 0..thread.messages.len().saturating_sub(1).min(4) {
            let at = [x0 + 0.2 * w + i as f32 * 0.14 * w, y1 - 0.14 * h];
            out.doc_line(at, at, 2.4, ink);
        }
        if thread.closed {
            let green = [38, 160, 84, 255];
            out.doc_line([x0 + 0.25 * w, y0 + 0.55 * h], [x0 + 0.45 * w, y0 + 0.78 * h], 2.4, green);
            out.doc_line([x0 + 0.45 * w, y0 + 0.78 * h], [x0 + 0.85 * w, y0 + 0.2 * h], 2.4, green);
        }
    }

    fn write(&self, data: &str) -> Note {
        Thread::decode(data).to_note()
    }

    fn read(&self, note: &Note) -> Option<String> {
        Some(Thread::from_note(note).encode())
    }

    fn text(&self, data: &str) -> Option<String> {
        let thread = Thread::decode(data);
        (!thread.messages.is_empty()).then(|| thread.messages.iter().map(|m| m.text.as_str()).collect::<Vec<_>>().join("\n"))
    }

    fn card(&self, data: &str) -> Option<Card> {
        let thread = Thread::decode(data);
        let rows = thread
            .messages
            .iter()
            .map(|m| {
                let who = if m.author.trim().is_empty() { "Someone" } else { m.author.trim() };
                let when = short_date(&m.date);
                CardRow { head: if when.is_empty() { who.to_string() } else { format!("{who}, {when}") }, body: m.text.clone() }
            })
            .collect();
        let hint = if thread.messages.is_empty() { "Write a note" } else { "Reply" };
        let done = if thread.closed { "Reopen" } else { "Done" };
        Some(Card {
            title: if thread.closed { "Note (done)".into() } else { "Note".into() },
            rows,
            field: Some(hint.into()),
            actions: vec![CardAction { id: "done", label: done.into() }],
        })
    }

    fn act(&self, data: &str, action: &str, input: &str, who: &Who<'_>) -> Option<String> {
        let mut thread = Thread::decode(data);
        match action {
            "" => thread.say(who.author, who.date, input).then(|| thread.encode()),
            "done" => {
                thread.closed = !thread.closed;
                Some(thread.encode())
            }
            _ => None,
        }
    }
}

/// A press that is under way: on a note it may be a drag or a click; on the page it may be a click that puts a note there.
#[derive(Clone, Copy, Debug)]
struct Press {
    device: Device,
    id: u32,
    /// Where it began and where it is now, in document space.
    from: P,
    at: P,
    /// The note under it, if there was one.
    note: Option<ItemId>,
}

/// The tool.
#[derive(Default)]
struct Notes {
    press: Option<Press>,
    /// Where the pen or the mouse hovers (document space), for the outline of where a note would go.
    hover: Option<P>,
}

/// A point that can be used: both numbers are numbers.
fn finite(p: P) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

impl Notes {
    /// Where a note put at `at` would be: its top-left corner there, kept inside the page `at` is on.
    fn box_at(cx: &dyn Context, at: P) -> Option<[f32; 4]> {
        let page = cx.page_at(at)?;
        let x = at[0].clamp(page[0], (page[2] - SIDE).max(page[0]));
        let y = at[1].clamp(page[1], (page[3] - SIDE).max(page[1]));
        Some([x, y, x + SIDE, y + SIDE])
    }

    fn slop(cx: &dyn Context) -> f32 {
        SLOP_PX * cx.dpr() / cx.zoom().max(1e-3)
    }

    fn moved_far(&self, cx: &dyn Context, press: &Press) -> bool {
        let d = [press.at[0] - press.from[0], press.at[1] - press.from[1]];
        d[0].hypot(d[1]) > Self::slop(cx)
    }

    /// The press ends: a click puts a note on the page or opens the one pressed, a drag of a note moves it.
    fn finish(&mut self, cx: &mut dyn Context) {
        let Some(press) = self.press.take() else { return };
        let far = self.moved_far(cx, &press);
        match press.note {
            Some(note) if far => {
                cx.move_item(note, press.at[0] - press.from[0], press.at[1] - press.from[1]);
            }
            Some(note) => cx.open_item(note),
            None if !far => {
                if let Some(bounds) = Self::box_at(cx, press.from)
                    && let Some(note) = cx.add_item(KIND, bounds, Thread::default().encode())
                {
                    cx.open_item(note);
                }
            }
            None => {}
        }
        cx.request_redraw();
    }
}

impl Tool for Notes {
    fn policy(&self, desktop: bool) -> Policy {
        Policy::stylus_tool(desktop)
    }

    fn pointer(&mut self, cx: &mut dyn Context, ev: &PointerEvent<'_>) {
        let Some(last) = ev.samples.last().map(|s| cx.to_doc(s.pos)).filter(|p| finite(*p)) else {
            if matches!(ev.phase, Phase::Leave | Phase::Cancel) {
                self.hover = None;
                if ev.phase == Phase::Cancel && self.owns(ev.device, ev.id) {
                    self.press = None;
                }
                cx.request_redraw();
            }
            return;
        };
        match ev.phase {
            Phase::Down if cx.has_document() => {
                // Whichever end or button of the pen touches, it presses.
                let note = cx.item_at(last, Self::slop(cx)).filter(|(_, kind)| kind == KIND).map(|(id, _)| id);
                self.press = Some(Press { device: ev.device, id: ev.id, from: last, at: last, note });
                cx.request_redraw();
            }
            Phase::Move if self.owns(ev.device, ev.id) => {
                if let Some(press) = &mut self.press {
                    press.at = last;
                }
                cx.request_redraw();
            }
            Phase::Up if self.owns(ev.device, ev.id) => {
                if let Some(press) = &mut self.press {
                    press.at = last;
                }
                self.finish(cx);
            }
            Phase::Cancel if self.owns(ev.device, ev.id) => {
                self.press = None;
                cx.request_redraw();
            }
            Phase::Hover => {
                self.hover = cx.has_document().then_some(last);
                cx.request_redraw();
            }
            Phase::Leave => {
                self.hover = None;
                cx.request_redraw();
            }
            _ => {}
        }
    }

    fn owns(&self, device: Device, id: u32) -> bool {
        self.press.is_some_and(|p| (p.device, p.id) == (device, id))
    }

    fn busy(&self) -> bool {
        self.press.is_some()
    }

    fn end(&mut self, cx: &mut dyn Context) {
        self.finish(cx);
    }

    fn leave(&mut self, _cx: &mut dyn Context) {
        self.press = None;
        self.hover = None;
    }

    fn cancel(&mut self, _cx: &mut dyn Context) {
        self.press = None;
    }

    fn pages_edited(&mut self, _cx: &mut dyn Context) {
        self.press = None;
        self.hover = None;
    }

    fn item_kinds(&self) -> Vec<Box<dyn ItemKind>> {
        vec![Box::new(Sticky)]
    }

    /// Where a note would go (an outline under the pen or the mouse), or where the one being dragged would land.
    fn paint(&self, cx: &dyn Context, out: &mut dyn Painter) {
        let accent = cx.theme().accent;
        let outline = |out: &mut dyn Painter, b: [f32; 4], alpha: u8| {
            let colour = [accent[0], accent[1], accent[2], alpha];
            let px = cx.dpr() / cx.zoom().max(1e-3);
            for (a, c) in [([b[0], b[1]], [b[2], b[1]]), ([b[2], b[1]], [b[2], b[3]]), ([b[2], b[3]], [b[0], b[3]]), ([b[0], b[3]], [b[0], b[1]])] {
                out.doc_line(a, c, 1.5 * px, colour);
            }
        };
        match self.press {
            Some(Press { note: Some(_), from, at, .. }) if self.moved_far(cx, &Press { device: Device::Pen, id: 0, from, at, note: None }) => {
                // (The note's own box is not known here: a note is a square of a known side, held by its middle.)
                let half = SIDE / 2.0;
                outline(out, [at[0] - half, at[1] - half, at[0] + half, at[1] + half], 200);
            }
            Some(_) => {}
            None => {
                if let Some(b) = self.hover.and_then(|at| Self::box_at(cx, at)) {
                    outline(out, b, 110);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkpdf_api::{MouseUse, Rgba, Sample, Theme};

    /// A window with one page, 612 by 792 points at the document's origin, zoom 2, the document's origin at the screen's (10, 20): the API's
    /// `Context`, made by hand, with the notes a test puts on it.
    #[derive(Default)]
    struct Window {
        notes: Vec<(ItemId, [f32; 4])>,
        opened: Vec<ItemId>,
        moved: Vec<(ItemId, f32, f32)>,
        added: Vec<(String, [f32; 4], String)>,
        redraws: usize,
    }

    impl Context for Window {
        fn to_doc(&self, s: P) -> P {
            [(s[0] - 10.0) / 2.0, (s[1] - 20.0) / 2.0]
        }
        fn doc_to_screen(&self, d: P) -> P {
            [d[0] * 2.0 + 10.0, d[1] * 2.0 + 20.0]
        }
        fn zoom(&self) -> f32 {
            2.0
        }
        fn dpr(&self) -> f32 {
            1.0
        }
        fn has_document(&self) -> bool {
            true
        }
        fn request_redraw(&mut self) {
            self.redraws += 1;
        }
        fn theme(&self) -> Theme {
            Theme { ink: [0, 0, 0, 255], accent: [32, 96, 224, 255], selection: [0; 4], danger: [0; 4], white: [255; 4] }
        }
        fn add_item(&mut self, kind: &str, bounds: [f32; 4], data: String) -> Option<ItemId> {
            self.added.push((kind.to_string(), bounds, data));
            let id = 100 + self.added.len() as ItemId;
            self.notes.push((id, bounds));
            Some(id)
        }
        fn item_at(&self, at: P, radius: f32) -> Option<(ItemId, String)> {
            self.notes
                .iter()
                .rev()
                .find(|(_, b)| at[0] >= b[0] - radius && at[0] <= b[2] + radius && at[1] >= b[1] - radius && at[1] <= b[3] + radius)
                .map(|(id, _)| (*id, KIND.to_string()))
        }
        fn open_item(&mut self, id: ItemId) {
            self.opened.push(id);
        }
        fn move_item(&mut self, id: ItemId, dx: f32, dy: f32) -> bool {
            self.moved.push((id, dx, dy));
            true
        }
        fn page_at(&self, at: P) -> Option<[f32; 4]> {
            (at[0] >= 0.0 && at[0] <= 612.0 && at[1] >= 0.0 && at[1] <= 792.0).then_some([0.0, 0.0, 612.0, 792.0])
        }
    }

    fn at(doc: P) -> Sample {
        Sample { pos: [doc[0] * 2.0 + 10.0, doc[1] * 2.0 + 20.0], pressure: 0.5, t_ms: 0.0 }
    }

    fn event(device: Device, phase: Phase, samples: &[Sample]) -> PointerEvent<'_> {
        PointerEvent { device, id: 1, phase, samples, pen: Default::default() }
    }

    /// A whole press: down at `from`, up at `to` (a click, or a drag).
    fn press(tool: &mut Notes, w: &mut Window, device: Device, from: P, to: P) {
        tool.pointer(w, &event(device, Phase::Down, &[at(from)]));
        tool.pointer(w, &event(device, Phase::Move, &[at(to)]));
        tool.pointer(w, &event(device, Phase::Up, &[at(to)]));
    }

    #[derive(Default)]
    struct Lines(Vec<(P, P, f32, Rgba)>);

    impl Painter for Lines {
        fn doc_line(&mut self, a: P, b: P, width: f32, rgba: Rgba) {
            self.0.push((a, b, width, rgba));
        }
        fn screen_line(&mut self, a: P, b: P, width: f32, rgba: Rgba) {
            self.0.push((a, b, width, rgba));
        }
        fn now_ms(&self) -> f64 {
            0.0
        }
    }

    #[test]
    fn it_is_a_well_formed_extension_that_makes_the_notes_and_listens_to_the_stylus_and_the_mouse_but_not_the_keyboard() {
        let e = extension();
        assert_eq!(e.problem(), None);
        let tool = (e.create)();
        for desktop in [true, false] {
            let p = tool.policy(desktop);
            assert!(p.stylus && !p.keyboard);
            assert_eq!(p.mouse, if desktop { MouseUse::AsStylus } else { MouseUse::Scrolls });
        }
        let kinds = tool.item_kinds();
        assert_eq!(kinds.iter().map(|k| k.id()).collect::<Vec<_>>(), [KIND]);
    }

    #[test]
    fn a_click_on_the_page_puts_a_note_there_and_opens_its_thread_with_the_mouse_and_with_the_tip() {
        for device in [Device::Mouse, Device::Pen] {
            let (mut tool, mut w) = (Notes::default(), Window::default());
            press(&mut tool, &mut w, device, [100.0, 200.0], [101.0, 200.5]);
            assert_eq!(w.added.len(), 1, "{device:?}");
            let (kind, bounds, data) = &w.added[0];
            assert_eq!((kind.as_str(), bounds[..2].to_vec()), (KIND, vec![100.0, 200.0]), "its corner is where the click was");
            assert_eq!((bounds[2] - bounds[0], bounds[3] - bounds[1]), (SIDE, SIDE));
            assert_eq!(Thread::decode(data), Thread::default(), "nobody has said anything yet");
            assert_eq!(w.opened, [101], "and its card opens, to write in");
            assert!(!tool.busy());
        }
    }

    #[test]
    fn a_note_put_near_the_edge_of_the_page_is_kept_inside_it_and_off_the_page_there_is_none() {
        let (mut tool, mut w) = (Notes::default(), Window::default());
        press(&mut tool, &mut w, Device::Mouse, [611.0, 791.0], [611.0, 791.0]);
        press(&mut tool, &mut w, Device::Mouse, [1.0, 0.5], [1.0, 0.5]);
        assert_eq!(w.added[0].1, [612.0 - SIDE, 792.0 - SIDE, 612.0, 792.0]);
        assert_eq!(w.added[1].1[..2], [1.0, 0.5]);
        press(&mut tool, &mut w, Device::Mouse, [700.0, 400.0], [700.0, 400.0]);
        assert_eq!(w.added.len(), 2, "off the page");
    }

    #[test]
    fn a_click_on_a_note_opens_it_and_makes_no_other_and_a_drag_of_it_moves_it_and_a_drag_on_the_page_does_nothing() {
        let (mut tool, mut w) = (Notes::default(), Window::default());
        w.notes.push((7, [100.0, 100.0, 120.0, 120.0]));
        press(&mut tool, &mut w, Device::Pen, [110.0, 110.0], [110.5, 110.0]);
        assert_eq!((w.opened.clone(), w.added.len(), w.moved.len()), (vec![7], 0, 0));
        press(&mut tool, &mut w, Device::Mouse, [110.0, 110.0], [210.0, 160.0]);
        assert_eq!(w.moved, [(7, 100.0, 50.0)], "by as far as it was dragged");
        assert_eq!(w.opened, [7], "a drag does not open it");
        press(&mut tool, &mut w, Device::Pen, [300.0, 300.0], [400.0, 400.0]);
        assert_eq!((w.added.len(), w.moved.len(), w.opened.len()), (0, 1, 1), "a drag on the page is a drag, not a note");
    }

    #[test]
    fn a_press_that_is_called_off_the_tool_being_put_away_and_the_pages_being_edited_leave_nothing_done() {
        for ending in 0..3 {
            let (mut tool, mut w) = (Notes::default(), Window::default());
            tool.pointer(&mut w, &event(Device::Pen, Phase::Down, &[at([50.0, 50.0])]));
            assert!(tool.busy() && tool.owns(Device::Pen, 1) && !tool.owns(Device::Mouse, 1));
            match ending {
                0 => tool.cancel(&mut w),
                1 => tool.leave(&mut w),
                _ => tool.pages_edited(&mut w),
            }
            assert!(!tool.busy(), "ending {ending}");
            tool.pointer(&mut w, &event(Device::Pen, Phase::Up, &[at([50.0, 50.0])]));
            assert!(w.added.is_empty(), "ending {ending}: an up with no down is nothing");
        }
        // A press elsewhere ends it as a lift would.
        let (mut tool, mut w) = (Notes::default(), Window::default());
        tool.pointer(&mut w, &event(Device::Pen, Phase::Down, &[at([50.0, 50.0])]));
        tool.end(&mut w);
        assert_eq!(w.added.len(), 1);
    }

    #[test]
    fn it_shows_where_a_note_would_go_and_where_the_one_being_dragged_would_land_and_nothing_when_idle() {
        let (mut tool, mut w) = (Notes::default(), Window::default());
        let painted = |tool: &Notes, w: &Window| {
            let mut out = Lines::default();
            tool.paint(w, &mut out);
            out.0
        };
        assert!(painted(&tool, &w).is_empty());
        tool.pointer(&mut w, &event(Device::Pen, Phase::Hover, &[at([50.0, 60.0])]));
        let hover = painted(&tool, &w);
        assert_eq!(hover.len(), 4, "a square outline");
        assert!(hover.iter().all(|l| l.0.iter().chain(&l.1).all(|v| v.is_finite())));
        tool.pointer(&mut w, &event(Device::Pen, Phase::Leave, &[]));
        assert!(painted(&tool, &w).is_empty());
        w.notes.push((7, [100.0, 100.0, 120.0, 120.0]));
        tool.pointer(&mut w, &event(Device::Mouse, Phase::Down, &[at([110.0, 110.0])]));
        assert!(painted(&tool, &w).is_empty(), "pressed but not dragged: nothing yet");
        tool.pointer(&mut w, &event(Device::Mouse, Phase::Move, &[at([180.0, 150.0])]));
        assert_eq!(painted(&tool, &w).len(), 4, "the note's new place");
    }

    #[test]
    fn hostile_events_make_no_panic_and_nothing_that_is_not_a_number() {
        let (mut tool, mut w) = (Notes::default(), Window::default());
        let bad = [
            Sample { pos: [f32::NAN, 0.0], pressure: 0.5, t_ms: 0.0 },
            Sample { pos: [f32::INFINITY, f32::NEG_INFINITY], pressure: f32::NAN, t_ms: f64::NAN },
            Sample { pos: [1e30, -1e30], pressure: 0.5, t_ms: -1.0 },
        ];
        for phase in [Phase::Hover, Phase::Down, Phase::Move, Phase::Up, Phase::Cancel, Phase::Leave, Phase::Down, Phase::Up] {
            tool.pointer(&mut w, &event(Device::Mouse, phase, &bad));
            tool.pointer(&mut w, &event(Device::Pen, phase, &[]));
            tool.pointer(&mut w, &event(Device::Pen, phase, &bad[2..]));
        }
        assert!(w.added.iter().all(|(_, b, _)| b.iter().all(|v| v.is_finite())));
        let mut out = Lines::default();
        tool.paint(&w, &mut out);
        assert!(out.0.iter().all(|l| l.0.iter().chain(&l.1).chain(&[l.2]).all(|v| v.is_finite())));
    }

    #[test]
    fn a_note_is_drawn_inside_its_box_with_a_dot_for_each_reply_and_a_tick_when_done_and_a_box_that_is_nothing_draws_nothing() {
        let kind = Sticky;
        let draw = |data: &str, bounds: [f32; 4]| {
            let mut out = Lines::default();
            kind.paint(&ItemView { data, bounds, colour: [255, 204, 0, 255], selected: false }, &mut out);
            out.0
        };
        let bounds = [100.0, 100.0, 120.0, 120.0];
        let mut thread = Thread::default();
        let plain = draw(&thread.encode(), bounds);
        assert!(!plain.is_empty());
        for l in &plain {
            for p in [l.0, l.1] {
                assert!(p[0] >= 99.0 && p[0] <= 121.0 && p[1] >= 99.0 && p[1] <= 121.0, "{p:?} is outside the note");
            }
        }
        for n in 0..6 {
            thread.say("a", "", &format!("message {n}"));
        }
        assert_eq!(draw(&thread.encode(), bounds).len(), plain.len() + 4, "four dots at most");
        thread.closed = true;
        let done = draw(&thread.encode(), bounds);
        assert_eq!(done.len(), plain.len() + 4 + 2, "and the tick");
        assert!(done.iter().all(|l| l.2.is_finite() && l.0.iter().chain(&l.1).all(|v| v.is_finite())));
        for bad in [[0.0, 0.0, 0.0, 0.0], [f32::NAN, 0.0, 1.0, 1.0], [5.0, 5.0, 1.0, 1.0]] {
            assert!(draw("", bad).is_empty(), "{bad:?}");
        }
    }

    #[test]
    fn the_card_shows_who_said_what_and_when_and_a_reply_or_the_done_button_changes_the_thread() {
        let kind = Sticky;
        let who = Who { author: "Ada", date: "D:20261031120500Z" };
        let empty = Thread::default().encode();
        let card = kind.card(&empty).expect("a card");
        assert_eq!((card.title.as_str(), card.rows.len(), card.field.as_deref()), ("Note", 0, Some("Write a note")));
        let first = kind.act(&empty, "", "Is this right?", &who).expect("said");
        let second = kind.act(&first, "", "  Yes.  ", &Who { author: "Bob", date: "" }).expect("replied");
        let card = kind.card(&second).expect("a card");
        assert_eq!(card.field.as_deref(), Some("Reply"));
        assert_eq!(card.rows.iter().map(|r| (r.head.as_str(), r.body.as_str())).collect::<Vec<_>>(), [("Ada, 31 Oct 2026, 12:05", "Is this right?"), ("Bob", "Yes.")]);
        assert_eq!(kind.act(&second, "", "   ", &who), None, "nothing to say");
        assert_eq!(kind.act(&second, "nonsense", "", &who), None);
        // Done, then a reply reopens it.
        let done = kind.act(&second, "done", "", &who).expect("done");
        let card = kind.card(&done).expect("a card");
        assert_eq!((card.title.as_str(), card.actions[0].label.as_str()), ("Note (done)", "Reopen"));
        assert!(Thread::decode(&done).closed);
        let again = kind.act(&done, "done", "", &who).expect("reopened");
        assert!(!Thread::decode(&again).closed);
        assert!(!Thread::decode(&kind.act(&done, "", "one more thing", &who).expect("said")).closed, "a reply opens it again");
    }

    #[test]
    fn a_note_is_written_into_a_pdf_as_a_note_with_replies_and_read_back_as_the_same_thread_and_its_words_are_searchable() {
        let kind = Sticky;
        let mut thread = Thread::default();
        thread.say("Ann", "D:20261031120500Z", "Check this");
        thread.say("Bob", "D:20261101000000Z", "Agreed");
        thread.closed = true;
        let data = thread.encode();
        let note = kind.write(&data);
        assert_eq!((note.text.as_str(), note.author.as_str(), note.replies.len(), note.closed), ("Check this", "Ann", 1, true));
        assert_eq!(kind.read(&note).as_deref(), Some(data.as_str()));
        assert_eq!(kind.text(&data).as_deref(), Some("Check this\nAgreed"));
        assert_eq!(kind.text(&Thread::default().encode()), None, "an empty note has nothing to find");
        // A note of another program, with only some text, is a thread of one message.
        let foreign = Note { text: "hello".into(), ..Note::default() };
        assert_eq!(Thread::decode(&kind.read(&foreign).expect("read")).messages.len(), 1);
    }
}
