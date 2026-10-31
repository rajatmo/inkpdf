//! `--record FILE` and `--replay FILE`: the input given to the editor, saved as it happened and played back (see
//! `input::record` for the format). Recording is a copy of each event on its way to the editor; playing hands the events back
//! at their old pace.

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::input::InputEvent;
use crate::input::record::{self, Recorded};

/// What was given to the editor so far, written out when the program ends.
pub(super) struct Recorder {
    path: PathBuf,
    events: Vec<Recorded>,
}

impl Recorder {
    pub fn new(path: PathBuf) -> Self {
        Self { path, events: Vec::new() }
    }

    pub fn push(&mut self, event: &InputEvent) {
        self.events.push(Recorded { at_ms: crate::clock::now_ms(), event: event.clone() });
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        match std::fs::write(&self.path, record::encode(&self.events)) {
            Ok(()) => eprintln!("recorded {} input events to {}", self.events.len(), self.path.display()),
            Err(e) => eprintln!("could not write {}: {e}", self.path.display()),
        }
    }
}

/// A recording being played: its events come due at the pace they were given, from the moment the first frame asks.
pub(super) struct Player {
    events: VecDeque<Recorded>,
    /// What to add to a recorded time to get this run's clock; known at the first frame.
    delta_ms: Option<f64>,
}

impl Player {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let events = record::decode(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { events: events.into(), delta_ms: None })
    }

    /// The events due by `now_ms` (on the clock), oldest first, with their times made this run's.
    pub fn due(&mut self, now_ms: f64) -> Vec<Recorded> {
        let delta = *self.delta_ms.get_or_insert_with(|| now_ms - self.events.front().map_or(now_ms, |e| e.at_ms));
        let mut out = Vec::new();
        while self.events.front().is_some_and(|e| e.at_ms + delta <= now_ms) {
            out.extend(self.events.pop_front().map(|e| e.shifted(delta)));
        }
        out
    }

    pub fn finished(&self) -> bool {
        self.events.is_empty()
    }
}

/// `--inject FILE`: input handed in while the program runs. Lines in the recording's format (the header is optional) are appended to the file, and each
/// is given to the editor as it is found, as of that moment: whatever times it carries are dropped (a pointer's samples are stamped with the moment too).
pub(super) struct Injector {
    path: PathBuf,
    /// How much of the file has been read.
    read: usize,
    /// The end of the file after its last whole line.
    partial: String,
}

impl Injector {
    pub fn new(path: PathBuf) -> Self {
        Self { path, read: 0, partial: String::new() }
    }

    /// The events appended since the last ask, made to happen at `now_ms`.
    pub fn poll(&mut self, now_ms: f64) -> Vec<InputEvent> {
        let Ok(bytes) = std::fs::read(&self.path) else { return Vec::new() };
        if bytes.len() <= self.read {
            return Vec::new();
        }
        self.partial.push_str(&String::from_utf8_lossy(&bytes[self.read..]));
        self.read = bytes.len();
        let Some(end) = self.partial.rfind('\n') else { return Vec::new() };
        let whole: String = self.partial.drain(..=end).collect();
        let body: String = whole.lines().filter(|l| l.trim() != record::HEADER).map(|l| format!("{l}\n")).collect();
        match record::decode(&format!("{}\n{body}", record::HEADER)) {
            Ok(events) => events.into_iter().map(|r| r.shifted(now_ms - r.at_ms).event).collect(),
            Err(e) => {
                eprintln!("{}: {e}", self.path.display());
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, Wheel};

    #[test]
    fn injected_lines_are_given_once_each_when_whole_and_stamped_with_the_moment() {
        let path = std::env::temp_dir().join(format!("inkpdf-inject-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut inject = Injector::new(path.clone());
        assert!(inject.poll(10.0).is_empty(), "no file yet");
        std::fs::write(&path, "0 ptr pen 1 down 3 0 1 5 6 0.5 0\n0 key escape").unwrap();
        let first = inject.poll(100.0);
        assert_eq!(first.len(), 1, "the half-written last line waits");
        match &first[0] {
            InputEvent::Pointer(p) => assert_eq!(p.samples[0].t_ms, 100.0),
            other => panic!("{other:?}"),
        }
        std::fs::write(&path, "0 ptr pen 1 down 3 0 1 5 6 0.5 0\n0 key escape\n").unwrap();
        assert_eq!(inject.poll(110.0), vec![InputEvent::Key(crate::input::KeyInput::Escape)]);
        assert!(inject.poll(120.0).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    fn at(ms: f64) -> Recorded {
        Recorded { at_ms: ms, event: InputEvent::Wheel(Wheel::Lines(0.0, ms as f32)) }
    }

    #[test]
    fn events_come_due_at_the_pace_they_were_given_from_the_first_ask() {
        let mut p = Player { events: VecDeque::from([at(1000.0), at(1010.0), at(1100.0)]), delta_ms: None };
        // The first ask is at 5000: the first event is due at once, the others 10 and 100 ms later.
        assert_eq!(p.due(5000.0).iter().map(|r| r.at_ms).collect::<Vec<_>>(), [5000.0]);
        assert!(p.due(5005.0).is_empty());
        assert_eq!(p.due(5010.0).len(), 1);
        assert!(!p.finished());
        assert_eq!(p.due(9000.0).len(), 1, "a late frame gets what it missed");
        assert!(p.finished());
    }
}
