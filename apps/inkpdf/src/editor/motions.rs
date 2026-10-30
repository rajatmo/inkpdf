//! The editor's motion state: every animation of the interface, held in one place and stepped once per frame. What is
//! animated is told to it as where things are heading (the look they would rest in); it moves the numbers there, and
//! hands the drawing plain values (see [`crate::toolbar::Look`]), so the drawing knows nothing of time. With reduced
//! motion every duration is nothing, and the look is the resting one at once.

use crate::motion::{Curve, Prefs, Presence, Tween, seconds};
use crate::toolbar::{BUTTONS, Look, PanelView, PromptView};

pub struct Motions {
    prefs: Prefs,
    /// Whether the toolbar's values have been put where they rest (the first time, they start there, not at nothing).
    primed: bool,
    hover: [Tween; BUTTONS],
    enabled: [Tween; BUTTONS],
    ripple: [Tween; BUTTONS],
    tool: Tween,
    unsaved: Tween,
    /// The button a press is on, to start a ripple when it is a new one.
    pressed: Option<usize>,
    /// How much of the overview that has just been put away is still there (the shell draws it as it was while it goes).
    overview_out: Tween,
    /// The "save your changes?" prompt, and the tool's panel: kept while they go out, so that they can be drawn so.
    prompt: Presence<PromptView>,
    panel: Presence<PanelView>,
    /// How far the bar under the tools has come in.
    bar_in: Tween,
    /// How dark the interface is, 0 (the light scheme) to 1 (the dark one): a change of look is a fade between them.
    theme: Tween,
    theme_primed: bool,
}

impl Default for Motions {
    fn default() -> Self {
        Self::new(Prefs::default())
    }
}

impl Motions {
    pub fn new(prefs: Prefs) -> Self {
        Self {
            prefs,
            primed: false,
            hover: [Tween::at(0.0); BUTTONS],
            enabled: [Tween::at(1.0); BUTTONS],
            ripple: [Tween::at(0.0); BUTTONS],
            tool: Tween::at(0.0),
            unsaved: Tween::at(0.0),
            pressed: None,
            overview_out: Tween::at(0.0),
            prompt: Presence::default(),
            panel: Presence::default(),
            bar_in: Tween::at(1.0),
            theme: Tween::at(0.0),
            theme_primed: false,
        }
    }

