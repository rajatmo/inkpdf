//! The device rules, as executable checks: feed the editor pen, touch, mouse and keyboard events
//! and see what each one is allowed to do.

mod anchors;
mod capability;
mod contract;
mod delete_page;
mod draft;
mod inputs;
mod latency_budget;
mod laser;
mod links;
mod modes;
mod motions;
mod overview;
mod pages;
mod pen;
mod prompt;
mod save_as;
mod shapes;
mod text;
mod text_editing;
mod toolbar;
mod touch;

use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use super::Request;
use super::*;
use crate::ink::{P, Style};
use crate::input::{Device, PenButtons, Phase, PointerEvent, Sample};
use crate::tool::{PALETTE, Tool, ToolStyle};
use crate::toolbar::{Channel, Item, PanelState, Slider};
use crate::view::Layout;

/// A 1200 x 900 window at 100% scale showing two 612 x 792 pages at zoom 1.
fn editor() -> Editor {
    let mut ed = Editor::new([1200.0, 900.0], 1.0);
    ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed
}

/// A point on page 0, in screen pixels.
fn on_page(ed: &Editor, x: f32, y: f32) -> [f32; 2] {
    ed.view.doc_to_screen([12.0 + x, 12.0 + y])
}

fn samples(pts: &[[f32; 2]], t0: f64) -> Vec<Sample> {
    pts.iter().enumerate().map(|(i, &pos)| Sample { pos, pressure: 0.5, t_ms: t0 + i as f64 * 4.0 }).collect()
}

const TIP: PenButtons = PenButtons { in_range: true, contact: true, eraser: false, barrel: false };
const ERASER: PenButtons = PenButtons { in_range: true, contact: true, eraser: true, barrel: false };
const BARREL: PenButtons = PenButtons { in_range: true, contact: true, eraser: false, barrel: true };
const HOVER: PenButtons = PenButtons { in_range: true, contact: false, eraser: false, barrel: false };

fn pen(phase: Phase, buttons: PenButtons, pts: &[[f32; 2]]) -> PointerEvent {
    PointerEvent { device: Device::Pen, id: 1, phase, samples: samples(pts, 1000.0), pen: buttons, contact_px: 0.0 }
}

/// A whole pen stroke: down at the first point, move through the rest, up at the last.
fn pen_stroke(ed: &mut Editor, buttons: PenButtons, pts: &[[f32; 2]]) {
    ed.pointer_event(pen(Phase::Down, buttons, &pts[..1]));
    ed.pointer_event(pen(Phase::Move, buttons, &pts[1..]));
    ed.pointer_event(pen(Phase::Up, buttons, &pts[pts.len() - 1..]));
}

fn touch(id: u32, phase: Phase, pos: [f32; 2], size: f32) -> PointerEvent {
    PointerEvent {
        device: Device::Touch,
        id,
        phase,
        samples: vec![Sample { pos, pressure: 1.0, t_ms: crate::clock::now_ms() }],
        pen: PenButtons::default(),
        contact_px: size,
    }
}

/// Let the view's glide run to its end.
fn settle(ed: &mut Editor) {
    for _ in 0..300 {
        ed.step_view(0.016);
    }
}

/// A stroke straight into the store, without going through the pen.
fn add_stroke(ed: &mut Editor, pts: &[[f32; 2]]) -> u64 {
    let doc: Vec<P> = pts.iter().map(|p| [12.0 + p[0], 12.0 + p[1]]).collect();
    ed.ink.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, doc).expect("stroke")
}

/// A straight line (a shape) straight into the store, without going through the shape tool.
fn add_shape(ed: &mut Editor, a: [f32; 2], b: [f32; 2]) -> u64 {
    let figure =
        crate::figure::Figure::from_shape(crate::geometry::Shape::Line, [12.0 + a[0], 12.0 + a[1]], [12.0 + b[0], 12.0 + b[1]]).expect("a line");
    ed.ink.add_figure(0, Style { rgba: [0, 0, 255, 255], width: 2.0 }, figure).expect("shape")
}

/// A text box straight into the store, its top-left corner at (`x`, `y`) on page 0.
fn add_text(ed: &mut Editor, x: f32, y: f32, words: &str) -> u64 {
    let outline = crate::text::outline([12.0 + x, 12.0 + y], words, 18.0);
    ed.ink
        .add_text(0, Style { rgba: [0, 0, 0, 255], width: 0.0 }, crate::ink::TextBlock { text: words.to_string(), size: 18.0 }, outline)
        .expect("text box")
}

/// Tap the settings button of the bar under the tools with the pen: the tool's panel unfolds (or folds up).
fn open_settings(ed: &mut Editor) {
    let at = ed.context_bar().expect("the bar").centre_of(crate::toolbar::ContextItem::Style);
    pen_stroke(ed, TIP, &[at, at]);
}

fn mouse_click_at(ed: &mut Editor, pos: [f32; 2]) {
    ed.mouse_moved(pos);
    ed.mouse_button(ElementState::Pressed, MouseButton::Left);
    ed.mouse_button(ElementState::Released, MouseButton::Left);
}
