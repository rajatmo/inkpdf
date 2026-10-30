//! The colours of every piece of interface, as they are today: a hash of what each scenario draws (kind and colour of every panel, line
//! and label, and the text of the labels; positions are left out, so the hashes are the same on every platform) against a checked-in
//! file. It is the proof that a change of how colours are made (the palette) changes nothing for the light scheme: it must pass before
//! and after. After a change of looks that is meant, `INKPDF_UPDATE_GOLDEN=1 cargo test -p inkpdf --lib golden` rewrites the file.

use std::fmt::Write as _;

use super::*;
use crate::browse::{Entry, Place};
use crate::toolbar::{OverviewControl, Ui};

/// What a scenario drew, reduced to what the colours say.
fn dump(ui: &Ui) -> String {
    let mut out = String::new();
    for p in &ui.panels {
        let _ = writeln!(out, "panel {:?}", p.color);
    }
    for l in &ui.lines {
        let _ = writeln!(out, "line {:?}", l.color);
    }
    for i in &ui.images {
        let _ = writeln!(out, "image page {} alpha {:.2}", i.page, i.alpha);
    }
    for l in &ui.labels {
        let _ = writeln!(out, "label {:?} {:?} {:.0} alpha {:.2}", l.text, l.color, l.size, l.alpha);
    }
    out
}

