//! The promise of the input design, as executable checks: with a tool selected, in either mode, every item made with
//! the tool can be created, selected, moved, restyled, duplicated, copied, pasted, erased and deleted, and each of
//! those undone, using only the devices that mode guarantees: in the desktop mode the left mouse button and the
//! keyboard; in the tablet mode the tip of the stylus. (The stylus's barrel button and eraser end, and the mouse's
//! other buttons, are shortcuts; nothing here may need them.)

use crate::geometry::Shape;
use crate::tool::Kind;
use crate::toolbar::{ContextItem, Slider};
use crate::tools::{Dir, Key};

use super::*;

const TOOLS: [Tool; 3] = [Tool::Pen, Tool::Geometry, Tool::Text];
const MODES: [Mode; 2] = [Mode::Desktop, Mode::Tablet];

/// An editor in `mode` with `tool`, and the one device that mode guarantees to point with.
struct Rig {
    ed: Editor,
    tool: Tool,
    mode: Mode,
    /// Pen events carry their own clock, moving on with every gesture, so that two taps are never a double tap.
    clock: f64,
}

impl Rig {
    fn new(tool: Tool, mode: Mode) -> Rig {
        let mut ed = editor();
        ed.set_mode(mode);
        ed.set_tool(tool);
        ed.chrome.panel = None;
        ed.shape = Shape::Line;
        Rig { ed, tool, mode, clock: 10_000.0 }
    }

    /// A point on page 0, in screen pixels.
    fn at(&self, x: f32, y: f32) -> [f32; 2] {
        on_page(&self.ed, x, y)
    }

    /// Press, move through the rest and let go, with the mode's pointing device: the left mouse button, or the tip.
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

    /// Press a button of the bar under the tools.
    fn bar(&mut self, item: ContextItem) {
        let at = self.ed.context_bar().expect("the bar is there").centre_of(item);
        self.tap(at);
    }

    /// Choose a tool with its button on the toolbar.
    fn choose(&mut self, tool: Tool) {
        let at = self.ed.toolbar().centre_of(Item::Tool(tool));
        self.tap(at);
        assert_eq!(self.ed.tool, tool);
    }

    /// Take back the last step with the toolbar's undo button.
    fn undo(&mut self) {
        let at = self.ed.toolbar().centre_of(Item::Undo);
        self.tap(at);
    }

    fn kind(&self) -> Kind {
        self.tool.kind().expect("a tool that makes items")
    }

    /// The tool's own items, in drawing order.
    fn items(&self) -> Vec<u64> {
        self.ed.ink.strokes().iter().filter(|s| Kind::of(s) == self.kind()).map(|s| s.id).collect()
    }

    /// Where on an item made by `make` a press lands on it.
    fn on_item(&self) -> [f32; 2] {
        match self.tool {
            Tool::Text => self.at(110.0, 310.0),
            _ => self.at(200.0, 325.0),
        }
    }

    /// A line across an item, to erase it.
    fn across(&self) -> [[f32; 2]; 3] {
        match self.tool {
            Tool::Text => [self.at(110.0, 285.0), self.at(110.0, 310.0), self.at(110.0, 345.0)],
            _ => [self.at(200.0, 285.0), self.at(200.0, 325.0), self.at(200.0, 365.0)],
        }
    }

    /// Make one item with the tool, as it is used (drawn, dragged out, clicked and typed), and let go of it.
    fn make(&mut self) -> u64 {
        self.make_lower(0.0)
    }

    /// The same, `down` points further down the page.
    fn make_lower(&mut self, down: f32) -> u64 {
        let before = self.items();
        match self.tool {
            Tool::Text => {
                let corner = self.at(100.0, 300.0 + down);
                self.tap(corner);
                self.ed.type_text("Hello");
                self.ed.finish_text();
            }
            _ => {
                let path = [self.at(100.0, 300.0 + down), self.at(200.0, 325.0 + down), self.at(300.0, 350.0 + down)];
                self.drag(&path);
            }
        }
        let made: Vec<u64> = self.items().into_iter().filter(|id| !before.contains(id)).collect();
        assert_eq!(made.len(), 1, "{:?} in the {:?} mode: one item made", self.tool, self.mode);
        self.ed.selection.clear();
        made[0]
    }

