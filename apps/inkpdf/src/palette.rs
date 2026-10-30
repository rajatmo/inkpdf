//! The colours of the program, by the job they do. Every colour the interface is drawn in is a role of a [`Palette`]; a [`Scheme`] picks
//! the palette, so the light and the dark look are two sets of values for the same roles, and a change of scheme can be a mix of the two
//! ([`Palette::mix`]). Plain data and arithmetic: nothing here knows what a toolbar is.
//!
//! The page itself (the paper, the ink the user chose) is not part of the interface's scheme: it stays as the document has it, unless the
//! user turns on dark pages, which is its own setting (see `Workspace::display_colour`).

#[allow(dead_code, reason = "chosen by the appearance setting (T4)")]
/// Which look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Scheme {
    #[default]
    Light,
    Dark,
}

/// A colour: straight (not premultiplied) RGBA.
pub type Rgba = [u8; 4];

/// Every colour the interface is drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// A floating panel's fill: opaque, so page text never ghosts through it.
    pub panel: Rgba,
    /// The hairline round a panel.
    pub panel_edge: Rgba,
    /// Icons and text-weight lines, and a dimmed (disabled) icon.
    pub ink: Rgba,
    pub ink_dim: Rgba,
    /// The one accent, and the wash behind an active button.
    pub accent: Rgba,
    pub accent_wash: Rgba,
    /// The wash behind a button the pointer is over, one being pressed, and a neutral button.
    pub hover_wash: Rgba,
    pub pressed_wash: Rgba,
    pub neutral_wash: Rgba,
    /// A hairline between groups; the empty part of a slider; the rim of a colour dot or a slider thumb (so a white one still shows).
    pub separator: Rgba,
    pub track: Rgba,
    pub rim: Rgba,
    /// What can destroy (the bin, Discard) and its wash.
    pub danger: Rgba,
    pub danger_wash: Rgba,
    /// A mark on a coloured dot or on the accent: white in every scheme.
    pub white: Rgba,
    /// The paper of a page as the overview draws it behind its picture.
    pub paper: Rgba,
    /// What shows what is selected on the page (the selection's outline and handles).
    pub selection: Rgba,
    /// A ring and cross where a drag has snapped to another shape's anchor; the laser pointer's red; the grey of an eraser's or lasso's guide.
    pub snap: Rgba,
    pub laser: Rgba,
    pub guide: Rgba,
    /// The folder and page icons of the Save As card, and the ink of a file's name.
    pub folder: Rgba,
    pub folder_tab: Rgba,
    pub page: Rgba,
    pub page_rim: Rgba,
    pub file_ink: Rgba,
    /// The colour the window is cleared to behind the pages (each 0 to 1).
    pub window: [f64; 3],
    /// The colour of a shadow, of a scrim over the document, and of a wash: their alpha is the caller's (see [`Palette::shadow`]).
    shadow_rgb: [u8; 3],
    wash_rgb: [u8; 3],
    /// What a pressed button darkens (or, in the dark scheme, lightens) by.
    press_rgb: [u8; 3],
}

fn with_alpha(rgb: [u8; 3], alpha: u8) -> Rgba {
    [rgb[0], rgb[1], rgb[2], alpha]
}

#[allow(dead_code, reason = "the scheme is resolved by the editor (T4)")]
impl Palette {
    /// A shadow, or a scrim over the document, of this opacity (0 to 255).
    pub fn shadow(&self, alpha: u8) -> Rgba {
        with_alpha(self.shadow_rgb, alpha)
    }

    /// A wash of the interface's ink colour, of this opacity: a well, a faint line.
    pub fn wash(&self, alpha: u8) -> Rgba {
        with_alpha(self.wash_rgb, alpha)
    }

    /// What a press puts over a button, of this opacity.
    pub fn press(&self, alpha: u8) -> Rgba {
        with_alpha(self.press_rgb, alpha)
    }

    /// White, of this opacity.
    pub fn white_a(&self, alpha: u8) -> Rgba {
        with_alpha([self.white[0], self.white[1], self.white[2]], alpha)
    }

    /// The palette of a scheme.
    pub fn of(scheme: Scheme) -> Palette {
        match scheme {
            Scheme::Light => LIGHT,
            Scheme::Dark => LIGHT,
        }
    }

