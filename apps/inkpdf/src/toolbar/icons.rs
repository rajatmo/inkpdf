//! Toolbar icons. They are drawn in TikZ (`icons/*.tex`), compiled to SVG with `icons/build.ps1`
//! and embedded here. At start-up each SVG's stroked paths are flattened into polylines in a box
//! from -1 to 1 (y down), which the toolbar draws as round-ended capsules at any size.

use std::sync::LazyLock;

use crate::scene::Capsule;

/// An icon: open or closed polylines in a box from -1 to 1, y down.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Icon {
    lines: Vec<Vec<[f32; 2]>>,
}

pub(super) struct Icons {
    pub undo: Icon,
    pub redo: Icon,
    pub save: Icon,
    pub fullscreen: Icon,
    pub exit_fullscreen: Icon,
    pub close: Icon,
    pub trash: Icon,
    pub back: Icon,
    pub pen: Icon,
    pub laser: Icon,
    pub geometry: Icon,
    pub plus: Icon,
    pub text: Icon,
    pub desktop: Icon,
    pub tablet: Icon,
    pub select: Icon,
    pub erase: Icon,
    pub duplicate: Icon,
    pub copy: Icon,
    pub paste: Icon,
    pub node_remove: Icon,
    pub node_add: Icon,
    pub node_smooth: Icon,
    pub options: Icon,
    /// Four small pages: the pages overview.
    pub pages: Icon,
    /// A tick in a circle.
    pub check: Icon,
    /// Arrows pointing left and right.
    pub earlier: Icon,
    /// The look: the system's (a circle half in shade), light (a sun), dark (a moon).
    pub theme_system: Icon,
    pub theme_light: Icon,
    pub theme_dark: Icon,
    /// A page with part of it in shade: whether the pages go dark with the look.
    pub dark_pages: Icon,
    pub later: Icon,
}

static ICONS: LazyLock<Icons> = LazyLock::new(|| Icons {
    undo: Icon::from_svg(include_str!("../../icons/svg/undo.svg")),
    redo: Icon::from_svg(include_str!("../../icons/svg/redo.svg")),
    save: Icon::from_svg(include_str!("../../icons/svg/save.svg")),
    fullscreen: Icon::from_svg(include_str!("../../icons/svg/fullscreen.svg")),
    exit_fullscreen: Icon::from_svg(include_str!("../../icons/svg/exit-fullscreen.svg")),
    close: Icon::from_svg(include_str!("../../icons/svg/close.svg")),
    trash: Icon::from_svg(include_str!("../../icons/svg/trash.svg")),
    back: Icon::from_svg(include_str!("../../icons/svg/back.svg")),
    pen: Icon::from_svg(include_str!("../../icons/svg/pen.svg")),
    laser: Icon::from_svg(include_str!("../../icons/svg/laser.svg")),
    geometry: Icon::from_svg(include_str!("../../icons/svg/geometry.svg")),
    plus: Icon::from_svg(include_str!("../../icons/svg/plus.svg")),
    text: Icon::from_svg(include_str!("../../icons/svg/text.svg")),
    desktop: Icon::from_svg(include_str!("../../icons/svg/desktop.svg")),
    tablet: Icon::from_svg(include_str!("../../icons/svg/tablet.svg")),
    select: Icon::from_svg(include_str!("../../icons/svg/select.svg")),
    erase: Icon::from_svg(include_str!("../../icons/svg/erase.svg")),
    duplicate: Icon::from_svg(include_str!("../../icons/svg/duplicate.svg")),
    copy: Icon::from_svg(include_str!("../../icons/svg/copy.svg")),
    paste: Icon::from_svg(include_str!("../../icons/svg/paste.svg")),
    node_remove: Icon::from_svg(include_str!("../../icons/svg/node-remove.svg")),
    node_add: Icon::from_svg(include_str!("../../icons/svg/node-add.svg")),
    node_smooth: Icon::from_svg(include_str!("../../icons/svg/node-smooth.svg")),
    options: Icon::from_svg(include_str!("../../icons/svg/options.svg")),
    pages: Icon::pages(),
    check: Icon::check(),
    earlier: Icon::arrow(-1.0),
    later: Icon::arrow(1.0),
    theme_system: Icon::from_svg(include_str!("../../icons/svg/theme-system.svg")),
    theme_light: Icon::from_svg(include_str!("../../icons/svg/theme-light.svg")),
    theme_dark: Icon::from_svg(include_str!("../../icons/svg/theme-dark.svg")),
    dark_pages: Icon::from_svg(include_str!("../../icons/svg/dark-pages.svg")),
});

