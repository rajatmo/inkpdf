//! What the Save As dialog is made of, as plain data and rules: the entries of a folder, the places to start from, and
//! what makes a file name acceptable. No file system in here (the shell lists folders and writes files); the rules are
//! the same on every platform on purpose, so that a file saved on one can be copied to any other: the characters
//! Windows forbids are refused everywhere.

use std::fmt;
use std::path::{Path, PathBuf};

/// The longest file name, in bytes (what every file system InkPDF runs on allows).
pub const MAX_NAME_BYTES: usize = 255;
/// How many entries of a folder are shown (a folder of a million files must not make the dialog a million rows).
pub const MAX_ENTRIES: usize = 5_000;
/// The extension of what is saved.
pub const EXTENSION: &str = "pdf";

/// One thing in a folder that the dialog shows: a folder to go into, or a PDF (shown so that a name already taken is seen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub folder: bool,
}

/// A place the dialog can go to in one press: Documents, the desktop, a drive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
}

/// Why a name was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    /// A character no file system everywhere accepts, or a control character.
    BadChar(char),
    /// `.`, `..`, or a name that ends in a dot (Windows drops it, so the file would not be the one named).
    Dots,
    /// A name Windows keeps for devices (`CON`, `NUL`, `COM1`, …), which cannot be a file there.
    Reserved,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "Type a name for the file"),
            NameError::TooLong => write!(f, "That name is too long"),
            NameError::BadChar(c) if c.is_control() => write!(f, "A name cannot hold control characters"),
            NameError::BadChar(c) => write!(f, "A name cannot contain {c}"),
            NameError::Dots => write!(f, "A name cannot be dots, or end in a dot"),
            NameError::Reserved => write!(f, "That name is reserved by Windows"),
        }
    }
}

/// Whether `c` may not be in a file name anywhere.
fn forbidden(c: char) -> bool {
    c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
}

/// The names Windows keeps for devices, whatever the case and whatever follows a dot.
fn reserved(stem: &str) -> bool {
    let base = stem.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4 && (base.starts_with("COM") || base.starts_with("LPT")) && matches!(base.as_bytes()[3], b'1'..=b'9'))
}

/// Whether `name` ends in `.pdf`, in any case.
pub fn has_extension(name: &str) -> bool {
    name.len() > EXTENSION.len() + 1
        && name.is_char_boundary(name.len() - EXTENSION.len() - 1)
        && name[name.len() - EXTENSION.len() - 1..].eq_ignore_ascii_case(".pdf")
}

/// The file name for what was typed: trimmed, checked, and with `.pdf` added unless it is there already (`report` is
/// `report.pdf`; `v1.2` is `v1.2.pdf`; `REPORT.PDF` stays as typed).
pub fn clean_name(typed: &str) -> Result<String, NameError> {
    let name = typed.trim();
    // (A space left before the extension is not part of the name: `Invoice .pdf` is `Invoice.pdf`.)
    let (stem, extension) = if has_extension(name) {
        (name[..name.len() - EXTENSION.len() - 1].trim(), &name[name.len() - EXTENSION.len()..])
    } else {
        (name, EXTENSION)
    };
    if stem.is_empty() || name.eq_ignore_ascii_case(".pdf") {
        return Err(NameError::Empty);
    }
    if let Some(c) = stem.chars().find(|c| forbidden(*c)) {
        return Err(NameError::BadChar(c));
    }
    if stem == "." || stem == ".." || stem.ends_with('.') {
        return Err(NameError::Dots);
    }
    let file = format!("{stem}.{extension}");
    if reserved(&file) {
        return Err(NameError::Reserved);
    }
    if file.len() > MAX_NAME_BYTES {
        return Err(NameError::TooLong);
    }
    Ok(file)
}

/// Whether `name` is one of the file entries of a listing (the case is ignored: two names that differ only in case are one
/// file on Windows and macOS, and asking once too often on Linux costs nothing).
pub fn taken<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
}

/// Folders first, then files, each by name without regard to case (the names themselves settle a tie, so that the
/// order is the same every time).
pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.folder.cmp(&a.folder).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.name.cmp(&b.name)));
}

/// The folder above `dir`, if there is one (a drive's root and `/` have none).
pub fn parent_of(dir: &Path) -> Option<PathBuf> {
    dir.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf)
}

