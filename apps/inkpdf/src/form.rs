//! Forms: the fields a PDF has to fill in, and what the person has put in them. A field is placed on the pages by its boxes, one link each
//! (`Target::Field`, which follows the pages through edits as links do); this holds the fields themselves. Reading them out of a file is
//! `pdf_read`; writing the values in is `ink_io`.
//!
//! Filling is a set of small acts, each done with a click or tap and, for text, the keyboard: a text field takes typed text (at its end,
//! as a text box does), a check box turns on and off, a radio button is chosen, and a choice goes to the next of its options.

/// Longest text kept in a field, in characters (a field's own limit may be less).
const MAX_CHARS: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Check,
    Radio,
    /// A combo box or a list box.
    Choice,
}

/// What a field holds, as it is written into a file.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Check(bool),
    /// The "on" look of the button chosen, if one is.
    Radio(Option<String>),
    /// The values of the options chosen.
    Choice(Vec<String>),
}

/// What a file says about a field, for [`Field::new`].
#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub name: String,
    pub multiline: bool,
    pub max_len: Option<usize>,
    /// For a check box or a radio group, the name of the "on" look of each of its boxes, in the order of their widgets.
    pub on_states: Vec<Option<String>>,
    /// A choice's options: (value, shown text).
    pub options: Vec<(String, String)>,
    /// What it holds now: the text; the look it is on with (nothing when it is off); the values chosen.
    pub value: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub multiline: bool,
    max_len: Option<usize>,
    on_states: Vec<Option<String>>,
    options: Vec<(String, String)>,
    /// A text: the text.
    text: String,
    /// A check box: the "on" look it has when checked. A radio group: the look of the button chosen. A choice: the value chosen.
    chosen: Option<String>,
    /// The person has put something in it (it is written when the file is saved, and drawn by the program until the file is read again).
    changed: bool,
}

impl Field {
    pub fn new(kind: Kind, spec: Spec) -> Self {
        let first = spec.value.first().cloned().filter(|v| !v.is_empty() && v != "Off");
        Self {
            name: spec.name,
            kind,
            multiline: spec.multiline,
            max_len: spec.max_len.filter(|&n| n > 0),
            on_states: spec.on_states,
            options: spec.options,
            text: if kind == Kind::Text { crate::text::clean(&spec.value.concat()) } else { String::new() },
            chosen: if kind == Kind::Text { None } else { first },
            changed: false,
        }
    }

    /// What a text or a choice shows: the text typed, or the shown text of the option chosen.
    pub fn shown(&self) -> String {
        match self.kind {
            Kind::Text => self.text.clone(),
            Kind::Choice => {
                self.chosen.as_ref().map_or_else(String::new, |v| self.options.iter().find(|(o, _)| o == v).map_or(v.clone(), |(_, t)| t.clone()))
            }
            Kind::Check | Kind::Radio => String::new(),
        }
    }

    /// Whether box `widget` of a check box or radio group is on.
    pub fn is_on(&self, widget: usize) -> bool {
        match self.kind {
            Kind::Check => self.chosen.is_some(),
            Kind::Radio => self.chosen.is_some() && self.chosen.as_deref() == self.on_states.get(widget).and_then(|s| s.as_deref()),
            _ => false,
        }
    }

    /// Whether the person has changed it.
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// A press on box `widget` of the field: a check box turns on or off, a radio button is chosen, a choice goes to its next option (and after the
    /// last, to none). `false` for a text, which is typed into instead, and for a press that changes nothing.
    pub fn press(&mut self, widget: usize) -> bool {
        match self.kind {
            Kind::Text => return false,
            Kind::Check => {
                self.chosen = match self.chosen {
                    Some(_) => None,
                    None => self.on_states.iter().flatten().next().cloned(),
                };
            }
            Kind::Radio => {
                let Some(state) = self.on_states.get(widget).cloned().flatten() else { return false };
                // (A press on the button that is on leaves it on: a radio group is turned off by none of its buttons.)
                if self.chosen.as_ref() == Some(&state) {
                    return false;
                }
                self.chosen = Some(state);
            }
            Kind::Choice => {
                let next = match self.chosen.as_ref().and_then(|c| self.options.iter().position(|(o, _)| o == c)) {
                    Some(at) => self.options.get(at + 1),
                    None => self.options.first(),
                };
                self.chosen = next.map(|(o, _)| o.clone());
            }
        }
        self.changed = true;
        true
    }

