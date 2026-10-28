//! The system's text clipboard, as the editing code sees it: something that holds a string. The shell gives the
//! workspace the real one (see [`crate::native`]); until it does, and in tests, text is kept in memory.

pub trait TextClipboard {
    /// The text on the clipboard, if there is any.
    fn get(&mut self) -> Option<String>;

    fn set(&mut self, text: &str);
}

/// A clipboard that lives in the program: copying in one place and pasting in another works, nothing leaves it.
#[derive(Default)]
pub struct MemoryClipboard(Option<String>);

impl TextClipboard for MemoryClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.clone()
    }

    fn set(&mut self, text: &str) {
        self.0 = Some(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_set_is_what_comes_back() {
        let mut c = MemoryClipboard::default();
        assert_eq!(c.get(), None);
        c.set("one\ntwo");
        assert_eq!(c.get().as_deref(), Some("one\ntwo"));
        c.set("");
        assert_eq!(c.get().as_deref(), Some(""));
    }
}
