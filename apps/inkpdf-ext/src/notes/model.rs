//! A note's thread, as the text an item holds, and what a person does to it. Nothing here knows the window or the document: it is plain text in,
//! plain text out, so that every way of changing a thread (a reply, marking it done) is one value turned into another.

use inkpdf_api::{Message, Note};

/// Between the status of a thread and its messages, between one message and the next, and between the parts of a message. (Control characters, so
/// that nothing a person can type is one: see [`clean`].)
const GROUP: char = '\u{1d}';
const RECORD: char = '\u{1e}';
const UNIT: char = '\u{1f}';

/// The most messages a thread keeps and the most characters of one: a file made by someone else cannot make a note cost much.
pub const MAX_MESSAGES: usize = 500;
pub const MAX_CHARS: usize = 4000;

/// A text without the characters that are this format's own (and the other control characters but the new line).
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(MAX_CHARS).collect()
}

/// A note and the replies to it: who said what and when, oldest first, and whether the person is done with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thread {
    pub closed: bool,
    pub messages: Vec<Message>,
}

impl Thread {
    /// The text an item holds for this thread.
    pub fn encode(&self) -> String {
        let status = if self.closed { "closed" } else { "open" };
        let messages: Vec<String> = self.messages.iter().map(|m| format!("{}{UNIT}{}{UNIT}{}", clean(&m.author), clean(&m.date), clean(&m.text))).collect();
        format!("{status}{GROUP}{}", messages.join(&RECORD.to_string()))
    }

    /// The thread a text says it is. Whatever the text is, there is one: a text that is not one of ours is a thread with nothing in it.
    pub fn decode(data: &str) -> Thread {
        let Some((status, rest)) = data.split_once(GROUP) else { return Thread::default() };
        let messages = rest
            .split(RECORD)
            .filter(|m| !m.is_empty())
            .take(MAX_MESSAGES)
            .map(|m| {
                let mut parts = m.splitn(3, UNIT);
                let (author, date, text) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                Message { author: clean(author), date: clean(date), text: clean(text) }
            })
            .collect();
        Thread { closed: status == "closed", messages }
    }

    /// Add what someone said (a reply to a thread that was done opens it again). `false` if there was nothing to say or no room.
    pub fn say(&mut self, author: &str, date: &str, text: &str) -> bool {
        let text = clean(text);
        if text.trim().is_empty() || self.messages.len() >= MAX_MESSAGES {
            return false;
        }
        self.messages.push(Message { author: clean(author).trim().to_string(), date: clean(date), text: text.trim().to_string() });
        self.closed = false;
        true
    }

    /// The thread a PDF's note is: its text is the first message, the replies the rest.
    pub fn from_note(note: &Note) -> Thread {
        let first = Message { author: note.author.clone(), date: note.date.clone(), text: note.text.clone() };
        let first = (!first.text.trim().is_empty()).then_some(first);
        let messages = first.into_iter().chain(note.replies.iter().cloned()).take(MAX_MESSAGES).collect();
        Thread { closed: note.closed, messages }
    }

    /// The note a PDF holds for this thread.
    pub fn to_note(&self) -> Note {
        let mut messages = self.messages.iter().cloned();
        let first = messages.next().unwrap_or_default();
        Note { author: first.author, date: first.date, text: first.text, replies: messages.collect(), closed: self.closed }
    }
}

