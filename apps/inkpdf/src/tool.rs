//! Which tool is in use and how each one writes: the four tools by name, the colour slots they share, and the thickness
//! (or text size) each may have. Plain data that the document state, the saved settings and the toolbar all use, so that
//! none of them has to reach into the toolbar's module for it.

use crate::ink::Stroke;

/// The colour slots a fresh install starts with; each can be replaced with any colour.
pub const PALETTE: [[u8; 4]; 5] = [[28, 30, 36, 255], [32, 96, 224, 255], [224, 52, 52, 255], [38, 160, 84, 255], [244, 148, 28, 255]];
/// The pen's thickness, in points, until it is changed.
pub const DEFAULT_WIDTH: f32 = 2.5;
/// The thinnest and thickest pen, in points.
pub const MIN_WIDTH: f32 = 0.25;
pub const MAX_WIDTH: f32 = 24.0;

/// What the pen does on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Draws ink (and its eraser end erases, its barrel button lassos).
    #[default]
    Pen,
    /// Draws shapes (see [crate::geometry]): a drag makes a line, a rectangle, an ellipse or a
    /// set of axes; a curve is dragged out and then shaped by its handles.
    Geometry,
    /// Types text: a tap of the stylus starts a text box (or edits the one tapped), and the keyboard
    /// types into it.
    Text,
    /// Selects the text a PDF has: to copy it, and to highlight, underline or strike it out.
    Select,
    /// Points: a dot that follows the pen, with a trail that fades. Draws nothing.
    Laser,
}

/// What an item of the drawing is, which is what decides which tool works on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Handwriting.
    Ink,
    /// A shape, with anchors to edit.
    Shape,
    /// A text box.
    Text,
}

impl Kind {
    pub fn of(stroke: &Stroke) -> Kind {
        if stroke.text.is_some() {
            Kind::Text
        } else if stroke.figure.is_some() {
            Kind::Shape
        } else {
            Kind::Ink
        }
    }
}

impl Tool {
    /// Every tool, in the order the toolbar shows them.
    pub const ALL: [Tool; 5] = [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Select, Tool::Laser];

    /// The kind of item the tool makes and works on; the pointer makes none.
    pub fn kind(self) -> Option<Kind> {
        match self {
            Tool::Pen => Some(Kind::Ink),
            Tool::Geometry => Some(Kind::Shape),
            Tool::Text => Some(Kind::Text),
            Tool::Select | Tool::Laser => None,
        }
    }

    /// Whether the bar under the tools is shown with the tool: the tools that make items have it (to select, copy and restyle them), and
    /// the text selection has it for its marks.
    pub fn has_bar(self) -> bool {
        self.makes_items() || self == Tool::Select
    }

    /// Whether the tool makes items, so that there is something to select, copy and restyle, and settings to choose.
    pub fn makes_items(self) -> bool {
        self.kind().is_some()
    }

    /// The name used in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Geometry => "geometry",
            Tool::Text => "text",
            Tool::Select => "select",
            Tool::Laser => "laser",
        }
    }

    pub fn from_name(name: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// How a tool that draws ink writes: which colour slot and how thick, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolStyle {
    pub colour: usize,
    pub width: f32,
}

impl ToolStyle {
    pub const PEN: ToolStyle = ToolStyle { colour: 0, width: DEFAULT_WIDTH };
    /// Shapes start thinner than handwriting, in blue.
    pub const SHAPES: ToolStyle = ToolStyle { colour: 1, width: 1.5 };
    /// Text starts at a comfortable reading size (the "width" is its size in points).
    pub const TEXT: ToolStyle = ToolStyle { colour: 0, width: 18.0 };
}

/// The smallest and largest `width` a tool's style may have: a pen's line width, or the size of
/// text in points.
pub fn width_range(tool: Tool) -> (f32, f32) {
    if tool == Tool::Text { (TEXT_SIZES.0, TEXT_SIZES.1) } else { (MIN_WIDTH, MAX_WIDTH) }
}

/// The smallest and largest text, in points.
pub const TEXT_SIZES: (f32, f32) = (6.0, 144.0);

/// Most actions of its own a tool may offer at once (they are buttons of the bar).
pub const MAX_ACTIONS: usize = 3;

/// An action a tool offers for what it is editing, as a button of the bar beyond the ones every tool has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAction {
    /// Something done to a point of the shape being edited.
    Node(crate::figure::NodeAction),
    /// A markup made of the text that is selected.
    Mark(crate::ink::MarkupKind),
}

/// The colour (and opacity) a markup of `kind` is made in.
pub fn markup_colour(kind: crate::ink::MarkupKind) -> [u8; 4] {
    use crate::ink::MarkupKind;
    match kind {
        MarkupKind::Highlight => [255, 222, 0, 120],
        MarkupKind::Underline => [32, 96, 224, 255],
        MarkupKind::StrikeOut => [224, 52, 52, 255],
        MarkupKind::Squiggly => [38, 160, 84, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_name_that_finds_it_again_and_nothing_else_does() {
        for tool in Tool::ALL {
            assert_eq!(Tool::from_name(tool.name()), Some(tool));
        }
        // Every tool but the pointer makes one kind of item, and a different one each.
        let kinds: Vec<Option<Kind>> = Tool::ALL.iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [Some(Kind::Ink), Some(Kind::Shape), Some(Kind::Text), None, None]);
        assert_eq!(Tool::ALL.map(Tool::makes_items), [true, true, true, false, false]);
        assert_eq!(Tool::ALL.map(Tool::has_bar), [true, true, true, true, false], "the text selection has its marks in the bar");
        assert_eq!(Tool::from_name("brush"), None);
        assert_eq!(Tool::default(), Tool::Pen);
    }

    #[test]
    fn text_has_its_own_range_and_the_defaults_are_inside_the_ranges() {
        assert_eq!(width_range(Tool::Text), TEXT_SIZES);
        for tool in [Tool::Pen, Tool::Geometry, Tool::Laser] {
            assert_eq!(width_range(tool), (MIN_WIDTH, MAX_WIDTH));
        }
        for (tool, style) in [(Tool::Pen, ToolStyle::PEN), (Tool::Geometry, ToolStyle::SHAPES), (Tool::Text, ToolStyle::TEXT)] {
            let (least, most) = width_range(tool);
            assert!((least..=most).contains(&style.width), "{tool:?}");
            assert!(style.colour < PALETTE.len(), "{tool:?}");
        }
    }
}