pub(super) fn icons() -> &'static Icons {
    &ICONS
}

impl Icon {
    /// Four small pages, two by two.
    fn pages() -> Icon {
        let square = |x0: f32, y0: f32| vec![[x0, y0], [x0 + 0.78, y0], [x0 + 0.78, y0 + 0.78], [x0, y0 + 0.78], [x0, y0]];
        Icon { lines: vec![square(-0.9, -0.9), square(0.12, -0.9), square(-0.9, 0.12), square(0.12, 0.12)] }
    }

    /// An arrow pointing right (`way` 1) or left (-1).
    fn arrow(way: f32) -> Icon {
        Icon { lines: vec![vec![[-0.8 * way, 0.0], [0.8 * way, 0.0]], vec![[0.2 * way, -0.55], [0.8 * way, 0.0], [0.2 * way, 0.55]]] }
    }

    /// A tick in a circle.
    fn check() -> Icon {
        let circle: Vec<[f32; 2]> = (0..=32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                [0.9 * a.cos(), 0.9 * a.sin()]
            })
            .collect();
        Icon { lines: vec![circle, vec![[-0.42, 0.02], [-0.1, 0.36], [0.46, -0.32]]] }
    }

    /// The paths of an SVG written by dvisvgm. Anything that cannot be read gives an empty icon.
    fn from_svg(svg: &str) -> Icon {
        let Some([vx, vy, vw, vh]) = view_box(svg) else { return Icon::default() };
        let to_unit = |p: [f32; 2]| [(p[0] - vx) / vw * 2.0 - 1.0, (p[1] - vy) / vh * 2.0 - 1.0];
        let lines = path_data(svg).flat_map(flatten).map(|line| line.into_iter().map(to_unit).collect()).collect();
        Icon { lines }
    }

    /// Draw it centred on `(cx, cy)`, `half` pixels from the centre to each edge of its box.
    pub fn draw(&self, out: &mut Vec<Capsule>, cx: f32, cy: f32, half: f32, width: f32, color: [u8; 4]) {
        for line in &self.lines {
            for w in line.windows(2) {
                out.push(Capsule::new([cx + w[0][0] * half, cy + w[0][1] * half], [cx + w[1][0] * half, cy + w[1][1] * half], width, color));
            }
        }
    }
}

/// `viewBox='x y w h'`, if it is there and not degenerate.
fn view_box(svg: &str) -> Option<[f32; 4]> {
    let rest = &svg[svg.find("viewBox=")? + "viewBox=".len()..];
    let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = &rest[1..];
    let numbers: Vec<f32> = inner[..inner.find(quote)?].split_whitespace().filter_map(|n| n.parse().ok()).collect();
    match numbers[..] {
        [x, y, w, h] if w > 0.0 && h > 0.0 => Some([x, y, w, h]),
        _ => None,
    }
}

/// The `d` attribute of every `<path>`.
fn path_data(svg: &str) -> impl Iterator<Item = &str> {
    svg.split("<path").skip(1).filter_map(|tag| {
        let rest = &tag[tag.find(" d=")? + " d=".len()..];
        let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
        let inner = &rest[1..];
        Some(&inner[..inner.find(quote)?])
    })
}

