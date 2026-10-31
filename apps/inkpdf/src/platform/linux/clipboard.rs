//! The system clipboard on Linux, for the text of text boxes, the search field and the card of a note: copy in the program, paste in another, and
//! the other way. It is arboard's, which speaks Wayland's data-control protocol where the compositor has it and X11's selections everywhere else
//! (a clipboard on X11 is owned by the program that put the text there, so this object lives as long as the window). Where neither can be reached
//! (a compositor with no data-control, as GNOME's, in a session with no X server to fall back on) there is no system clipboard and the program keeps
//! what was copied in its own, as it does in tests.

use winit::raw_window_handle::HasWindowHandle;

use crate::clipboard::TextClipboard;

pub struct SystemClipboard(arboard::Clipboard);

impl SystemClipboard {
    /// The clipboard of the session this program is in, if there is one it can reach.
    pub fn new(_window: &dyn HasWindowHandle) -> Option<Self> {
        match arboard::Clipboard::new() {
            Ok(clipboard) => Some(Self(clipboard)),
            Err(e) => {
                eprintln!("no system clipboard ({e}): what is copied stays in the program");
                None
            }
        }
    }
}

impl TextClipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.get_text().ok().filter(|text| !text.is_empty())
    }

    fn set(&mut self, text: &str) {
        if let Err(e) = self.0.set_text(text.to_string()) {
            eprintln!("could not copy to the system clipboard: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the session has, a clipboard that can be made takes text and gives it back (a session with none, such as a build machine's, is not a
    /// failure: there is nothing to try).
    #[test]
    fn text_put_on_the_clipboard_comes_back_as_it_was_where_there_is_one() {
        let Ok(inner) = arboard::Clipboard::new() else { return };
        let mut clipboard = SystemClipboard(inner);
        for text in ["hello", "two\nlines and ünïcode ✓", "x"] {
            clipboard.set(text);
            assert_eq!(clipboard.get().as_deref(), Some(text));
        }
    }
}
