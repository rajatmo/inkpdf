//! The window's own controls, as state: which toolbar button the pointer is over, whether the window fills the screen,
//! the "save your changes?" prompt, the tool's panel with the colour editor in it, and the press of a button under way
//! (see [`Act`]). Its small state machines (the prompt's buttons, the colour editor) need nothing but the document
//! state, so they live here and take it as an argument; what also needs the tools and the input devices stays with the
//! [`Editor`](super::Editor), which routes input and owns one of these.

use crate::colour::{self, Hsv};
use crate::input::{Device, Phase};
use crate::requests::{Choice, Request};
use crate::toolbar::{Channel, Item, PanelState, Pill, PromptLayout, PromptView};
use crate::workspace::Workspace;

use super::Act;

/// The pointer that pressed a button of the prompt, and which button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grab {
    dev: Device,
    id: u32,
    choice: Choice,
}

#[derive(Default)]
pub struct Chrome {
    /// The toolbar button the mouse is over.
    pub hover: Option<Item>,
    /// Whether the window fills the screen (kept in step with the window by the shell).
    pub fullscreen: bool,
    /// The "save your changes?" prompt, while it is up (see [`Editor::ask_unsaved`](super::Editor::ask_unsaved)).
    pub prompt: Option<PromptView>,
    prompt_grab: Option<Grab>,
    /// The panel the tool's settings unfold in, while it is open.
    pub panel: Option<PanelState>,
    /// The colour being made in the colour editor (kept as hue, saturation and brightness, so a grey does not forget
    /// its hue).
    pub colour_hsv: Hsv,
    /// The pages overview, while it is up (see [`super::overview`]).
    pub overview: Option<super::overview::OverviewState>,
    /// The round button under the theme button is shown (see [`super::theme_menu`]).
    pub theme_menu: bool,
    /// The pill unfolded from its round button, in a window too narrow for all three whole (see [`super::pills`]).
    pub pill: Option<Pill>,
    /// The bar under the tools is unfolded from its round button (see [`super::context`]).
    pub bar_open: bool,
    /// The press under way on the window's own controls, if any.
    pub(crate) act: Act,
}

impl Chrome {
    /// A new document: no press is under way, and no prompt is up.
    pub(super) fn clear_press_and_prompt(&mut self) {
        self.act = Act::Idle;
        self.pill = None;
        self.bar_open = false;
        self.theme_menu = false;
        self.overview = None;
        self.prompt = None;
        self.prompt_grab = None;
    }

    /// Put the prompt up, and everything else of the chrome away.
    pub(super) fn put_up_prompt(&mut self) {
        self.act = Act::Idle;
        self.overview = None;
        self.panel = None;
        self.pill = None;
        self.hover = None;
        self.prompt = Some(PromptView::default());
        self.prompt_grab = None;
    }

    /// One pointer's position and phase, while the prompt is up. A press on a button only counts if it is released on
    /// the same button.
    pub(super) fn prompt_pointer(&mut self, ws: &mut Workspace, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        ws.redraw = true;
        let over = PromptLayout::new(ws.view.viewport, ws.dpr).hit(pos);
        let mine = |grab: &Option<Grab>| grab.is_some_and(|g| g.dev == dev && g.id == id);
        match phase {
            Phase::Down => self.prompt_grab = over.map(|choice| Grab { dev, id, choice }),
            Phase::Up => {
                if mine(&self.prompt_grab)
                    && let Some(grab) = self.prompt_grab.take()
                    && over == Some(grab.choice)
                {
                    self.answer_with(ws, grab.choice);
                    return;
                }
            }
            Phase::Cancel => {
                if mine(&self.prompt_grab) {
                    self.prompt_grab = None;
                }
            }
            Phase::Move | Phase::Hover | Phase::Leave => {}
        }
        let pressed = self.prompt_grab.filter(|g| over == Some(g.choice)).map(|g| g.choice);
        self.prompt = Some(PromptView { hover: if phase == Phase::Leave { None } else { over }, pressed });
    }

    fn answer_with(&mut self, ws: &mut Workspace, choice: Choice) {
        ws.requests.push(Request::Answer(choice));
        self.prompt = None;
        self.prompt_grab = None;
        ws.redraw = true;
    }

    /// Open the colour editor for colour slot `slot` (starting from the colour it has now), or put it away (`None`).
    pub(super) fn set_editing(&mut self, ws: &mut Workspace, slot: Option<usize>) {
        let colour = slot.and_then(|s| ws.styles.palette.get(s));
        if slot.is_some() && colour.is_none() {
            return;
        }
        if let Some(c) = colour {
            self.colour_hsv = colour::rgb_to_hsv([c[0], c[1], c[2]]);
        }
        if let Some(panel) = &mut self.panel {
            panel.editing = slot;
        }
        ws.redraw = true;
    }

    /// Set one part of the colour being edited, and use the result as the tool's colour.
    pub(super) fn set_channel(&mut self, ws: &mut Workspace, channel: Channel, value: f32) {
        let Some(slot) = self.panel.and_then(|p| p.editing) else { return };
        let part = match channel {
            Channel::Hue => 0,
            Channel::Saturation => 1,
            Channel::Value => 2,
        };
        self.colour_hsv[part] = value;
        let rgb = colour::hsv_to_rgb(self.colour_hsv);
        if let Some(cell) = ws.styles.palette.get_mut(slot) {
            *cell = [rgb[0], rgb[1], rgb[2], 255];
            ws.style_mut().colour = slot;
        }
        ws.redraw = true;
    }
}
