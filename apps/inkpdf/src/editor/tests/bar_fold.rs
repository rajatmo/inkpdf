//! The bar under the tools folds into a round button where it would lie over the round button that hangs from the theme button, and unfolds when it is
//! pressed, with every device; a press on the page folds it again, and it stays whole while something is selected or its panel is open.

use super::*;
use crate::toolbar::ContextItem;

/// A window of 700 points (the bar and the round button under the theme button do not both fit under the toolbar) with the pen in use.
fn narrow() -> Editor {
    let mut ed = Editor::reference([700.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.set_tool(Tool::Pen);
    ed.chrome.panel = None;
    ed
}

fn folded(ed: &Editor) -> bool {
    ed.context_bar().is_some_and(|b| b.is_folded())
}

fn unfold_button(ed: &Editor) -> [f32; 2] {
    ed.context_bar().expect("a bar").centre_of(ContextItem::Unfold)
}

#[test]
fn the_bar_is_a_round_button_in_a_window_that_has_no_room_for_it_beside_the_theme_button_and_whole_in_a_wide_one() {
    let ed = narrow();
    assert!(folded(&ed), "the bar is folded at 700 points");
    let bar = ed.context_bar().expect("a bar");
    let toolbar = ed.toolbar();
    let r = bar.rect;
    assert!((r[2] - r[0] - (r[3] - r[1])).abs() < 0.5, "it is round: {r:?}");
    let button = toolbar.items()[0].1;
    assert!(r[3] - r[1] >= button[3] - button[1] - 0.5, "it is as big as a button");
    assert!(((r[0] + r[2]) / 2.0 - 350.0).abs() < 1.0 && r[1] > toolbar.bottom(), "in the middle, under the tools");
    let wide = Editor::reference([1600.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    let mut wide = wide;
    wide.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    wide.set_tool(Tool::Pen);
    assert!(!folded(&wide), "and whole at 1600");
}

#[test]
fn a_press_on_the_round_button_unfolds_the_bar_with_every_device_and_it_puts_the_round_button_under_the_theme_button_away_while_it_covers_its_place()
{
    for how in 0..3 {
        let mut ed = narrow();
        ed.chrome.theme_menu = true;
        assert!(ed.theme_layout().is_some(), "the theme button's round button is shown");
        let at = unfold_button(&ed);
        match how {
            0 => mouse_click_at(&mut ed, at),
            1 => pen_tap(&mut ed, at),
            _ => finger_tap(&mut ed, at),
        }
        assert!(!folded(&ed), "device {how}: the bar is whole");
        assert!(ed.theme_layout().is_none(), "device {how}: the round button under the theme button steps aside");
        assert_eq!(ed.ink.len(), 0, "device {how}: nothing was drawn");
        // A button of the bar is pressed, and the bar stays.
        let swatch = ed.context_bar().expect("a bar").centre_of(ContextItem::Swatch(2));
        pen_tap(&mut ed, swatch);
        assert!(!folded(&ed), "device {how}: pressing the bar leaves it whole");
        // A press on the page folds it (it puts the round button under the theme button away too, as it always did).
        let page = on_page(&ed, 300.0, 500.0);
        mouse_click_at(&mut ed, page);
        assert!(folded(&ed), "device {how}: a press on the page folds the bar again");
        assert!(!ed.theme_menu_open(), "device {how}");
    }
}

#[test]
fn the_bar_stays_whole_while_something_is_selected_or_the_panel_is_open_and_folds_again_when_the_tool_changes() {
    let mut ed = narrow();
    let id = add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
    ed.selection = vec![id];
    assert!(!folded(&ed), "with a selection the bar is whole: its buttons are what is wanted");
    ed.selection.clear();
    assert!(folded(&ed));
    ed.chrome.panel = Some(crate::toolbar::PanelState::default());
    assert!(!folded(&ed), "with its panel open it is whole");
    ed.chrome.panel = None;
    let at = unfold_button(&ed);
    pen_tap(&mut ed, at);
    assert!(!folded(&ed));
    ed.set_tool(Tool::Lasso);
    assert!(folded(&ed), "a new tool has the bar folded again");
}

#[test]
fn the_bar_waits_while_a_pill_is_unfolded_over_the_tools() {
    let mut ed = Editor::reference([720.0, 1280.0], 2.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    ed.set_tool(Tool::Pen);
    ed.chrome.pill = Some(crate::toolbar::Pill::View);
    if ed.toolbar().pills[1][2] <= ed.toolbar().pills[1][0] {
        assert!(ed.context_bar().is_none(), "no tools in view, no bar under them");
    }
    ed.chrome.pill = None;
    assert!(ed.context_bar().is_some(), "and it is back when the pill folds");
}