    fn first_point(&self, id: u64) -> [f32; 2] {
        self.ed.ink.get(id).expect("the item").pts[0]
    }

    /// The colour, thickness (or size) of an item.
    fn look(&self, id: u64) -> ([u8; 4], f32) {
        let s = self.ed.ink.get(id).expect("the item");
        (s.style.rgba, s.text.as_ref().map_or(s.style.width, |t| t.size))
    }

    fn label(&self) -> String {
        format!("{:?} in the {:?} mode", self.tool, self.mode)
    }
}

/// Run `check` for every tool in every mode.
fn for_each_tool_and_mode(check: impl Fn(&mut Rig)) {
    for tool in TOOLS {
        for mode in MODES {
            check(&mut Rig::new(tool, mode));
        }
    }
}

#[test]
fn every_tool_makes_an_item_in_both_modes() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        assert_eq!(rig.items(), [id], "{}", rig.label());
        // Undo takes it back and is one press of a button.
        rig.undo();
        assert!(rig.items().is_empty(), "{}", rig.label());
    });
}

#[test]
fn a_tap_with_select_chosen_selects_the_item_under_it_and_a_tap_elsewhere_lets_go() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        assert_eq!(rig.ed.selection, [id], "{}: a tap on it", rig.label());
        let off = rig.at(500.0, 650.0);
        rig.tap(off);
        assert!(rig.ed.selection.is_empty(), "{}: a tap elsewhere", rig.label());
    });
}

#[test]
fn a_loop_with_select_chosen_selects_what_it_encloses() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        rig.choose(Tool::Lasso);
        let ring = [rig.at(60.0, 260.0), rig.at(340.0, 260.0), rig.at(340.0, 400.0), rig.at(60.0, 400.0), rig.at(60.0, 262.0)];
        rig.drag(&ring);
        assert_eq!(rig.ed.selection, [id], "{}", rig.label());
    });
}

#[test]
fn a_selected_item_is_dragged_where_it_is_taken_and_undo_puts_it_back() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        let from = rig.first_point(id);
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        let to = [on[0] + 60.0, on[1] + 40.0];
        rig.drag(&[on, [on[0] + 30.0, on[1] + 20.0], to]);
        let moved = rig.first_point(id);
        assert!(
            (moved[0] - from[0] - 60.0).abs() < 1.0 && (moved[1] - from[1] - 40.0).abs() < 1.0,
            "{}: moved by {moved:?} from {from:?}",
            rig.label()
        );
        rig.undo();
        assert_eq!(rig.first_point(id), from, "{}: undone", rig.label());
    });
}

#[test]
fn the_colour_and_thickness_chosen_in_the_panel_restyle_what_is_selected_and_undo_restores_it() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        let (colour, size) = rig.look(id);
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        // The bar's style button opens the tool's panel; a colour pressed there is the item's colour.
        rig.bar(ContextItem::Style);
        assert!(rig.ed.chrome.panel.is_some(), "{}", rig.label());
        let panel = || rig.ed.toolbar().tool_panel(rig.ed.tool, rig.ed.view.viewport[0], false).expect("panel");
        let (swatch, wide) = (panel().swatch_centre(3), panel().point_on(Slider::Width, 0.95));
        let want = rig.ed.palette[3];
        rig.tap(swatch);
        assert_eq!(rig.look(id).0, want, "{}: colour", rig.label());
        assert_eq!(rig.ed.selection, [id], "{}: still selected", rig.label());
        rig.tap(wide);
        assert!(rig.look(id).1 > size, "{}: thicker or bigger ({} was {size})", rig.label(), rig.look(id).1);
        // The panel changed the tool too, so what is made next looks the same.
        assert_eq!(rig.ed.colour_of(rig.ed.style()), want, "{}", rig.label());
        // Undo: the size goes first (one step for the whole drag), then the colour.
        rig.undo();
        assert_eq!(rig.look(id), (want, size), "{}: size undone", rig.label());
        rig.undo();
        assert_eq!(rig.look(id), (colour, size), "{}: colour undone", rig.label());
    });
}

