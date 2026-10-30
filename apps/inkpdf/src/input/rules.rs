//! One table that says what a press on the page does, and which cursor to show over it. Pure: no window, no tool state. The cursor is
//! asked on a hover, the press on the press, and both read this, so the cursor cannot promise what the press does not do (see
//! `docs/INPUT.md`, B1). It is for the page: the toolbar, the cards and the two modal layers (the prompt, the pages overview) are the
//! chrome's, and over them the cursor is the arrow.
//!
//! The rules it states:
//!
//! * A finger never draws: it follows a link, or moves the view.
//! * In the tablet mode the mouse only scrolls. In the desktop mode it stands in for a stylus (the Text tool is the exception: there it is
//!   itself), and its right and middle buttons are the eraser end and the barrel button.
//! * **Ink cannot wait.** A drawing tool acts on the press, so it never loses a press to a link or to text: for every tool but Select, a link
//!   or a letter under the pointer is page paper. Reading (following a link, selecting text) is the Select tool's.
//! * The Select tool is a reader: its drag on empty page pans, and so does the middle button of a mouse.

use super::{Device, Mode, Role};
use crate::tool::Tool;

/// What is under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Page paper, or the window beside the pages.
    Empty,
    /// An item of ours: ink, a shape, a text box.
    Item,
    /// A markup (a highlight and the like) made here.
    Markup,
    /// A handle of a selection or of a shape.
    Handle,
    /// A letter of the PDF's own text.
    TextRun,
    /// A link of the PDF.
    Link,
}

#[cfg(test)]
impl Target {
    pub const ALL: [Target; 6] = [Target::Empty, Target::Item, Target::Markup, Target::Handle, Target::TextRun, Target::Link];
}

/// What a press begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing: the press is not this device's here.
    Nothing,
    /// The view follows the pointer.
    Pan,
    /// A click follows the link.
    FollowLink,
    /// A drag selects the PDF's text.
    SelectText,
    /// The tool's own mark: a stroke, a shape's draft, the laser's trail.
    Mark,
    /// A text box is placed or edited.
    PlaceText,
    /// A loop picks up items, or a tap picks one.
    Loop,
    /// What is under the pointer is taken hold of: moved, or stretched by a handle.
    Grab,
    /// Whatever the pointer passes over is taken away.
    Erase,
}

/// The shape of the pointer over something. The shell turns it into the system's cursor (or an image of its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    /// A link.
    Hand,
    /// Text to select, or a text box to type in.
    Text,
    Crosshair,
    /// A small dot: the pen and the laser.
    Dot,
    /// A ring the size of the eraser.
    Eraser,
    /// Something that can be moved.
    Move,
    /// A handle that stretches.
    Resize,
    /// The page can be moved by dragging, and while it is.
    Grab,
    Grabbing,
}

/// What a press will do and what the pointer should look like until it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Affordance {
    pub action: Action,
    pub cursor: CursorKind,
}

const fn is(action: Action, cursor: CursorKind) -> Affordance {
    Affordance { action, cursor }
}

const NOTHING: Affordance = is(Action::Nothing, CursorKind::Arrow);

