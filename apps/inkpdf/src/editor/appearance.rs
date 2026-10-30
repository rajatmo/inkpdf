//! The look of the interface: which scheme it has (the user's choice, or what the system says), the fade between two, and the colours
//! the tools paint their handles in. The pages stay as the document has them unless dark pages is on and the scheme is dark.

use crate::palette::{DARK, DARK_PAPER, LIGHT, Palette, Scheme};
use crate::requests::Request;
use crate::settings::Appearance;

use super::Editor;

impl Editor {
    /// The look the user chose.
    pub fn appearance(&self) -> Appearance {
        self.ws.look.appearance
    }

    /// Choose a look (the system's, light or dark): the interface fades to it, and it is remembered.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        if self.ws.look.appearance == appearance {
            return;
        }
        self.ws.look.appearance = appearance;
        self.theme_changed();
    }

    /// The next look, in the order the theme button goes through them: the system's, light, dark.
    pub fn cycle_appearance(&mut self) {
        self.set_appearance(self.ws.look.appearance.next());
    }

    /// Whether the pages are to be drawn dark when the look is dark (the setting, which is kept even while the look is light).
    pub fn dark_pages(&self) -> bool {
        self.ws.look.dark_pages
    }

    pub fn set_dark_pages(&mut self, on: bool) {
        if self.ws.look.dark_pages == on {
            return;
        }
        self.ws.look.dark_pages = on;
        self.theme_changed();
    }

    /// Whether the pages are drawn dark now: the setting is on and the look is dark.
    pub fn dark_pages_active(&self) -> bool {
        self.ws.dark_pages_active()
    }

    /// What the system says its look is (the shell tells it at the start and when it changes).
    pub fn set_system_scheme(&mut self, scheme: Scheme) {
        if self.ws.look.system_scheme != scheme {
            self.ws.look.system_scheme = scheme;
            self.sync_theme();
        }
    }

    /// The look the interface is heading for: the user's choice, or the system's.
    pub fn scheme(&self) -> Scheme {
        self.ws.look.appearance.resolve(self.ws.look.system_scheme)
    }

    /// The colours the interface is drawn in now: those of the look it is heading for, or a mix of the two while it fades.
    pub fn palette(&self) -> Palette {
        let mut palette = Palette::mix(&LIGHT, &DARK, self.motions.theme_amount());
        // (The paper the overview draws behind each page's picture is the page's: dark when the pages are.)
        if self.ws.dark_pages_active() {
            palette.paper = [DARK_PAPER[0], DARK_PAPER[1], DARK_PAPER[2], 255];
        }
        palette
    }

    /// The tools paint in what the interface is drawn in; this brings the workspace's copy up to date.
    pub(super) fn sync_theme(&mut self) {
        self.motions.retarget_theme(self.scheme() == Scheme::Dark);
        self.ws.look.theme = self.palette();
        self.ws.redraw = true;
    }

    fn theme_changed(&mut self) {
        self.sync_theme();
        self.ws.requests.push(Request::SettingsChanged);
    }
}