#[test]
fn a_colour_in_the_bar_is_one_tap_for_the_tool_and_for_what_is_selected_and_the_one_in_use_opens_its_editor() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        let (colour, size) = rig.look(id);
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        let want = rig.ed.palette[3];
        rig.bar(ContextItem::Swatch(3));
        assert_eq!(rig.look(id).0, want, "{}: the selected item took the colour", rig.label());
        assert_eq!(rig.ed.style().colour, 3, "{}: and so did the tool", rig.label());
        assert!(rig.ed.chrome.panel.is_none(), "{}: one tap, no panel", rig.label());
        rig.undo();
        assert_eq!(rig.look(id), (colour, size), "{}: undone", rig.label());
        // The colour in use again: its editor, in the panel unfolded to hold it.
        rig.bar(ContextItem::Swatch(3));
        assert_eq!(rig.ed.chrome.panel, Some(crate::toolbar::PanelState { editing: Some(3) }), "{}", rig.label());
        // Another colour while the editor is open: it moves to that colour.
        rig.bar(ContextItem::Swatch(1));
        assert_eq!(rig.ed.chrome.panel.and_then(|p| p.editing), Some(1), "{}", rig.label());
    });
}

#[test]
fn the_panel_unfolds_from_the_bar_as_one_card_whose_blank_parts_do_nothing() {
    for_each_tool_and_mode(|rig| {
        rig.bar(ContextItem::Style);
        let card = rig.ed.drawer().expect("the card");
        let bar = rig.ed.context_bar().expect("bar");
        assert!(card[0] == bar.rect[0] && card[2] == bar.rect[2] && card[1] == bar.rect[1] && card[3] > bar.rect[3], "{}: {card:?}", rig.label());
        // A press in a corner of the card, beside the panel's controls: nothing happens and the panel stays.
        let before = (rig.ed.ink.len(), rig.ed.ink.revision());
        let blank = [card[0] + 3.0, card[3] - 6.0];
        rig.tap(blank);
        assert!(rig.ed.chrome.panel.is_some(), "{}", rig.label());
        assert_eq!((rig.ed.ink.len(), rig.ed.ink.revision()), before, "{}", rig.label());
        // The settings button folds it up again.
        rig.bar(ContextItem::Style);
        assert!(rig.ed.chrome.panel.is_none() && rig.ed.drawer().is_none(), "{}", rig.label());
        // A tap on the page does too, and makes nothing.
        rig.bar(ContextItem::Style);
        let page = rig.at(500.0, 650.0);
        rig.tap(page);
        assert!(rig.ed.chrome.panel.is_none(), "{}", rig.label());
        assert_eq!(rig.ed.ink.len(), before.0, "{}: the press that put it away made nothing", rig.label());
    });
}

#[test]
fn dragging_the_thickness_slider_over_a_selection_is_one_undo_step() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        let (colour, size) = rig.look(id);
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        rig.bar(ContextItem::Style);
        let panel = rig.ed.toolbar().tool_panel(rig.ed.tool, rig.ed.view.viewport[0], false).expect("panel");
        let path: Vec<[f32; 2]> = [0.2, 0.4, 0.6, 0.8, 0.95].iter().map(|&t| panel.point_on(Slider::Width, t)).collect();
        rig.drag(&path);
        assert!(rig.look(id).1 > size, "{}: the drag made it bigger", rig.label());
        rig.undo();
        assert_eq!(rig.look(id), (colour, size), "{}: one undo takes the whole drag back", rig.label());
    });
}

