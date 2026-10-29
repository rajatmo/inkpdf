use super::*;

/// A tap on a toolbar button by the pen.
fn pen_tap(ed: &mut Editor, at: [f32; 2]) {
    pen_stroke(ed, TIP, &[at, at]);
}

/// The pen panel as it is laid out now.
fn panel(ed: &Editor) -> crate::toolbar::ToolPanel {
    ed.toolbar().tool_panel(ed.tool, ed.view.viewport[0], ed.chrome.panel.is_some_and(|p| p.editing.is_some())).expect("tool panel")
}

/// The button of the bar under the tools that unfolds the panel (the tool's own button only chooses the tool).
fn pen_button(ed: &Editor) -> [f32; 2] {
    ed.context_bar().expect("the bar").centre_of(crate::toolbar::ContextItem::Style)
}

/// A tap on the settings button by the pen: the panel unfolds, or folds up.
fn tap_pen_button(ed: &mut Editor) {
    let at = pen_button(ed);
    pen_tap(ed, at);
}

/// Press at the start of a slider, drag to its end and let go, with the pen.
fn drag_slider(ed: &mut Editor, slider: Slider, from: f32, to: f32) {
    let (start, end) = (panel(ed).point_on(slider, from), panel(ed).point_on(slider, to));
    ed.pointer_event(pen(Phase::Down, TIP, &[start]));
    ed.pointer_event(pen(Phase::Move, TIP, &[end]));
    ed.pointer_event(pen(Phase::Up, TIP, &[end]));
}

/// Open the colour editor for slot `slot` with the pen.
fn open_editor(ed: &mut Editor, slot: usize) {
    if ed.chrome.panel.is_none() {
        pen_tap(ed, pen_button(ed));
    }
    let swatch = panel(ed).swatch_centre(slot);
    pen_tap(ed, swatch);
    if ed.pen.colour != slot || ed.chrome.panel.and_then(|p| p.editing) != Some(slot) {
        pen_tap(ed, swatch);
    }
    assert_eq!(ed.chrome.panel.and_then(|p| p.editing), Some(slot));
}

#[test]
fn the_pen_is_the_tool_at_the_start_and_its_button_opens_and_closes_the_panel() {
    let mut ed = editor();
    assert_eq!((ed.tool, ed.chrome.panel), (Tool::Pen, None));
    tap_pen_button(&mut ed);
    assert_eq!(ed.chrome.panel, Some(PanelState::default()), "a tap on the pen opens its panel");
    assert_eq!(ed.ink.len(), 0, "and draws nothing");
    tap_pen_button(&mut ed);
    assert_eq!(ed.chrome.panel, None, "a second tap puts it away");
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn the_laser_button_chooses_the_laser_and_puts_the_panel_and_the_bar_away_and_the_pen_button_brings_back_the_bar() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    let laser = ed.toolbar().centre_of(Item::Tool(Tool::Laser));
    pen_tap(&mut ed, laser);
    assert_eq!((ed.tool, ed.chrome.panel), (Tool::Laser, None));
    assert!(ed.context_bar().is_none(), "the pointer has no settings");
    let pen_at = ed.toolbar().centre_of(Item::Tool(Tool::Pen));
    pen_tap(&mut ed, pen_at);
    assert_eq!((ed.tool, ed.chrome.panel), (Tool::Pen, None), "choosing the pen again only chooses it");
    assert!(ed.context_bar().is_some());
}

#[test]
fn a_tools_own_button_only_chooses_the_tool_and_puts_an_open_panel_away() {
    let mut ed = editor();
    let shape = ed.toolbar().centre_of(Item::Tool(Tool::Geometry));
    pen_tap(&mut ed, shape);
    assert_eq!((ed.tool, ed.chrome.panel), (Tool::Geometry, None), "no panel opens by itself");
    open_settings(&mut ed);
    assert!(ed.chrome.panel.is_some());
    pen_tap(&mut ed, shape);
    assert_eq!((ed.tool, ed.chrome.panel), (Tool::Geometry, None), "the press put the panel away and did nothing else");
}