    /// What a text field takes, for the keyboard that types into it (see [`crate::field`]): no more characters than the field's limit, and a new
    /// line only if it has several lines.
    pub fn rules(&self) -> (usize, bool) {
        (self.max_len.unwrap_or(MAX_CHARS).min(MAX_CHARS), self.multiline)
    }

    /// A text field now holds `text` (what was typed into it): `false` if it did already, or is not a text.
    pub fn set_text(&mut self, text: String) -> bool {
        if self.kind != Kind::Text || self.text == text {
            return false;
        }
        self.text = text;
        self.changed = true;
        true
    }
    /// What to write into the file for the field.
    fn value(&self) -> Value {
        match self.kind {
            Kind::Text => Value::Text(self.text.clone()),
            Kind::Check => Value::Check(self.chosen.is_some()),
            Kind::Radio => Value::Radio(self.chosen.clone()),
            Kind::Choice => Value::Choice(self.chosen.iter().cloned().collect()),
        }
    }
}

/// The fields of a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Form {
    pub fields: Vec<Field>,
}

impl Form {
    /// The values to write into the file: those of the fields the person changed.
    pub fn changes(&self) -> Vec<(String, Value)> {
        self.fields.iter().filter(|f| f.changed).map(|f| (f.name.clone(), f.value())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, value: &[&str], states: &[Option<&str>], options: &[&str]) -> Spec {
        Spec {
            name: name.into(),
            value: value.iter().map(ToString::to_string).collect(),
            on_states: states.iter().map(|s| s.map(str::to_string)).collect(),
            options: options.iter().map(|o| (o.to_string(), o.to_uppercase())).collect(),
            ..Spec::default()
        }
    }

    #[test]
    fn a_text_holds_what_was_typed_into_it_within_a_limit_that_the_keyboard_is_given() {
        let mut one = Field::new(Kind::Text, Spec { max_len: Some(5), ..spec("a", &["ab"], &[None], &[]) });
        assert_eq!((one.shown().as_str(), one.rules()), ("ab", (5, false)));
        assert!(one.set_text("abcde".into()) && one.changed());
        assert!(!one.set_text("abcde".into()), "the same text is no change");
        let many = Field::new(Kind::Text, Spec { multiline: true, ..spec("b", &[""], &[None], &[]) });
        assert_eq!(many.rules(), (2_000, true));
        let mut check = Field::new(Kind::Check, spec("c", &[], &[Some("Yes")], &[]));
        assert!(!check.set_text("x".into()), "only a text is typed into");
    }
    #[test]
    fn a_check_box_turns_on_and_off_and_a_radio_group_has_one_button_on_that_stays_on() {
        let mut check = Field::new(Kind::Check, spec("c", &[], &[Some("Yes")], &[]));
        assert!(!check.is_on(0) && !check.changed());
        assert!(check.press(0) && check.is_on(0));
        assert!(check.press(0) && !check.is_on(0));
        assert_eq!(check.value(), Value::Check(false));
        let mut radio = Field::new(Kind::Radio, spec("r", &["Small"], &[Some("Small"), Some("Large")], &[]));
        assert!(radio.is_on(0) && !radio.is_on(1), "what the file had");
        assert!(!radio.press(0), "a press on the one that is on changes nothing");
        assert!(radio.press(1) && radio.is_on(1) && !radio.is_on(0));
        assert_eq!(radio.value(), Value::Radio(Some("Large".into())));
        assert!(!Field::new(Kind::Text, spec("t", &[""], &[None], &[])).press(0), "a text is typed into, not pressed");
    }

    #[test]
    fn a_choice_goes_through_its_options_and_back_to_none_and_shows_the_text_of_the_one_chosen() {
        let mut choice = Field::new(Kind::Choice, spec("o", &[], &[None], &["red", "green"]));
        assert_eq!(choice.shown(), "");
        assert!(choice.press(0));
        assert_eq!((choice.shown().as_str(), choice.value()), ("RED", Value::Choice(vec!["red".into()])));
        choice.press(0);
        assert_eq!(choice.shown(), "GREEN");
        choice.press(0);
        assert_eq!((choice.shown().as_str(), choice.value()), ("", Value::Choice(Vec::new())));
    }

    #[test]
    fn only_the_fields_the_person_changed_are_written() {
        let mut form = Form {
            fields: vec![Field::new(Kind::Text, spec("a", &["kept"], &[None], &[])), Field::new(Kind::Check, spec("b", &[], &[Some("Yes")], &[]))],
        };
        assert!(form.changes().is_empty());
        form.fields[1].press(0);
        assert_eq!(form.changes(), [("b".to_string(), Value::Check(true))]);
    }
}