/// A path's subpaths as polylines: lines kept, cubic curves cut into short straight pieces.
/// Handles the commands dvisvgm writes (`M L H V C S Z`, absolute); stops at any other.
fn flatten(d: &str) -> Vec<Vec<[f32; 2]>> {
    let mut lines: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut at = [0.0_f32; 2];
    let mut start = at;
    let mut command = ' ';
    // The second control point of the cubic just drawn: a smooth cubic (`S`) continues from its reflection.
    let mut last_control: Option<[f32; 2]> = None;
    let mut numbers = Numbers::new(d);
    while let Some(token) = numbers.next_token() {
        match token {
            Token::Command(c) => {
                command = c;
                if c == 'Z' || c == 'z' {
                    last_control = None;
                    if !current.is_empty() {
                        current.push(start);
                        lines.push(std::mem::take(&mut current));
                    }
                    at = start;
                }
            }
            Token::Number(first) => {
                let mut take = |count: usize| -> Option<Vec<f32>> {
                    let mut values = vec![first];
                    while values.len() < count {
                        match numbers.next_token()? {
                            Token::Number(n) => values.push(n),
                            Token::Command(_) => return None,
                        }
                    }
                    Some(values)
                };
                let arguments = match command {
                    'M' | 'L' => 2,
                    'H' | 'V' => 1,
                    'S' => 4,
                    'C' => 6,
                    _ => break,
                };
                let Some(v) = take(arguments) else { break };
                if !matches!(command, 'C' | 'S') {
                    last_control = None;
                }
                match command {
                    'M' => {
                        if current.len() > 1 {
                            lines.push(std::mem::take(&mut current));
                        }
                        current.clear();
                        at = [v[0], v[1]];
                        start = at;
                        current.push(at);
                        // Further pairs after a move are lines.
                        command = 'L';
                    }
                    'L' => {
                        at = [v[0], v[1]];
                        current.push(at);
                    }
                    'H' => {
                        at = [v[0], at[1]];
                        current.push(at);
                    }
                    'V' => {
                        at = [at[0], v[0]];
                        current.push(at);
                    }
                    'S' => {
                        let p1 = last_control.map_or(at, |c| [2.0 * at[0] - c[0], 2.0 * at[1] - c[1]]);
                        let (p2, p3) = ([v[0], v[1]], [v[2], v[3]]);
                        cubic(at, p1, p2, p3, &mut current);
                        last_control = Some(p2);
                        at = p3;
                    }
                    _ => {
                        let (p1, p2, p3) = ([v[0], v[1]], [v[2], v[3]], [v[4], v[5]]);
                        cubic(at, p1, p2, p3, &mut current);
                        last_control = Some(p2);
                        at = p3;
                    }
                }
            }
        }
    }
    if current.len() > 1 {
        lines.push(current);
    }
    lines
}

/// Append the points of the cubic from `p0` (already in `out`) to `p3`.
fn cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], out: &mut Vec<[f32; 2]>) {
    // The control polygon's length bounds the curve's: aim for pieces about 1.2 pt long, and never
    // fewer than 4 or more than 24.
    let length: f32 = [(p0, p1), (p1, p2), (p2, p3)].iter().map(|(a, b)| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()).sum();
    let pieces = (length / 1.2).ceil().clamp(4.0, 24.0) as usize;
    for i in 1..=pieces {
        let t = i as f32 / pieces as f32;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push([a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]);
    }
}

enum Token {
    Command(char),
    Number(f32),
}

/// Splits path data into commands and numbers. dvisvgm packs numbers tightly: `-3.5-2` is two.
struct Numbers<'a> {
    rest: &'a str,
}

impl<'a> Numbers<'a> {
    fn new(d: &'a str) -> Self {
        Self { rest: d }
    }

    fn next_token(&mut self) -> Option<Token> {
        let trimmed = self.rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        let first = trimmed.chars().next()?;
        if first.is_ascii_alphabetic() {
            self.rest = &trimmed[first.len_utf8()..];
            return Some(Token::Command(first));
        }
        let mut end = 0;
        let mut seen_dot = false;
        for (i, c) in trimmed.char_indices() {
            let part_of_number = match c {
                '0'..='9' => true,
                '.' if !seen_dot => {
                    seen_dot = true;
                    true
                }
                '-' | '+' => i == 0,
                _ => false,
            };
            if !part_of_number {
                break;
            }
            end = i + c.len_utf8();
        }
        let (number, rest) = trimmed.split_at(end);
        self.rest = rest;
        number.parse().ok().map(Token::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tightly_packed_numbers_split_at_signs_and_second_dots() {
        let mut n = Numbers::new("M-60.6602-49.32422L1.5.25");
        let mut values = Vec::new();
        while let Some(t) = n.next_token() {
            values.push(match t {
                Token::Command(c) => format!("{c}"),
                Token::Number(v) => format!("{v}"),
            });
        }
        assert_eq!(values, ["M", "-60.6602", "-49.32422", "L", "1.5", "0.25"]);
    }

    #[test]
    fn lines_curves_and_closed_paths_flatten() {
        let svg = "<svg viewBox='0 0 10 10'><path d='M0 0H10V10L0 10Z' stroke='#000'/><path d='M0 5C0 0 10 0 10 5' fill='none'/></svg>";
        let icon = Icon::from_svg(svg);
        assert_eq!(icon.lines.len(), 2);
        // The square: four corners and back to the start, in the -1..1 box.
        assert_eq!(icon.lines[0], vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0]]);
        // The curve starts and ends where it says, with points between.
        let curve = &icon.lines[1];
        assert!(curve.len() > 4);
        assert_eq!(curve[0], [-1.0, 0.0]);
        assert_eq!(curve[curve.len() - 1], [1.0, 0.0]);
        assert!(curve.iter().all(|p| p[1] <= 0.0), "the bump rises above its ends");
    }