#[test]
fn choosing_the_laser_lets_go_of_a_selection() {
    let mut ed = editor();
    let id = add_stroke(&mut ed, &[[100.0, 100.0], [200.0, 100.0]]);
    ed.selection = vec![id];
    let laser = ed.toolbar().centre_of(Item::Tool(Tool::Laser));
    mouse_click_at(&mut ed, laser);
    assert!(ed.selection.is_empty());
    assert_eq!(ed.ink.len(), 1, "the ink is untouched");
}

#[test]
fn tapping_the_colour_in_use_opens_the_colour_editor_and_its_sliders_change_it() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    let red = panel(&ed).swatch_centre(2);
    pen_tap(&mut ed, red);
    assert_eq!((ed.pen.colour, ed.chrome.panel), (2, Some(PanelState::default())), "the first tap only selects it");
    pen_tap(&mut ed, red);
    assert_eq!(ed.chrome.panel, Some(PanelState { editing: Some(2) }), "the second tap on the colour in use opens the editor");
    // The editor starts from the slot's colour.
    let [hue, _, _] = ed.chrome.colour_hsv;
    assert!(hue < 5.0 || hue > 355.0, "it is a red: {hue}");

    // Drag the hue slider a third of the way along: a green.
    drag_slider(&mut ed, Slider::Channel(Channel::Hue), 0.0, 1.0 / 3.0);
    let [r, g, b, a] = ed.palette[2];
    assert!(g > r && g > b && a == 255, "a third of the way round the rainbow is green: {:?}", ed.palette[2]);
    assert!(ed.requests.has(&Request::SettingsChanged), "the colour is to be remembered");
    assert_eq!(ed.chrome.panel, Some(PanelState { editing: Some(2) }), "the editor stays open for the next slider");
    assert_eq!(ed.ink.len(), 0, "using it drew nothing");

    // Brightness all the way down makes it black; the stroke drawn next uses the new colour.
    let low = panel(&ed).point_on(Slider::Channel(Channel::Value), 0.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[low]));
    ed.pointer_event(pen(Phase::Up, TIP, &[low]));
    assert_eq!(ed.palette[2], [0, 0, 0, 255]);
    let high = panel(&ed).point_on(Slider::Channel(Channel::Value), 1.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[high]));
    ed.pointer_event(pen(Phase::Up, TIP, &[high]));
    let chosen = ed.palette[2];
    assert!(chosen[1] > chosen[0], "back to full brightness the hue is still green: {chosen:?}");
    let path = [on_page(&ed, 10.0, 400.0), on_page(&ed, 40.0, 420.0), on_page(&ed, 70.0, 400.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.chrome.panel, None, "a press on the page puts the panel away");
    assert_eq!(ed.ink.len(), 0, "and that press drew nothing");
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.ink.strokes()[0].style.rgba, chosen);
    assert_eq!(ed.toolbar_state().pen_colour, chosen, "the pen button shows it too");
}

#[test]
fn tapping_the_colour_in_use_again_closes_the_editor_and_another_colour_takes_it_over() {
    let mut ed = editor();
    open_editor(&mut ed, 0);
    let swatch = panel(&ed).swatch_centre(0);
    pen_tap(&mut ed, swatch);
    assert_eq!(ed.chrome.panel, Some(PanelState::default()), "the editor is put away, the panel stays");
    pen_tap(&mut ed, swatch);
    assert_eq!(ed.chrome.panel.and_then(|p| p.editing), Some(0));
    // Another colour, with the editor open: it is picked and the editor follows it.
    let blue = panel(&ed).swatch_centre(1);
    pen_tap(&mut ed, blue);
    assert_eq!((ed.pen.colour, ed.chrome.panel), (1, Some(PanelState { editing: Some(1) })));
    let [hue, _, _] = ed.chrome.colour_hsv;
    assert!((200.0..260.0).contains(&hue), "the editor now holds the blue: {hue}");
}