/// The answer for a press on the page by `device` with `role` (what its buttons make of it: the tip or left button draws, the eraser end
/// or right button erases, the barrel button or middle button loops) over `target`, with `tool` in use in `mode`.
pub fn decide(mode: Mode, tool: Tool, device: Device, role: Role, target: Target) -> Affordance {
    // A finger moves the view. It has no hover, so there is no cursor to promise anything with.
    if device == Device::Touch {
        return match target {
            Target::Link => is(Action::FollowLink, CursorKind::Arrow),
            _ => is(Action::Pan, CursorKind::Arrow),
        };
    }
    // The mouse of a tablet only scrolls.
    if device == Device::Mouse && mode == Mode::Tablet {
        return NOTHING;
    }
    // The Text tool in the desktop mode is the mouse's, as itself; the stylus does nothing there (it types nothing).
    if tool == Tool::Text {
        if device == Device::Pen && mode == Mode::Desktop {
            return NOTHING;
        }
        return match (role, target) {
            (Role::Draw, Target::Item) => is(Action::PlaceText, CursorKind::Text),
            (Role::Draw, _) => is(Action::PlaceText, CursorKind::Crosshair),
            (Role::Erase, Target::Item) => is(Action::Erase, CursorKind::Eraser),
            (Role::Lasso, _) => is(Action::Loop, CursorKind::Crosshair),
            _ => NOTHING,
        };
    }
    let mouse = device == Device::Mouse;
    if tool == Tool::Select {
        return match (role, target) {
            (Role::Draw, Target::Link) => is(Action::FollowLink, CursorKind::Hand),
            (Role::Draw, Target::TextRun) => is(Action::SelectText, CursorKind::Text),
            (Role::Draw, Target::Handle) => is(Action::Grab, CursorKind::Resize),
            (Role::Draw, Target::Markup) => is(Action::Grab, CursorKind::Arrow),
            // (A press on one of our items in the reader is paper: the reader does not edit.)
            (Role::Draw, Target::Empty | Target::Item) => is(Action::Pan, CursorKind::Grab),
            // The middle button of a mouse pans in the reader; a barrel button loops. The eraser end and the right button do nothing.
            (Role::Lasso, _) if mouse => is(Action::Pan, CursorKind::Grab),
            (Role::Lasso, _) => is(Action::Loop, CursorKind::Crosshair),
            (Role::Erase, _) => NOTHING,
        };
    }
    // The other tools: ink cannot wait, so a link or a letter is paper.
    let target = match target {
        Target::Link | Target::TextRun => Target::Empty,
        other => other,
    };
    match role {
        Role::Erase => {
            // The eraser end erases with every tool that makes items, and with the lasso and the eraser; the laser and the like have none.
            if tool.selects_items() || tool.picks_any() { is(Action::Erase, CursorKind::Eraser) } else { NOTHING }
        }
        Role::Lasso => {
            if tool.selects_items() {
                is(Action::Loop, CursorKind::Crosshair)
            } else {
                NOTHING
            }
        }
        Role::Draw => match (tool, target) {
            (Tool::Eraser, _) => is(Action::Erase, CursorKind::Eraser),
            (Tool::Lasso, Target::Item | Target::Markup) => is(Action::Grab, CursorKind::Move),
            (Tool::Lasso, Target::Handle) => is(Action::Grab, CursorKind::Resize),
            (Tool::Lasso, _) => is(Action::Loop, CursorKind::Crosshair),
            (Tool::Pen, _) => is(Action::Mark, CursorKind::Dot),
            (Tool::Geometry, Target::Handle) => is(Action::Grab, CursorKind::Resize),
            (Tool::Geometry, Target::Item) => is(Action::Grab, CursorKind::Move),
            (Tool::Geometry, _) => is(Action::Mark, CursorKind::Crosshair),
            // An extension's tool: its own marks, the dot of a pointer.
            _ => is(Action::Mark, CursorKind::Dot),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: [Device; 3] = [Device::Mouse, Device::Touch, Device::Pen];
    const ROLES: [Role; 3] = [Role::Draw, Role::Erase, Role::Lasso];
    const MODES: [Mode; 2] = [Mode::Desktop, Mode::Tablet];

    /// Every combination the table is asked about.
    fn every() -> Vec<(Mode, Tool, Device, Role, Target)> {
        let mut all = Vec::new();
        for mode in MODES {
            for tool in Tool::all() {
                for device in DEVICES {
                    for role in ROLES {
                        for target in Target::ALL {
                            all.push((mode, tool, device, role, target));
                        }
                    }
                }
            }
        }
        all
    }

    #[test]
    fn every_combination_has_exactly_one_answer_and_it_is_the_same_each_time() {
        let all = every();
        assert!(all.len() > 6 * 2 * 3 * 3 * 6, "every tool is covered: {} combinations", all.len());
        for (mode, tool, device, role, target) in all {
            let a = decide(mode, tool, device, role, target);
            assert_eq!(a, decide(mode, tool, device, role, target), "{mode:?} {tool:?} {device:?} {role:?} {target:?}");
        }
    }

    #[test]
    fn a_finger_never_draws_it_follows_a_link_or_moves_the_view() {
        for (mode, tool, device, role, target) in every().into_iter().filter(|c| c.2 == Device::Touch) {
            let a = decide(mode, tool, device, role, target);
            let want = if target == Target::Link { Action::FollowLink } else { Action::Pan };
            assert_eq!(a.action, want, "{tool:?} {target:?}");
        }
    }

    #[test]
    fn the_mouse_of_a_tablet_only_scrolls() {
        for (mode, tool, device, role, target) in every().into_iter().filter(|c| c.0 == Mode::Tablet && c.2 == Device::Mouse) {
            assert_eq!(decide(mode, tool, device, role, target), NOTHING, "{tool:?} {role:?} {target:?}");
        }
    }

    #[test]
    fn ink_cannot_wait_so_a_drawing_tool_never_loses_a_press_to_a_link_or_to_text() {
        // (A finger follows a link with every tool: it is not a drawing device.)
        for (mode, tool, device, role, target) in every().into_iter().filter(|c| c.1 != Tool::Select && c.1 != Tool::Text && c.2 != Device::Touch) {
            if matches!(target, Target::Link | Target::TextRun) {
                assert_eq!(decide(mode, tool, device, role, target), decide(mode, tool, device, role, Target::Empty), "{tool:?} {device:?} {role:?}");
            }
        }
    }

    #[test]
    fn the_reader_reads_and_its_empty_page_drag_pans() {
        let read = |device, role, target| decide(Mode::Desktop, Tool::Select, device, role, target);
        for device in [Device::Mouse, Device::Pen] {
            assert_eq!(read(device, Role::Draw, Target::Link), is(Action::FollowLink, CursorKind::Hand));
            assert_eq!(read(device, Role::Draw, Target::TextRun), is(Action::SelectText, CursorKind::Text));
            assert_eq!(read(device, Role::Draw, Target::Handle), is(Action::Grab, CursorKind::Resize));
            assert_eq!(read(device, Role::Draw, Target::Empty), is(Action::Pan, CursorKind::Grab));
            assert_eq!(read(device, Role::Erase, Target::Empty), NOTHING, "no eraser in a reader");
        }
        assert_eq!(read(Device::Mouse, Role::Lasso, Target::Empty).action, Action::Pan, "the middle button pans");
        assert_eq!(read(Device::Pen, Role::Lasso, Target::Empty).action, Action::Loop, "the barrel button loops");
    }

    #[test]
    fn each_drawing_tool_has_its_own_cursor_and_the_text_tool_is_the_mouses_as_itself() {
        let on = |tool: Tool, target| decide(Mode::Desktop, tool, Device::Mouse, Role::Draw, target);
        assert_eq!(on(Tool::Pen, Target::Empty), is(Action::Mark, CursorKind::Dot));
        assert_eq!(on(Tool::Geometry, Target::Empty), is(Action::Mark, CursorKind::Crosshair));
        assert_eq!(on(Tool::Eraser, Target::Item), is(Action::Erase, CursorKind::Eraser));
        assert_eq!(on(Tool::Lasso, Target::Item), is(Action::Grab, CursorKind::Move));
        assert_eq!(on(Tool::Lasso, Target::Empty), is(Action::Loop, CursorKind::Crosshair));
        assert_eq!(on(Tool::Text, Target::Item), is(Action::PlaceText, CursorKind::Text));
        assert_eq!(on(Tool::Text, Target::Empty), is(Action::PlaceText, CursorKind::Crosshair));
        assert_eq!(decide(Mode::Desktop, Tool::Text, Device::Pen, Role::Draw, Target::Empty), NOTHING, "the stylus does not type at a desk");
        assert_eq!(decide(Mode::Tablet, Tool::Text, Device::Pen, Role::Draw, Target::Empty).action, Action::PlaceText);
    }

    #[test]
    fn the_hardware_buttons_work_in_every_tool_that_makes_items_and_only_there() {
        for tool in Tool::all().into_iter().filter(|t| *t != Tool::Select && *t != Tool::Text) {
            let erase = decide(Mode::Desktop, tool, Device::Pen, Role::Erase, Target::Empty).action;
            let lasso = decide(Mode::Desktop, tool, Device::Pen, Role::Lasso, Target::Empty).action;
            assert_eq!(erase != Action::Nothing, tool.selects_items() || tool.picks_any(), "the eraser end with {tool:?}");
            assert_eq!(lasso != Action::Nothing, tool.selects_items(), "the barrel button with {tool:?}");
        }
    }

    /// The pen's path allocates nothing and reads no table (`docs/INPUT.md`, B10): `decide` and the cursor are asked after a mouse moves or a
    /// button goes down or up, by the shell, and by nothing that a stylus's samples pass through. A source check, as the layering test is.
    #[test]
    fn the_pens_path_never_asks_the_rules_or_the_cursor() {
        let hot = [
            ("editor/pen.rs", include_str!("../editor/pen.rs")),
            ("editor/routing.rs", include_str!("../editor/routing.rs")),
            ("tools/pen.rs", include_str!("../tools/pen.rs")),
            ("tools/mod.rs", include_str!("../tools/mod.rs")),
            ("ink/stroke.rs", include_str!("../ink/stroke.rs")),
        ];
        for (name, source) in hot {
            assert!(!source.contains("decide(") && !source.contains("cursor_kind") && !source.contains("CursorKind"), "{name} is on the pen's path");
        }
    }
}
