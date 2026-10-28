//! What the editor asks of the shell: a queue of requests the shell drains after each input event.
//! It replaces a set of public flags the shell used to poll and reset, so that the editor (and the tools
//! under it) can ask for things without knowing who answers: a window, a test, or nobody.

use std::collections::VecDeque;

/// What the person can answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Write the changes to the file, then carry on.
    Save,
    /// Carry on without them.
    Discard,
    /// Stay where we were.
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Write the document into its file (asking where first, if it has none).
    Save,
    /// Switch between windowed and full screen.
    Fullscreen,
    /// Close this document and start a blank one.
    Close,
    /// Add a blank page at the end.
    AddPage,
    /// Add a blank page after this page.
    InsertPage(usize),
    /// Delete this page; nobody is asked.
    DeletePage(usize),
    /// Open a web address (a link was tapped).
    OpenUrl(String),
    /// The tool, colours or sizes changed: write them to the settings file.
    SettingsChanged,
    /// The "save your changes?" prompt was answered.
    Answer(Choice),
}

#[derive(Clone, Debug, Default)]
pub struct Requests {
    queue: VecDeque<Request>,
}

impl Requests {
    /// Ask for something. Asking again for what is already waiting (a save, a settings write) changes nothing.
    pub fn push(&mut self, request: Request) {
        if !self.queue.contains(&request) {
            self.queue.push_back(request);
        }
    }

    /// The oldest request, for the shell to carry out.
    pub fn pop(&mut self) -> Option<Request> {
        self.queue.pop_front()
    }

    /// Whether `request` is waiting.
    #[cfg(test)]
    pub fn has(&self, request: &Request) -> bool {
        self.queue.contains(request)
    }

    /// Remove `request` if it is waiting: whether it was.
    #[cfg(test)]
    pub fn take(&mut self, request: &Request) -> bool {
        match self.queue.iter().position(|r| r == request) {
            Some(i) => self.queue.remove(i).is_some(),
            None => false,
        }
    }

    /// The page after which a blank page was asked for.
    #[cfg(test)]
    pub fn insert_page(&self) -> Option<usize> {
        self.queue.iter().find_map(|r| if let Request::InsertPage(n) = r { Some(*n) } else { None })
    }

    /// The page a delete was asked for.
    #[cfg(test)]
    pub fn delete_page(&self) -> Option<usize> {
        self.queue.iter().find_map(|r| if let Request::DeletePage(n) = r { Some(*n) } else { None })
    }

    #[cfg(test)]
    pub fn take_delete_page(&mut self) -> Option<usize> {
        let n = self.delete_page()?;
        self.take(&Request::DeletePage(n));
        Some(n)
    }

    #[cfg(test)]
    pub fn take_insert_page(&mut self) -> Option<usize> {
        let n = self.insert_page()?;
        self.take(&Request::InsertPage(n));
        Some(n)
    }

    /// The web address waiting to be opened.
    #[cfg(test)]
    pub fn url(&self) -> Option<&str> {
        self.queue.iter().find_map(|r| if let Request::OpenUrl(u) = r { Some(u.as_str()) } else { None })
    }

    #[cfg(test)]
    pub fn clear_urls(&mut self) {
        self.queue.retain(|r| !matches!(r, Request::OpenUrl(_)));
    }

    /// The prompt's answer, if it has been given and not yet taken.
    #[cfg(test)]
    pub fn answer(&self) -> Option<Choice> {
        self.queue.iter().find_map(|r| if let Request::Answer(c) = r { Some(*c) } else { None })
    }

    #[cfg(test)]
    pub fn take_answer(&mut self) -> Option<Choice> {
        let c = self.answer()?;
        self.take(&Request::Answer(c));
        Some(c)
    }

    pub fn clear_answer(&mut self) {
        self.queue.retain(|r| !matches!(r, Request::Answer(_)));
    }
}