    /// `a` going to `b` as `t` goes from 0 to 1, role by role (a change of scheme as a fade).
    pub fn mix(a: &Palette, b: &Palette, t: f32) -> Palette {
        let t = t.clamp(0.0, 1.0);
        let c = |x: Rgba, y: Rgba| -> Rgba { [0, 1, 2, 3].map(|i| (f32::from(x[i]) + (f32::from(y[i]) - f32::from(x[i])) * t).round() as u8) };
        let r = |x: [u8; 3], y: [u8; 3]| -> [u8; 3] { [0, 1, 2].map(|i| (f32::from(x[i]) + (f32::from(y[i]) - f32::from(x[i])) * t).round() as u8) };
        let w = |x: f64, y: f64| x + (y - x) * f64::from(t);
        Palette {
            panel: c(a.panel, b.panel),
            panel_edge: c(a.panel_edge, b.panel_edge),
            ink: c(a.ink, b.ink),
            ink_dim: c(a.ink_dim, b.ink_dim),
            accent: c(a.accent, b.accent),
            accent_wash: c(a.accent_wash, b.accent_wash),
            hover_wash: c(a.hover_wash, b.hover_wash),
            pressed_wash: c(a.pressed_wash, b.pressed_wash),
            neutral_wash: c(a.neutral_wash, b.neutral_wash),
            separator: c(a.separator, b.separator),
            track: c(a.track, b.track),
            rim: c(a.rim, b.rim),
            danger: c(a.danger, b.danger),
            danger_wash: c(a.danger_wash, b.danger_wash),
            white: c(a.white, b.white),
            paper: c(a.paper, b.paper),
            selection: c(a.selection, b.selection),
            snap: c(a.snap, b.snap),
            laser: c(a.laser, b.laser),
            guide: c(a.guide, b.guide),
            folder: c(a.folder, b.folder),
            folder_tab: c(a.folder_tab, b.folder_tab),
            page: c(a.page, b.page),
            page_rim: c(a.page_rim, b.page_rim),
            file_ink: c(a.file_ink, b.file_ink),
            window: [w(a.window[0], b.window[0]), w(a.window[1], b.window[1]), w(a.window[2], b.window[2])],
            shadow_rgb: r(a.shadow_rgb, b.shadow_rgb),
            wash_rgb: r(a.wash_rgb, b.wash_rgb),
            press_rgb: r(a.press_rgb, b.press_rgb),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        LIGHT
    }
}

/// The light scheme: white rounded panels with soft shadows, slate icons, one blue accent.
pub const LIGHT: Palette = Palette {
    panel: [255, 255, 255, 255],
    panel_edge: [20, 24, 40, 26],
    ink: [64, 69, 82, 255],
    ink_dim: [176, 181, 192, 255],
    accent: [47, 128, 237, 255],
    accent_wash: [47, 128, 237, 58],
    hover_wash: [20, 24, 40, 16],
    pressed_wash: [20, 24, 40, 30],
    neutral_wash: [20, 24, 40, 20],
    separator: [20, 24, 40, 24],
    track: [20, 24, 40, 34],
    rim: [20, 24, 40, 56],
    danger: [214, 52, 62, 255],
    danger_wash: [214, 52, 62, 34],
    white: [255, 255, 255, 255],
    paper: [255, 255, 255, 255],
    selection: [64, 132, 255, 255],
    snap: [226, 40, 150, 255],
    laser: [236, 36, 48, 255],
    guide: [90, 90, 96, 230],
    folder: [240, 178, 52, 255],
    folder_tab: [226, 160, 36, 255],
    page: [222, 226, 235, 255],
    page_rim: [170, 176, 192, 255],
    file_ink: [112, 118, 134, 255],
    window: [0.93, 0.936, 0.947],
    shadow_rgb: [16, 20, 44],
    wash_rgb: [20, 24, 40],
    press_rgb: [0, 0, 0],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_palette_is_the_default_and_a_scheme_is_a_palette() {
        assert_eq!(Palette::default(), LIGHT);
        assert_eq!(Palette::of(Scheme::Light), LIGHT);
        assert_eq!(Scheme::default(), Scheme::Light);
    }

    #[test]
    fn a_mix_is_exact_at_its_ends_and_in_between_in_the_middle() {
        let (a, b) = (Palette::of(Scheme::Light), Palette::of(Scheme::Dark));
        assert_eq!(Palette::mix(&a, &b, 0.0), a);
        assert_eq!(Palette::mix(&a, &b, 1.0), b);
        assert_eq!(Palette::mix(&a, &b, -3.0), a, "before its start is its start");
        assert_eq!(Palette::mix(&a, &b, 9.0), b, "beyond its end is its end");
        let (x, y) = (Palette { ink: [0, 100, 200, 255], ..a }, Palette { ink: [100, 200, 0, 255], ..a });
        assert_eq!(Palette::mix(&x, &y, 0.5).ink, [50, 150, 100, 255]);
    }

    #[test]
    fn the_alpha_helpers_keep_their_colour_and_take_the_alpha_given() {
        let p = LIGHT;
        assert_eq!(p.shadow(70), [16, 20, 44, 70]);
        assert_eq!(p.wash(10), [20, 24, 40, 10]);
        assert_eq!(p.press(44), [0, 0, 0, 44]);
        assert_eq!(p.white_a(200), [255, 255, 255, 200]);
    }
}