#[test]
fn duplicate_copies_the_selection_and_selects_the_copy() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        let from = rig.first_point(id);
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        rig.bar(ContextItem::Duplicate);
        let items = rig.items();
        assert_eq!(items.len(), 2, "{}", rig.label());
        assert_eq!(rig.ed.selection, [items[1]], "{}: the copy is what is selected", rig.label());
        assert_eq!(rig.first_point(id), from, "{}: the original stays", rig.label());
        assert_ne!(rig.first_point(items[1]), from, "{}: the copy is somewhere else", rig.label());
        let (a, b) = (rig.ed.ink.get(id).expect("original"), rig.ed.ink.get(items[1]).expect("copy"));
        assert_eq!((a.style, a.text.clone(), a.figure.is_some()), (b.style, b.text.clone(), b.figure.is_some()), "{}: the same item", rig.label());
        rig.undo();
        assert_eq!(rig.items(), [id], "{}: undone", rig.label());
    });
}

#[test]
fn copy_then_paste_brings_the_item_back_as_often_as_asked_each_time_a_little_further() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        // (The text tool can always try: words may be on the system clipboard from somewhere else.)
        assert_eq!(rig.ed.can_paste(), rig.tool == Tool::Text, "{}: nothing to paste yet", rig.label());
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        rig.bar(ContextItem::Copy);
        assert!(rig.ed.can_paste(), "{}", rig.label());
        // Let go of the item, then paste.
        let off = rig.at(500.0, 650.0);
        rig.tap(off);
        assert!(rig.ed.selection.is_empty());
        rig.bar(ContextItem::Paste);
        rig.bar(ContextItem::Paste);
        let items = rig.items();
        assert_eq!(items.len(), 3, "{}", rig.label());
        let (a, b) = (rig.first_point(items[1]), rig.first_point(items[2]));
        assert_ne!(a, b, "{}: the second paste is not on top of the first", rig.label());
        assert_eq!(rig.ed.selection, [items[2]], "{}: the last paste is selected", rig.label());
        assert!(rig.ed.ink.get(id).is_some());
        rig.undo();
        rig.undo();
        assert_eq!(rig.items(), [id], "{}: undone", rig.label());
    });
}

#[test]
fn delete_takes_the_selection_away_and_undo_brings_it_back() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        rig.choose(Tool::Lasso);
        let on = rig.on_item();
        rig.tap(on);
        rig.bar(ContextItem::Delete);
        assert!(rig.items().is_empty() && rig.ed.selection.is_empty(), "{}", rig.label());
        rig.undo();
        assert_eq!(rig.items(), [id], "{}: undone", rig.label());
    });
}

#[test]
fn with_erase_chosen_the_tip_erases_what_it_passes_over_and_undo_brings_it_back() {
    for_each_tool_and_mode(|rig| {
        let id = rig.make();
        rig.choose(Tool::Eraser);
        let across = rig.across();
        rig.drag(&across);
        assert!(rig.items().is_empty(), "{}", rig.label());
        rig.undo();
        assert_eq!(rig.items(), [id], "{}: undone", rig.label());
        // Back to drawing, the tip makes things again.
        rig.choose(rig.tool);
        rig.make_lower(200.0);
    });
}

