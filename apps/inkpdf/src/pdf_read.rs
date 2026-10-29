//! What a PDF says about its pages, in the model's own terms: the links placed in document space, and the table of contents tidied.
//! The engine hands over raw data (`pdf::RawLink`, `pdf::RawOutlineItem`: the page's own user space, the file's own text); this
//! places it on the layout and drops what the program will not follow. (`links` and `outline` hold the results and carry them through
//! edits and know nothing of the file; `ink_io` does the same for the ink.)

use crate::coords::PageGeom;
use crate::links::{Link, Target, safe_url};
use crate::outline::OutlineItem;
use crate::pdf::{PdfDocument, RawTarget};
use crate::view::Layout;

/// Longest outline title kept, in characters.
const MAX_TITLE_CHARS: usize = 200;

/// Every link in the document that goes somewhere this app can follow, placed on the layout.
pub fn links(doc: &dyn PdfDocument, geoms: &[PageGeom], layout: &Layout) -> Vec<Link> {
    doc.links()
        .into_iter()
        .filter_map(|raw| {
            let (geom, placed) = (geoms.get(raw.page)?, layout.page(raw.page)?);
            let target = match raw.target {
                RawTarget::Page(page) => Target::Page(page),
                RawTarget::Url(url) => Target::Url(safe_url(&url)?),
            };
            // Both corners to view space, then the box around them, then onto the page's place in the document.
            let [x0, y0, x1, y1] = raw.rect;
            let (a, b) = (geom.user_to_view([x0, y0]), geom.user_to_view([x1, y1]));
            let rect = [placed.x + a[0].min(b[0]), placed.y + a[1].min(b[1]), placed.x + a[0].max(b[0]), placed.y + a[1].max(b[1])];
            rect.iter().all(|c| c.is_finite()).then_some(Link { rect, target })
        })
        .collect()
}

/// The outline of the document, in reading order; empty if it has none.
pub fn outline(doc: &dyn PdfDocument) -> Vec<OutlineItem> {
    let mut items: Vec<OutlineItem> = doc
        .outline()
        .into_iter()
        .map(|b| OutlineItem { title: clean(&b.title), depth: b.depth, page: b.page, open: b.open, has_children: false })
        .collect();
    for i in 0..items.len().saturating_sub(1) {
        items[i].has_children = items[i + 1].depth > items[i].depth;
    }
    items
}

/// A title as it is shown: no control characters, no runs of blanks, not unreasonably long.
fn clean(title: &str) -> String {
    let words: Vec<&str> = title.split_whitespace().collect();
    let joined: String = words.join(" ").chars().filter(|c| !c.is_control()).collect();
    joined.chars().take(MAX_TITLE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::Target;
    use crate::testutil::pdf_with_links;

    fn read_sample() -> Vec<Link> {
        let doc = crate::pdf::native().open(&std::sync::Arc::new(pdf_with_links())).expect("open");
        let geoms = vec![PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 }; 2];
        links(&*doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]))
    }

    fn item(title: &str, depth: usize, page: Option<usize>) -> OutlineItem {
        OutlineItem { title: title.into(), depth, page, open: false, has_children: false }
    }

    /// The sample outline, as a list: `Chapter 1 > (1.1, 1.2)`, `Chapter 2`, and `Appendix`, which goes nowhere.
    fn sample() -> Vec<OutlineItem> {
        let mut items = vec![
            item("Chapter 1", 0, Some(0)),
            item("1.1 Basics", 1, Some(1)),
            item("1.2 More", 1, Some(2)),
            item("Chapter 2", 0, Some(3)),
            item("Appendix", 0, None),
        ];
        items[0].has_children = true;
        items[0].open = true;
        items
    }

    #[test]
    fn links_come_back_with_their_page_position_and_destination() {
        let links = read_sample();
        // A web link, a go-to-page link, a link to a named destination: the Launch action and the
        // javascript address are not followed.
        assert_eq!(links.len(), 4, "{links:#?}");
        assert_eq!(links[0].target, Target::Url("https://example.com/".into()));
        // Page 0 starts 12 points in from the corner of the document; the PDF's y axis points up.
        let expected = [12.0 + 100.0, 12.0 + 792.0 - 700.0, 12.0 + 300.0, 12.0 + 792.0 - 680.0];
        assert!(links[0].rect.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", links[0].rect);
        assert_eq!(links[1].target, Target::Page(1));
        assert_eq!(links[2].target, Target::Page(1), "a named destination is found in the /Names tree");
        assert_eq!(links[3].target, Target::Page(0), "and in the older /Dests dictionary");
    }

    #[test]
    fn broken_link_data_gives_fewer_links_not_a_crash() {
        let mut bytes = pdf_with_links();
        // Damage the file in many different places: reading links must still return something.
        for i in (200..bytes.len().saturating_sub(100)).step_by(37) {
            let saved = bytes[i];
            bytes[i] = b'#';
            if let Ok(doc) = crate::pdf::native().open(&std::sync::Arc::new(bytes.clone())) {
                let geoms = vec![PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 }; 2];
                let _ = links(&*doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]));
            }
            bytes[i] = saved;
        }
    }
    #[test]
    fn the_outline_of_a_file_is_read_in_reading_order_with_its_depths_and_pages() {
        let bytes = crate::testutil::pdf_with_outline();
        let doc = crate::pdf::native().open(&std::sync::Arc::new(bytes)).expect("open");
        assert_eq!(outline(&*doc), sample());
        let plain = crate::pdf::native().open(&std::sync::Arc::new(crate::testutil::sample_pdf())).expect("open");
        assert!(outline(&*plain).is_empty(), "a file with no outline has none");
    }

    #[test]
    fn titles_are_shown_clean() {
        assert_eq!(clean("  Chapter \t 1\n"), "Chapter 1");
        assert_eq!(clean("a\u{0}b\u{7}c"), "abc");
        assert_eq!(clean(&"x".repeat(1000)).chars().count(), MAX_TITLE_CHARS);
    }
}