#[test]
fn a_grey_keeps_its_hue_while_the_saturation_slider_passes_through_zero() {
    let mut ed = editor();
    open_editor(&mut ed, 2);
    let hue_before = ed.chrome.colour_hsv[0];
    let low = panel(&ed).point_on(Slider::Channel(Channel::Saturation), 0.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[low]));
    ed.pointer_event(pen(Phase::Up, TIP, &[low]));
    let [r, g, b, _] = ed.palette[2];
    assert!(r == g && g == b, "no saturation is a grey: {:?}", ed.palette[2]);
    let high = panel(&ed).point_on(Slider::Channel(Channel::Saturation), 1.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[high]));
    ed.pointer_event(pen(Phase::Up, TIP, &[high]));
    assert!((ed.chrome.colour_hsv[0] - hue_before).abs() < 1e-3, "the hue was kept while it was grey");
    assert!(ed.palette[2][0] > ed.palette[2][1], "and it is red again");
}

#[test]
fn a_colour_slot_that_does_not_exist_is_ignored_and_so_is_a_colour_with_no_editor() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    ed.set_editing(Some(99));
    assert_eq!(ed.chrome.panel, Some(PanelState::default()));
    ed.set_channel(Channel::Hue, 100.0);
    assert_eq!(ed.palette, crate::tool::PALETTE);
    assert!(!ed.requests.has(&Request::SettingsChanged));
}

#[test]
fn dragging_the_thickness_slider_sets_the_width_for_new_strokes_and_the_panel_stays_open() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    drag_slider(&mut ed, Slider::Width, 0.0, 1.0);
    assert!((ed.pen.width - crate::tool::MAX_WIDTH).abs() < 0.01, "the thick end: {}", ed.pen.width);
    assert!(ed.chrome.panel.is_some(), "the panel stays for the next setting");
    assert!(ed.requests.has(&Request::SettingsChanged));
    assert_eq!(ed.ink.len(), 0, "using the slider drew nothing");
    drag_slider(&mut ed, Slider::Width, 0.5, 0.0);
    assert!((ed.pen.width - crate::tool::MIN_WIDTH).abs() < 0.01, "the thin end: {}", ed.pen.width);
    // The next stroke is that thin (after the press that puts the panel away).
    let path = [on_page(&ed, 10.0, 300.0), on_page(&ed, 40.0, 320.0), on_page(&ed, 70.0, 300.0)];
    pen_stroke(&mut ed, TIP, &path);
    pen_stroke(&mut ed, TIP, &path);
    assert!((ed.ink.strokes()[0].style.width - crate::tool::MIN_WIDTH).abs() < 0.01);
}

#[test]
fn dragging_the_smoothing_slider_sets_how_much_the_pen_is_steadied() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    drag_slider(&mut ed, Slider::Smoothing, 0.5, 1.0);
    assert_eq!(ed.smoothing, 1.0);
    drag_slider(&mut ed, Slider::Smoothing, 0.5, 0.0);
    assert_eq!(ed.smoothing, 0.0);
    drag_slider(&mut ed, Slider::Smoothing, 0.0, 0.25);
    assert!((ed.smoothing - 0.25).abs() < 1e-3);
    assert!(ed.requests.has(&Request::SettingsChanged));
    // And a stroke drawn now follows the pen with that much steadying: none at 0.
    drag_slider(&mut ed, Slider::Smoothing, 0.5, 0.0);
    ed.chrome.panel = None;
    let wobbly: Vec<[f32; 2]> = (0..40).map(|i| on_page(&ed, 100.0 + i as f32 * 5.0, 300.0 + if i % 2 == 0 { 3.0 } else { -3.0 })).collect();
    pen_stroke(&mut ed, TIP, &wobbly);
    let stroke = &ed.ink.strokes()[0];
    let spread = stroke.pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max) - stroke.pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    assert!(spread > 4.0, "with no smoothing the wobble is kept: {spread}");
}

#[test]
fn a_press_away_from_the_panel_closes_it_and_draws_nothing() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    let path = [on_page(&ed, 700.0, 400.0), on_page(&ed, 750.0, 420.0), on_page(&ed, 800.0, 400.0)];
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.chrome.panel, None);
    assert_eq!(ed.ink.len(), 0, "that press only closed the panel");
}

