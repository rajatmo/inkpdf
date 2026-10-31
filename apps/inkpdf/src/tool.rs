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

/// Which tool: a name, and nothing else (what a tool does is its object's, see [`crate::tools`]). The tools the program is made of are
/// the constants here; an extension's tool is any other name, and is known by the [`Registry`] that lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tool(&'static str);

#[allow(non_upper_case_globals, reason = "the tools read like the cases of an enum, which they were")]
impl Tool {
    /// Draws ink (and its eraser end erases, its barrel button lassos).
    pub const Pen: Tool = Tool("pen");
    /// Draws shapes (see [crate::geometry]): a drag makes a line, a rectangle, an ellipse or a
    /// set of axes; a curve is dragged out and then shaped by its handles.
    pub const Geometry: Tool = Tool("geometry");
    /// Types text: a tap of the stylus starts a text box (or edits the one tapped), and the keyboard
    /// types into it.
    pub const Text: Tool = Tool("text");
    /// Selects the text a PDF has: to copy it, and to highlight, underline or strike it out.
    pub const Select: Tool = Tool("select");
    /// Selects what has been drawn, of any kind, with a loop (or a tap): to move, copy and delete it.
    pub const Lasso: Tool = Tool("lasso");
    /// Erases whatever it passes over, of any kind.
    pub const Eraser: Tool = Tool("eraser");

    /// A tool known by `name` (an extension's: the name it registered under).
    pub const fn named(name: &'static str) -> Tool {
        Tool(name)
    }
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Select
    }
}

/// The tools there are, in the order the toolbar shows them: the program's own, then the extensions', each with what the toolbar needs
/// to draw its button. A value, so that a test (or a fork) can have a set of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Registry {
    tools: Vec<(Tool, Option<&'static str>)>,
    /// Whether the toolbar has a button to open a PDF (where the system has a picker of its own: see `Platform::has_picker`).
    open_button: bool,
}

/// The most tools a toolbar can hold.
pub const MAX_TOOLS: usize = 10;

impl Registry {
    /// The program's own tools.
    pub fn builtin() -> Registry {
        Registry {
            tools: [Tool::Select, Tool::Pen, Tool::Geometry, Tool::Text, Tool::Lasso, Tool::Eraser].into_iter().map(|t| (t, None)).collect(),
            open_button: false,
        }
    }

    /// Add a tool of an extension, drawn on its button from the SVG `icon` (a name already there, more tools than fit, or no icon is refused: an
    /// extension with no icon is not a tool to choose, and its button would be an empty space in the pill).
    pub fn with(mut self, tool: Tool, icon: &'static str) -> Registry {
        if !icon.is_empty() && self.tools.len() < MAX_TOOLS && self.tools.iter().all(|(t, _)| *t != tool) {
            self.tools.push((tool, Some(icon)));
        }
        self
    }

    /// The same, with (or without) a button to open a PDF.
    pub fn with_open_button(mut self, on: bool) -> Registry {
        self.open_button = on;
        self
    }

    /// Whether the toolbar has a button to open a PDF.
    pub fn has_open_button(&self) -> bool {
        self.open_button
    }

    /// The tools, in the toolbar's order.
    pub fn tools(&self) -> impl Iterator<Item = Tool> + '_ {
        self.tools.iter().map(|(t, _)| *t)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Where the tool is in the order, if there is such a tool.
    #[cfg(test)]
    pub fn position(&self, tool: Tool) -> Option<usize> {
        self.tools.iter().position(|(t, _)| *t == tool)
    }

    /// The tool of this name, if there is one.
    pub fn find(&self, name: &str) -> Option<Tool> {
        self.tools().find(|t| t.name() == name)
    }

    /// The icon of an extension's tool (the program's own are drawn by the toolbar).
    pub fn icon(&self, tool: Tool) -> Option<&'static str> {
        self.tools.iter().find(|(t, _)| *t == tool).and_then(|(_, icon)| *icon)
    }
}

/// The tools tests are written against: the program's own and the laser (the one extension that is the program's own to keep), whatever
/// other extensions a build has, so that an extension added does not change what a test of the toolbar or the interface sees.
#[cfg(test)]
pub fn reference_extensions() -> Vec<inkpdf_api::Extension> {
    crate::tools::extensions().into_iter().filter(|e| e.id == "laser").collect()
}

/// The registry that goes with [`reference_extensions`].
#[cfg(test)]
pub fn reference_registry() -> Registry {
    reference_extensions().iter().fold(Registry::builtin(), |r, e| r.with(Tool::named(e.id), e.icon_svg))
}

