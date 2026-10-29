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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, Wheel};

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
