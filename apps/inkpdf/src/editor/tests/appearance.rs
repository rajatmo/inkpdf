//! The interface's look: the user's choice against the system's, the fade between two, and what is kept.

use super::*;
use crate::motion::Prefs;
use crate::palette::{DARK, LIGHT, Scheme};
use crate::requests::Request;
use crate::settings::Appearance;

fn drain(ed: &mut Editor) -> Vec<Request> {
    std::iter::from_fn(|| ed.requests().pop()).collect()
}

#[test]
fn it_starts_in_the_systems_look_and_follows_it() {
    let mut ed = editor();
    assert_eq!(ed.appearance(), Appearance::System);
    assert_eq!(ed.scheme(), Scheme::Light);
    ed.set_system_scheme(Scheme::Dark);
    assert_eq!(ed.scheme(), Scheme::Dark);
    ed.step_view(0.016);
    assert_eq!(ed.palette(), DARK);
    ed.set_system_scheme(Scheme::Light);
    ed.step_view(0.016);
    assert_eq!(ed.palette(), LIGHT);
}

#[test]
fn a_chosen_look_wins_over_the_system_and_is_asked_to_be_saved() {
    let mut ed = editor();
    ed.set_system_scheme(Scheme::Dark);
    ed.set_appearance(Appearance::Light);
    assert_eq!(ed.scheme(), Scheme::Light, "the user's choice beats the system's");
    ed.set_system_scheme(Scheme::Light);
    ed.set_appearance(Appearance::Dark);
    assert_eq!(ed.scheme(), Scheme::Dark);
    assert!(drain(&mut ed).contains(&Request::SettingsChanged));
    // The same choice again is nothing.
    ed.set_appearance(Appearance::Dark);
    assert!(drain(&mut ed).is_empty());
}

#[test]
fn the_button_goes_system_light_dark_and_round() {
    let mut ed = editor();
    let seen: Vec<_> = (0..4)
        .map(|_| {
            ed.cycle_appearance();
            ed.appearance()
        })
        .collect();
    assert_eq!(seen, [Appearance::Light, Appearance::Dark, Appearance::System, Appearance::Light]);
}

#[test]
fn the_colours_fade_between_the_two_looks_when_motion_is_on_and_jump_when_it_is_off() {
    let mut ed = editor();
    ed.motions.set_prefs(Prefs::FULL);
    ed.step_view(0.016);
    ed.set_appearance(Appearance::Dark);
    ed.step_view(0.016);
    let early = ed.palette();
    assert!(early != LIGHT && early != DARK, "on its way");
    assert_eq!(ed.ws.theme, early, "the tools paint in what the interface is drawn in");
    for _ in 0..120 {
        ed.step_view(0.016);
    }
    assert_eq!(ed.palette(), DARK);

    let mut still = editor();
    still.set_appearance(Appearance::Dark);
    still.step_view(0.016);
    assert_eq!(still.palette(), DARK, "reduced motion changes at once");
}

#[test]
fn dark_pages_need_the_setting_and_a_dark_look() {
    let mut ed = editor();
    ed.set_dark_pages(true);
    assert!(ed.dark_pages());
    assert!(!ed.dark_pages_active(), "the look is light");
    ed.set_appearance(Appearance::Dark);
    assert!(ed.dark_pages_active());
    ed.set_appearance(Appearance::Light);
    assert!(!ed.dark_pages_active());
    assert!(ed.dark_pages(), "the setting is kept while the look is light");
}

#[test]
fn the_look_goes_through_the_settings_and_back() {
    let mut ed = editor();
    ed.set_appearance(Appearance::Dark);
    ed.set_dark_pages(true);
    let saved = ed.settings();
    assert_eq!(saved.appearance, Appearance::Dark);
    assert!(saved.dark_pages);
    let mut fresh = editor();
    fresh.apply_settings(&saved);
    assert_eq!(fresh.appearance(), Appearance::Dark);
    assert!(fresh.dark_pages());
    fresh.step_view(0.016);
    assert_eq!(fresh.palette(), DARK, "a restored look is there from the first frame, not faded to");
}

fn pen_tap(ed: &mut Editor, at: [f32; 2]) {
    pen_stroke(ed, TIP, &[at, at]);
}

#[test]
fn the_theme_button_works_with_the_left_button_alone_and_with_the_tip_alone() {
    let mut ed = editor();
    let theme = ed.toolbar().centre_of(Item::Theme);
    mouse_click_at(&mut ed, theme);
    assert_eq!(ed.appearance(), Appearance::Light);
    pen_tap(&mut ed, theme);
    assert_eq!(ed.appearance(), Appearance::Dark);
    mouse_click_at(&mut ed, theme);
    assert_eq!(ed.appearance(), Appearance::System);
    assert_eq!(ed.ink.len(), 0, "the presses drew nothing on the page");
}

#[test]
fn the_dark_pages_button_switches_the_setting_with_either_device() {
    let mut ed = editor();
    let at = ed.toolbar().centre_of(Item::DarkPages);
    mouse_click_at(&mut ed, at);
    assert!(ed.dark_pages());
    pen_tap(&mut ed, at);
    assert!(!ed.dark_pages());
    assert!(drain(&mut ed).contains(&Request::SettingsChanged));
}

#[test]
fn the_buttons_show_the_choice_they_hold() {
    let mut ed = editor();
    let before = ed.toolbar_state();
    assert_eq!((before.appearance, before.dark, before.dark_pages), (Appearance::System, false, false));
    ed.set_appearance(Appearance::Dark);
    ed.set_dark_pages(true);
    let after = ed.toolbar_state();
    assert_eq!((after.appearance, after.dark, after.dark_pages), (Appearance::Dark, true, true));
    let draw = |state| {
        let mut ui = crate::toolbar::Ui::default();
        ed.toolbar().draw(&state, &mut ui);
        format!("{:?}", ui.lines)
    };
    assert_ne!(draw(before), draw(after), "another icon, another colour");
}
