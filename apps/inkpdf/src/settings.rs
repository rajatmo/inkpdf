//! What the app remembers between sessions: the tool in use, the five colours, and each tool's own
//! settings (the pen's colour, thickness and smoothing; the shape tool's shape, colour and
//! thickness; the text tool's colour and size).
//!
//! A small text file (kept by `app::config` in the user's application-data folder), one setting per line:
//!
//! ```text
//! tool geometry
//! color 2 d72828
//! slot 2
//! width 2.50
//! smoothing 0.50
//! shape ellipse
//! shape-slot 1
//! shape-width 1.50
//! text-slot 0
//! text-size 18.00
//! ```
//!
//! Anything unreadable is ignored, so a damaged or hand-edited file can never stop the app
//! starting; it just falls back to the defaults for those lines. (Older files had three width
//! slots, `width 1 2.5`; those lines are ignored.)

use crate::geometry::Shape;
use crate::palette::Scheme;
use crate::smooth::DEFAULT_AMOUNT;
use crate::tool::{MAX_WIDTH, MIN_WIDTH, PALETTE, TEXT_SIZES, Tool, ToolStyle};

/// Which look the interface has: the system's (light or dark as the desktop says), or one of them always.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    /// The look it comes to, given what the system says.
    pub fn resolve(self, system: Scheme) -> Scheme {
        match self {
            Appearance::System => system,
            Appearance::Light => Scheme::Light,
            Appearance::Dark => Scheme::Dark,
        }
    }

    /// The next one, in the order the theme button goes through them.
    pub fn next(self) -> Appearance {
        match self {
            Appearance::System => Appearance::Light,
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::System,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn from_name(name: &str) -> Option<Appearance> {
        [Appearance::System, Appearance::Light, Appearance::Dark].into_iter().find(|a| a.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The look of the interface, and whether the pages are drawn dark too (only in effect when the look is dark).
    pub appearance: Appearance,
    pub dark_pages: bool,
    /// How the search card matches: with the case, and as whole words.
    pub search_case: bool,
    pub search_words: bool,
    /// The name written on the notes and replies the person adds (empty until one is chosen: the shell offers the system's user name).
    pub author: String,
    pub palette: [[u8; 4]; 5],
    pub tool: Tool,
    pub pen: ToolStyle,
    pub smoothing: f32,
    pub shape: Shape,
    pub shape_style: ToolStyle,
    pub text_style: ToolStyle,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            dark_pages: false,
            search_case: false,
            search_words: false,
            author: String::new(),
            palette: PALETTE,
            tool: Tool::Select,
            pen: ToolStyle::PEN,
            smoothing: DEFAULT_AMOUNT,
            shape: Shape::default(),
            shape_style: ToolStyle::SHAPES,
            text_style: ToolStyle::TEXT,
        }
    }
}

impl Settings {
    /// Read settings from text, keeping the default for anything missing or unreadable.
    pub fn parse(text: &str) -> Settings {
        let mut out = Settings::default();
        for line in text.lines() {
            // (A name has spaces in it: it is the rest of the line.)
            if let Some(name) = line.strip_prefix("author ") {
                out.author = clean_name(name);
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[..] {
                ["color", slot, value] => {
                    if let (Ok(slot), Some(rgb)) = (slot.parse::<usize>(), parse_hex(value))
                        && let Some(cell) = out.palette.get_mut(slot)
                    {
                        *cell = [rgb[0], rgb[1], rgb[2], 255];
                    }
                }
                ["appearance", name] => out.appearance = Appearance::from_name(name).unwrap_or(out.appearance),
                ["dark-pages", value] => out.dark_pages = value == "on",
                ["search-case", value] => out.search_case = value == "on",
                ["search-words", value] => out.search_words = value == "on",
                ["tool", name] => out.tool = Tool::from_name(name).unwrap_or(out.tool),
                ["shape", name] => out.shape = Shape::from_name(name).unwrap_or(out.shape),
                ["slot", slot] => set_slot(&mut out.pen.colour, slot),
                ["shape-slot", slot] => set_slot(&mut out.shape_style.colour, slot),
                ["text-slot", slot] => set_slot(&mut out.text_style.colour, slot),
                ["text-size", value] => set_clamped(&mut out.text_style.width, value, TEXT_SIZES.0, TEXT_SIZES.1),
                ["width", value] => set_clamped(&mut out.pen.width, value, MIN_WIDTH, MAX_WIDTH),
                ["shape-width", value] => set_clamped(&mut out.shape_style.width, value, MIN_WIDTH, MAX_WIDTH),
                ["smoothing", value] => set_clamped(&mut out.smoothing, value, 0.0, 1.0),
                _ => {}
            }
        }
        out
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for (i, c) in self.palette.iter().enumerate() {
            text.push_str(&format!("color {i} {:02x}{:02x}{:02x}\n", c[0], c[1], c[2]));
        }
        text.push_str(&format!(
            "tool {}\nslot {}\nwidth {:.2}\nsmoothing {:.2}\n",
            self.tool.name(),
            self.pen.colour,
            self.pen.width,
            self.smoothing
        ));
        text.push_str(&format!("shape {}\nshape-slot {}\nshape-width {:.2}\n", self.shape.name(), self.shape_style.colour, self.shape_style.width));
        text.push_str(&format!("text-slot {}\ntext-size {:.2}\n", self.text_style.colour, self.text_style.width));
        let on = |b: bool| if b { "on" } else { "off" };
        text.push_str(&format!("appearance {}\ndark-pages {}\n", self.appearance.name(), on(self.dark_pages)));
        text.push_str(&format!("search-case {}\nsearch-words {}\n", on(self.search_case), on(self.search_words)));
        let author = clean_name(&self.author);
        if !author.is_empty() {
            text.push_str(&format!("author {author}\n"));
        }
        text
    }
}

/// A name as the file keeps it: one line of at most 60 characters, no control characters, trimmed.
pub fn clean_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(60).collect::<String>().trim().to_string()
}

fn parse_finite(text: &str) -> Option<f32> {
    text.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// `into` is the colour slot `text` names, if it names one.
fn set_slot(into: &mut usize, text: &str) {
    if let Some(slot) = parse_slot(text) {
        *into = slot;
    }
}

/// `into` is the number `text` is, kept from `low` to `high`, if it is one.
fn set_clamped(into: &mut f32, text: &str, low: f32, high: f32) {
    if let Some(value) = parse_finite(text) {
        *into = value.clamp(low, high);
    }
}

/// A colour slot: one of the five.
fn parse_slot(text: &str) -> Option<usize> {
    text.parse::<usize>().ok().filter(|&slot| slot < PALETTE.len())
}

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::DEFAULT_WIDTH;

    fn everything_changed() -> Settings {
        let mut s = Settings {
            appearance: Appearance::Dark,
            dark_pages: true,
            search_case: true,
            search_words: true,
            author: "Ada Lovelace".into(),
            ..Settings::default()
        };
        s.palette[2] = [12, 34, 255, 255];
        s.palette[4] = [0, 0, 0, 255];
        s.tool = Tool::Geometry;
        s.pen = ToolStyle { colour: 3, width: 17.5 };
        s.smoothing = 0.8;
        s.shape = Shape::Axes3d;
        s.shape_style = ToolStyle { colour: 4, width: 0.5 };
        s.text_style = ToolStyle { colour: 2, width: 40.0 };
        s
    }

    #[test]
    fn settings_round_trip_through_text() {
        let s = everything_changed();
        assert_eq!(Settings::parse(&s.to_text()), s);
        for tool in Tool::all() {
            for shape in crate::geometry::SHAPES {
                let s = Settings { tool, shape, ..everything_changed() };
                assert_eq!(Settings::parse(&s.to_text()), s, "{tool:?} {shape:?}");
            }
        }
    }

    #[test]
    fn an_empty_or_missing_file_means_the_defaults() {
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("\n\n   \n"), Settings::default());
    }

    #[test]
    fn unreadable_lines_are_ignored_and_the_rest_still_apply() {
        let text = "color 1 not-a-colour\ncolor 9 ff0000\nwidth x\nwidth NaN\nwidth\nsmoothing 2 3 4\nbanana 1 2\ntool hammer\nshape hexagon\nslot 9\nslot -1\nshape-slot x\ncolor 0 #00ff00\nwidth 3.5\ntool laser\n";
        let s = Settings::parse(text);
        // Only the three good lines took effect.
        assert_eq!(s.palette[0], [0, 255, 0, 255]);
        assert_eq!(s.pen.width, 3.5);
        assert_eq!(s.tool, Tool::named("laser"));
        assert_eq!(s.palette[1], PALETTE[1]);
        assert_eq!(s.smoothing, DEFAULT_AMOUNT);
        assert_eq!((s.pen.colour, s.shape, s.shape_style), (Settings::default().pen.colour, Shape::default(), ToolStyle::SHAPES));
    }

    #[test]
    fn the_old_three_slot_widths_are_ignored() {
        assert_eq!(Settings::parse("width 0 7\nwidth 1 9\nwidth 2 11\n").pen.width, DEFAULT_WIDTH);
    }

    #[test]
    fn settings_from_before_the_shape_tool_still_read() {
        // What the previous version wrote.
        let old = "color 0 1c1e24\ncolor 1 2060e0\ncolor 2 e03434\ncolor 3 26a054\ncolor 4 f4941c\nwidth 4.00\nsmoothing 0.30\n";
        let s = Settings::parse(old);
        assert_eq!((s.pen.width, s.smoothing, s.tool), (4.0, 0.3, Tool::Select), "the tool is the default one, the selection of text");
        assert_eq!((s.shape, s.shape_style), (Shape::Line, ToolStyle::SHAPES), "the new settings are the defaults");
    }

    #[test]
    fn the_look_is_kept_and_a_file_from_before_it_still_reads_as_the_systems_with_light_pages() {
        let s = Settings::parse("appearance dark\ndark-pages on\n");
        assert_eq!((s.appearance, s.dark_pages), (Appearance::Dark, true));
        let old = Settings::parse("color 0 1c1e24\nwidth 4.00\nsmoothing 0.30\ntext-slot 1\ntext-size 20.00\n");
        assert_eq!((old.appearance, old.dark_pages), (Appearance::System, false), "what the previous version wrote");
        let bad = Settings::parse("appearance purple\ndark-pages maybe\n");
        assert_eq!((bad.appearance, bad.dark_pages), (Appearance::System, false), "what is not a look is not one");
        for a in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(Appearance::from_name(a.name()), Some(a));
            assert_eq!(Settings::parse(&Settings { appearance: a, ..Settings::default() }.to_text()).appearance, a);
        }
    }

    #[test]
    fn an_appearance_and_what_the_system_says_make_a_look_in_every_case() {
        use Appearance as A;
        use Scheme as S;
        let table = [
            (A::System, S::Light, S::Light),
            (A::System, S::Dark, S::Dark),
            (A::Light, S::Light, S::Light),
            (A::Light, S::Dark, S::Light),
            (A::Dark, S::Light, S::Dark),
            (A::Dark, S::Dark, S::Dark),
        ];
        for (appearance, system, want) in table {
            assert_eq!(appearance.resolve(system), want, "{appearance:?} on a {system:?} system");
        }
        assert_eq!(A::System.next().next().next(), A::System, "the button goes round: system, light, dark");
        assert_eq!((A::System.next(), A::Light.next(), A::Dark.next()), (A::Light, A::Dark, A::System));
    }

    #[test]
    fn widths_and_smoothing_are_kept_in_range() {
        let s = Settings::parse("width -4\nsmoothing 9\nshape-width 1000\n");
        assert_eq!((s.pen.width, s.smoothing, s.shape_style.width), (MIN_WIDTH, 1.0, MAX_WIDTH));
        let s = Settings::parse("width 9999\nsmoothing -1\nshape-width 0\n");
        assert_eq!((s.pen.width, s.smoothing, s.shape_style.width), (MAX_WIDTH, 0.0, MIN_WIDTH));
        let s = Settings::parse("width inf\nsmoothing NaN\nshape-width NaN\n");
        assert_eq!(
            (s.pen.width, s.smoothing, s.shape_style.width),
            (DEFAULT_WIDTH, DEFAULT_AMOUNT, ToolStyle::SHAPES.width),
            "infinity and NaN are not settings"
        );
    }

    #[test]
    fn text_size_is_kept_in_the_text_range() {
        assert_eq!(Settings::parse("text-size 1\n").text_style.width, TEXT_SIZES.0);
        assert_eq!(Settings::parse("text-size 9999\n").text_style.width, TEXT_SIZES.1);
        assert_eq!(Settings::parse("text-size NaN\ntext-slot 9\n").text_style, ToolStyle::TEXT);
    }

    #[test]
    fn colour_codes_are_strict() {
        assert_eq!(parse_hex("ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("#FF8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("ff80"), None);
        assert_eq!(parse_hex("gg0000"), None);
        // Multi-byte text must not be sliced mid-character.
        assert_eq!(parse_hex("éééé"), None);
        assert_eq!(parse_hex("ff80000"), None);
    }

    #[test]
    fn saved_text_is_read_back_by_the_next_session_and_garbage_cannot_stop_it() {
        let s = everything_changed();
        assert_eq!(Settings::parse(&s.to_text()), s);
        // A file cut off in the middle of a line, or full of binary rubbish, still gives settings.
        let text = s.to_text();
        for cut in [0, 5, text.len() / 2, text.len() - 3] {
            let _ = Settings::parse(&text[..cut]);
        }
        let junk: String = (0..2000u32).map(|i| char::from_u32(32 + (i * 7919) % 200).unwrap_or(' ')).collect();
        let _ = Settings::parse(&junk);
    }
}