/// `D:20261031120500Z` as "31 Oct 2026, 12:05" (what a person reads); a date that is not in that form is nothing at all.
pub fn short_date(date: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let digits: Vec<u32> = date.strip_prefix("D:").unwrap_or(date).chars().take_while(char::is_ascii_digit).filter_map(|c| c.to_digit(10)).collect();
    let number = |from: usize, len: usize| digits.get(from..from + len).map(|d| d.iter().fold(0, |n, d| n * 10 + d));
    let (Some(year), Some(month), Some(day)) = (number(0, 4), number(4, 2), number(6, 2)) else { return String::new() };
    let Some(month) = MONTHS.get((month as usize).wrapping_sub(1)) else { return String::new() };
    match (number(8, 2), number(10, 2)) {
        (Some(h), Some(m)) if h < 24 && m < 60 => format!("{day} {month} {year}, {h:02}:{m:02}"),
        _ => format!("{day} {month} {year}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(author: &str, date: &str, text: &str) -> Message {
        Message { author: author.into(), date: date.into(), text: text.into() }
    }

    #[test]
    fn a_thread_comes_back_from_its_text_as_it_was_whatever_was_said_in_it() {
        let thread = Thread {
            closed: true,
            messages: vec![say("Ada", "D:20261031120500Z", "Is this right?\nI doubt it."), say("", "", "second"), say("Bob", "D:20261101000000Z", "")],
        };
        assert_eq!(Thread::decode(&thread.encode()), thread);
        assert_eq!(Thread::decode(&Thread::default().encode()), Thread::default());
        assert!(Thread::decode("closed\u{1d}").closed && !Thread::decode("open\u{1d}").closed);
    }

    #[test]
    fn a_text_that_is_not_a_thread_is_an_empty_one_and_what_could_break_the_format_is_taken_out() {
        for odd in ["", "no separator at all", "\u{1d}", "x\u{1d}\u{1e}\u{1e}", "open\u{1d}a\u{1f}b", "\u{1f}\u{1f}\u{1f}\u{1e}"] {
            let thread = Thread::decode(odd);
            assert_eq!(Thread::decode(&thread.encode()), thread, "{odd:?}");
        }
        let mut thread = Thread::default();
        assert!(thread.say("A\u{1e}da", "", "hi\u{1f}there\u{1d}\r\nyou"));
        assert_eq!(thread.messages[0].text, "hithere\nyou", "no separator survives, a new line does");
        let back = Thread::decode(&thread.encode());
        assert_eq!(back.messages.len(), 1);
        assert_eq!(back, thread);
    }

    #[test]
    fn saying_adds_one_message_reopens_a_thread_that_was_done_and_refuses_nothing_and_too_much() {
        let mut thread = Thread { closed: true, messages: vec![] };
        assert!(!thread.say("Ada", "", "   \n "), "spaces say nothing");
        assert!(thread.closed && thread.messages.is_empty());
        assert!(thread.say(" Ada ", "d", "  hello  "));
        assert_eq!((thread.closed, thread.messages[0].author.as_str(), thread.messages[0].text.as_str()), (false, "Ada", "hello"));
        let mut long = Thread::default();
        assert!(long.say("a", "", &"x".repeat(MAX_CHARS + 500)));
        assert_eq!(long.messages[0].text.chars().count(), MAX_CHARS);
        let mut full = Thread { closed: false, messages: vec![Message::default(); MAX_MESSAGES] };
        assert!(!full.say("a", "", "one more"));
        assert_eq!(Thread::decode(&full.encode()).messages.len(), MAX_MESSAGES, "a thread that is full is read back full, not fuller");
    }

    #[test]
    fn a_note_of_a_pdf_is_a_thread_whose_first_message_is_the_note_and_comes_back_as_the_same_note() {
        let note = Note {
            author: "Ann".into(),
            date: "D:20261031120500Z".into(),
            text: "Check this".into(),
            replies: vec![say("Bob", "D:20261101000000Z", "Agreed")],
            closed: true,
        };
        let thread = Thread::from_note(&note);
        assert_eq!((thread.closed, thread.messages.len(), thread.messages[0].text.as_str()), (true, 2, "Check this"));
        assert_eq!(thread.to_note(), note);
        // A note with no words is a thread with nobody having spoken: its replies are still there.
        let empty = Note { replies: vec![say("Bob", "", "first!")], ..Note::default() };
        assert_eq!(Thread::from_note(&empty).messages.len(), 1);
        assert_eq!(Thread::default().to_note(), Note::default());
    }

    #[test]
    fn a_date_is_read_the_way_a_person_reads_it_and_a_date_that_is_not_one_is_nothing() {
        assert_eq!(short_date("D:20261031120500Z"), "31 Oct 2026, 12:05");
        assert_eq!(short_date("D:20260102"), "2 Jan 2026");
        assert_eq!(short_date("20261231235959"), "31 Dec 2026, 23:59");
        for bad in ["", "D:", "D:2026", "yesterday", "D:20261331000000Z", "D:20260001"] {
            assert_eq!(short_date(bad), "", "{bad:?}");
        }
        assert_eq!(short_date("D:20261031991500Z"), "31 Oct 2026", "an hour that is not one is left out");
    }
}