#[test]
fn a_press_on_the_panel_that_is_not_a_control_does_nothing() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    let p = panel(&ed);
    let preview = [(p.rect[0] + p.rect[2]) / 2.0, p.rect[1] + 0.5 * (p.rect[3] - p.rect[1]) / 6.3];
    pen_stroke(&mut ed, TIP, &[preview, [preview[0] + 20.0, preview[1]]]);
    assert!(ed.chrome.panel.is_some(), "the panel is still there");
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn the_sliders_work_with_the_mouse_and_with_touch() {
    // Mouse.
    let mut ed = editor();
    let at = pen_button(&ed);
    mouse_click_at(&mut ed, at);
    assert!(ed.chrome.panel.is_some());
    let (left, right) = (panel(&ed).point_on(Slider::Width, 0.0), panel(&ed).point_on(Slider::Width, 1.0));
    ed.mouse_moved(left);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(right);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert!((ed.pen.width - crate::tool::MAX_WIDTH).abs() < 0.01);
    assert!(ed.chrome.panel.is_some());

    // Touch: dragging on the slider must not pan the page.
    let mut ed = editor();
    let button = pen_button(&ed);
    ed.pointer_event(touch(1, Phase::Down, button, 30.0));
    ed.pointer_event(touch(1, Phase::Up, button, 30.0));
    assert!(ed.chrome.panel.is_some());
    let (left, right) = (panel(&ed).point_on(Slider::Smoothing, 0.0), panel(&ed).point_on(Slider::Smoothing, 1.0));
    let before = ed.view.scroll;
    ed.pointer_event(touch(2, Phase::Down, left, 30.0));
    ed.pointer_event(touch(2, Phase::Move, right, 30.0));
    ed.pointer_event(touch(2, Phase::Up, right, 30.0));
    assert_eq!(ed.smoothing, 1.0);
    assert_eq!(ed.view.scroll, before, "the page did not move");
}

#[test]
fn the_toolbar_acts_for_every_device() {
    let mut ed = editor();
    let bar = ed.toolbar();
    // The mouse chooses the laser.
    let laser = bar.centre_of(Item::Tool(Tool::Laser));
    ed.mouse_moved(laser);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(ed.tool, Tool::Laser);
    // A finger chooses the pen again, which only chooses it: the panel is the bar's.
    let pen_at = bar.centre_of(Item::Tool(Tool::Pen));
    ed.pointer_event(touch(1, Phase::Down, pen_at, 30.0));
    ed.pointer_event(touch(1, Phase::Up, pen_at, 30.0));
    assert_eq!((ed.tool, ed.chrome.panel.is_some()), (Tool::Pen, false));
    // The pen's save asks the shell to save.
    let s = bar.centre_of(Item::Save);
    pen_stroke(&mut ed, TIP, &[s, s]);
    assert!(ed.requests.has(&Request::Save));
    assert_eq!(ed.ink.len(), 0, "pressing toolbar buttons made no ink");
}

#[test]
fn pressing_another_button_while_the_panel_is_open_closes_it_and_presses_that_button() {
    let mut ed = editor();
    tap_pen_button(&mut ed);
    let save = ed.toolbar().centre_of(Item::Save);
    pen_tap(&mut ed, save);
    assert!(ed.requests.has(&Request::Save), "the press went on to the button");
    assert_eq!(ed.chrome.panel, None);
}

#[test]
fn the_fullscreen_button_asks_the_shell_to_switch() {
    let mut ed = editor();
    let c = ed.toolbar().centre_of(Item::Fullscreen);
    mouse_click_at(&mut ed, c);
    assert!(ed.requests.has(&Request::Fullscreen));
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn the_mouse_lights_the_button_it_is_over_and_only_draws_again_when_that_changes() {
    let mut ed = editor();
    let undo = ed.toolbar().centre_of(Item::Undo);
    ed.redraw = false;
    ed.mouse_moved(undo);
    assert_eq!(ed.chrome.hover, Some(Item::Undo));
    assert!(ed.redraw);
    ed.redraw = false;
    ed.mouse_moved([undo[0] + 2.0, undo[1]]);
    assert!(!ed.redraw, "still over the same button");
    assert_eq!(ed.toolbar_state().hover, Some(Item::Undo));
    ed.mouse_moved([600.0, 600.0]);
    assert_eq!(ed.chrome.hover, None);
    assert!(ed.redraw);
    // Leaving the window clears it too.
    ed.mouse_moved(undo);
    ed.mouse_left();
    assert_eq!(ed.chrome.hover, None);
}
