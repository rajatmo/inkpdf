//! A layout audit: every card and pill that floats over the page, all up at once, in windows from a small phone to a 4K screen and in both
//! orientations: each is inside the window, big enough to press, and none lies over another (except what is meant to: the toolbar's panel unfolds from
//! its bar, the dim scrim of a prompt covers the rest, the card that says something went wrong may cover the page).

use super::*;
use crate::input::Mode;
use crate::links::{Link, Target};
use crate::tools::Key;
use inkpdf_api::ItemId;

/// (width, height, dpr), in device pixels as the window has them: a small phone, a phone, the same on its side, a small tablet and a big one, a small
/// laptop, a laptop, a big window, a 4K screen, and a small window a person has dragged out.
const WINDOWS: [(f32, f32, f32); 10] = [
    (720.0, 1280.0, 2.0),
    (1080.0, 2400.0, 2.6),
    (2400.0, 1080.0, 2.6),
    (1600.0, 2560.0, 2.0),
    (2560.0, 1600.0, 2.0),
    (1366.0, 768.0, 1.0),
    (1920.0, 1080.0, 1.25),
    (2560.0, 1440.0, 1.5),
    (3840.0, 2160.0, 2.0),
    (360.0, 480.0, 1.0),
];
/// Everything up, with the card at the right that was opened last being `last`: 0 the search card, 1 the card of an item, 2 the card of a link.
fn everything_up(width: f32, height: f32, dpr: f32, last: u8) -> Editor {
    let exts: Vec<_> = inkpdf_ext::all().into_iter().filter(|e| e.id == "notes" || e.id == "weblink").collect();
    let mut ed = Editor::with_extensions([width, height], dpr, crate::motion::Prefs::REDUCED, &exts);
    ed.load(crate::view::Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.set_mode(Mode::Tablet);
    ed.ink.add_item(0, crate::ink::Style { rgba: [255, 204, 0, 255], width: 1.0 }, "note", "open\u{1d}".to_string(), [112.0, 312.0, 132.0, 332.0]);
    let id: ItemId = ed.ink.strokes().iter().find(|s| s.item.is_some()).map(|s| s.id).expect("a note");
    ed.set_tool(crate::tool::Tool::Pen);
    ed.chrome.panel = Some(crate::toolbar::PanelState::default());
    ed.set_dark_pages(true);
    ed.chrome.theme_menu = true;
    ed.set_text_files(vec![Some(0), Some(1)]);
    ed.add_text(
        0,
        Some(std::sync::Arc::new(crate::pagetext::FakeText::new(&[
            "hello world",
            "hello again",
            "hello",
            "hello there",
            "hello hello",
            "and hello",
            "hello end",
        ]))),
    );
    ed.links = vec![
        Link { rect: [112.0, 402.0, 312.0, 422.0], target: Target::Page(1) },
        Link { rect: [112.0, 462.0, 312.0, 482.0], target: Target::Url("https://example.com/".into()) },
    ];
    ed.jumping(|e| e.ws.scroll_to_page(1));
    // (They are opened in turn, so that the one that is `last` is the one that is left: there is one card at the right at a time.)
    let order = [[1u8, 2, 0], [0, 2, 1], [0, 1, 2]][last as usize];
    for card in order {
        match card {
            0 => {
                ed.open_search();
                ed.press(Key::Text("hello"));
            }
            1 => {
                ed.card.open = Some(id);
                ed.step_view(0.016);
            }
            _ => ed.follow_web_link("https://example.com/a/very/long/address/that/goes/on/and/on/and/on/and/on".into()),
        }
    }
    ed.show_error("Could not save:\nThe disk is full, and the file that was to be written is a long way from fitting on what is left of it. Free some room and try again.");
    ed
}

/// Every floating thing that is up, by name and rectangle `[x0, y0, x1, y1]`.
fn floating(ed: &Editor) -> Vec<(String, [f32; 4])> {
    let mut all: Vec<(String, [f32; 4])> = Vec::new();
    let bar = ed.toolbar();
    for (i, pill) in bar.pills.iter().enumerate() {
        if pill[2] > pill[0] {
            all.push((format!("toolbar pill {i}"), *pill));
        }
    }
    if let Some(b) = ed.context_bar() {
        all.push(("the bar under the tools".into(), b.rect));
    }
    if let Some(d) = ed.drawer() {
        // (The bar and the panel unfolded from it are one card: the panel is what is added to the bar.)
        all.push(("the tool panel".into(), [d[0], d[1].max(ed.context_bar().map_or(d[1], |b| b.rect[3])), d[2], d[3]]));
    }
    if let Some(b) = ed.theme_layout() {
        all.push(("the round button under the theme button".into(), b.rect));
    }
    if let Some(c) = ed.search_layout() {
        all.push(("the search card".into(), c.card));
    }
    if let Some(c) = ed.item_card_layout() {
        all.push(("the card of an item".into(), c.card));
    }
    if let Some(c) = ed.link_layout() {
        all.push(("the card of a link".into(), c.card));
    }
    if let Some(c) = ed.trail_layout() {
        all.push(("the back and forward pill".into(), c.card));
    }
    if let Some(c) = ed.error_layout() {
        all.push(("the card that says something went wrong".into(), c.card));
    }
    all
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] - 1.0 && b[0] < a[2] - 1.0 && a[1] < b[3] - 1.0 && b[1] < a[3] - 1.0
}

#[test]
fn every_card_and_pill_is_inside_the_window_and_none_lies_over_another_in_any_size_of_window() {
    let mut bad: Vec<String> = Vec::new();
    for (w, h, dpr) in WINDOWS {
        for last in 0..3 {
            let ed = everything_up(w, h, dpr, last);
            let things = floating(&ed);
            let rights = things.iter().filter(|(n, _)| n.contains("search card") || n.contains("of an item") || n.contains("of a link")).count();
            assert!(
                things.len() >= 6 && rights == 1,
                "{w}x{h}: one card at the right, and most of the rest up: {:?}",
                things.iter().map(|t| &t.0).collect::<Vec<_>>()
            );
            for (name, r) in &things {
                if r[0] < -0.5 || r[1] < -0.5 || r[2] > w + 0.5 || r[3] > h + 0.5 {
                    bad.push(format!("{w}x{h}@{dpr}: {name} {r:?} is not inside the window"));
                }
            }
            for (i, (a, ra)) in things.iter().enumerate() {
                for (b, rb) in &things[i + 1..] {
                    // (The bar under the tools and the panel unfolded from it are one card; the error card sits over whatever is at the bottom.)
                    if overlap(*ra, *rb) && !(a.starts_with("the card that says") || b.starts_with("the card that says")) {
                        bad.push(format!("{w}x{h}@{dpr} (card {last}): {a} {ra:?} lies over {b} {rb:?}"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
}
