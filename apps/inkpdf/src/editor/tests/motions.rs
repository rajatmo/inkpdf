//! The interface's animations, as executable checks. The other tests run with reduced motion (what they look at is where
//! things end up); these turn motion on and drive the frames themselves.

use super::*;
use crate::motion::Prefs;
use crate::toolbar::Look;

fn moving_editor() -> Editor {
    let mut ed = editor();
    ed.motions.set_prefs(Prefs::FULL);
    // The first frame puts the toolbar where it rests.
    ed.step_view(0.016);
    ed
}

fn frames(ed: &mut Editor, n: usize) {
    for _ in 0..n {
        ed.step_view(0.016);
    }
}

fn slot(ed: &Editor, item: Item) -> usize {
    ed.toolbar().slot(item).expect("a button")
}

#[test]
fn the_wash_under_the_pointer_fades_in_and_out_instead_of_switching() {
    let mut ed = moving_editor();
    let (pages, at) = (slot(&ed, Item::Pages), ed.toolbar().centre_of(Item::Pages));
    assert_eq!(ed.toolbar_look().hover[pages], 0.0);
    ed.mouse_moved(at);
    ed.step_view(0.016);
    let early = ed.toolbar_look().hover[pages];
    assert!(early > 0.0 && early < 1.0, "on its way: {early}");
    let mut last = early;
    for _ in 0..20 {
        ed.step_view(0.016);
        let now = ed.toolbar_look().hover[pages];
        assert!(now >= last, "never backwards");
        last = now;
    }
    assert_eq!(last, 1.0, "lit");
    ed.mouse_moved([600.0, 600.0]);
    ed.step_view(0.016);
    let going = ed.toolbar_look().hover[pages];
    assert!(going > 0.0 && going < 1.0, "fading out: {going}");
    frames(&mut ed, 20);
    assert_eq!(ed.toolbar_look().hover[pages], 0.0);
}

#[test]
fn a_press_spreads_from_the_middle_of_the_button_and_is_gone_when_it_has() {
    let mut ed = moving_editor();
    let (plus, at) = (slot(&ed, Item::AddPage), ed.toolbar().centre_of(Item::AddPage));
    ed.pointer_event(pen(Phase::Down, TIP, &[at]));
    ed.step_view(0.016);
    let spread = ed.toolbar_look().ripple[plus];
    assert!(spread > 0.0 && spread < 1.0, "spreading: {spread}");
    let mut last = spread;
    for _ in 0..40 {
        ed.step_view(0.016);
        let now = ed.toolbar_look().ripple[plus];
        assert!(now >= last);
        last = now;
    }
    assert_eq!(last, 1.0, "it has filled the button: nothing is drawn for it any more");
    let others = ed.toolbar_look().ripple.iter().enumerate().filter(|(i, r)| *i != plus && **r != 0.0).count();
    assert_eq!(others, 0, "only the pressed button");
    ed.pointer_event(pen(Phase::Up, TIP, &[at]));
}

#[test]
fn the_mark_of_the_tool_in_use_slides_to_the_tool_chosen() {
    let mut ed = moving_editor();
    assert_eq!(ed.toolbar_look().tool, 0.0);
    ed.set_tool(Tool::Text);
    ed.step_view(0.016);
    let early = ed.toolbar_look().tool;
    assert!(early > 0.0 && early < 2.0, "between the pen and the text button: {early}");
    let mut last = early;
    for _ in 0..40 {
        ed.step_view(0.016);
        let now = ed.toolbar_look().tool;
        assert!(now >= last && now <= 2.0, "never past it: {now}");
        last = now;
    }
    assert_eq!(last, 2.0);
}

#[test]
fn a_button_that_becomes_usable_fades_up() {
    let mut ed = moving_editor();
    let undo = slot(&ed, Item::Undo);
    assert_eq!(ed.toolbar_look().enabled[undo], 0.0, "nothing to undo");
    add_stroke(&mut ed, &[[10.0, 10.0], [80.0, 40.0]]);
    ed.step_view(0.016);
    let early = ed.toolbar_look().enabled[undo];
    assert!(early > 0.0 && early < 1.0, "{early}");
    frames(&mut ed, 30);
    assert_eq!(ed.toolbar_look().enabled[undo], 1.0);
}

#[test]
fn when_everything_has_settled_nothing_asks_for_another_frame_and_reduced_motion_never_does() {
    let mut ed = moving_editor();
    let at = ed.toolbar().centre_of(Item::Pages);
    ed.mouse_moved(at);
    assert!(ed.step_view(0.016), "moving");
    let mut asked = 0;
    while ed.step_view(0.016) && asked < 200 {
        asked += 1;
    }
    assert!(asked < 60, "it settles within a second: {asked}");
    assert!(!ed.step_view(0.016), "idle: no frame is asked for");
    // The same with motion reduced: there at once, and never a frame asked for.
    let mut still = editor();
    still.motions.set_prefs(Prefs::REDUCED);
    still.mouse_moved(still.toolbar().centre_of(Item::Pages));
    assert!(!still.step_view(0.016));
    let rest = Look::resting(&still.toolbar_state(), &still.toolbar());
    assert_eq!(still.toolbar_look(), rest);
}

