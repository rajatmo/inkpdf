//! The PDFs opened last, newest first, and where each was left: the page, how far down it, and the zoom. A document that is opened again is
//! shown as it was left, the toolbar lists the last ten to open again with one press, and the program starts on the one that was open when it was
//! closed.
//!
//! A small text file beside the settings (kept by `app::config`; this module does no I/O), a few lines for each document:
//!
//! ```text
//! open
//! path C:\Users\me\Documents\notes.pdf
//! name notes.pdf
//! place 3 120.50 0.00 1.2500 fit
//! uri content://com.android.externalstorage.documents/document/primary%3Apaper.pdf
//! name paper.pdf
//! ```
//!
//! `open` says the first document was the one open when this was written. A `place` is the page (counted from 1), how far below the top of that page
//! the top of the window was and how far from the left edge of the document its left edge was (in points), the zoom, and `fit` when the zoom followed
//! the width of the window (`free` when the person had chosen one). Anything unreadable is ignored, as in the settings: a damaged file loses a place
//! or a document, and never stops the program starting.

use std::path::PathBuf;

use crate::store::DocId;

/// Documents kept at most.
pub const MAX: usize = 10;

/// Where a document was left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// The page the top of the window was on (counted from 0).
    pub page: usize,
    /// How far below the top of that page the top of the window was, in points (above it, in the gap between pages, when negative).
    pub dy: f32,
    /// How far from the left edge of the document the left edge of the window was, in points.
    pub x: f32,
    pub zoom: f32,
    /// The zoom followed the width of the window: it does again, whatever the window's width is now.
    pub fit: bool,
}

impl Place {
    /// Whether every number of it is one a view can be put at.
    pub fn is_sound(&self) -> bool {
        self.dy.is_finite() && self.x.is_finite() && self.zoom.is_finite() && self.zoom > 0.0
    }
}

/// One document of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: DocId,
    /// The name to show for it (what the store knew it as when it was opened: an Android address does not say).
    pub name: String,
    pub place: Option<Place>,
}

impl Entry {
    /// The folder the file is in, to tell two files of one name apart (nothing for an address, which has none to show).
    pub fn folder(&self) -> String {
        self.id.as_path().and_then(|p| p.parent()).map(|p| p.display().to_string()).unwrap_or_default()
    }
}

/// Whether two ids name one file. (Windows does not tell `Notes.pdf` from `notes.pdf`, nor `/` from `\`.)
pub fn same(a: &DocId, b: &DocId) -> bool {
    match (a, b) {
        (DocId::Path(a), DocId::Path(b)) if cfg!(windows) => {
            let fold = |p: &PathBuf| p.to_string_lossy().to_lowercase().replace('/', "\\");
            fold(a) == fold(b)
        }
        _ => a == b,
    }
}

/// An id that can be kept on one line of the file.
fn keepable(id: &DocId) -> bool {
    let text = id.to_string();
    !text.is_empty() && !text.chars().any(char::is_control)
}

fn one_line(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(200).collect::<String>().trim().to_string()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    entries: Vec<Entry>,
    /// The first entry is the document that is open now (so the one to start on next time).
    open: bool,
}

impl Recent {
    /// The documents, newest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether the first entry is the document that is open.
    pub fn is_open(&self) -> bool {
        self.open && !self.entries.is_empty()
    }

    /// The document that was open when the program was last closed, if one was.
    pub fn last_open(&self) -> Option<&DocId> {
        self.entries.first().filter(|_| self.open).map(|e| &e.id)
    }

    /// Where `id` was left, if it is listed and was left somewhere.
    pub fn place_of(&self, id: &DocId) -> Option<Place> {
        self.entries.iter().find(|e| same(&e.id, id)).and_then(|e| e.place)
    }

    /// `id` was opened (or the document was saved as it): it is the first of the list and the one that is open; where it was left is kept. The
    /// oldest goes when there are more than [`MAX`]. (An id that cannot be written on a line is not kept.)
    pub fn opened(&mut self, id: DocId, name: &str) {
        if !keepable(&id) {
            self.open = false;
            return;
        }
        let place = self.place_of(&id);
        self.entries.retain(|e| !same(&e.id, &id));
        let name = one_line(name);
        self.entries.insert(0, Entry { name: if name.is_empty() { id.name() } else { name }, id, place });
        self.entries.truncate(MAX);
        self.open = true;
    }

    /// The document that is open is not one of the list (a new one, with no file yet).
    pub fn closed(&mut self) {
        self.open = false;
    }

    /// `id` is being left at `place` (another document is opened, or the program is closing).
    pub fn left(&mut self, id: &DocId, place: Place) {
        if let Some(entry) = self.entries.iter_mut().find(|e| same(&e.id, id)).filter(|_| place.is_sound()) {
            entry.place = Some(place);
        }
    }