#[test]
fn a_tool_selects_erases_and_pastes_only_what_it_made() {
    // One item of every kind in the same place, and the clipboard holding them all.
    for tool in TOOLS {
        for mode in MODES {
            let mut rig = Rig::new(tool, mode);
            let ink = add_stroke(&mut rig.ed, &[[100.0, 300.0], [200.0, 325.0], [300.0, 350.0]]);
            let shape = add_shape(&mut rig.ed, [100.0, 300.0], [300.0, 350.0]);
            let text = add_text(&mut rig.ed, 100.0, 300.0, "Hello");
            let mine = match tool {
                Tool::Pen => ink,
                Tool::Geometry => shape,
                _ => text,
            };
            // Everything selected and copied (so the clipboard has all three), then nothing selected.
            rig.ed.selection = vec![ink, shape, text];
            assert!(rig.ed.copy_selection());
            rig.ed.selection.clear();
            // Select all picks out only the tool's own; the lasso and the eraser tools work on all three.
            rig.ed.select_all();
            assert_eq!(rig.ed.selection, [mine], "{}: select all", rig.label());
            rig.ed.selection.clear();
            rig.choose(Tool::Lasso);
            let ring = [rig.at(60.0, 260.0), rig.at(340.0, 260.0), rig.at(340.0, 400.0), rig.at(60.0, 400.0), rig.at(60.0, 262.0)];
            rig.drag(&ring);
            let mut picked = rig.ed.selection.clone();
            picked.sort_unstable();
            assert_eq!(picked, [ink, shape, text], "{}: a loop round all three", rig.label());
            rig.ed.selection.clear();
            rig.choose(Tool::Eraser);
            let across =
                [rig.at(110.0, 285.0), rig.at(110.0, 310.0), rig.at(110.0, 345.0), rig.at(200.0, 285.0), rig.at(200.0, 325.0), rig.at(200.0, 365.0)];
            rig.drag(&across);
            let left: Vec<u64> = rig.ed.ink.strokes().iter().map(|s| s.id).collect();
            assert!(!left.contains(&mine), "{}: the eraser", rig.label());
            // Back to the tool: paste brings back one item, of its kind.
            rig.choose(rig.tool);
            let before = rig.ed.ink.len();
            rig.ed.selection.clear();
            assert!(rig.ed.paste());
            assert_eq!(rig.ed.ink.len(), before + 1, "{}: paste", rig.label());
            assert_eq!(rig.items().len(), 1, "{}: it is the tool's kind", rig.label());
        }
    }
}

#[test]
fn at_a_desk_the_keyboard_does_all_of_it_too() {
    for tool in TOOLS {
        let mut rig = Rig::new(tool, Mode::Desktop);
        let id = rig.make();
        let from = rig.first_point(id);
        // Select all, nudge, let go.
        assert!(rig.ed.shortcut("a", false));
        assert_eq!(rig.ed.selection, [id], "{tool:?}: Ctrl+A");
        assert!(rig.ed.press(Key::Arrow(Dir::Right, false)));
        assert!(rig.ed.press(Key::Arrow(Dir::Down, true)));
        let at = rig.first_point(id);
        assert!((at[0] - from[0] - 2.0).abs() < 0.01 && (at[1] - from[1] - 20.0).abs() < 0.01, "{tool:?}: arrows nudge ({at:?} from {from:?})");
        assert!(rig.ed.press(Key::Escape));
        assert!(rig.ed.selection.is_empty(), "{tool:?}: Escape lets go");
        // Duplicate, copy, paste, cut.
        rig.ed.shortcut("a", false);
        assert!(rig.ed.shortcut("d", false));
        assert_eq!(rig.items().len(), 2, "{tool:?}: Ctrl+D");
        assert!(rig.ed.shortcut("c", false) && rig.ed.shortcut("v", false));
        assert_eq!(rig.items().len(), 3, "{tool:?}: Ctrl+C, Ctrl+V");
        assert!(rig.ed.shortcut("x", false));
        assert_eq!(rig.items().len(), 2, "{tool:?}: Ctrl+X");
        // Delete takes the selection away, Ctrl+Z puts it back.
        assert!(rig.ed.shortcut("a", false));
        assert!(rig.ed.press(Key::Delete));
        assert!(rig.items().is_empty(), "{tool:?}: Delete");
        assert!(rig.ed.shortcut("z", false));
        assert_eq!(rig.items().len(), 2, "{tool:?}: Ctrl+Z");
    }
}

#[test]
fn the_arrows_keep_an_item_that_is_on_its_page_on_it() {
    for tool in TOOLS {
        let mut rig = Rig::new(tool, Mode::Desktop);
        let id = rig.make();
        rig.ed.selection = vec![id];
        let page = rig.ed.page_rect(0).expect("page");
        for _ in 0..400 {
            rig.ed.press(Key::Arrow(Dir::Left, true));
            rig.ed.press(Key::Arrow(Dir::Up, true));
        }
        let b = rig.ed.ink.bounds_of(&[id]).expect("bounds");
        assert!(b[0] >= page[0] - 0.01 && b[1] >= page[1] - 0.01, "{tool:?}: {b:?} left the page {page:?}");
        // At the corner a further press does nothing, and the other way still works.
        let at = rig.first_point(id);
        rig.ed.press(Key::Arrow(Dir::Left, false));
        assert_eq!(rig.first_point(id), at, "{tool:?}");
        assert!(rig.ed.press(Key::Arrow(Dir::Right, false)), "{tool:?}");
    }
}