/// The registry of this build: the program's own tools and every extension's.
pub fn registry() -> &'static Registry {
    static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    // (The extensions the program can serve: see `tools::extensions`, which says why one is left out.)
    REGISTRY.get_or_init(|| {
        // (An extension with no icon has no button: it is not a tool to choose.)
        inkpdf_ext::all()
            .iter()
            .filter(|e| e.problem().is_none() && !e.icon_svg.is_empty())
            .fold(Registry::builtin(), |r, e| r.with(Tool::named(e.id), e.icon_svg))
    })
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
    /// The kind of item the tool makes and works on; the others make none.
    pub fn kind(self) -> Option<Kind> {
        match self {
            Tool::Pen => Some(Kind::Ink),
            Tool::Geometry => Some(Kind::Shape),
            Tool::Text => Some(Kind::Text),
            _ => None,
        }
    }

    /// Whether the bar under the tools is shown with the tool: the tools that make items have it (to select, copy and restyle them), the
    /// text selection has it for its marks and the lasso for what it picked up.
    pub fn has_bar(self) -> bool {
        self.selects_items() || self == Tool::Select
    }

    /// Whether the tool has a selection of items to copy and restyle (the lasso picks any; the others, their own).
    pub fn selects_items(self) -> bool {
        self.makes_items() || self == Tool::Lasso
    }

    /// Whether the tool works on items of every kind (the lasso and the eraser), not just its own.
    pub fn picks_any(self) -> bool {
        self == Tool::Lasso || self == Tool::Eraser
    }

    /// Whether the tool makes items, so that there is something to select, copy and restyle, and settings to choose.
    pub fn makes_items(self) -> bool {
        self.kind().is_some()
    }

    /// The name used in the settings file.
    pub fn name(self) -> &'static str {
        self.0
    }

    /// Every tool of this build, in the toolbar's order.
    #[cfg(test)]
    pub fn all() -> Vec<Tool> {
        registry().tools().collect()
    }

    /// The tool of this name among those of this build (a name that is no tool, as one left in the settings by an extension that is gone,
    /// is `None`).
    pub fn from_name(name: &str) -> Option<Tool> {
        registry().find(name)
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
        for tool in Tool::all() {
            assert_eq!(Tool::from_name(tool.name()), Some(tool));
        }
        // The program's own tools come first, in the toolbar's order, each making its own kind of item (or none).
        let builtin = [Tool::Select, Tool::Pen, Tool::Geometry, Tool::Text, Tool::Lasso, Tool::Eraser];
        assert_eq!(&Tool::all()[..6], &builtin);
        assert_eq!(builtin.map(|t| t.kind()), [None, Some(Kind::Ink), Some(Kind::Shape), Some(Kind::Text), None, None]);
        assert_eq!(builtin.map(Tool::makes_items), [false, true, true, true, false, false]);
        assert_eq!(
            builtin.map(Tool::has_bar),
            [true, true, true, true, true, false],
            "the text selection has its marks in the bar, the lasso its picked-up items, and the eraser nothing"
        );
        // An extension's tool is only a name: it makes none of the program's items and has no bar.
        let laser = Tool::from_name("laser").expect("the laser extension");
        assert_eq!((laser.kind(), laser.has_bar()), (None, false));
        assert_eq!(Tool::from_name("brush"), None);
        assert_eq!(Tool::default(), Tool::Select, "the program starts on the selection of text");
        assert_eq!(Tool::all()[0], Tool::default(), "which is the first button");
    }

    #[test]
    fn a_registry_lists_tools_once_in_order_and_holds_no_more_than_a_toolbar_can() {
        let r = Registry::builtin().with(Tool::named("a"), "<path/>").with(Tool::named("a"), "<path/>").with(Tool::named("b"), "<path/>");
        assert_eq!(r.tools().map(Tool::name).collect::<Vec<_>>(), ["select", "pen", "geometry", "text", "lasso", "eraser", "a", "b"]);
        assert_eq!((r.position(Tool::named("b")), r.position(Tool::named("zzz"))), (Some(7), None));
        assert_eq!(r.position(Tool::Select), Some(0));
        assert_eq!(r.icon(Tool::named("a")), Some("<path/>"));
        assert_eq!(r.icon(Tool::Pen), None, "the toolbar draws the program's own");
        let full = (0..50).fold(Registry::builtin(), |r, i| r.with(Tool::named(Box::leak(format!("t{i}").into_boxed_str())), "<path/>"));
        assert_eq!(full.len(), MAX_TOOLS);
        assert_eq!(Registry::builtin().with(Tool::named("quiet"), "").len(), 6, "an extension with no icon has no button");
    }

    #[test]
    fn text_has_its_own_range_and_the_defaults_are_inside_the_ranges() {
        assert_eq!(width_range(Tool::Text), TEXT_SIZES);
        for tool in [Tool::Pen, Tool::Geometry, Tool::Select, Tool::named("laser")] {
            assert_eq!(width_range(tool), (MIN_WIDTH, MAX_WIDTH));
        }
        for (tool, style) in [(Tool::Pen, ToolStyle::PEN), (Tool::Geometry, ToolStyle::SHAPES), (Tool::Text, ToolStyle::TEXT)] {
            let (least, most) = width_range(tool);
            assert!((least..=most).contains(&style.width), "{tool:?}");
            assert!(style.colour < PALETTE.len(), "{tool:?}");
        }
    }
}