    /// Read the list from text, leaving out whatever cannot be read.
    pub fn parse(text: &str) -> Recent {
        let mut out = Recent::default();
        let mut open = false;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            // (A path or a name has spaces in it: it is the rest of the line.)
            let id = line
                .strip_prefix("path ")
                .map(|p| DocId::Path(PathBuf::from(p)))
                .or_else(|| line.strip_prefix("uri ").map(|u| DocId::Uri(u.to_string())));
            if let Some(id) = id {
                out.entries.push(Entry { name: id.name(), id, place: None });
            } else if line == "open" {
                open = true;
            } else if let Some(name) = line.strip_prefix("name ")
                && let Some(entry) = out.entries.last_mut()
                && !one_line(name).is_empty()
            {
                entry.name = one_line(name);
            } else if let Some(place) = line.strip_prefix("place ")
                && let Some(entry) = out.entries.last_mut()
            {
                entry.place = parse_place(place).or(entry.place);
            }
        }
        // (A file edited by hand may say a thing twice, or too much: the first of each is kept, and no more than the list holds.)
        let mut kept: Vec<Entry> = Vec::new();
        for entry in out.entries {
            if keepable(&entry.id) && kept.len() < MAX && !kept.iter().any(|k| same(&k.id, &entry.id)) {
                kept.push(entry);
            }
        }
        Recent { open: open && !kept.is_empty(), entries: kept }
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        if self.is_open() {
            text.push_str("open\n");
        }
        for entry in &self.entries {
            match &entry.id {
                DocId::Path(path) => text.push_str(&format!("path {}\n", path.display())),
                DocId::Uri(uri) => text.push_str(&format!("uri {uri}\n")),
            }
            text.push_str(&format!("name {}\n", one_line(&entry.name)));
            if let Some(p) = entry.place.filter(Place::is_sound) {
                let fit = if p.fit { "fit" } else { "free" };
                text.push_str(&format!("place {} {:.2} {:.2} {:.4} {fit}\n", p.page + 1, p.dy, p.x, p.zoom));
            }
        }
        text
    }
}