#[test]
fn a_look_in_motion_stays_within_its_ends_whatever_the_frame_times() {
    let mut ed = moving_editor();
    let at = ed.toolbar().centre_of(Item::Pages);
    for (k, dt) in [0.0, 0.016, 5.0, f32::NAN, -1.0, 0.001, f32::INFINITY, 0.05].into_iter().cycle().take(64).enumerate() {
        match k % 4 {
            0 => ed.mouse_moved(at),
            1 => ed.set_tool(Tool::Geometry),
            2 => ed.mouse_moved([700.0, 500.0]),
            _ => ed.set_tool(Tool::Laser),
        }
        ed.step_view(dt);
        let look = ed.toolbar_look();
        let inside = |v: f32| v.is_finite() && (-1e-4..=1.0 + 1e-4).contains(&v);
        assert!(look.hover.iter().chain(&look.enabled).chain(&look.ripple).all(|&v| inside(v)), "{look:?}");
        assert!(look.tool.is_finite() && (0.0..=3.0).contains(&look.tool) && inside(look.unsaved), "{look:?}");
    }
}

#[test]
fn the_prompt_grows_in_and_goes_out_as_it_was_and_with_motion_reduced_is_just_there_or_not() {
    let mut ed = moving_editor();
    assert!(ed.prompt_look().is_none());
    ed.ask_unsaved();
    ed.step_view(0.016);
    let (_, early) = ed.prompt_look().expect("coming");
    assert!(early > 0.0 && early < 1.0, "{early}");
    frames(&mut ed, 30);
    assert_eq!(ed.prompt_look().map(|l| l.1), Some(1.0));
    // Answered: it is no longer wanted, and is drawn a moment longer, going out.
    let cancel = {
        let layout = crate::toolbar::PromptLayout::new(ed.view.viewport, ed.dpr);
        let r = layout.buttons.iter().find(|(c, _)| *c == crate::requests::Choice::Cancel).expect("button").1;
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    };
    ed.pointer_event(pen(Phase::Down, TIP, &[cancel]));
    ed.pointer_event(pen(Phase::Up, TIP, &[cancel]));
    assert!(ed.chrome.prompt.is_none());
    ed.step_view(0.016);
    let (_, going) = ed.prompt_look().expect("going out");
    assert!(going > 0.0 && going < 1.0, "{going}");
    frames(&mut ed, 30);
    assert!(ed.prompt_look().is_none(), "gone");
    assert!(!ed.step_view(0.016), "idle");
    // Reduced motion: there while asked, gone when answered.
    let mut still = editor();
    still.motions.set_prefs(Prefs::REDUCED);
    still.ask_unsaved();
    still.step_view(0.016);
    assert_eq!(still.prompt_look().map(|l| l.1), Some(1.0));
    still.chrome.prompt = None;
    still.step_view(0.016);
    assert!(still.prompt_look().is_none());
}

#[test]
fn the_tool_panel_unfolds_and_folds_over_a_moment_and_the_bar_comes_in_when_a_tool_that_has_one_is_chosen() {
    let mut ed = moving_editor();
    assert!(ed.panel_look().is_none());
    assert_eq!(ed.motions.bar_amount(), 1.0, "the bar is there from the start");
    open_settings(&mut ed);
    assert!(ed.chrome.panel.is_some());
    ed.step_view(0.016);
    let (_, early) = ed.panel_look().expect("unfolding");
    assert!(early > 0.0 && early < 1.0, "{early}");
    frames(&mut ed, 30);
    assert_eq!(ed.panel_look().map(|l| l.1), Some(1.0));
    open_settings(&mut ed);
    assert!(ed.chrome.panel.is_none());
    ed.step_view(0.016);
    let (_, folding) = ed.panel_look().expect("still drawn while it folds");
    assert!(folding > 0.0 && folding < 1.0, "{folding}");
    frames(&mut ed, 30);
    assert!(ed.panel_look().is_none());
    // A tool with no bar: nothing of it; chosen again, it comes in from nothing.
    ed.set_tool(Tool::Laser);
    ed.step_view(0.016);
    assert!(ed.context_bar().is_none());
    ed.set_tool(Tool::Pen);
    ed.step_view(0.016);
    let coming = ed.motions.bar_amount();
    assert!(coming > 0.0 && coming < 1.0, "{coming}");
    frames(&mut ed, 30);
    assert_eq!(ed.motions.bar_amount(), 1.0);
}

