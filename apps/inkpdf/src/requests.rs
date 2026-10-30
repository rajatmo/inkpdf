//! What the editor asks of the shell: a queue of requests the shell drains after each input event.
//! It replaces a set of public flags the shell used to poll and reset, so that the editor (and the tools
//! under it) can ask for things without knowing who answers: a window, a test, or nobody.

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::pages::PageEdit;
use crate::store::DocId;

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

#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Write the document into its file (asking where first, if it has none).
    Save,
    /// Write the document into a file of the person's choosing, whether or not it has one (Save As).
    SaveAs,
    /// The Save As dialog wants the folder's PDFs and subfolders: answered with `Editor::folder_listed`.
    ListFolder(PathBuf),
    /// The Save As dialog was confirmed with this file: written, and answered with `Editor::save_as_done`.
    SaveTo(DocId),
    /// The Save As dialog was put away without saving.
    SaveAsCancelled,
    /// Ask the system's picker for a PDF to open.
    Open,
    /// Switch between windowed and full screen.
    Fullscreen,
    /// Close this document and start a blank one.
    Close,
    /// The pages were edited (added, deleted, rearranged): the file's page table, and what is drawn for the pages, follow.
    PagesEdited(PageEdit),
    /// These deletions can no longer be undone: the pages kept for them can go.
    GravesForgotten(Vec<u64>),
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
    /// Ask for something. Asking again for what is already waiting (a save, a settings write) changes nothing; a
    /// page edit is not asking but telling, and each one is told: two pages added in a row are two.
    pub fn push(&mut self, request: Request) {
        if matches!(request, Request::PagesEdited(_)) || !self.queue.contains(&request) {
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

    /// The edit of the pages the shell has not yet been given effect, if there is one.
    #[cfg(test)]
    pub fn page_edit(&self) -> Option<&PageEdit> {
        self.queue.iter().find_map(|r| if let Request::PagesEdited(e) = r { Some(e) } else { None })
    }

    /// Every edit of the pages waiting, taken: as the shell would take them.
    #[cfg(test)]
    pub fn take_page_edits(&mut self) -> Vec<PageEdit> {
        let edits = self.queue.iter().filter_map(|r| if let Request::PagesEdited(e) = r { Some(e.clone()) } else { None }).collect();
        self.queue.retain(|r| !matches!(r, Request::PagesEdited(_)));
        edits
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_twice_for_the_same_thing_is_asking_once_but_every_page_edit_is_told() {
        let mut requests = Requests::default();
        requests.push(Request::Save);
        requests.push(Request::Save);
        assert_eq!(requests.pop(), Some(Request::Save));
        assert_eq!(requests.pop(), None, "a save asked for twice is one");
        let add = PageEdit::Insert { at: 1, size: [600.0, 800.0] };
        requests.push(Request::PagesEdited(add.clone()));
        requests.push(Request::PagesEdited(add.clone()));
        assert_eq!(requests.take_page_edits(), vec![add.clone(), add], "two pages added in a row are two edits for the shell");
    }
}
