//! The logic of Android's files and clipboard, apart from Android: what the Java side of the app (`tools/android/java`) offers is a [`Bridge`],
//! and a document kept at a `content://` address is read and written through it, the clipboard is held by it, and a file the person picked
//! comes back from it. Written against the bridge so that it is tested everywhere with a fake one; only the bridge itself (JNI) is Android's.

use std::sync::Arc;

use super::{Pick, Picked};
use crate::clipboard::TextClipboard;
use crate::path_store::PathStore;
use crate::store::{DocId, FileStore};

/// What the Java side of the app does, in the program's terms. Every call may be made from any thread.
pub trait Bridge: Send + Sync {
    /// The bytes at a `content://` address.
    fn read(&self, uri: &str) -> Result<Vec<u8>, String>;
    /// Replace what is at a `content://` address with `bytes`.
    fn write(&self, uri: &str, bytes: &[u8]) -> Result<(), String>;
    /// Put text on the system clipboard.
    fn copy(&self, text: &str);
    /// The text on the system clipboard, if there is any.
    fn paste(&self) -> Option<String>;
    /// Open a web address in the system's program for it.
    fn open_url(&self, url: &str);
    /// The file's own name, which the system's provider knows and a `content://` address does not say, if it will tell.
    fn display_name(&self, uri: &str) -> Option<String>;
    /// Hide the system's bars (status and navigation), or show them again.
    fn set_bars_hidden(&self, hidden: bool);
    /// Show the system pointer icon of this PointerIcon type over the window (what a mouse or a hovering stylus shows).
    fn set_pointer_icon(&self, type_code: i32);
    /// How many pixels, from the bottom of the window, the on-screen keyboard covers now (0 when it is not up).
    fn keyboard_height(&self) -> i32 {
        0
    }
    /// Ask the system's picker for a file to open, or for a place and name to save under (the answer comes by [`Bridge::take_picked`]).
    fn pick(&self, kind: Pick, name: &str);
    /// What the picker answered, if it has, as the Java side words it: the kind (`0` open, `1` save), a tab, and the address (none if the
    /// person gave up).
    fn take_picked(&self) -> Option<String>;
    /// The address of a document the app was started with by another app ("Open with", a share), if there is one not yet taken.
    fn take_launch(&self) -> Option<String>;
}

/// The files of a platform that hands out addresses: a `content://` address through the bridge, a path as everywhere else.
pub struct SafStore {
    bridge: Arc<dyn Bridge>,
}

impl SafStore {
    pub fn new(bridge: Arc<dyn Bridge>) -> SafStore {
        SafStore { bridge }
    }
}

impl FileStore for SafStore {
    fn read(&self, id: &DocId) -> Result<Vec<u8>, String> {
        match id {
            DocId::Uri(uri) => self.bridge.read(uri),
            DocId::Path(_) => PathStore.read(id),
        }
    }

    fn write_atomic(&self, id: &DocId, bytes: &[u8]) -> Result<(), String> {
        match id {
            // (The system truncates and writes: if it fails part-way the file may be cut short, which no call here can prevent. The
            // document is only ever written whole, from memory, in one call.)
            DocId::Uri(uri) => self.bridge.write(uri, bytes),
            DocId::Path(_) => PathStore.write_atomic(id, bytes),
        }
    }

    fn display_name(&self, id: &DocId) -> String {
        match id {
            DocId::Uri(uri) => self.bridge.display_name(uri).filter(|n| !n.is_empty()).unwrap_or_else(|| id.name()),
            DocId::Path(_) => id.name(),
        }
    }
}

/// The system clipboard, held through the bridge.
pub struct BridgeClipboard(pub Arc<dyn Bridge>);

impl TextClipboard for BridgeClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.paste()
    }

    fn set(&mut self, text: &str) {
        self.0.copy(text);
    }
}

