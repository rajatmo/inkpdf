//! The workspace: the document state every tool works on and the settings they share. Tools borrow it
//! (`&mut Workspace`) and nothing else of the session, so a tool cannot reach the toolbar, the prompt or
//! the window, and the session can lend it out while it holds the tools themselves.

use std::collections::HashSet;

use crate::clipboard::{MemoryClipboard, TextClipboard};
use crate::geometry::Shape;
use crate::ink::{InkStore, P, Stroke};
use crate::input::{PenButtons, Role};
use crate::palette::{Palette, Scheme};
use crate::settings::Appearance;
use crate::tool::{PALETTE, Tool, ToolStyle};
use crate::view::{Layout, View};

use crate::requests::Requests;

/// What the tools write with: the five colours (shared), and how each tool that writes uses them: the colour slot in use and the thickness (or,
/// for text, the size) in points; the shape the shape tool makes, and how much the pen is steadied.
pub struct Styles {
    pub palette: [[u8; 4]; 5],
    pub pen: ToolStyle,
    pub shape_style: ToolStyle,
    /// How the lasso restyles what it picked up (not kept in the settings: it is chosen anew each time).
    pub lasso_style: ToolStyle,
    pub text_style: ToolStyle,
    /// The shape the shape tool makes.
    pub shape: Shape,
    /// How much the pen is steadied while drawing, 0 to 1 (see [`crate::smooth`]).
    pub smoothing: f32,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            palette: PALETTE,
            pen: ToolStyle::PEN,
            shape_style: ToolStyle::SHAPES,
            lasso_style: ToolStyle::PEN,
            text_style: ToolStyle::TEXT,
            shape: Shape::default(),
            smoothing: crate::smooth::DEFAULT_AMOUNT,
        }
    }
}

/// How the interface looks: the look the user chose (the system's, light or dark), whether the pages are drawn dark too, what the system says
/// now, and the colours the interface is drawn in now (what the tools paint their handles and outlines in), as the editor resolved the scheme.
#[derive(Default)]
pub struct Look {
    pub appearance: Appearance,
    pub dark_pages: bool,
    pub system_scheme: Scheme,
    pub theme: Palette,
}

/// What was copied, and the system's text clipboard.
pub struct Clipboards {
    /// What was copied: the items themselves, to be pasted again (see [`crate::selection`]).
    pub items: Vec<Stroke>,
    /// How many times the items have been pasted since they were copied.
    pub pasted: u32,
    /// The system's text clipboard: text copied from a text box goes there, and text from anywhere can be pasted.
    pub text: Box<dyn TextClipboard>,
    /// The text that copying a text box put on the system clipboard (so pasting can tell it from text that came from elsewhere).
    pub echo: Option<String>,
}

impl Default for Clipboards {
    fn default() -> Self {
        Self { items: Vec::new(), pasted: 0, text: Box::new(MemoryClipboard::default()), echo: None }
    }
}

/// The card of an item that is open (see `inkpdf_api::ItemKind::card`): which item, and what is being typed in its field.
#[derive(Default)]
pub struct ItemCard {
    pub open: Option<u64>,
    /// What is being written to send.
    pub field: crate::field::Field,
    /// The name written on what is sent (kept in the settings), and whether it has the keyboard instead of the field.
    pub name: crate::field::Field,
    pub typing_name: bool,
}

pub struct Workspace {
    pub layout: Layout,
    pub has_doc: bool,
    pub view: View,
    pub dpr: f32,
    /// Whether the zoom follows the window width (until the user zooms).
    pub fit_mode: bool,
    /// Whether Shift is held (the shell says, with `InputEvent::Shift`): a press with the Select tool then extends the selection instead of
    /// beginning one.
    pub shift: bool,
    pub ink: InkStore,
    pub selection: Vec<u64>,
    /// Strokes the eraser has touched in the current gesture: hidden now, removed when it ends.
    pub erasing: HashSet<u64>,
    /// The tool in use.
    pub tool: Tool,
    pub clip: Clipboards,
    pub styles: Styles,
    pub look: Look,
    /// The card of the item that is open, if one is.
    pub card: ItemCard,
    /// The text of the pages, as far as it has been read (see `doctext`).
    pub text: crate::doctext::DocText,
    /// What the shell is asked to do, and whether a frame is wanted.
    pub requests: Requests,
    pub redraw: bool,
}