fn parse_place(text: &str) -> Option<Place> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let [page, dy, x, zoom, fit] = words[..] else { return None };
    let number = |t: &str| t.parse::<f32>().ok().filter(|v| v.is_finite());
    let place = Place {
        page: page.parse::<usize>().ok().filter(|&p| p >= 1)? - 1,
        dy: number(dy)?,
        x: number(x)?,
        zoom: number(zoom)?,
        fit: match fit {
            "fit" => true,
            "free" => false,
            _ => return None,
        },
    };
    place.is_sound().then_some(place)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> DocId {
        DocId::Path(PathBuf::from(if cfg!(windows) { format!("C:\\docs\\{name}") } else { format!("/docs/{name}") }))
    }

    fn place(page: usize) -> Place {
        Place { page, dy: 120.5, x: -8.25, zoom: 1.25, fit: false }
    }

    #[test]
    fn the_one_opened_last_is_first_and_is_listed_once_and_the_eleventh_pushes_the_oldest_out() {
        let mut r = Recent::default();
        assert_eq!((r.entries().len(), r.last_open(), r.is_open()), (0, None, false));
        for i in 0..MAX {
            r.opened(path(&format!("{i}.pdf")), &format!("{i}.pdf"));
        }
        assert_eq!(r.entries().len(), MAX);
        assert_eq!(r.entries()[0].name, "9.pdf");
        r.opened(path("3.pdf"), "3.pdf");
        assert_eq!(r.entries().len(), MAX, "opened again, it is not listed twice");
        assert_eq!((r.entries()[0].name.as_str(), r.entries()[1].name.as_str()), ("3.pdf", "9.pdf"));
        r.opened(path("new.pdf"), "new.pdf");
        assert_eq!(r.entries().len(), MAX);
        assert!(r.entries().iter().all(|e| e.name != "0.pdf"), "the oldest went");
        assert_eq!(r.last_open(), Some(&path("new.pdf")));
        r.closed();
        assert_eq!((r.last_open(), r.is_open()), (None, false), "a blank document is open: nothing to start on next time");
        assert_eq!(r.entries().len(), MAX, "but the list is as it was");
    }

    #[test]
    fn where_a_document_was_left_is_kept_while_it_is_listed_and_found_again_when_it_is_opened() {
        let mut r = Recent::default();
        r.opened(path("a.pdf"), "a.pdf");
        assert_eq!(r.place_of(&path("a.pdf")), None, "never left anywhere yet");
        r.left(&path("a.pdf"), place(4));
        r.left(&path("unknown.pdf"), place(1));
        assert_eq!(r.entries().len(), 1, "leaving a document that is not listed lists nothing");
        r.opened(path("b.pdf"), "b.pdf");
        r.opened(path("a.pdf"), "a.pdf");
        assert_eq!(r.place_of(&path("a.pdf")), Some(place(4)), "opening it again keeps where it was left");
        r.left(&path("a.pdf"), Place { zoom: f32::NAN, ..place(2) });
        r.left(&path("a.pdf"), Place { zoom: 0.0, ..place(2) });
        r.left(&path("a.pdf"), Place { dy: f32::INFINITY, ..place(2) });
        assert_eq!(r.place_of(&path("a.pdf")), Some(place(4)), "a place that is not one is not kept");
    }

    #[test]
    fn the_list_round_trips_through_text_with_paths_that_have_spaces_and_addresses() {
        let mut r = Recent::default();
        r.opened(DocId::Uri("content://com.android.externalstorage.documents/document/primary%3ADocuments%2Fnotes.pdf".into()), "notes.pdf");
        r.left(&r.entries()[0].id.clone(), Place { page: 0, dy: -12.0, x: 0.0, zoom: 0.5, fit: true });
        r.opened(path("my thesis (final) v2.pdf"), "my thesis (final) v2.pdf");
        r.left(&path("my thesis (final) v2.pdf"), place(41));
        r.opened(path("never left.pdf"), "never left.pdf");
        let text = r.to_text();
        assert!(text.starts_with("open\n"), "{text}");
        assert!(text.contains("place 42 120.50 -8.25 1.2500 free\n") && text.contains("place 1 -12.00 0.00 0.5000 fit\n"), "{text}");
        assert_eq!(Recent::parse(&text), r);
        // (Written by Windows, or carried from it: the line ends are its own.)
        assert_eq!(Recent::parse(&text.replace('\n', "\r\n")), r);
        r.closed();
        assert!(!r.to_text().contains("open"));
        assert_eq!(Recent::parse(&r.to_text()), r);
        assert_eq!(Recent::parse(""), Recent::default());
    }

    #[test]
    fn unreadable_lines_are_ignored_and_the_rest_still_apply() {
        let text = "banana\nplace 3 1 1 1 fit\nname nobody\npath /a/one.pdf\nplace 0 1 1 1 fit\nplace x 1 1 1 fit\nplace 2 NaN 1 1 fit\nplace 2 1 1 0 fit\n\
                    place 2 1 1 1 maybe\nplace 2 1 1\nname \npath /a/two.pdf\nname Two\nplace 7 10.00 0.00 2.0000 free\npath /a/one.pdf\nuri \nopen\n";
        let r = Recent::parse(text);
        let names: Vec<&str> = r.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["one.pdf", "Two"], "a document said twice is listed once, and one with no address is not one");
        assert_eq!(r.entries()[0].place, None, "none of its places could be read");
        assert_eq!(r.entries()[1].place, Some(Place { page: 6, dy: 10.0, x: 0.0, zoom: 2.0, fit: false }));
        assert!(r.is_open());
        assert!(!Recent::parse("open\n").is_open(), "nothing listed, nothing open");
        // More than the list holds, cut off in the middle of a line, or binary rubbish: still a list.
        let many: String = (0..40).map(|i| format!("path /a/{i}.pdf\n")).collect();
        assert_eq!(Recent::parse(&many).entries().len(), MAX);
        for cut in [0, 7, text.len() / 2, text.len() - 2] {
            let _ = Recent::parse(&text[..cut]);
        }
        let junk: String = (0..2000u32).map(|i| char::from_u32(32 + (i * 7919) % 200).unwrap_or(' ')).collect();
        let _ = Recent::parse(&junk);
    }

    #[test]
    fn a_name_is_one_line_and_an_address_that_is_not_one_line_is_not_kept() {
        let mut r = Recent::default();
        r.opened(path("a.pdf"), "two\nlines\tof name");
        assert_eq!(r.entries()[0].name, "twolinesof name");
        r.opened(path("b.pdf"), "");
        assert_eq!(r.entries()[0].name, "b.pdf", "with no name given, the one the address says");
        r.opened(DocId::Path(PathBuf::from("bad\nname.pdf")), "bad");
        assert_eq!(r.entries().len(), 2, "it could not be read back: it is not listed");
        assert!(!r.is_open(), "and what is open is then not of the list");
        assert_eq!(Recent::parse(&r.to_text()).entries().len(), 2);
    }

    #[test]
    fn a_file_is_the_same_file_however_windows_spells_it_and_the_folder_tells_two_of_one_name_apart() {
        let (a, b) = (DocId::Path("C:\\Docs\\Notes.pdf".into()), DocId::Path("c:/docs/notes.pdf".into()));
        assert_eq!(same(&a, &b), cfg!(windows));
        assert!(same(&a, &a) && !same(&a, &DocId::Uri("C:\\Docs\\Notes.pdf".into())));
        let entry = |id: DocId| Entry { name: id.name(), id, place: None };
        assert_eq!(entry(DocId::Uri("content://x/y.pdf".into())).folder(), "");
        let folder = entry(path("a.pdf")).folder();
        assert!(folder.ends_with("docs") && !folder.contains("a.pdf"), "{folder}");
    }
}
