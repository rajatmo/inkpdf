//! What winit says, in the program's own words (see [`crate::input::InputEvent`]): the one place its input types are named.
//! The editor knows none of them, so nothing but this file changes if the toolkit does.

use winit::event::{ElementState, MouseButton as Button, MouseScrollDelta, TouchPhase};
use winit::keyboard::{Key as Typed, NamedKey};
use winit::window::CursorIcon;

use crate::input::{ButtonState, CursorKind, Device, Dir, KeyInput, MouseButton, PenButtons, Phase, PointerEvent, Sample, Wheel};

/// A mouse button: one of the three the program uses, or `Other` (back, forward, …), which it ignores.
pub(super) fn button(button: Button) -> MouseButton {
    match button {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
        _ => MouseButton::Other,
    }
}

pub(super) fn button_state(state: ElementState) -> ButtonState {
    match state {
        ElementState::Pressed => ButtonState::Pressed,
        ElementState::Released => ButtonState::Released,
    }
}

pub(super) fn wheel(delta: MouseScrollDelta) -> Wheel {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Wheel::Lines(x, y),
        MouseScrollDelta::PixelDelta(p) => Wheel::Pixels(p.x as f32, p.y as f32),
    }
}

/// A cursor drawn in code, for the kinds the system has none for: the pen's dot and the eraser's ring, dark with a light edge so that they show on
/// paper and on ink alike. RGBA, square, with its hot spot in the middle; None for a kind the system's own cursor serves.
pub(super) fn cursor_image(kind: CursorKind) -> Option<(Vec<u8>, u16)> {
    // (The size in pixels, and where the dark part is: from inner to outer from the middle.)
    let (size, inner, outer) = match kind {
        CursorKind::Dot => (21u16, 0.0_f32, 3.5_f32),
        CursorKind::Eraser => (33, 10.5, 12.5),
        _ => return None,
    };
    let middle = f32::from(size) / 2.0;
    // How much a pixel at distance d from the middle is within  to , with a soft edge.
    let within = |d: f32, a: f32, b: f32| (if a <= 0.0 { 1.0 } else { (d - a + 0.5).clamp(0.0, 1.0) }) * (b - d + 0.5).clamp(0.0, 1.0);
    let mut rgba = Vec::with_capacity(usize::from(size) * usize::from(size) * 4);
    for y in 0..size {
        for x in 0..size {
            let d = (f32::from(x) + 0.5 - middle).hypot(f32::from(y) + 0.5 - middle);
            let dark = within(d, inner, outer);
            let halo = within(d, (inner - 1.6).max(0.0), outer + 1.6) * 0.9;
            let alpha = dark + halo * (1.0 - dark);
            let ink = [28.0, 30.0, 36.0];
            let colour = |c: usize| if alpha > 0.0 { (dark * ink[c] + (1.0 - dark) * halo * 255.0) / alpha } else { 0.0 };
            rgba.extend([colour(0) as u8, colour(1) as u8, colour(2) as u8, (alpha * 255.0).round() as u8]);
        }
    }
    Some((rgba, size))
}

/// The system's cursor for what the rules ask for. (The pen's dot and the eraser's ring have images of their own, see [`cursor_image`]; a
/// crosshair stands in where the window system will not take one. No window system shows a cursor for a stylus anyway.)
pub(super) fn cursor_icon(kind: CursorKind) -> CursorIcon {
    match kind {
        CursorKind::Arrow => CursorIcon::Default,
        CursorKind::Hand => CursorIcon::Pointer,
        CursorKind::Text => CursorIcon::Text,
        CursorKind::Crosshair | CursorKind::Dot | CursorKind::Eraser => CursorIcon::Crosshair,
        CursorKind::Move => CursorIcon::Move,
        CursorKind::Resize => CursorIcon::ColResize,
        CursorKind::Grab => CursorIcon::Grab,
        CursorKind::Grabbing => CursorIcon::Grabbing,
    }
}

/// A turn of the wheel as notches of zoom (positive is in): a wheel's notches as they are, a touchpad's pixels at 120 to a notch.
pub(super) fn wheel_steps(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
    }
}

/// A touch that is only a place (no pen or size): its phase as the pointer layer's.
pub(super) fn touch_phase(phase: TouchPhase) -> Phase {
    match phase {
        TouchPhase::Started => Phase::Down,
        TouchPhase::Moved => Phase::Move,
        TouchPhase::Ended => Phase::Up,
        TouchPhase::Cancelled => Phase::Cancel,
    }
}

/// A touch from the toolkit as a pointer event of the kind the OS's own reader makes: a finger (the toolkit cannot tell a pen from
/// one), at the place, with the force it reports (1 if none), stamped `now_ms`. It presses buttons, pans and zooms as every finger does.
pub(super) fn touch(touch: &winit::event::Touch, now_ms: f64) -> PointerEvent {
    let pressure = match touch.force {
        Some(winit::event::Force::Normalized(f)) => f as f32,
        Some(winit::event::Force::Calibrated { force, max_possible_force, .. }) if max_possible_force > 0.0 => (force / max_possible_force) as f32,
        _ => 1.0,
    };
    PointerEvent {
        device: Device::Touch,
        id: touch.id as u32,
        phase: touch_phase(touch.phase),
        samples: vec![Sample {
            pos: [touch.location.x as f32, touch.location.y as f32],
            pressure: if pressure.is_finite() { pressure.clamp(0.0, 1.0) } else { 1.0 },
            t_ms: now_ms,
        }],
        pen: PenButtons::default(),
        contact_px: 0.0,
    }
}

