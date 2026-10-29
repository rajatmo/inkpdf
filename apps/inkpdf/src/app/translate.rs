//! What winit says, in the program's own words (see [`crate::input::InputEvent`]): the one place its input types are named.
//! The editor knows none of them, so nothing but this file changes if the toolkit does.

use winit::event::{ElementState, MouseButton as Button, MouseScrollDelta, TouchPhase};
use winit::keyboard::{Key as Typed, NamedKey};

use crate::input::{ButtonState, Device, Dir, KeyInput, MouseButton, PenButtons, Phase, PointerEvent, Sample, Wheel};

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