    #[test]
    fn a_smooth_cubic_continues_from_the_reflection_of_the_last_control_point() {
        // A circle as dvisvgm writes it: one cubic, then smooth cubics round to the start.
        let svg = "<svg viewBox='0 0 4 4'><path d='M4 2C4 3.1 3.1 4 2 4S0 3.1 0 2S0.9 0 2 0S4 0.9 4 2Z'/></svg>";
        let icon = Icon::from_svg(svg);
        assert_eq!(icon.lines.len(), 1, "one closed outline");
        let line = &icon.lines[0];
        assert!(line.len() > 12, "the curves were cut into pieces, not dropped");
        // Round: every point is about one unit (half the box) from the centre.
        assert!(line.iter().all(|p| ((p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0).abs() < 0.06), "{line:?}");
    }

    #[test]
    fn broken_input_gives_an_empty_icon_not_a_panic() {
        for svg in [
            "",
            "<svg>",
            "<svg viewBox='0 0 0 0'><path d='M0 0L1 1'/></svg>",
            "<svg viewBox='0 0 1 1'><path d='M0 0L'/></svg>",
            "<svg viewBox='a b c d'/>",
            "<svg viewBox='0 0 1 1'><path d='",
        ] {
            let icon = Icon::from_svg(svg);
            assert!(icon.lines.iter().all(|l| l.len() >= 2), "{svg:?}");
        }
    }

    #[test]
    fn every_embedded_icon_has_strokes_inside_its_box() {
        let all = icons();
        for (name, icon) in [
            ("undo", &all.undo),
            ("redo", &all.redo),
            ("save", &all.save),
            ("fullscreen", &all.fullscreen),
            ("exit-fullscreen", &all.exit_fullscreen),
            ("close", &all.close),
            ("trash", &all.trash),
            ("back", &all.back),
            ("pen", &all.pen),
            ("laser", &all.laser),
            ("geometry", &all.geometry),
            ("plus", &all.plus),
            ("text", &all.text),
            ("desktop", &all.desktop),
            ("tablet", &all.tablet),
            ("select", &all.select),
            ("erase", &all.erase),
            ("duplicate", &all.duplicate),
            ("copy", &all.copy),
            ("paste", &all.paste),
            ("node-remove", &all.node_remove),
            ("node-add", &all.node_add),
            ("node-smooth", &all.node_smooth),
            ("options", &all.options),
            ("theme-system", &all.theme_system),
            ("theme-light", &all.theme_light),
            ("theme-dark", &all.theme_dark),
            ("dark-pages", &all.dark_pages),
        ] {
            assert!(!icon.lines.is_empty(), "{name} has no strokes");
            for p in icon.lines.iter().flatten() {
                assert!(p[0].abs() <= 1.0 + 1e-3 && p[1].abs() <= 1.0 + 1e-3, "{name} leaves its box at {p:?}");
            }
        }
    }

    #[test]
    fn redo_is_undo_mirrored() {
        let all = icons();
        let mirrored: Vec<Vec<[f32; 2]>> = all.undo.lines.iter().map(|l| l.iter().map(|p| [-p[0], p[1]]).collect()).collect();
        assert_eq!(mirrored.len(), all.redo.lines.len());
        for (a, b) in mirrored.iter().flatten().zip(all.redo.lines.iter().flatten()) {
            assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3);
        }
    }
}
