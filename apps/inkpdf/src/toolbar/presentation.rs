//! What is drawn over the page while the document is shown as slides: black over everything round the page (so that nothing of the pages next to it shows), and,
//! for a moment after anything is done, how far through the slides it is and a button to leave. The editor says which page is shown and where it is; this is where
//! the rest is and how it looks, as plain data.

use super::icons::icons;
use super::theme::{Label, Ui};

/// What is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationView {
    /// The page shown (0-based) and how many there are.
    pub page: usize,
    pub pages: usize,
    /// The counter and the button to leave are shown (for a moment after anything is done).
    pub hint: bool,
    /// Where the page is, on the screen, `[x0, y0, x1, y1]` (bits, so that this can be compared to see whether anything changed).
    pub page_rect: [u32; 4],
}

/// Where the button to leave is, in window pixels.
#[derive(Clone, Copy, Debug)]
pub struct PresentationLayout {
    pub exit: [f32; 4],
    /// The middle of the counter, at the bottom.
    pub counter: [f32; 2],
    px: f32,
}

impl PresentationLayout {
    pub fn new(viewport: [f32; 2], dpr: f32) -> Self {
        let size = (46.0 * dpr).min(viewport[0] / 4.0).min(viewport[1] / 4.0).max(24.0);
        let margin = (14.0 * dpr).min(size / 2.0);
        let exit = [viewport[0] - margin - size, margin, viewport[0] - margin, margin + size];
        Self { exit, counter: [viewport[0] / 2.0, viewport[1] - margin - 0.5 * size], px: dpr }
    }

    /// Whether `p` is on the button to leave.
    pub fn on_exit(&self, p: [f32; 2]) -> bool {
        p[0] >= self.exit[0] && p[0] <= self.exit[2] && p[1] >= self.exit[1] && p[1] <= self.exit[3]
    }

    pub fn exit_centre(&self) -> [f32; 2] {
        [(self.exit[0] + self.exit[2]) / 2.0, (self.exit[1] + self.exit[3]) / 2.0]
    }

    pub fn draw(&self, view: &PresentationView, viewport: [f32; 2], ui: &mut Ui) {
        let (black, dim, white) = (ui.pal.shadow(255), ui.pal.shadow(150), ui.pal.white_a(235));
        let page = view.page_rect.map(f32::from_bits);
        let (vw, vh) = (viewport[0], viewport[1]);
        // The curtain: the four pieces of the window the page is not in.
        for rect in [[0.0, 0.0, vw, page[1]], [0.0, page[3], vw, vh], [0.0, page[1], page[0], page[3]], [page[2], page[1], vw, page[3]]] {
            if rect[2] > rect[0] && rect[3] > rect[1] {
                ui.panel(rect, 0.0, black);
            }
        }
        if !view.hint {
            return;
        }
        let size = self.exit[2] - self.exit[0];
        let [cx, cy] = self.exit_centre();
        ui.disc([cx, cy], size, dim);
        icons().close.draw(&mut ui.lines, cx, cy, 0.3 * size, (0.05 * size).max(1.5 * self.px), white);
        // How far through: "3 / 12", in a dark pill so that it reads on any page.
        let text = format!("{} / {}", view.page + 1, view.pages);
        let font = 0.34 * size;
        let width = crate::text::measure(&text, font)[0] + 0.9 * size;
        let [mx, my] = self.counter;
        ui.panel([mx - width / 2.0, my - 0.4 * size, mx + width / 2.0, my + 0.4 * size], 0.4 * size, dim);
        ui.labels.push(Label::centre(text, font, white, [mx, my]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curtain_covers_all_but_the_page_and_the_hint_is_inside_the_window() {
        for ([w, h], dpr) in [([1920.0, 1080.0], 1.0), ([360.0, 480.0], 1.0), ([2400.0, 1080.0], 2.6), ([800.0, 2000.0], 2.0)] {
            let l = PresentationLayout::new([w, h], dpr);
            assert!(l.exit[0] >= 0.0 && l.exit[2] <= w && l.exit[1] >= 0.0 && l.exit[3] <= h, "{w}x{h}: {:?}", l.exit);
            assert!(l.exit[2] - l.exit[0] >= 24.0, "big enough to press");
            let page = [w * 0.25, 0.0, w * 0.75, h].map(f32::to_bits);
            let mut ui = Ui::default();
            l.draw(&PresentationView { page: 2, pages: 12, hint: false, page_rect: page }, [w, h], &mut ui);
            let covered: f32 = ui.panels.iter().map(|p| (p.rect[2] - p.rect[0]) * (p.rect[3] - p.rect[1])).sum();
            assert!((covered - w * 0.5 * h).abs() < 1.0, "the two sides, and nothing of the page: {covered}");
            let mut ui = Ui::default();
            l.draw(&PresentationView { page: 2, pages: 12, hint: true, page_rect: page }, [w, h], &mut ui);
            assert!(ui.labels.iter().any(|x| x.text == "3 / 12"), "the counter says where it is");
            assert!(l.on_exit(l.exit_centre()) && !l.on_exit([w / 2.0, h / 2.0]));
        }
    }
}
