//! Web links, as an extension with no button: a link in a PDF that goes out to the web is not handed to the browser unseen. A card says where it goes
//! (the site's name first and plainly, then the whole address), warns of the tricks an address can play (a name in front of the site's own, a site named
//! in letters that only look like another's), and has a button to open it in the system's browser, one to copy it, and one to close the card. It is the place
//! a web view would go: the card is the same for it, only what fills it would change.
//!
//! It touches neither the system nor the network: it says what the card shows, and asks the program to open or copy.

use inkpdf_api::{API_VERSION, Card, CardAction, CardRow, Context, Device, Extension, LinkAct, LinkHandler, PointerEvent, Policy, Tool};

/// The extension, for the list of the program's. (No icon: it is not a tool to choose.)
pub fn extension() -> Extension {
    Extension { api: API_VERSION, id: "weblink", label: "Web links", icon_svg: "", create: || Box::new(Links) }
}

/// The tool this extension has to have to be one: it is never chosen, hears nothing and shows nothing.
struct Links;

impl Tool for Links {
    fn policy(&self, _desktop: bool) -> Policy {
        Policy { stylus: false, mouse: inkpdf_api::MouseUse::Scrolls, keyboard: false }
    }
    fn pointer(&mut self, _cx: &mut dyn Context, _ev: &PointerEvent<'_>) {}
    fn owns(&self, _device: Device, _id: u32) -> bool {
        false
    }
    fn busy(&self) -> bool {
        false
    }
    fn end(&mut self, _cx: &mut dyn Context) {}
    fn cancel(&mut self, _cx: &mut dyn Context) {}
    fn link_handlers(&self) -> Vec<Box<dyn LinkHandler>> {
        vec![Box::new(Web)]
    }
}

struct Web;

/// The part of a web address before the path: `user@host:port` of `https://user@host:port/path`, without the scheme.
fn authority(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|a| !a.is_empty())
}

/// What an address says about where it goes.
#[derive(Debug, PartialEq, Eq)]
struct Where {
    /// The site's name, as the browser will read it (after any `user@`, before any `:port`), lower case.
    host: String,
    /// There is a name in front of the site's (`https://paypal.com@elsewhere.example/`): what a person reads first is not where it goes.
    userinfo: bool,
    /// Some part of the site's name is written in an encoding that stands for letters of other alphabets (`xn--`).
    disguised: bool,
}

fn where_to(url: &str) -> Option<Where> {
    let authority = authority(url)?;
    // (The last `@` is where the host begins: what is before it, a name and perhaps a password, is not the site.)
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(at) => (true, &authority[at + 1..]),
        None => (false, authority),
    };
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        // An address in brackets (`[::1]:8080`): up to the closing bracket.
        let end = rest.find(']')?;
        format!("[{}]", &rest[..end])
    } else {
        host_port.split(':').next().unwrap_or_default().trim_end_matches('.').to_lowercase()
    };
    if host.is_empty() {
        return None;
    }
    let disguised = host.split('.').any(|label| label.starts_with("xn--"));
    Some(Where { host, userinfo, disguised })
}

