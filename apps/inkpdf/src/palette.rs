//! The colours of the program, by the job they do. Every colour the interface is drawn in is a role of a [`Palette`]; a [`Scheme`] picks
//! the palette, so the light and the dark look are two sets of values for the same roles, and a change of scheme can be a mix of the two
//! ([`Palette::mix`]). Plain data and arithmetic: nothing here knows what a toolbar is.
//!
//! The page itself (the paper, the ink the user chose) is not part of the interface's scheme: it stays as the document has it, unless the
//! user turns on dark pages, which is its own setting (see `Workspace::on_page`).

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
    /// What marks the places the search found on the page, and the one that is current (drawn over the page: translucent).
    pub search_hit: Rgba,
    pub search_current: Rgba,
    /// A ring and cross where a drag has snapped to another shape's anchor; the grey of an eraser's or lasso's guide.
    pub snap: Rgba,
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

    #[cfg(test)]
    /// The palette of a scheme.
    pub fn of(scheme: Scheme) -> Palette {
        match scheme {
            Scheme::Light => LIGHT,
            Scheme::Dark => DARK,
        }
    }

    /// `a` going to `b` as `t` goes from 0 to 1, role by role (a change of scheme as a fade).
    pub fn mix(a: &Palette, b: &Palette, t: f32) -> Palette {
        let t = t.clamp(0.0, 1.0);
        let c = |x: Rgba, y: Rgba| -> Rgba { [0, 1, 2, 3].map(|i| (f32::from(x[i]) + (f32::from(y[i]) - f32::from(x[i])) * t).round() as u8) };
        let r = |x: [u8; 3], y: [u8; 3]| -> [u8; 3] { [0, 1, 2].map(|i| (f32::from(x[i]) + (f32::from(y[i]) - f32::from(x[i])) * t).round() as u8) };
        let w = |x: f64, y: f64| x * f64::from(1.0 - t) + y * f64::from(t);
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
            search_hit: c(a.search_hit, b.search_hit),
            search_current: c(a.search_current, b.search_current),
            snap: c(a.snap, b.snap),
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
    search_hit: [255, 200, 0, 110],
    search_current: [255, 120, 0, 170],
    snap: [226, 40, 150, 255],
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

/// The dark scheme: the same roles on a dark slate, the ink light, the accent and the danger lifted so they keep their contrast, the
/// washes made of light instead of dark. (The pages are not part of it: they stay as the document has them unless dark pages is on.)
pub const DARK: Palette = Palette {
    panel: [30, 33, 40, 255],
    panel_edge: [235, 238, 245, 30],
    ink: [221, 225, 235, 255],
    ink_dim: [96, 102, 116, 255],
    accent: [70, 140, 245, 255],
    accent_wash: [70, 140, 245, 70],
    hover_wash: [235, 238, 245, 22],
    pressed_wash: [235, 238, 245, 40],
    neutral_wash: [235, 238, 245, 24],
    separator: [235, 238, 245, 30],
    track: [235, 238, 245, 44],
    rim: [235, 238, 245, 80],
    danger: [240, 90, 100, 255],
    danger_wash: [240, 90, 100, 52],
    white: [255, 255, 255, 255],
    paper: [255, 255, 255, 255],
    selection: [64, 132, 255, 255],
    search_hit: [255, 200, 0, 110],
    search_current: [255, 120, 0, 170],
    snap: [226, 40, 150, 255],
    guide: [90, 90, 96, 230],
    folder: [240, 178, 52, 255],
    folder_tab: [226, 160, 36, 255],
    page: [70, 76, 90, 255],
    page_rim: [122, 128, 144, 255],
    file_ink: [160, 166, 182, 255],
    window: [0.085, 0.09, 0.105],
    shadow_rgb: [0, 0, 0],
    wash_rgb: [235, 238, 245],
    press_rgb: [255, 255, 255],
};

/// The paper of a dark page, and the colour text on it is drawn in (what the PDF engine maps white and black to).
pub const DARK_PAPER: [u8; 3] = [36, 39, 47];
pub const DARK_TEXT: [u8; 3] = [226, 229, 236];

/// How much brighter than the paper ink must be to be read on it (WCAG's AA for text).
const INK_CONTRAST: f64 = 4.5;

/// WCAG relative luminance of an opaque colour.
fn luminance(c: Rgba) -> f64 {
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
}

/// The contrast ratio of two colours (1 to 21), the lighter over the darker.
fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Red, green, blue (0 to 255) as hue (0 to 6), saturation and lightness (0 to 1).
fn to_hsl(c: Rgba) -> [f64; 3] {
    let [r, g, b] = [c[0], c[1], c[2]].map(|v| f64::from(v) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h, s, l]
}

fn from_hsl([h, s, l]: [f64; 3], alpha: u8) -> Rgba {
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - (h.rem_euclid(6.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match h.rem_euclid(6.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(r), byte(g), byte(b), alpha]
}

/// How a colour the user chose for ink looks on a dark page: its lightness turned over (black ink becomes white), its hue and strength
/// kept, then lifted until it can be read on the paper. Display only: what is saved keeps the colour that was chosen.
pub fn on_dark_page(c: Rgba) -> Rgba {
    let [h, s, l] = to_hsl(c);
    let paper = [DARK_PAPER[0], DARK_PAPER[1], DARK_PAPER[2], 255];
    let mut l = 1.0 - l;
    while l < 1.0 && contrast(from_hsl([h, s, l], 255), paper) < INK_CONTRAST {
        l = (l + 0.01).min(1.0);
    }
    from_hsl([h, s, l], c[3])
}

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

    /// What lies under a translucent colour, flattened onto the panel.
    fn over(top: Rgba, under: Rgba) -> Rgba {
        let a = f32::from(top[3]) / 255.0;
        [0, 1, 2]
            .map(|i| (f32::from(top[i]) * a + f32::from(under[i]) * (1.0 - a)).round() as u8)
            .iter()
            .copied()
            .chain([255])
            .collect::<Vec<_>>()
            .try_into()
            .expect("four")
    }

    #[test]
    fn what_is_read_and_what_is_pressed_keeps_its_contrast_in_both_schemes() {
        for (name, p) in [("light", LIGHT), ("dark", DARK)] {
            assert_eq!(p.panel[3], 255, "{name}: a panel is opaque, so page text never ghosts through it");
            assert!(contrast(p.ink, p.panel) >= 7.0, "{name}: ink on a panel is {:.1}", contrast(p.ink, p.panel));
            assert!(contrast(p.ink_dim, p.panel) >= 1.8, "{name}: a dim icon is still seen ({:.1})", contrast(p.ink_dim, p.panel));
            assert!(contrast(p.ink_dim, p.panel) < contrast(p.ink, p.panel) / 2.0, "{name}: and dim");
            assert!(contrast(p.accent, p.panel) >= 3.0, "{name}: the accent on a panel is {:.1}", contrast(p.accent, p.panel));
            assert!(contrast(p.white, p.accent) >= 3.0, "{name}: white on the accent (the Save button) is {:.1}", contrast(p.white, p.accent));
            assert!(contrast(p.danger, p.panel) >= 3.0, "{name}: danger on a panel is {:.1}", contrast(p.danger, p.panel));
            assert!(
                contrast(over(p.danger_wash, p.panel), p.panel) < 1.5 && contrast(p.danger, over(p.danger_wash, p.panel)) >= 3.0,
                "{name}: danger on its wash"
            );
            assert!(contrast(p.ink, over(p.accent_wash, p.panel)) >= 4.5, "{name}: ink on the accent wash of an active button");
            assert!(contrast(p.ink, over(p.hover_wash, p.panel)) >= 6.0, "{name}: ink on the wash of a button the pointer is over");
            assert!(contrast(p.ink, over(p.pressed_wash, p.panel)) >= 5.0, "{name}: ink on a pressed button");
            assert!(contrast(p.file_ink, p.panel) >= 3.0, "{name}: the name of a file on the card ({:.1})", contrast(p.file_ink, p.panel));
            assert!(contrast(p.rim, p.panel) >= 1.0 && over(p.rim, p.panel) != p.panel, "{name}: a rim shows on a panel");
        }
        assert!(luminance(DARK.panel) < 0.05 && luminance(LIGHT.panel) > 0.9, "the dark scheme is dark and the light one is light");
        assert!(luminance(DARK.ink) > luminance(DARK.panel) && luminance(LIGHT.ink) < luminance(LIGHT.panel), "ink is the other way from its panel");
    }

    /// Every role of a palette, to treat them alike.
    fn roles(p: &Palette) -> Vec<Rgba> {
        vec![
            p.panel,
            p.panel_edge,
            p.ink,
            p.ink_dim,
            p.accent,
            p.accent_wash,
            p.hover_wash,
            p.pressed_wash,
            p.neutral_wash,
            p.separator,
            p.track,
            p.rim,
            p.danger,
            p.danger_wash,
            p.white,
            p.paper,
            p.selection,
            p.snap,
            p.search_hit,
            p.search_current,
            p.guide,
            p.folder,
            p.folder_tab,
            p.page,
            p.page_rim,
            p.file_ink,
            p.shadow(255),
            p.wash(255),
            p.press(255),
        ]
    }

    #[test]
    fn a_mix_of_the_schemes_goes_from_one_to_the_other_without_overshooting_in_any_role() {
        let (a, b) = (LIGHT, DARK);
        let (ra, rb) = (roles(&a), roles(&b));
        let mut last = roles(&Palette::mix(&a, &b, 0.0));
        assert_eq!(last, ra);
        for step in 1..=20 {
            let now = roles(&Palette::mix(&a, &b, step as f32 / 20.0));
            for (role, (before, after)) in last.iter().zip(&now).enumerate() {
                for ch in 0..4 {
                    let (from, to) = (ra[role][ch], rb[role][ch]);
                    let moving_up = to >= from;
                    assert!((after[ch] >= before[ch]) == moving_up || after[ch] == before[ch], "role {role} channel {ch} turned back at step {step}");
                    assert!(after[ch] >= from.min(to) && after[ch] <= from.max(to), "role {role} channel {ch} overshoots at step {step}");
                }
            }
            last = now;
        }
        assert_eq!(last, rb);
        assert_eq!(Palette::mix(&a, &b, 1.0).window, b.window);
    }

    #[test]
    fn the_schemes_differ_where_they_should_and_agree_where_a_mark_is_the_same_in_both() {
        assert_ne!(LIGHT.panel, DARK.panel);
        assert_ne!(LIGHT.ink, DARK.ink);
        assert_ne!(LIGHT.window, DARK.window);
        assert_eq!((LIGHT.white, DARK.white), ([255; 4], [255; 4]), "a mark on a coloured dot is white in both");
        assert_eq!(LIGHT.snap, DARK.snap, "a snap's colour does not change with the look of the bars");
    }

    #[test]
    fn the_alpha_helpers_keep_their_colour_and_take_the_alpha_given() {
        let p = LIGHT;
        assert_eq!(p.shadow(70), [16, 20, 44, 70]);
        assert_eq!(p.wash(10), [20, 24, 40, 10]);
        assert_eq!(p.press(44), [0, 0, 0, 44]);
        assert_eq!(p.white_a(200), [255, 255, 255, 200]);
    }

    #[test]
    fn ink_on_a_dark_page_is_turned_over_keeps_its_hue_and_can_be_read() {
        assert_eq!(on_dark_page([0, 0, 0, 255]), [255, 255, 255, 255], "black ink becomes white");
        assert_eq!(on_dark_page([10, 20, 30, 77])[3], 77, "its opacity is kept");
        let paper = [DARK_PAPER[0], DARK_PAPER[1], DARK_PAPER[2], 255];
        for colour in crate::tool::PALETTE.iter().chain(&[
            [255, 255, 255, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [0, 255, 0, 255],
            [128, 128, 128, 255],
            [0, 0, 0, 255],
            [40, 40, 50, 255],
        ]) {
            let shown = on_dark_page(*colour);
            assert!(contrast(shown, paper) >= INK_CONTRAST - 0.05, "{colour:?} shows as {shown:?}, too close to the paper");
            if colour[0] != colour[1] || colour[1] != colour[2] {
                // The hue stays (within what rounding bytes allows).
                let (a, b) = (to_hsl(*colour)[0], to_hsl(shown)[0]);
                let apart = (a - b).abs().min(6.0 - (a - b).abs());
                assert!(apart < 0.15, "{colour:?} -> {shown:?}: the hue moved by {apart}");
            }
        }
    }

    #[test]
    fn the_dark_paper_and_its_text_read_well_together() {
        let paper = [DARK_PAPER[0], DARK_PAPER[1], DARK_PAPER[2], 255];
        let text = [DARK_TEXT[0], DARK_TEXT[1], DARK_TEXT[2], 255];
        assert!(contrast(text, paper) >= 10.0, "text on the dark paper: {}", contrast(text, paper));
    }
}