impl Workspace {
    pub fn new(viewport: [f32; 2], dpr: f32) -> Self {
        Self {
            layout: Layout::default(),
            has_doc: false,
            view: View::new(viewport),
            dpr,
            fit_mode: true,
            shift: false,
            ink: InkStore::new(),
            selection: Vec::new(),
            erasing: HashSet::new(),
            tool: Tool::Select,
            clip: Clipboards::default(),
            styles: Styles::default(),
            look: Look::default(),
            card: ItemCard::default(),
            text: crate::doctext::DocText::default(),
            requests: Requests::default(),
            redraw: false,
        }
    }
    /// What a press of the stylus (or the mouse standing in for it) does: its own buttons if one is held, else
    /// what the tool in use makes of the plain tip (the lasso tool loops, the eraser tool erases).
    pub fn role(&self, pen: &PenButtons) -> Role {
        match (pen.role(), self.tool) {
            (Role::Draw, Tool::Lasso) => Role::Lasso,
            (Role::Draw, Tool::Eraser) => Role::Erase,
            (held, _) => held,
        }
    }

    /// A screen position in document space.
    pub fn to_doc(&self, screen: [f32; 2]) -> P {
        self.view.screen_to_doc(screen)
    }

    /// The page a document point is on.
    pub fn page_hit(&self, p: P) -> Option<u32> {
        let i = self.layout.page_at(p[1]);
        let r = self.layout.page(i)?;
        crate::ink::rect_contains([r.x, r.y, r.x + r.w, r.bottom()], p).then_some(i as u32)
    }

    /// `page`'s rectangle in document space.
    pub fn page_rect(&self, page: u32) -> Option<[f32; 4]> {
        self.layout.page(page as usize).map(|r| [r.x, r.y, r.x + r.w, r.bottom()])
    }

    /// Whether `p` is on the selection (its bounding box, with a little slack).
    pub fn selection_hit(&self, p: P) -> bool {
        let pad = 6.0 * self.dpr / self.view.zoom;
        self.ink.bounds_of(&self.selection).is_some_and(|b| crate::ink::rect_contains([b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad], p))
    }

    /// How the tool in use writes (the pointer, which writes nothing, reports the pen's).
    pub fn style(&self) -> ToolStyle {
        self.style_of(self.tool)
    }

    pub fn style_of(&self, tool: Tool) -> ToolStyle {
        match tool {
            Tool::Geometry => self.styles.shape_style,
            Tool::Text => self.styles.text_style,
            Tool::Lasso => self.styles.lasso_style,
            _ => self.styles.pen,
        }
    }

    pub fn style_mut(&mut self) -> &mut ToolStyle {
        match self.tool {
            Tool::Geometry => &mut self.styles.shape_style,
            Tool::Text => &mut self.styles.text_style,
            Tool::Lasso => &mut self.styles.lasso_style,
            _ => &mut self.styles.pen,
        }
    }

    pub fn colour_of(&self, style: ToolStyle) -> [u8; 4] {
        self.styles.palette[style.colour % self.styles.palette.len()]
    }

    /// Whether pages are drawn dark now: the setting is on and the look is dark.
    pub fn dark_pages_active(&self) -> bool {
        self.look.dark_pages && self.look.appearance.resolve(self.look.system_scheme) == crate::palette::Scheme::Dark
    }

    /// A colour of the page's own (ink, text) as it is shown: turned over on a dark page, as it is on a light one. What is saved is
    /// the colour chosen.
    pub fn on_page(&self, rgba: [u8; 4]) -> [u8; 4] {
        if self.dark_pages_active() { crate::palette::on_dark_page(rgba) } else { rgba }
    }

    /// The colour and width new strokes get.
    pub fn current_style(&self) -> ([u8; 4], f32) {
        let style = self.style();
        (self.colour_of(style), style.width)
    }

    // The view moves over the layout: these do the split borrow, so callers neither juggle two fields
    // nor reach into the view.

    pub fn pan_px(&mut self, dx: f32, dy: f32) {
        self.view.pan_px(&self.layout, dx, dy);
    }

    pub fn scroll_by_px(&mut self, dx: f32, dy: f32) {
        self.view.scroll_by_px(&self.layout, dx, dy);
    }

    pub fn scroll_to_page(&mut self, page: usize) {
        self.view.scroll_to_page(&self.layout, page);
    }

    pub fn fit_width(&mut self) {
        self.view.fit_width(&self.layout);
    }

    pub fn zoom_about(&mut self, zoom: f32, about: [f32; 2]) {
        self.view.zoom_about(&self.layout, zoom, about);
    }

    /// The window changed size: the view follows it (and the width, while the zoom is fitted).
    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.view.set_viewport(&self.layout, size);
        if self.fit_mode {
            self.view.fit_width(&self.layout);
        }
        self.redraw = true;
    }

    /// One frame of the view's glide and zoom animation: whether it is still moving.
    pub fn step_view(&mut self, dt: f32) -> bool {
        self.view.step(&self.layout, dt)
    }

    /// Let go of selected strokes that no longer exist (after an undo, an erase).
    pub fn prune_selection(&mut self) {
        let ink = &self.ink;
        self.selection.retain(|&id| ink.get(id).is_some());
    }
}
