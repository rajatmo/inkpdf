//! `--layout FILE`: where every button of the window is, written to a file whenever it changes, so that the real-input checks
//! (`tools/e2e`) press buttons by name instead of by remembered positions. A small JSON object, in window (client) pixels:
//!
//! ```text
//! {"client":[w,h],"dpr":1.5,"buttons":{"undo":[x,y],"pen":[x,y],…},"bar":{"delete":[x,y],"swatch2":[x,y],…},
//!  "overview":{"controls":{"edit":[x,y],…},"actions":{"duplicate":[x,y],…},"pages":{"1":[x,y,w,h],…},"toc":{"Chapter 2":[x,y],…}}}
//! ```
//!
//! Only the middle of each control is given (a page also has its picture's size). It costs nothing unless asked for: the shell
//! keeps no reporter without the flag. The file is replaced whole (written beside it, then renamed), so a reader never sees half.

use std::fmt::Write;
use std::path::PathBuf;

use crate::editor::Editor;
use crate::toolbar::{ContextItem, Item};

pub(super) struct Reporter {
    path: PathBuf,
    /// What was written last: the file is written again only when this changes.
    last: String,
}

impl Reporter {
    pub fn new(path: PathBuf) -> Self {
        Self { path, last: String::new() }
    }

    /// Write where things are now, if that is not what was written last.
    pub fn report(&mut self, ed: &Editor) {
        let now = describe(ed);
        if now == self.last {
            return;
        }
        let part = self.path.with_extension("part");
        if std::fs::write(&part, &now).and_then(|()| std::fs::rename(&part, &self.path)).is_err() {
            eprintln!("could not write the layout to {}", self.path.display());
        }
        self.last = now;
    }
}

/// The name a check presses a toolbar button by.
fn item_name(item: Item) -> String {
    match item {
        Item::Close => "close".into(),
        Item::Undo => "undo".into(),
        Item::Redo => "redo".into(),
        Item::Save => "save".into(),
        Item::Open => "open".into(),
        Item::DeletePage => "del".into(),
        Item::AddPage => "plus".into(),
        Item::Pages => "pages".into(),
        Item::Search => "search".into(),
        Item::Tool(tool) => tool.name().into(),
        Item::ModeSwitch => "mode".into(),
        Item::Fullscreen => "full".into(),
        Item::Theme => "theme".into(),
    }
}

/// The name a check presses a button of the bar under the tools by.
fn bar_name(item: ContextItem) -> String {
    match item {
        ContextItem::Duplicate => "duplicate".into(),
        ContextItem::Copy => "copy".into(),
        ContextItem::Paste => "paste".into(),
        ContextItem::Delete => "delete".into(),
        ContextItem::Swatch(i) => format!("swatch{i}"),
        ContextItem::Style => "style".into(),
        // (`Mark(Highlight)` is pressed as "highlight": the innermost name.)
        ContextItem::Action(action) => {
            let name = format!("{action:?}").to_lowercase();
            name.rsplit('(').next().unwrap_or_default().trim_end_matches(')').to_string()
        }
    }
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn middle(r: [f32; 4]) -> [f32; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

/// `"name":[x,y]` entries, joined.
fn points(entries: impl Iterator<Item = (String, [f32; 2])>) -> String {
    entries.map(|(name, p)| format!("{}:[{:.0},{:.0}]", quoted(&name), p[0], p[1])).collect::<Vec<_>>().join(",")
}

fn describe(ed: &Editor) -> String {
    let [w, h] = ed.view.viewport;
    let buttons = points(ed.toolbar().items().iter().map(|&(item, r)| (item_name(item), middle(r))));
    let bar = ed.context_bar().map(|b| points(b.items().iter().map(|&(item, r)| (bar_name(item), middle(r))))).unwrap_or_default();
    let mut out = format!("{{\"client\":[{w:.0},{h:.0}],\"dpr\":{:.3},\"buttons\":{{{buttons}}},\"bar\":{{{bar}}}", ed.dpr);
    if let (Some(state), Some(shown)) = (ed.chrome.overview.as_ref(), ed.overview_pages()) {
        let l = ed.overview_layout();
        let controls = points(l.controls.iter().map(|&(c, r)| (format!("{c:?}").to_lowercase(), middle(r))));
        // (The bar of actions is there only in edit mode; outside it its places are nothing.)
        let actions = points(l.actions.iter().filter(|(_, r)| r[2] > r[0]).map(|&(a, r)| (format!("{a:?}").to_lowercase(), middle(r))));
        let pages = shown
            .filter_map(|i| {
                let page = ed.layout.page(i)?;
                let t = l.thumb(i, state.scroll, [page.w, page.h]);
                Some(format!("\"{}\":[{:.0},{:.0},{:.0},{:.0}]", i + 1, (t[0] + t[2]) / 2.0, (t[1] + t[3]) / 2.0, t[2] - t[0], t[3] - t[1]))
            })
            .collect::<Vec<_>>()
            .join(",");
        let rows = ed.toc_rows();
        let toc = points(
            rows.iter().enumerate().filter_map(|(k, row)| Some((ed.outline.get(row.item)?.title.clone(), middle(l.toc_row(k, state.toc_scroll))))),
        );
        let _ = write!(out, ",\"overview\":{{\"controls\":{{{controls}}},\"actions\":{{{actions}}},\"pages\":{{{pages}}},\"toc\":{{{toc}}}}}");
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_every_button_and_is_json_a_reader_can_take() {
        let mut ed = Editor::new([1600.0, 1000.0], 1.5, crate::motion::Prefs::REDUCED);
        ed.load(crate::view::Layout::new(&[[612.0, 792.0]; 3]), vec![]);
        let text = describe(&ed);
        for name in [
            "\"undo\":[",
            "\"redo\":[",
            "\"save\":[",
            "\"del\":[",
            "\"plus\":[",
            "\"pages\":[",
            "\"pen\":[",
            "\"select\":[",
            "\"mode\":[",
            "\"theme\":[",
        ] {
            assert!(text.contains(name), "{name} in {text}");
        }
        assert!(text.starts_with("{\"client\":[1600,1000]") && text.ends_with('}'));
        assert!(!text.contains("\"overview\""), "no overview while it is put away");
        ed.open_overview();
        assert!(!describe(&ed).contains("\"duplicate\":["), "the bar of actions only in edit mode");
        ed.chrome.overview.as_mut().expect("open").edit = true;
        let open = describe(&ed);
        for name in ["\"overview\":{", "\"edit\":[", "\"duplicate\":[", "\"1\":["] {
            assert!(open.contains(name), "{name} in {open}");
        }
        // Braces and brackets balance (a reader would refuse anything else).
        let depth = open.chars().fold(0i32, |d, c| d + i32::from(c == '{' || c == '[') - i32::from(c == '}' || c == ']'));
        assert_eq!(depth, 0);
    }

    #[test]
    fn titles_are_written_as_json_strings() {
        assert_eq!(quoted("Chapter \"2\" \\ end\n"), "\"Chapter \\\"2\\\" \\\\ end\\u000a\"");
    }
}