/// What the picker answered, from the Java side's words (see [`Bridge::take_picked`]).
pub fn parse_picked(raw: &str) -> Option<Picked> {
    let (kind, uri) = raw.split_once('\t')?;
    let kind = match kind {
        "0" => Pick::Open,
        "1" => Pick::Save,
        _ => return None,
    };
    Some(Picked { kind, id: Some(uri).filter(|u| !u.is_empty()).map(|u| DocId::Uri(u.to_string())) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        files: Mutex<Vec<(String, Vec<u8>)>>,
        clipboard: Mutex<Option<String>>,
        picks: Mutex<Vec<(Pick, String)>>,
        answer: Mutex<Option<String>>,
        launch: Mutex<Option<String>>,
        urls: Mutex<Vec<String>>,
        bars_hidden: Mutex<bool>,
        pointer: Mutex<i32>,
    }

    impl Bridge for Fake {
        fn display_name(&self, uri: &str) -> Option<String> {
            // (A provider that knows a file's name, as Android's does for a picked one; this one knows only the addresses that say "named".)
            uri.contains("named").then(|| "Meeting notes.pdf".to_string())
        }
        fn set_bars_hidden(&self, hidden: bool) {
            *self.bars_hidden.lock().unwrap() = hidden;
        }
        fn set_pointer_icon(&self, type_code: i32) {
            *self.pointer.lock().unwrap() = type_code;
        }
        fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
            self.files.lock().unwrap().iter().find(|(u, _)| u == uri).map(|(_, b)| b.clone()).ok_or_else(|| format!("no such file: {uri}"))
        }
        fn write(&self, uri: &str, bytes: &[u8]) -> Result<(), String> {
            if uri.contains("readonly") {
                return Err("read-only".into());
            }
            let mut files = self.files.lock().unwrap();
            match files.iter_mut().find(|(u, _)| u == uri) {
                Some(file) => file.1 = bytes.to_vec(),
                None => files.push((uri.to_string(), bytes.to_vec())),
            }
            Ok(())
        }
        fn copy(&self, text: &str) {
            *self.clipboard.lock().unwrap() = Some(text.to_string());
        }
        fn paste(&self) -> Option<String> {
            self.clipboard.lock().unwrap().clone()
        }
        fn open_url(&self, url: &str) {
            self.urls.lock().unwrap().push(url.to_string());
        }
        fn pick(&self, kind: Pick, name: &str) {
            self.picks.lock().unwrap().push((kind, name.to_string()));
        }
        fn take_picked(&self) -> Option<String> {
            self.answer.lock().unwrap().take()
        }
        fn take_launch(&self) -> Option<String> {
            self.launch.lock().unwrap().take()
        }
    }

    #[test]
    fn an_address_is_read_and_written_through_the_bridge_and_a_path_as_a_file() {
        let bridge = Arc::new(Fake::default());
        let store = SafStore::new(bridge.clone());
        let uri = DocId::Uri("content://docs/42".into());
        assert!(store.read(&uri).is_err(), "no such file yet");
        store.write_atomic(&uri, b"%PDF one").expect("write");
        assert_eq!(store.read(&uri).expect("read"), b"%PDF one");
        store.write_atomic(&uri, b"%PDF two").expect("replace");
        assert_eq!(store.read(&uri).expect("read"), b"%PDF two");
        assert_eq!(
            store.write_atomic(&DocId::Uri("content://readonly/1".into()), b"x"),
            Err("read-only".to_string()),
            "the system's refusal is the person's to read"
        );
        // A path never goes near the bridge.
        let dir = std::env::temp_dir().join(format!("inkpdf-saf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = DocId::Path(dir.join("a.pdf"));
        store.write_atomic(&path, b"%PDF path").expect("write a path");
        assert_eq!(store.read(&path).expect("read a path"), b"%PDF path");
        assert_eq!(bridge.files.lock().unwrap().len(), 1, "one address (the refused one is not kept), and the path is not among them");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_clipboard_is_the_bridges() {
        let bridge = Arc::new(Fake::default());
        let mut clip = BridgeClipboard(bridge.clone());
        assert_eq!(clip.get(), None);
        clip.set("two\nlines é");
        assert_eq!(clip.get().as_deref(), Some("two\nlines é"));
        assert_eq!(bridge.paste().as_deref(), Some("two\nlines é"));
    }

    #[test]
    fn what_the_picker_answers_is_a_kind_and_an_address_or_nothing_if_the_person_gave_up() {
        assert_eq!(
            parse_picked("0\tcontent://com.android.externalstorage.documents/document/primary%3AMy.pdf"),
            Some(Picked {
                kind: Pick::Open,
                id: Some(DocId::Uri("content://com.android.externalstorage.documents/document/primary%3AMy.pdf".into()))
            })
        );
        assert_eq!(parse_picked("1\tcontent://x/1"), Some(Picked { kind: Pick::Save, id: Some(DocId::Uri("content://x/1".into())) }));
        assert_eq!(parse_picked("1\t"), Some(Picked { kind: Pick::Save, id: None }), "cancelled");
        for hostile in ["", "\t", "2\tcontent://x", "0", "open\tcontent://x", "\u{0}\t\u{0}"] {
            assert!(parse_picked(hostile).is_none_or(|p| p.id.as_ref().is_none_or(|i| matches!(i, DocId::Uri(_)))), "{hostile:?}");
        }
        assert_eq!(parse_picked("9\tcontent://x"), None);
    }

    #[test]
    fn a_pick_and_an_answer_go_through_the_bridge_once() {
        let bridge = Fake::default();
        bridge.pick(Pick::Save, "Untitled.pdf");
        assert_eq!(bridge.picks.lock().unwrap().as_slice(), [(Pick::Save, "Untitled.pdf".to_string())]);
        assert_eq!(bridge.take_picked(), None);
        *bridge.answer.lock().unwrap() = Some("1\tcontent://x/2".into());
        assert_eq!(bridge.take_picked().as_deref().and_then(parse_picked).and_then(|p| p.id), Some(DocId::Uri("content://x/2".into())));
        assert_eq!(bridge.take_picked(), None, "taken once");
        *bridge.launch.lock().unwrap() = Some("content://x/3".into());
        assert_eq!(bridge.take_launch().as_deref(), Some("content://x/3"));
        assert_eq!(bridge.take_launch(), None);
    }

    #[test]
    fn the_bars_are_hidden_and_shown_through_the_bridge() {
        let bridge = Fake::default();
        bridge.set_bars_hidden(true);
        assert!(*bridge.bars_hidden.lock().unwrap());
        bridge.set_bars_hidden(false);
        assert!(!*bridge.bars_hidden.lock().unwrap());
    }

    #[test]
    fn the_name_shown_is_the_providers_own_when_it_has_one_and_the_addresss_last_part_when_it_has_not() {
        let store = SafStore::new(Arc::new(Fake::default()));
        assert_eq!(store.display_name(&DocId::Uri("content://media/external/file/10-named".into())), "Meeting notes.pdf");
        assert_eq!(store.display_name(&DocId::Uri("content://media/external/file/10".into())), "10");
        assert_eq!(store.display_name(&DocId::Path("/a/b.pdf".into())), "b.pdf");
    }
}
