//! Links in a PDF: where each one is on its page and where it goes, and how they follow the pages through edits. Following a link is
//! up to the editor, and only a finger's tap does it. (Reading them out of a file is `pdf_read`.)

use crate::ink::{P, rect_contains};
use crate::pages::Remap;
use crate::view::Layout;

/// Longest web address followed, in bytes.
const MAX_URL_BYTES: usize = 2048;

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page of this document (0-based).
    Page(usize),
    /// A web address that [`safe_url`] allows.
    Url(String),
    /// A box of a form field (see [`crate::form`]): the field's place among the form's, and which of its boxes this is. It is filled, not
    /// followed.
    Field { field: usize, widget: usize },
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

/// What a damaged file can say: a link to a page the document does not show (it has `pages` pages) is not a link.
pub fn limit_to(links: &mut Vec<Link>, pages: usize) {
    links.retain(|l| !matches!(l.target, Target::Page(p) if p >= pages));
}

/// Carry the links through an edit of the pages (`before` is the layout the links are in): each link keeps its place on
/// its page, and a link to a page points to its new number. A link on a page that went, or to one, has nothing left to be.
/// Returns the links that went, each with its place in the list, for [`restore`].
pub fn remap(links: &mut Vec<Link>, before: &Layout, remap: &Remap) -> Vec<(usize, Link)> {
    let mut gone = Vec::new();
    for (i, mut link) in std::mem::take(links).into_iter().enumerate() {
        let carried = remap.moved(before.page_at((link.rect[1] + link.rect[3]) / 2.0)).filter(|_| match &link.target {
            Target::Page(p) => remap.page(*p).is_some(),
            Target::Url(_) | Target::Field { .. } => true,
        });
        let Some(moved) = carried else {
            gone.push((i, link));
            continue;
        };
        if let Target::Page(p) = &mut link.target {
            *p = remap.page(*p).unwrap_or(*p);
        }
        link.rect = [link.rect[0] + moved.dx, link.rect[1] + moved.dy, link.rect[2] + moved.dx, link.rect[3] + moved.dy];
        links.push(link);
    }
    gone
}

/// The links that copies of pages have: those on the page each is a copy of, in the copy's place on the copy, going where
/// they went (to the number the page has now). `before` is the layout the links are in, before `map` is done.
pub fn copies(links: &[Link], before: &Layout, map: &Remap) -> Vec<Link> {
    let mut made = Vec::new();
    for copy in map.copies() {
        for link in links.iter().filter(|l| before.page_at((l.rect[1] + l.rect[3]) / 2.0) == copy.from) {
            let target = match &link.target {
                Target::Page(p) => map.page(*p).map(Target::Page),
                url => Some(url.clone()),
            };
            if let Some(target) = target {
                let r = link.rect;
                made.push(Link { rect: [r[0] + copy.dx, r[1] + copy.dy, r[2] + copy.dx, r[3] + copy.dy], target });
            }
        }
    }
    made
}

/// Undo what [`remap`] did for an edit that deleted pages: `map` is what the pages coming back does to the numbers and
/// places (`before` is the layout the links are in now), and `gone` what [`remap`] returned for the deletion: those links
/// are as they were, and go back where they were in the list.
pub fn restore(links: &mut Vec<Link>, before: &Layout, map: &Remap, gone: Vec<(usize, Link)>) {
    remap(links, before, map);
    for (i, link) in gone {
        links.insert(i.min(links.len()), link);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Layout;

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
    fn links_follow_their_pages_through_every_kind_of_edit() {
        use crate::pages::PageEdit;
        let before = Layout::new(&[[600.0, 800.0]; 3]);
        let on = |page: usize, target: Target| {
            let r = before.page(page).expect("page");
            Link { rect: [r.x + 10.0, r.y + 10.0, r.x + 50.0, r.y + 30.0], target }
        };
        let links = vec![on(0, Target::Page(2)), on(1, Target::Page(0)), on(2, Target::Url("https://example.com".into()))];
        let through = |edit: PageEdit| {
            let after = before.edited(&edit).expect("layout");
            let map = Remap::new(&edit, &before, &after).expect("remap");
            let mut kept = links.clone();
            remap(&mut kept, &before, &map);
            (kept, after)
        };
        // A page inserted in the middle: the links after it move down with their pages, and so do the numbers they go to.
        let (kept, after) = through(PageEdit::Insert { at: 1, size: [600.0, 800.0] });
        assert_eq!(kept.iter().map(|l| l.target.clone()).collect::<Vec<_>>()[..2], [Target::Page(3), Target::Page(0)]);
        let third = after.page(3).expect("page");
        assert_eq!(kept[2].rect, [third.x + 10.0, third.y + 10.0, third.x + 50.0, third.y + 30.0], "in the same place on its page");
        // A page deleted: its links go, and the links to it.
        let (kept, _) = through(PageEdit::delete(2));
        assert_eq!(kept.len(), 1, "one on the deleted page, one to it");
        assert_eq!(kept[0].target, Target::Page(0));
        // Pages moved: the links keep their pages, whatever the pages are now called.
        let (kept, after) = through(PageEdit::Move { pages: vec![2], to: 0 });
        assert_eq!(
            kept.iter().map(|l| l.target.clone()).collect::<Vec<_>>(),
            [Target::Page(0), Target::Page(1), Target::Url("https://example.com".into())]
        );
        let top = after.page(0).expect("page");
        assert_eq!(kept[2].rect[1], top.y + 10.0, "the link on the moved page is on the first page now");
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
}
