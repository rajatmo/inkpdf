//! Reading what a PDF says about its pages for the model to keep: the geometry of each page, the links on them and the table of
//! contents. (The model, `links` and `outline`, holds them and carries them through edits and knows nothing of the file's
//! structure; `ink_io` reads and writes the ink the same way.)

use std::collections::{HashMap, HashSet};

use pdfcraft_annot::page_refs;
use pdfcraft_cos::{Dict, Document, ObjRef, Object};
use pdfcraft_organize::bookmarks;

use crate::coords::PageGeom;
use crate::links::{Link, Target};
use crate::outline::OutlineItem;
use crate::view::Layout;

/// Most links read from one file, so a hostile file cannot fill memory.
const MAX_LINKS: usize = 200_000;
/// Named destinations read from one file, and how deep their tree may go.
const MAX_NAMED: usize = 200_000;
const MAX_TREE_DEPTH: usize = 24;
/// Most outline entries read from one file, so a hostile file cannot fill memory.
const MAX_ITEMS: usize = 20_000;
/// Longest outline title kept, in characters.
const MAX_TITLE_CHARS: usize = 200;

/// What the renderer says about a page, as the geometry the program keeps.
pub fn page_geom(info: &pdfcraft_render::PageInfo) -> PageGeom {
    PageGeom { width: info.width, height: info.height, crop: info.crop, rotation: info.rotation }
}

/// Every link in the document that goes somewhere this app can follow.
pub fn links(doc: &Document, geoms: &[PageGeom], layout: &Layout) -> Vec<Link> {
    let Ok(pages) = page_refs(doc) else { return Vec::new() };
    let mut named = NamedDests::new(doc, &pages);
    let mut out = Vec::new();
    for (page, page_ref) in pages.iter().enumerate() {
        let (Some(geom), Some(placed)) = (geoms.get(page), layout.page(page)) else { continue };
        let page_obj = doc.get(*page_ref);
        let Some(annots) = page_obj.as_dict().and_then(|d| d.get(b"Annots")).map(|a| doc.resolve(a)) else { continue };
        let Some(list) = annots.as_array() else { continue };
        for entry in list {
            let obj = doc.resolve(entry);
            let Some(d) = obj.as_dict() else { continue };
            if d.name(b"Subtype") != Some(b"Link") {
                continue;
            }
            let (Some(rect), Some(target)) = (link_rect(doc, d, geom, [placed.x, placed.y]), target_of(doc, d, &pages, &mut named)) else { continue };
            out.push(Link { rect, target });
            if out.len() >= MAX_LINKS {
                return out;
            }
        }
    }
    out
}

/// The link's `/Rect` in document space: both corners to view space, then the box around them.
fn link_rect(doc: &Document, d: &Dict, geom: &PageGeom, origin: [f32; 2]) -> Option<[f32; 4]> {
    let rect = d.get(b"Rect").map(|r| doc.resolve(r))?;
    let nums: Vec<f64> = rect.as_array()?.iter().filter_map(|x| doc.resolve(x).as_f64()).collect();
    let [x0, y0, x1, y1] = nums[..] else { return None };
    let (a, b) = (geom.user_to_view([x0 as f32, y0 as f32]), geom.user_to_view([x1 as f32, y1 as f32]));
    let out = [origin[0] + a[0].min(b[0]), origin[1] + a[1].min(b[1]), origin[0] + a[0].max(b[0]), origin[1] + a[1].max(b[1])];
    out.iter().all(|c| c.is_finite()).then_some(out)
}

fn target_of(doc: &Document, d: &Dict, pages: &[ObjRef], named: &mut NamedDests) -> Option<Target> {
    if let Some(dest) = d.get(b"Dest") {
        return dest_page(doc, dest, pages, named).map(Target::Page);
    }
    let action = d.get(b"A").map(|a| doc.resolve(a))?;
    let action = action.as_dict()?;
    match action.name(b"S")? {
        b"URI" => {
            let uri = action.get(b"URI").map(|u| doc.resolve(u))?;
            let bytes = &uri.as_string()?.bytes;
            crate::links::safe_url(&String::from_utf8_lossy(bytes)).map(Target::Url)
        }
        b"GoTo" => dest_page(doc, action.get(b"D")?, pages, named).map(Target::Page),
        _ => None,
    }
}

/// The page a destination (an explicit array, or the name of one) is on.
fn dest_page(doc: &Document, dest: &Object, pages: &[ObjRef], named: &mut NamedDests) -> Option<usize> {
    let dest = doc.resolve(dest);
    match &*dest {
        Object::Array(a) => page_of_array(a, pages),
        Object::Name(_) | Object::String(_) => named.page_of(&text_bytes(&dest)?),
        _ => None,
    }
}

/// An explicit destination, `[page /XYZ ...]`: the page object it names.
fn page_of_array(array: &[Object], pages: &[ObjRef]) -> Option<usize> {
    let page = array.first()?.as_ref()?;
    pages.iter().position(|p| *p == page)
}

