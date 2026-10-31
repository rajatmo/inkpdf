//! The card for a link that goes out to the web (the web-link extension's): shown instead of the address going straight to the system, with buttons to
//! open it and copy it, for every device; and what happens when no extension takes the link.

use super::*;
use crate::input::Mode;
use crate::links::{Link, Target};
use crate::toolbar::CardItem;
use crate::tools::Key;

const ADDRESS: &str = "https://example.com/some/page";

/// A window with the extension that has a card for web links.
fn with_weblink() -> Editor {
    let web: Vec<_> = inkpdf_ext::all().into_iter().filter(|e| e.id == "weblink").collect();
    assert_eq!(web.len(), 1, "the web link extension is in this build");
    let mut ed = Editor::with_extensions([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED, &web);
    ed.load(crate::view::Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    ed.fit_mode = false;
    ed.view.zoom = 1.0;
    ed.pan_px(0.0, 0.0);
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Url(ADDRESS.into()) }];
    ed
}

fn follow(ed: &mut Editor) {
    let on = on_page(ed, 150.0, 300.0);
    finger_tap(ed, on);
}

/// A click of the left button on the link (with the Select tool, which is the one chosen, a click follows a link as a finger's tap does).
fn click_link(ed: &mut Editor) {
    let on = on_page(ed, 150.0, 300.0);
    mouse_click_at(ed, on);
}

fn press(ed: &Editor, item: CardItem) -> [f32; 2] {
    ed.link_layout().expect("the card").centre_of(item)
}

#[test]
fn a_web_link_shows_a_card_that_says_where_it_goes_and_asks_the_system_for_nothing_yet() {
    let mut ed = with_weblink();
    follow(&mut ed);
    assert!(ed.link_card_open() && ed.requests.url().is_none(), "a card, and the address is not handed over unseen");
    let view = ed.link_view().expect("the card");
    assert_eq!(view.title, "Web link");
    assert_eq!(view.rows[0].1, "example.com", "the site first and plainly");
    assert_eq!(view.rows.last().map(|r| r.1.as_str()), Some(ADDRESS), "then the whole address");
    assert_eq!(view.actions, ["Open", "Copy"]);
    assert!(view.hint.is_none() && !ed.is_typing(), "no field: the keyboard is not raised for it");
}

#[test]
fn open_hands_the_address_to_the_system_and_copy_puts_it_on_the_clipboard_each_putting_the_card_away() {
    let mut ed = with_weblink();
    follow(&mut ed);
    let open = press(&ed, CardItem::Action(0));
    pen_tap(&mut ed, open);
    assert_eq!(ed.requests.url(), Some(ADDRESS), "Open");
    assert!(!ed.link_card_open());
    ed.requests.clear_urls();
    // (A finger, in an editor the pen has not been near.)
    let mut other = with_weblink();
    follow(&mut other);
    let copy = press(&other, CardItem::Action(1));
    finger_tap(&mut other, copy);
    assert_eq!(other.clip.text.get().as_deref(), Some(ADDRESS), "Copy, with a finger");
    assert!(!other.link_card_open() && other.requests.url().is_none(), "and nothing was opened");
}

#[test]
fn every_device_closes_the_card_and_escape_does_and_a_press_on_it_never_reaches_the_page() {
    let mut ed = with_weblink();
    ed.set_mode(Mode::Desktop);
    click_link(&mut ed);
    let close = press(&ed, CardItem::Close);
    mouse_click_at(&mut ed, close);
    assert!(!ed.link_card_open(), "the left button");
    click_link(&mut ed);
    let close = press(&ed, CardItem::Close);
    pen_tap(&mut ed, close);
    assert!(!ed.link_card_open(), "the tip");
    click_link(&mut ed);
    assert!(ed.press(Key::Escape) && !ed.link_card_open(), "Escape");
    click_link(&mut ed);
    let l = ed.link_layout().expect("the card");
    let blank = [l.card[0] + 2.0, l.card[3] - 2.0];
    assert!(ed.over_interface(blank));
    mouse_click_at(&mut ed, blank);
    assert!(ed.link_card_open() && ed.ink.len() == 0, "a press on the card that is not a button changes nothing and draws nothing");
    let mut touched = with_weblink();
    follow(&mut touched);
    let close = press(&touched, CardItem::Close);
    finger_tap(&mut touched, close);
    assert!(!touched.link_card_open(), "a finger");
}

#[test]
fn a_page_link_does_not_get_a_card_a_new_link_replaces_the_card_and_a_new_document_forgets_it() {
    let mut ed = with_weblink();
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Page(1) }];
    follow(&mut ed);
    assert!(!ed.link_card_open(), "a link inside the document goes to its page");
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Url(ADDRESS.into()) }];
    ed.pan_px(0.0, 10_000.0);
    settle(&mut ed);
    follow(&mut ed);
    assert!(ed.link_card_open());
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Url("https://paypal.com@evil.example/".into()) }];
    ed.close_link_card();
    follow(&mut ed);
    let view = ed.link_view().expect("the card");
    assert_eq!(
        (view.rows[0].1.as_str(), view.rows[1].0.as_str()),
        ("evil.example", "Look twice"),
        "the site, and the warning about the name before it"
    );
    ed.load(crate::view::Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.link_card_open(), "a new document has no card");
}

#[test]
fn without_the_extension_the_address_goes_to_the_system_as_it_always_did() {
    let mut ed = editor();
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target: Target::Url(ADDRESS.into()) }];
    let on = on_page(&ed, 150.0, 300.0);
    finger_tap(&mut ed, on);
    assert!(!ed.link_card_open());
    assert_eq!(ed.requests.url(), Some(ADDRESS));
}