/// The key that was pressed, if the program hears it. Which of them it hears depends on what is being typed into (the tool in use
/// says, see `InputPolicy`): a text box or the Save As card takes them all (`typing`); otherwise only the keys that work on a
/// selection (Delete, Escape, the arrows) are of use, and the editor leaves the rest to the tool.
pub(super) fn key(logical: &Typed, text: Option<&str>, shift: bool, typing: bool) -> Option<KeyInput> {
    Some(match logical {
        Typed::Named(NamedKey::Backspace) => KeyInput::Backspace,
        Typed::Named(NamedKey::Delete) => KeyInput::Delete,
        Typed::Named(NamedKey::Escape) => KeyInput::Escape,
        Typed::Named(NamedKey::ArrowLeft) => KeyInput::Arrow(Dir::Left, shift),
        Typed::Named(NamedKey::ArrowRight) => KeyInput::Arrow(Dir::Right, shift),
        Typed::Named(NamedKey::ArrowUp) => KeyInput::Arrow(Dir::Up, shift),
        Typed::Named(NamedKey::ArrowDown) => KeyInput::Arrow(Dir::Down, shift),
        Typed::Named(NamedKey::Home) => KeyInput::Home(shift),
        Typed::Named(NamedKey::End) => KeyInput::End(shift),
        // (A page at a time: Page Down, and the space bar when nothing is being typed; Shift+Space and Page Up go back.)
        Typed::Named(NamedKey::PageDown) => KeyInput::Page(true),
        Typed::Named(NamedKey::PageUp) => KeyInput::Page(false),
        Typed::Named(NamedKey::Space) if !typing => KeyInput::Page(!shift),
        Typed::Named(NamedKey::Enter) if typing => KeyInput::Text("\n".to_string()),
        _ if typing => KeyInput::Text(text?.to_string()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalPosition;

    use super::*;

    #[test]
    fn the_pens_dot_and_the_erasers_ring_are_images_with_their_hot_spot_in_the_middle_and_the_others_are_the_systems() {
        let (dot, size) = cursor_image(CursorKind::Dot).expect("a dot");
        let (ring, big) = cursor_image(CursorKind::Eraser).expect("a ring");
        assert_eq!((dot.len(), ring.len()), (usize::from(size) * usize::from(size) * 4, usize::from(big) * usize::from(big) * 4));
        let alpha = |image: &[u8], size: u16, x: u16, y: u16| image[(usize::from(y) * usize::from(size) + usize::from(x)) * 4 + 3];
        assert!(alpha(&dot, size, size / 2, size / 2) > 250, "the dot is solid in the middle");
        assert!(alpha(&ring, big, big / 2, big / 2) == 0, "the ring is open in the middle, where the page shows");
        assert!(alpha(&ring, big, big / 2, 3) > 200, "and drawn near its edge");
        assert_eq!(alpha(&dot, size, 0, 0), 0, "and nothing in the corners");
        for kind in [CursorKind::Arrow, CursorKind::Hand, CursorKind::Text, CursorKind::Crosshair, CursorKind::Grab] {
            assert!(cursor_image(kind).is_none(), "{kind:?}");
        }
    }

    #[test]
    fn the_three_buttons_the_program_uses_are_translated_and_the_others_are_other() {
        assert_eq!(button(Button::Left), MouseButton::Left);
        assert_eq!(button(Button::Right), MouseButton::Right);
        assert_eq!(button(Button::Middle), MouseButton::Middle);
        for other in [Button::Back, Button::Forward, Button::Other(7)] {
            assert_eq!(button(other), MouseButton::Other, "{other:?}");
        }
        assert_eq!(button_state(ElementState::Pressed), ButtonState::Pressed);
        assert_eq!(button_state(ElementState::Released), ButtonState::Released);
    }

    #[test]
    fn a_wheel_keeps_its_numbers_and_its_kind() {
        assert_eq!(wheel(MouseScrollDelta::LineDelta(1.5, -2.0)), Wheel::Lines(1.5, -2.0));
        assert_eq!(wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(12.0, -40.5))), Wheel::Pixels(12.0, -40.5));
    }

    #[test]
    fn each_phase_of_a_touch_is_the_pointer_layers() {
        assert_eq!(
            [TouchPhase::Started, TouchPhase::Moved, TouchPhase::Ended, TouchPhase::Cancelled].map(touch_phase),
            [Phase::Down, Phase::Move, Phase::Up, Phase::Cancel]
        );
    }

    #[test]
    fn named_keys_are_heard_always_and_typed_text_only_while_something_takes_it() {
        let named = |k: NamedKey, shift: bool, typing: bool| key(&Typed::Named(k), None, shift, typing);
        assert_eq!(named(NamedKey::Backspace, false, false), Some(KeyInput::Backspace));
        assert_eq!(named(NamedKey::ArrowLeft, true, false), Some(KeyInput::Arrow(Dir::Left, true)));
        assert_eq!(named(NamedKey::Home, false, true), Some(KeyInput::Home(false)));
        assert_eq!(named(NamedKey::Enter, false, true), Some(KeyInput::Text("\n".to_string())), "a new line, when typing");
        assert_eq!(named(NamedKey::Enter, false, false), None, "and nothing otherwise");
        let letter = Typed::Character("a".into());
        assert_eq!(key(&letter, Some("a"), false, true), Some(KeyInput::Text("a".to_string())));
        assert_eq!(key(&letter, Some("a"), false, false), None, "a letter is for nothing when nothing is typed into");
        assert_eq!(key(&letter, None, false, true), None, "a key with no text types nothing");
    }
}