/// FNV-1a: small, and the same everywhere.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn base(n: usize) -> Editor {
    let mut ed = Editor::new([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&vec![[612.0, 792.0]; n]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed.set_mode(Mode::Desktop);
    ed
}

fn interface(ed: &Editor) -> Ui {
    let mut ui = Ui::default();
    ed.draw_interface(&mut ui);
    ui
}

fn overview(ed: &Editor) -> Ui {
    let mut ui = Ui::default();
    ed.draw_overview(&mut ui);
    ui
}

fn centre(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

/// Every piece of interface in a state of its own.
fn scenarios() -> Vec<(&'static str, Ui)> {
    let mut all: Vec<(&'static str, Ui)> = Vec::new();

    let ed = base(2);
    all.push(("toolbar, desktop, pen, nothing under way", interface(&ed)));

    let mut ed = base(2);
    ed.set_mode(Mode::Tablet);
    ed.set_tool(Tool::Text);
    ed.chrome.hover = Some(Item::Undo);
    all.push(("toolbar, tablet, text, hovering undo", interface(&ed)));

    let mut ed = base(1);
    ed.set_tool(Tool::Laser);
    ed.chrome.fullscreen = true;
    all.push(("toolbar, one page, laser, fullscreen", interface(&ed)));

    for tool in [Tool::Pen, Tool::Geometry, Tool::Text] {
        let mut ed = base(2);
        ed.set_tool(tool);
        ed.chrome.panel = Some(crate::toolbar::PanelState::default());
        all.push((
            match tool {
                Tool::Pen => "panel, pen",
                Tool::Geometry => "panel, shapes",
                _ => "panel, text",
            },
            interface(&ed),
        ));
    }

    let mut ed = base(2);
    ed.set_tool(Tool::Pen);
    ed.chrome.panel = Some(crate::toolbar::PanelState { editing: Some(2) });
    all.push(("panel, pen, the colour editor", interface(&ed)));

    // A selection: the bar under the tools has its buttons lit.
    let mut ed = base(2);
    let r = ed.layout.page(0).expect("page");
    let id =
        ed.ink.add(0, Style { rgba: [200, 30, 30, 255], width: 3.0 }, vec![[r.x + 40.0, r.y + 50.0], [r.x + 120.0, r.y + 90.0]]).expect("stroke");
    ed.selection = vec![id];
    all.push(("bar under the tools, with a selection", interface(&ed)));

    // The unsaved-changes prompt, and a document with ink to save.
    let mut ed = base(2);
    ed.ink.add(0, Style { rgba: [0, 0, 0, 255], width: 2.0 }, vec![[100.0, 100.0], [200.0, 150.0]]);
    all.push(("toolbar, unsaved ink", interface(&ed)));
    ed.ask_unsaved();
    all.push(("the prompt", interface(&ed)));

    // The pages overview, in each of its states.
    let mut ed = base(6);
    ed.outline = vec![
        crate::outline::OutlineItem { title: "Chapter 1".into(), depth: 0, page: Some(0), open: true, has_children: true },
        crate::outline::OutlineItem { title: "1.1 Basics".into(), depth: 1, page: Some(2), open: true, has_children: false },
        crate::outline::OutlineItem { title: "Appendix".into(), depth: 0, page: None, open: true, has_children: false },
    ];
    ed.open_overview();
    ed.step_view(10.0);
    all.push(("overview", overview(&ed)));
    let edit = ed.overview_layout().button(OverviewControl::Edit);
    mouse_click_at(&mut ed, centre(edit));
    all.push(("overview, edit mode", overview(&ed)));
    let cell = ed.overview_layout().cell(1, 0.0);
    mouse_click_at(&mut ed, centre(cell));
    let cell = ed.overview_layout().cell(3, 0.0);
    mouse_click_at(&mut ed, centre(cell));
    all.push(("overview, edit mode, two pages chosen", overview(&ed)));
    ed.mouse_moved(centre(ed.overview_layout().cell(0, 0.0)));
    all.push(("overview, hovering a page", overview(&ed)));

    // The Save As card.
    let mut ed = base(1);
    let places =
        vec![Place { label: "Documents".into(), path: "/home/me/Documents".into() }, Place { label: "Home".into(), path: "/home/me".into() }];
    ed.ask_save_as("/home/me/Documents".into(), "Untitled.pdf", places);
    ed.folder_listed(
        std::path::Path::new("/home/me/Documents"),
        Ok(vec![Entry { name: "Invoices".into(), folder: true }, Entry { name: "old.pdf".into(), folder: false }]),
    );
    let mut ui = Ui::default();
    ed.draw_save_as(&mut ui);
    all.push(("the Save As card", ui));

    all
}

const FILE: &str = include_str!("golden_light.txt");

#[test]
fn the_light_interface_looks_as_it_always_did() {
    let now: Vec<(String, u64)> = scenarios().iter().map(|(name, ui)| (name.to_string(), hash(&dump(ui)))).collect();
    let text: String = now.iter().map(|(name, h)| format!("{name} = {h:016x}\n")).collect();
    if std::env::var_os("INKPDF_UPDATE_GOLDEN").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/editor/tests/golden_light.txt");
        std::fs::write(path, &text).expect("write the golden file");
        return;
    }
    let was: std::collections::HashMap<&str, &str> = FILE.lines().filter_map(|l| l.rsplit_once(" = ")).collect();
    let mut changed = Vec::new();
    for (name, h) in &now {
        match was.get(name.as_str()) {
            Some(w) if *w == format!("{h:016x}") => {}
            Some(w) => changed.push(format!("{name}: was {w}, is {h:016x}")),
            None => changed.push(format!("{name}: not in the golden file")),
        }
    }
    assert!(changed.is_empty(), "the look of the light interface changed:\n{}\n(if that was meant: INKPDF_UPDATE_GOLDEN=1)", changed.join("\n"));
    assert_eq!(was.len(), now.len(), "a scenario was taken away");
}

#[test]
fn every_scenario_draws_something_and_two_different_states_look_different() {
    let all = scenarios();
    for (name, ui) in &all {
        assert!(!ui.panels.is_empty() || !ui.lines.is_empty(), "{name} drew nothing");
    }
    let hashes: std::collections::HashSet<u64> = all.iter().map(|(_, ui)| hash(&dump(ui))).collect();
    assert!(hashes.len() >= all.len() - 2, "the scenarios are not all different: the golden file would prove too little");
}