    pub fn prefs(&self) -> Prefs {
        self.prefs
    }

    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.prefs = prefs;
    }

    /// Where the toolbar is heading: `rest` is how it looks when it has settled, and `pressed` the button a press is on.
    pub fn retarget_toolbar(&mut self, rest: &Look, pressed: Option<usize>) {
        if !self.primed {
            self.primed = true;
            for slot in 0..BUTTONS {
                self.hover[slot].jump(rest.hover[slot]);
                self.enabled[slot].jump(rest.enabled[slot]);
            }
            self.tool.jump(rest.tool);
            self.unsaved.jump(rest.unsaved);
        }
        let p = self.prefs;
        for slot in 0..BUTTONS {
            self.hover[slot].go(rest.hover[slot], p.time(seconds::QUICK), Curve::Standard);
            self.enabled[slot].go(rest.enabled[slot], p.time(seconds::SHORT), Curve::Standard);
        }
        self.tool.go(rest.tool, p.time(seconds::MEDIUM), Curve::Standard);
        self.unsaved.go(rest.unsaved, p.time(seconds::SHORT), Curve::Standard);
        if pressed != self.pressed {
            if let Some(slot) = pressed {
                // A press spreads from the middle of the button.
                self.ripple[slot].jump(0.0);
                self.ripple[slot].go(1.0, p.time(seconds::MEDIUM), Curve::Linear);
            }
            self.pressed = pressed;
        }
    }

    /// The interface is heading for the dark scheme (or the light one): it fades there (at once the first time, and with reduced motion).
    pub fn retarget_theme(&mut self, dark: bool) {
        let target = f32::from(dark);
        if !self.theme_primed || self.prefs.reduced {
            self.theme_primed = true;
            self.theme.jump(target);
        } else {
            self.theme.go(target, self.prefs.time(seconds::MEDIUM), Curve::Standard);
        }
    }

    /// How far from the light scheme to the dark one the interface is now, 0 to 1.
    pub fn theme_amount(&self) -> f32 {
        self.theme.value()
    }

    /// Where the prompt, the tool's panel and the bar under the tools are heading: `now` is each as it is wanted now.
    pub fn retarget_drawers(&mut self, prompt: Option<PromptView>, panel: Option<PanelView>, bar: bool) {
        let p = self.prefs;
        match prompt {
            Some(view) => self.prompt.show(view, p.time(seconds::MEDIUM)),
            None => self.prompt.hide(p.time(seconds::SHORT)),
        }
        match panel {
            Some(view) => self.panel.show(view, p.time(seconds::MEDIUM)),
            None => self.panel.hide(p.time(seconds::SHORT)),
        }
        if bar {
            self.bar_in.go(1.0, p.time(seconds::MEDIUM), Curve::Decelerate);
        } else {
            // (It goes out as it was: the editor keeps what it looked like and draws that while it does.)
            self.bar_in.go(0.0, p.time(seconds::SHORT), Curve::Accelerate);
        }
        if p.reduced {
            self.prompt.snap();
            self.panel.snap();
            self.bar_in.jump(f32::from(bar));
        }
    }

    /// The prompt as it is drawn now, and how much of it (0 to 1): it grows in, and goes out as it was.
    pub fn prompt_look(&self, now: Option<PromptView>) -> Option<(PromptView, f32)> {
        if self.prefs.reduced { now.map(|v| (v, 1.0)) } else { self.prompt.current().map(|(v, a)| (*v, a)) }
    }

    /// The tool's panel, the same way.
    pub fn panel_look(&self, now: Option<PanelView>) -> Option<(PanelView, f32)> {
        if self.prefs.reduced { now.map(|v| (v, 1.0)) } else { self.panel.current().map(|(v, a)| (*v, a)) }
    }

    /// How far the bar under the tools has come in, 0 to 1.
    pub fn bar_amount(&self) -> f32 {
        self.bar_in.value()
    }

    /// The overview was just put away: it goes out over a moment.
    pub fn overview_left(&mut self) {
        self.overview_out.jump(1.0);
        self.overview_out.go(0.0, self.prefs.time(seconds::SHORT), Curve::Accelerate);
    }

    /// The overview is up (again): none of it is going out.
    pub fn overview_back(&mut self) {
        self.overview_out.jump(0.0);
    }

    pub fn overview_leaving(&self) -> f32 {
        self.overview_out.value()
    }

    /// The toolbar as it looks now: `rest` is how it would look settled (what reduced motion shows).
    pub fn toolbar_look(&self, rest: &Look) -> Look {
        if self.prefs.reduced || !self.primed {
            return *rest;
        }
        let mut look = *rest;
        for slot in 0..BUTTONS {
            look.hover[slot] = self.hover[slot].value();
            look.enabled[slot] = self.enabled[slot].value();
            look.ripple[slot] = self.ripple[slot].value();
        }
        look.tool = self.tool.value();
        look.unsaved = self.unsaved.value();
        look
    }

    /// Let `dt` seconds go by. Whether anything is still moving (the frame is drawn again if so, and not otherwise).
    pub fn step(&mut self, dt: f32) -> bool {
        let mut moving = false;
        for slot in 0..BUTTONS {
            moving |= self.hover[slot].step(dt);
            moving |= self.enabled[slot].step(dt);
            moving |= self.ripple[slot].step(dt);
        }
        moving |= self.tool.step(dt);
        moving |= self.unsaved.step(dt);
        moving |= self.overview_out.step(dt);
        moving |= self.prompt.step(dt);
        moving |= self.panel.step(dt);
        moving |= self.bar_in.step(dt);
        moving |= self.theme.step(dt);
        moving
    }
}