#[test]
fn as_a_tablet_there_is_no_keyboard_to_rely_on_so_it_does_nothing() {
    // (The text tool takes the keyboard in both modes, for typing; its Delete and arrows only work on text.)
    for tool in [Tool::Pen, Tool::Geometry] {
        let mut rig = Rig::new(tool, Mode::Tablet);
        let id = rig.make();
        rig.ed.selection = vec![id];
        let before = (rig.ed.ink.revision(), rig.ed.selection.clone());
        assert!(!rig.ed.press(Key::Delete) && !rig.ed.press(Key::Arrow(Dir::Left, false)) && !rig.ed.press(Key::Escape));
        for letter in ["a", "c", "x", "v", "d"] {
            assert!(!rig.ed.shortcut(letter, false), "{tool:?}: Ctrl+{letter}");
        }
        assert_eq!((rig.ed.ink.revision(), rig.ed.selection.clone()), before, "{tool:?}");
    }
    // Shortcuts with Ctrl are the desktop's, for every tool.
    let mut rig = Rig::new(Tool::Text, Mode::Tablet);
    let id = rig.make();
    rig.ed.selection = vec![id];
    for letter in ["a", "c", "x", "v", "d"] {
        assert!(!rig.ed.shortcut(letter, false), "Text: Ctrl+{letter}");
    }
    assert_eq!(rig.items(), [id]);
}

#[test]
fn the_bar_is_there_for_the_tools_that_make_items_and_not_for_the_pointer_or_while_the_prompt_is_up() {
    let mut ed = editor();
    for tool in TOOLS {
        ed.set_tool(tool);
        assert!(ed.context_bar().is_some(), "{tool:?}");
    }
    ed.set_tool(LASER);
    assert!(ed.context_bar().is_none() && ed.context_view().is_none());
    // A press where the bar would be reaches the page, not a button.
    ed.set_tool(Tool::Pen);
    let bar = ed.context_bar().expect("bar");
    let centre = bar.centre_of(ContextItem::Delete);
    assert!(ed.over_interface(centre));
    ed.set_tool(LASER);
    assert!(!ed.over_interface(centre));
}

#[test]
fn choosing_another_tool_after_the_eraser_gives_that_tool() {
    let mut rig = Rig::new(Tool::Pen, Mode::Tablet);
    rig.choose(Tool::Eraser);
    let at = rig.ed.toolbar().centre_of(Item::Tool(Tool::Geometry));
    rig.tap(at);
    assert_eq!(rig.ed.tool, Tool::Geometry);
}

#[test]
fn the_shape_buttons_edit_a_points_of_a_line_with_the_tip_alone() {
    for mode in MODES {
        let mut rig = Rig::new(Tool::Geometry, mode);
        let id = rig.make();
        // (The shape tool picks a shape up by a tap itself.)
        let on = rig.on_item();
        rig.tap(on);
        let nodes = |rig: &Rig| rig.ed.ink.get(id).and_then(|s| s.figure.as_ref()).map_or(0, |f| f.anchor_count());
        assert_eq!(nodes(&rig), 2);
        // A line has points that can be added: its buttons are in the bar.
        assert!(rig.ed.context_bar().is_some());
        rig.bar(ContextItem::Action(crate::tool::ToolAction::Node(crate::figure::NodeAction::Add)));
        assert_eq!(nodes(&rig), 3, "{mode:?}: a point added");
        // The one just added is the one that is selected: take it away again.
        rig.bar(ContextItem::Action(crate::tool::ToolAction::Node(crate::figure::NodeAction::Remove)));
        assert_eq!(nodes(&rig), 2, "{mode:?}: and taken away");
        rig.undo();
        assert_eq!(nodes(&rig), 3, "{mode:?}: undone");
    }
}
