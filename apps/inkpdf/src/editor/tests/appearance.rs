//! The interface's look: the user's choice against the system's, the fade between two, and what is kept.

use super::*;
use crate::motion::Prefs;
use crate::palette::{DARK, LIGHT, Scheme};
use crate::requests::Request;
use crate::settings::Appearance;
use crate::tools::Key;

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
    let mut dark = DARK;
    dark.paper = [crate::palette::DARK_PAPER[0], crate::palette::DARK_PAPER[1], crate::palette::DARK_PAPER[2], 255];
    assert_eq!(
        fresh.palette(),
        dark,
        "a restored look is there from the first frame, not faded to (and the overview's paper is dark with the pages)"
    );
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
fn the_theme_button_drops_a_card_down_and_its_row_switches_whether_the_pages_follow_with_every_device() {
    let mut ed = editor();
    assert!(!ed.theme_menu_open() && ed.theme_layout().is_none());
    let theme = ed.toolbar().centre_of(Item::Theme);
    mouse_click_at(&mut ed, theme);
    assert!(ed.theme_menu_open(), "the button drops the card");
    assert_eq!(ed.appearance(), Appearance::Light, "and goes on to the next look");
    // The mouse presses the row.
    let row = ed.theme_layout().expect("the card").centre();
    mouse_click_at(&mut ed, row);
    assert!(ed.dark_pages() && ed.theme_menu_open(), "the row switched it on and the card stays");
    // The stylus tip too.
    pen_tap(&mut ed, row);
    assert!(!ed.dark_pages());
    assert!(drain(&mut ed).contains(&Request::SettingsChanged));
    // A finger, in an editor the pen has not been near.
    let mut finger = editor();
    let theme = finger.toolbar().centre_of(Item::Theme);
    finger.pointer_event(touch(1, Phase::Down, theme, 8.0));
    finger.pointer_event(touch(1, Phase::Up, theme, 8.0));
    let row = finger.theme_layout().expect("the card").centre();
    finger.pointer_event(touch(2, Phase::Down, row, 8.0));
    finger.pointer_event(touch(2, Phase::Up, row, 8.0));
    assert!(finger.dark_pages(), "a finger");
    assert_eq!(finger.ink.len(), 0);
}

#[test]
fn the_card_goes_on_a_press_elsewhere_escape_or_another_button_and_not_on_a_press_on_itself_or_its_button() {
    let open = |ed: &mut Editor| {
        let theme = ed.toolbar().centre_of(Item::Theme);
        pen_tap(ed, theme);
        assert!(ed.theme_menu_open());
    };
    // On the page.
    let mut ed = editor();
    open(&mut ed);
    let page = on_page(&ed, 300.0, 400.0);
    mouse_click_at(&mut ed, page);
    assert!(!ed.theme_menu_open(), "a press on the page puts it away");
    // On the card, and on the theme button again (which goes to the next look and keeps it).
    open(&mut ed);
    let row = ed.theme_layout().expect("card").centre();
    pen_tap(&mut ed, row);
    assert!(ed.theme_menu_open());
    let before = ed.appearance();
    let theme = ed.toolbar().centre_of(Item::Theme);
    pen_tap(&mut ed, theme);
    assert!(ed.theme_menu_open() && ed.appearance() != before);
    // Escape.
    assert!(ed.press(Key::Escape));
    assert!(!ed.theme_menu_open());
    // Another toolbar button.
    open(&mut ed);
    let pages = ed.toolbar().centre_of(Item::Search);
    pen_tap(&mut ed, pages);
    assert!(!ed.theme_menu_open());
    // A new document.
    ed.close_search();
    open(&mut ed);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.theme_menu_open());
}

#[test]
fn the_setting_is_kept_through_a_look_that_is_light_and_the_pages_are_dark_only_when_it_is_on_and_the_look_is_dark() {
    let mut ed = editor();
    let theme = ed.toolbar().centre_of(Item::Theme);
    pen_tap(&mut ed, theme);
    let row = ed.theme_layout().expect("card").centre();
    pen_tap(&mut ed, row);
    assert!(ed.dark_pages() && !ed.dark_pages_active(), "the look is light");
    pen_tap(&mut ed, theme);
    assert_eq!(ed.appearance(), Appearance::Dark);
    assert!(ed.dark_pages_active());
    assert_eq!(ed.theme_view().map(|v| (v.pages_follow, v.dark)), Some((true, true)));
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

#[test]
fn ink_is_shown_turned_over_on_dark_pages_and_what_is_kept_is_the_colour_chosen() {
    let mut ed = editor();
    let black = [28, 30, 36, 255];
    assert_eq!(ed.on_page(black), black, "light pages show the colour as it is");
    ed.set_appearance(Appearance::Dark);
    assert_eq!(ed.on_page(black), black, "a dark look alone does not touch the pages");
    ed.set_dark_pages(true);
    let shown = ed.on_page(black);
    assert!(shown[0] > 200 && shown[1] > 200 && shown[2] > 200, "black ink is light on a dark page: {shown:?}");
    let pts: Vec<[f32; 2]> = (0..20).map(|i| on_page(&ed, 100.0 + i as f32 * 8.0, 200.0 + (i as f32 * 0.5).sin() * 20.0)).collect();
    pen_stroke(&mut ed, TIP, &pts);
    assert_eq!(ed.ink.strokes()[0].style.rgba, black, "the stroke keeps the colour chosen");
    ed.set_appearance(Appearance::Light);
    assert_eq!(ed.on_page(black), black, "and it is as it was when the look is light again");
}

#[test]
fn the_overviews_paper_follows_the_pages() {
    let mut ed = editor();
    ed.step_view(0.016);
    assert_eq!(ed.palette().paper, LIGHT.paper);
    ed.set_appearance(Appearance::Dark);
    ed.set_dark_pages(true);
    ed.step_view(0.016);
    let paper = ed.palette().paper;
    assert_eq!([paper[0], paper[1], paper[2]], crate::palette::DARK_PAPER);
}