impl LinkHandler for Web {
    fn id(&self) -> &'static str {
        "web"
    }

    fn card(&self, url: &str) -> Option<Card> {
        let place = where_to(url)?;
        let mut rows = vec![CardRow { head: "Opens in your browser, on the site".into(), body: place.host.clone() }];
        if place.userinfo {
            rows.push(CardRow { head: "Look twice".into(), body: "A name comes before the site's own in this address".into() });
        } else if place.disguised {
            rows.push(CardRow { head: "Look twice".into(), body: "The site's name is written in letters that can stand for others".into() });
        }
        rows.push(CardRow { head: "Address".into(), body: url.to_string() });
        Some(Card {
            title: "Web link".into(),
            rows,
            field: None,
            actions: vec![CardAction { id: "open", label: "Open".into() }, CardAction { id: "copy", label: "Copy".into() }],
        })
    }

    fn act(&self, _url: &str, action: &str) -> LinkAct {
        match action {
            "open" => LinkAct::Open,
            "copy" => LinkAct::Copy,
            _ => LinkAct::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(url: &str) -> Option<String> {
        where_to(url).map(|w| w.host)
    }

    #[test]
    fn the_site_is_what_the_browser_will_read_whatever_is_around_it() {
        assert_eq!(host("https://example.com/a/b?c=d#e").as_deref(), Some("example.com"));
        assert_eq!(host("HTTP://Example.COM").as_deref(), Some("example.com"), "lower case, and a path is not needed");
        assert_eq!(host("https://example.com:8443/x").as_deref(), Some("example.com"), "without the port");
        assert_eq!(host("https://example.com./x").as_deref(), Some("example.com"), "without the dot that ends a name");
        assert_eq!(host("https://[::1]:8080/x").as_deref(), Some("[::1]"));
        assert_eq!(host("https://example.com?x=1").as_deref(), Some("example.com"), "a query with no path");
        for odd in ["", "example.com", "https://", "https:///path", "https://@", "https://[::1", "mailto:someone@example.com"] {
            assert_eq!(host(odd), None, "{odd:?}");
        }
    }

    #[test]
    fn a_name_before_the_site_is_said_to_be_one_and_it_is_not_taken_for_the_site() {
        let w = where_to("https://paypal.com@elsewhere.example/login").expect("a place");
        assert_eq!((w.host.as_str(), w.userinfo, w.disguised), ("elsewhere.example", true, false), "the site is what follows the last @");
        let tricky = where_to("https://paypal.com:secret@a@b.example/").expect("a place");
        assert_eq!((tricky.host.as_str(), tricky.userinfo), ("b.example", true));
        assert!(!where_to("https://example.com/user@home").expect("a place").userinfo, "an @ in the path is nothing");
        assert!(where_to("https://xn--pypal-4ve.example/").expect("a place").disguised, "a name in an encoding of other letters");
        assert!(!where_to("https://example.com/xn--x").expect("a place").disguised, "only in the site's name");
    }

    #[test]
    fn the_card_says_where_it_goes_and_warns_and_has_open_and_copy_and_no_field() {
        let web = Web;
        let card = web.card("https://example.com/page").expect("a card");
        assert_eq!((card.title.as_str(), card.field.as_ref(), card.rows.len()), ("Web link", None, 2));
        assert_eq!((card.rows[0].body.as_str(), card.rows[1].body.as_str()), ("example.com", "https://example.com/page"));
        assert_eq!(card.actions.iter().map(|a| (a.id, a.label.as_str())).collect::<Vec<_>>(), [("open", "Open"), ("copy", "Copy")]);
        let warned = web.card("https://paypal.com@evil.example/").expect("a card");
        assert_eq!(warned.rows.len(), 3);
        assert_eq!(warned.rows[1].head, "Look twice");
        assert_eq!(warned.rows[0].body, "evil.example", "the site first, plainly");
        assert!(web.card("not an address").is_none(), "an address that goes nowhere has no card");
    }

    #[test]
    fn the_buttons_ask_the_program_to_open_or_copy_and_anything_else_does_nothing() {
        let web = Web;
        assert_eq!(web.act("https://example.com/", "open"), LinkAct::Open);
        assert_eq!(web.act("https://example.com/", "copy"), LinkAct::Copy);
        for other in ["", "close", "OPEN", "open "] {
            assert_eq!(web.act("https://example.com/", other), LinkAct::Nothing, "{other:?}");
        }
        let e = extension();
        assert_eq!(e.problem(), None, "a well formed extension with no button");
        assert!(e.icon_svg.is_empty());
        let tool = (e.create)();
        assert_eq!(tool.link_handlers().iter().map(|h| h.id()).collect::<Vec<_>>(), ["web"]);
        assert!(!tool.busy() && !tool.policy(true).stylus && !tool.policy(true).keyboard);
    }

    #[test]
    fn hostile_addresses_make_no_panic() {
        let web = Web;
        for url in ["https://\u{0}", "https://[", "https://@@@@", &format!("https://{}", "a".repeat(100_000)), "https://a b/c", "https://\u{202e}moc.elpmaxe/", "://"] {
            let _ = web.card(url);
        }
    }
}
