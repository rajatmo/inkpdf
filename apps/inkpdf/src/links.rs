//! Links in a PDF: where each one is on its page and where it goes. Following a link is up to the
//! editor, and only a finger's tap does it.

use std::collections::{HashMap, HashSet};

use pdfcraft_annot::page_refs;
use pdfcraft_cos::{Dict, Document, ObjRef, Object};

use crate::coords::PageGeom;
use crate::ink::{P, rect_contains};
use crate::view::Layout;

/// Most links read from one file, so a hostile file cannot fill memory.
const MAX_LINKS: usize = 200_000;
/// Longest web address followed, in bytes.
const MAX_URL_BYTES: usize = 2048;
/// Named destinations read from one file, and how deep their tree may go.
const MAX_NAMED: usize = 200_000;
const MAX_TREE_DEPTH: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page of this document (0-based).
    Page(usize),
    /// A web address that [`safe_url`] allows.
    Url(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// `[x0, y0, x1, y1]` in document space (see [`Layout`]).
    pub rect: [f32; 4],
    pub target: Target,
}

/// The link under `p` (document space), the topmost when several overlap.
pub fn hit(links: &[Link], p: P) -> Option<&Link> {
    links.iter().rev().find(|l| rect_contains(l.rect, p))
}

/// A web address that is safe to hand to the system: a plain `http`, `https` or `mailto` address
/// with no spaces or control characters. Anything else a PDF can ask for (a program, a file, a
/// script) is refused.
pub fn safe_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    if url.is_empty() || url.len() > MAX_URL_BYTES || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = url.split_once(':')?;
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => rest.strip_prefix("//").is_some_and(|host| host.chars().next().is_some_and(|c| c != '/')).then(|| url.to_string()),
        "mailto" => (!rest.is_empty()).then(|| url.to_string()),
        _ => None,
    }
}

/// Every link in the document that goes somewhere this app can follow.
pub fn read(doc: &Document, geoms: &[PageGeom], layout: &Layout) -> Vec<Link> {
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
            safe_url(&String::from_utf8_lossy(bytes)).map(Target::Url)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::pdf_with_links;
    use crate::view::Layout;

    fn read_sample() -> Vec<Link> {
        let doc = Document::open(std::sync::Arc::new(pdf_with_links())).expect("open");
        let geoms = vec![PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 }; 2];
        read(&doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]))
    }

    #[test]
    fn web_addresses_are_checked_before_anything_is_opened() {
        for ok in ["http://example.com", "https://example.com/a?b=c#d", "HTTPS://EXAMPLE.COM", "mailto:me@example.com", "  https://example.com  "] {
            assert!(safe_url(ok).is_some(), "{ok}");
        }
        for bad in [
            "",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "C:\\Windows\\System32\\cmd.exe",
            "cmd.exe /c calc",
            "ms-msdt:/id PCWDiagnostic",
            "https://",
            "https:///",
            "http://exa mple.com",
            "http://exam\nple.com",
            "mailto:",
            "ftp://example.com",
            "//example.com",
        ] {
            assert!(safe_url(bad).is_none(), "{bad:?}");
        }
        assert!(safe_url(&format!("https://example.com/{}", "a".repeat(3000))).is_none(), "absurdly long");
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
    fn the_topmost_link_under_a_point_wins() {
        let links = vec![
            Link { rect: [0.0, 0.0, 100.0, 100.0], target: Target::Page(0) },
            Link { rect: [50.0, 50.0, 150.0, 150.0], target: Target::Page(1) },
        ];
        assert_eq!(hit(&links, [60.0, 60.0]).map(|l| &l.target), Some(&Target::Page(1)));
        assert_eq!(hit(&links, [10.0, 10.0]).map(|l| &l.target), Some(&Target::Page(0)));
        assert_eq!(hit(&links, [200.0, 10.0]), None);
        assert_eq!(hit(&[], [0.0, 0.0]), None);
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
                let _ = read(&doc, &geoms, &Layout::new(&[[612.0, 792.0], [612.0, 792.0]]));
            }
            bytes[i] = saved;
        }
    }
}
