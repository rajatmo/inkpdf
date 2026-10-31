//! A frame is asked for by hand wherever something drawn changes (`redraw = true`), so a place that forgets leaves a stale screen and nothing says so.
//! This feeds the editor random input and checks the one rule that matters: whenever what is drawn (the ink, the selection, the tool, the bars
//! and cards, the panel, the form, the flags) is different after an event, a frame was asked for.

use super::*;

/// Everything that decides what the interface and the page show, apart from the view's own motion (a scroll or a zoom is a frame by itself, asked
/// for by the view as it moves).
fn drawn(ed: &Editor) -> Vec<(&'static str, String)> {
    vec![
        ("ink", format!("{:?}", ed.ink.revision())),
        ("selection", format!("{:?}", ed.selection)),
        ("tool", format!("{:?}", ed.tool)),
        // (The storm sets full screen by hand, as the shell does; that is not input.)
        ("toolbar", format!("{:?}", crate::toolbar::State { fullscreen: false, ..ed.toolbar_state() })),
        ("bar", format!("{:?}", ed.context_view())),
        ("panel", format!("{:?}", ed.panel_view())),
        ("search", format!("{:?}", ed.search_view().map(|v| (v.query, v.caret, v.total)))),
        ("item card", format!("{:?}", ed.item_card_view())),
        ("error card", format!("{:?}", ed.error_view())),
        ("theme button", format!("{:?}", ed.theme_view())),
        ("overview", format!("{:?}", ed.overview_open())),
        ("prompt", format!("{:?}", ed.chrome.prompt.is_some())),
        ("pill", format!("{:?}", ed.chrome.pill)),
        ("form", format!("{:?} {:?}", ed.form, ed.flags)),
        ("text box", format!("{:?}", ed.text_edit().map(|e| (e.text.clone(), e.caret)))),
    ]
}
#[test]
fn stress_whatever_changes_what_is_drawn_asks_for_a_frame() {
    let mut missed: Vec<String> = Vec::new();
    for seed in 1..=3u64 {
        let mut rng = Rng::new(seed * 7919);
        let mut ed = editor_with_pages(3);
        for i in 0..(30_000 * scale()) {
            let _ = ed.take_redraw();
            let before = drawn(&ed);
            let event = poke(&mut ed, &mut rng, false);
            // (What the shell does with the editor between events is not input: a page added or a document loaded stands in for it.)
            if event.starts_with("load") || event.contains("shell") {
                continue;
            }
            let after = drawn(&ed);
            if after != before && !ed.take_redraw() {
                let what: Vec<String> =
                    before.iter().zip(&after).filter(|(b, a)| b != a).map(|(b, a)| format!("{}: {} -> {}", b.0, b.1, a.1)).collect();
                missed.push(format!("{} (seed {seed}, event {i}: {event})", what.join(", ")));
            }
        }
    }
    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for m in &missed {
        *kinds.entry(m.split(":").next().unwrap_or_default().to_string()).or_default() += 1;
    }
    assert!(
        missed.is_empty(),
        "{} events changed what is drawn without asking for a frame: {kinds:?}; for example:\n{}",
        missed.len(),
        missed.iter().take(6).cloned().collect::<Vec<_>>().join("\n")
    );
}
