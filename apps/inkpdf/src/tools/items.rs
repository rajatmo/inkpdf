//! The kinds of item that extensions make (`inkpdf_api::ItemKind`): the program asks each extension for its kinds once, keeps them, and reads and
//! writes, draws, searches and opens an item through the kind its stroke names. An item whose kind this build does not have is left in a file as
//! the annotation it is (the renderer draws it), and is never loaded.

use std::sync::OnceLock;

use inkpdf_api::ItemKind;

use crate::ink::Stroke;

/// Every kind of item this build has, in the order the extensions list them; a kind whose name is used already, or is not well formed, is left out.
fn all() -> &'static [Box<dyn ItemKind>] {
    static KINDS: OnceLock<Vec<Box<dyn ItemKind>>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let mut kinds: Vec<Box<dyn ItemKind>> = Vec::new();
        for extension in inkpdf_ext::all().iter().filter(|e| e.problem().is_none()) {
            for kind in (extension.create)().item_kinds() {
                if well_named(kind.id()) && kinds.iter().all(|k| k.id() != kind.id()) {
                    kinds.push(kind);
                }
            }
        }
        #[cfg(test)]
        kinds.push(Box::new(test_kind::TestBox));
        kinds
    })
}

/// A name a file can carry: lower case letters, digits and `-`, beginning with a letter, at most 24.
fn well_named(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 24
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

/// The kind of item named `id`, if this build has one.
pub fn kind(id: &str) -> Option<&'static dyn ItemKind> {
    all().iter().find(|k| k.id() == id).map(|k| &**k)
}

/// The kind of an item stroke (`None` for any other stroke, and for an item of a kind this build does not have).
pub fn kind_of(stroke: &Stroke) -> Option<&'static dyn ItemKind> {
    stroke.item.as_ref().and_then(|i| kind(&i.kind))
}

/// A date as a PDF writes it (`D:YYYYMMDDHHMMSSZ`, in UTC), for `unix` seconds since 1970.
pub fn pdf_date(unix: i64) -> String {
    let (days, secs) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
    // (The civil date of a day number: Howard Hinnant's days-from-civil, run backwards.)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("D:{year:04}{month:02}{day:02}{:02}{:02}{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Now, as a PDF writes a date (the time of this computer's clock; a date on a message needs no more than that).
pub fn now_date() -> String {
    pdf_date(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64))
}

/// The text of an item stroke that a search finds, if its kind says there is some.
pub fn text_of(stroke: &Stroke) -> Option<String> {
    let item = stroke.item.as_ref()?;
    kind(&item.kind)?.text(&item.data).filter(|t| !t.trim().is_empty())
}

#[cfg(test)]
pub use test_kind::TestBox;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_written_as_a_pdf_writes_it_whatever_the_day() {
        assert_eq!(pdf_date(0), "D:19700101000000Z");
        assert_eq!(pdf_date(951_782_400), "D:20000229000000Z", "a leap day of a century that was one");
        assert_eq!(pdf_date(1_000_000_000), "D:20010909014640Z");
        assert_eq!(pdf_date(-1), "D:19691231235959Z", "a second before the epoch");
        assert!(now_date().starts_with("D:20") && now_date().len() == 17);
    }

    #[test]
    fn a_kind_is_found_by_its_name_and_a_name_that_a_file_cannot_carry_is_refused() {
        assert!(kind("testbox").is_some() && kind("nothing").is_none());
        for bad in ["", "Upper", "1abc", "a b", "a:b", "waytoolonganame-waytoolonganame"] {
            assert!(!well_named(bad), "{bad:?}");
        }
        assert!(well_named("sticky-note") && well_named("a1"));
    }
}

/// A kind of item for the program's own tests: a box holding some text, with a card that adds lines to it.
#[cfg(test)]
mod test_kind {
    use inkpdf_api::{Card, CardAction, CardRow, ItemKind, ItemView, Message, Note, Painter, Who};

    pub struct TestBox;

    impl TestBox {
        pub const ID: &'static str = "testbox";
    }

    impl ItemKind for TestBox {
        fn id(&self) -> &'static str {
            Self::ID
        }

        fn paint(&self, item: &ItemView<'_>, out: &mut dyn Painter) {
            let [x0, y0, x1, y1] = item.bounds;
            for (a, b) in [([x0, y0], [x1, y0]), ([x1, y0], [x1, y1]), ([x1, y1], [x0, y1]), ([x0, y1], [x0, y0])] {
                out.doc_line(a, b, 1.0, item.colour);
            }
        }

        /// The first line is the note's text, the others its replies.
        fn write(&self, data: &str) -> Note {
            let mut lines = data.lines();
            let text = lines.next().unwrap_or("").to_string();
            let replies = lines.map(|l| Message { author: "tester".into(), date: String::new(), text: l.into() }).collect();
            Note { author: "tester".into(), date: String::new(), text, replies, closed: false }
        }

        fn read(&self, note: &Note) -> Option<String> {
            Some(std::iter::once(note.text.as_str()).chain(note.replies.iter().map(|r| r.text.as_str())).collect::<Vec<_>>().join("\n"))
        }

        fn text(&self, data: &str) -> Option<String> {
            Some(data.to_string())
        }

        fn card(&self, data: &str) -> Option<Card> {
            Some(Card {
                title: "A test box".into(),
                rows: data.lines().map(|l| CardRow { head: String::new(), body: l.into() }).collect(),
                field: Some("say".into()),
                actions: vec![CardAction { id: "clear", label: "Clear".into() }],
            })
        }

        fn act(&self, data: &str, action: &str, input: &str, _who: &Who<'_>) -> Option<String> {
            match action {
                "clear" => Some(String::new()),
                "" if !input.trim().is_empty() => Some(if data.is_empty() { input.to_string() } else { format!("{data}\n{input}") }),
                _ => None,
            }
        }
    }
}