fn text_bytes(o: &Object) -> Option<Vec<u8>> {
    match o {
        Object::Name(_) => o.as_name().map(<[u8]>::to_vec),
        Object::String(s) => Some(s.bytes.clone()),
        _ => None,
    }
}

/// Named destinations (`/Dests` in the catalog, or the `/Names /Dests` tree), read once, when the
/// first link needs one, into a table from name to page.
struct NamedDests<'a> {
    doc: &'a Document,
    pages: &'a [ObjRef],
    table: Option<HashMap<Vec<u8>, usize>>,
}

impl<'a> NamedDests<'a> {
    fn new(doc: &'a Document, pages: &'a [ObjRef]) -> Self {
        Self { doc, pages, table: None }
    }

    fn page_of(&mut self, name: &[u8]) -> Option<usize> {
        if self.table.is_none() {
            self.table = Some(self.build());
        }
        self.table.as_ref()?.get(name).copied()
    }

    fn build(&self) -> HashMap<Vec<u8>, usize> {
        let mut table = HashMap::new();
        let Some(root) = self.doc.root() else { return table };
        let catalog = self.doc.get(root);
        let Some(catalog) = catalog.as_dict() else { return table };
        if let Some(legacy) = catalog.get(b"Dests").map(|d| self.doc.resolve(d))
            && let Some(dict) = legacy.as_dict()
        {
            for (name, value) in dict.iter() {
                if let Some(page) = self.value_page(value) {
                    table.insert(name.clone(), page);
                }
            }
        }
        let tree = catalog.get(b"Names").map(|n| self.doc.resolve(n));
        if let Some(tree) = tree.as_ref().and_then(|n| n.as_dict()).and_then(|n| n.get(b"Dests")) {
            self.walk(tree, 0, &mut HashSet::new(), &mut table);
        }
        table
    }

    /// A destination value: an array, or a dictionary holding one under `/D`.
    fn value_page(&self, value: &Object) -> Option<usize> {
        let value = self.doc.resolve(value);
        match &*value {
            Object::Array(a) => page_of_array(a, self.pages),
            Object::Dict(d) => match &*self.doc.resolve(d.get(b"D")?) {
                Object::Array(a) => page_of_array(a, self.pages),
                _ => None,
            },
            _ => None,
        }
    }

    fn walk(&self, node: &Object, depth: usize, seen: &mut HashSet<ObjRef>, table: &mut HashMap<Vec<u8>, usize>) {
        if depth > MAX_TREE_DEPTH || table.len() >= MAX_NAMED || node.as_ref().is_some_and(|r| !seen.insert(r)) {
            return;
        }
        let node = self.doc.resolve(node);
        let Some(dict) = node.as_dict() else { return };
        if let Some(names) = dict.get(b"Names").map(|n| self.doc.resolve(n))
            && let Some(pairs) = names.as_array()
        {
            for [name, value] in pairs.as_chunks::<2>().0 {
                if let (Some(name), Some(page)) = (text_bytes(&self.doc.resolve(name)), self.value_page(value)) {
                    table.insert(name, page);
                }
            }
        }
        if let Some(kids) = dict.get(b"Kids").map(|k| self.doc.resolve(k))
            && let Some(kids) = kids.as_array()
        {
            for kid in kids {
                self.walk(kid, depth + 1, seen, table);
            }
        }
    }
}

/// The outline of the document, in reading order; empty if it has none.
pub fn outline(doc: &Document) -> Vec<OutlineItem> {
    let listed = bookmarks(doc);
    let mut items: Vec<OutlineItem> = listed
        .iter()
        .take(MAX_ITEMS)
        .map(|b| OutlineItem { title: clean(&b.title), depth: b.path.len().saturating_sub(1), page: b.page, open: b.open, has_children: false })
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
        let doc = Document::open(std::sync::Arc::new(pdf_with_links())).expect("open");
        let geoms = vec![PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 }; 2];
        links(&doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]))
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
            if let Ok(doc) = Document::open(std::sync::Arc::new(bytes.clone())) {
                let geoms = vec![PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 }; 2];
                let _ = links(&doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]));
            }
            bytes[i] = saved;
        }
    }
    #[test]
    fn the_outline_of_a_file_is_read_in_reading_order_with_its_depths_and_pages() {
        let bytes = crate::testutil::pdf_with_outline();
        let doc = Document::open(std::sync::Arc::new(bytes)).expect("open");
        assert_eq!(outline(&doc), sample());
        let plain = Document::open(std::sync::Arc::new(crate::testutil::sample_pdf())).expect("open");
        assert!(outline(&plain).is_empty(), "a file with no outline has none");
    }

    #[test]
    fn titles_are_shown_clean() {
        assert_eq!(clean("  Chapter \t 1\n"), "Chapter 1");
        assert_eq!(clean("a\u{0}b\u{7}c"), "abc");
        assert_eq!(clean(&"x".repeat(1000)).chars().count(), MAX_TITLE_CHARS);
    }
}