/// `path` as one line that `fits`: the whole of it, or its end behind an ellipsis (the folder you are in matters more than
/// where it started), or just the last folder's name if even that is too much.
pub fn elide_left(path: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(path) {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    for keep in (1..parts.len()).rev() {
        let shown = format!("…/{}", parts[parts.len() - keep..].join("/"));
        if fits(&shown) {
            return shown;
        }
    }
    parts.last().map_or_else(|| path.to_string(), |last| (*last).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, folder: bool) -> Entry {
        Entry { name: name.to_string(), folder }
    }

    #[test]
    fn a_name_gets_pdf_unless_it_has_it_and_is_trimmed() {
        assert_eq!(clean_name("report"), Ok("report.pdf".to_string()));
        assert_eq!(clean_name("  report  "), Ok("report.pdf".to_string()));
        assert_eq!(clean_name("Invoice 7 .pdf"), Ok("Invoice 7.pdf".to_string()), "no space before the extension");
        assert_eq!(clean_name("report.pdf"), Ok("report.pdf".to_string()));
        assert_eq!(clean_name("REPORT.PDF"), Ok("REPORT.PDF".to_string()), "as typed");
        assert_eq!(clean_name("v1.2"), Ok("v1.2.pdf".to_string()), "a dot in the name is not an extension");
        assert_eq!(clean_name("notes.txt"), Ok("notes.txt.pdf".to_string()), "what is saved is a PDF whatever it is called");
        assert_eq!(clean_name("Ünïcode 文書"), Ok("Ünïcode 文書.pdf".to_string()));
    }

    #[test]
    fn what_no_file_system_everywhere_accepts_is_refused_with_the_character_named() {
        for c in ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\n', '\t', '\u{0}', '\u{7f}'] {
            assert_eq!(clean_name(&format!("a{c}b")), Err(NameError::BadChar(c)), "{c:?}");
        }
        assert!(NameError::BadChar('/').to_string().contains('/'));
    }

    #[test]
    fn nothing_dots_or_empty_or_too_long_or_a_device_is_a_name() {
        for empty in ["", "   ", ".pdf", ".PDF", " .pdf "] {
            assert_eq!(clean_name(empty), Err(NameError::Empty), "{empty:?}");
        }
        for dots in [".", "..", "report.", "a..", "x. "] {
            assert_eq!(clean_name(dots), Err(NameError::Dots), "{dots:?}");
        }
        for device in ["CON", "con", "Nul.pdf", "aux.txt", "COM1", "lpt9", "PRN"] {
            assert_eq!(clean_name(device), Err(NameError::Reserved), "{device:?}");
        }
        for fine in ["COM0", "COM10", "CONSOLE", "lpt", "comet"] {
            assert!(clean_name(fine).is_ok(), "{fine:?} is not a device");
        }
        let longest = "a".repeat(MAX_NAME_BYTES - 4);
        assert!(clean_name(&longest).is_ok());
        assert_eq!(clean_name(&format!("{longest}a")), Err(NameError::TooLong));
        // (Bytes, not characters: 'é' is two.)
        assert_eq!(clean_name(&"é".repeat(130)), Err(NameError::TooLong));
    }

    #[test]
    fn every_error_has_words_for_the_person() {
        for e in [NameError::Empty, NameError::TooLong, NameError::BadChar('*'), NameError::BadChar('\n'), NameError::Dots, NameError::Reserved] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn a_name_is_taken_whatever_its_case_and_a_folder_counts() {
        let entries = [entry("Invoices", true), entry("report.pdf", false)];
        assert_eq!(taken(&entries, "REPORT.pdf"), Some(&entries[1]));
        assert_eq!(taken(&entries, "invoices"), Some(&entries[0]));
        assert_eq!(taken(&entries, "other.pdf"), None);
    }

    #[test]
    fn folders_come_first_then_files_each_in_name_order_without_regard_to_case() {
        let mut entries = vec![entry("b.pdf", false), entry("Zed", true), entry("A.pdf", false), entry("alpha", true), entry("a.pdf", false)];
        sort(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Zed", "A.pdf", "a.pdf", "b.pdf"]);
        let mut again = entries.clone();
        again.reverse();
        sort(&mut again);
        assert_eq!(again, entries, "the order does not depend on the order they came in");
    }

    #[test]
    fn a_folder_has_a_parent_until_the_top() {
        assert_eq!(parent_of(Path::new("/home/me/Documents")), Some(PathBuf::from("/home/me")));
        assert_eq!(parent_of(Path::new("/home")), Some(PathBuf::from("/")));
        assert_eq!(parent_of(Path::new("/")), None);
        assert_eq!(parent_of(Path::new("")), None);
        assert_eq!(parent_of(Path::new("relative")), None, "an empty parent is none");
    }

    #[test]
    fn a_long_path_loses_its_beginning_before_its_end() {
        let all = |_: &str| true;
        assert_eq!(elide_left("/home/me/Documents", all), "/home/me/Documents");
        let room = |n: usize| move |s: &str| s.chars().count() <= n;
        assert_eq!(elide_left("/home/me/Documents/Invoices", room(20)), "…/Documents/Invoices");
        assert_eq!(elide_left("/home/me/Documents/Invoices", room(10)), "…/Invoices");
        assert_eq!(elide_left("/home/me/Documents/Invoices", room(3)), "Invoices", "the folder you are in, at least");
        assert_eq!(elide_left("C:\\Users\\me\\Documents", room(14)), "…/me/Documents");
        assert_eq!(elide_left("/", |_| false), "/");
        assert_eq!(elide_left("", |_| false), "");
    }
}
