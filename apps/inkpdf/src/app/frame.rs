//! One frame: wait for the display, read the pen, step the view, stream tiles, draw.

use std::time::{Duration, Instant};

use crate::gpu::{Drawn, InkScene};
use crate::input::InputEvent;
use crate::tiles::{self, Key};

use super::State;
use super::bench::FrameParts;

const BACKGROUND: [f64; 4] = [0.93, 0.936, 0.947, 1.0];
/// Texture memory kept for page tiles and previews.
const TEXTURE_BUDGET: usize = 768 << 20;
/// How long uploading finished tiles may take out of one frame.
const UPLOAD_BUDGET: Duration = Duration::from_millis(4);
/// The same while the pen is drawing: the frame carrying its newest samples is not made to wait for tiles
/// (they come a frame later; the pen's ink does not).
const UPLOAD_BUDGET_PEN_DOWN: Duration = Duration::from_millis(1);
/// How long the zoom must hold still before sharp tiles are requested for it.
const ZOOM_SETTLE: Duration = Duration::from_millis(90);
/// When a frame could not be drawn (minimised, occluded), how long until the next try.
const RETRY_AFTER: Duration = Duration::from_millis(33);

impl State {
    pub(super) fn redraw(&mut self) {
        // A frame can be asked for again from inside a dialog's message loop; that one is skipped
        // (it would wait for the swap-chain image this frame is holding).
        if self.timing.suspended || std::mem::replace(&mut self.timing.drawing, true) {
            return;
        }
        self.draw_frame();
        self.timing.drawing = false;
    }

    /// Handle the pen and touch messages Windows has queued. What they ask of the shell (a
    /// dialog, a save) waits until the frame is done: a dialog runs its own message loop, which
    /// must never happen while a frame is half drawn.
    fn take_pointer_input(&mut self) {
        if let Some(pointer) = &mut self.pointer {
            for ev in pointer.drain() {
                self.ed.handle(InputEvent::Pointer(ev));
            }
            for ev in pointer.drain_other() {
                self.ed.handle(ev);
            }
        }
    }

    fn draw_frame(&mut self) {
        // Wait for the display first (that is where a frame waits for the refresh), then read
        // the pen: the ink in this frame carries the newest samples, not ones a refresh old.
        let frame = self.gpu.acquire();
        if let Some(pointer) = &mut self.pointer {
            pointer.pump();
        }
        self.take_pointer_input();
        if self.timing.print_stats
            && let Some(t) = self.ed.pending_sample_ms
        {
            self.timing.read_age_ms.push((crate::clock::now_ms() - t) as f32);
        }
        let now = Instant::now();
        let dt = now.duration_since(self.timing.last_frame).as_secs_f32();
        self.timing.last_frame = now;
        self.timing.frame += 1;

        self.drive_bench(dt);
        let animating = self.ed.step_view(dt);
        // The laser's trail fades with the clock: keep drawing until it is gone, and no longer.
        let pen_clock = crate::clock::now_ms();
        let fading = self.ed.tools.pointer.prune(pen_clock);
        let settled = self.zoom_settled(now);
        let pending = self.stream_tiles(settled);

        let t_plan = Instant::now();
        self.sync_texts();
        let scene = self.scene();
        let t_scene = Instant::now();
        if scene.blank_pages > 0
            && let Some(bench) = &mut self.timing.bench
        {
            bench.note_blank_pages();
        }
        for &key in &scene.drawn {
            self.gpu.touch(key, self.timing.frame);
        }

        self.sync_committed();
        self.sync_ui();
        self.overview_frame();
        let ranges = self.visible_ink_ranges();
        let buf = std::mem::take(&mut self.layers.overlay_buf);
        let overlays = self.overlays(pen_clock, buf);
        let ink_scene = InkScene {
            scroll: self.ed.view.scroll,
            zoom: self.ed.view.zoom,
            committed: &ranges,
            overlay_doc: &overlays.doc,
            overlay_screen: &overlays.screen,
            thumbs: &self.layers.overview.thumbs,
        };
        if let Some(t) = self.ed.pending_sample_ms.take() {
            self.timing.latency_ms.push((crate::clock::now_ms() - t) as f32);
        }
        let t_ink = Instant::now();
        let result = self.gpu.draw(frame, &scene.quads, &self.layers.overview.quads, &ink_scene, BACKGROUND);
        let t_draw = Instant::now();
        // Keep the buffers (and their room) for the next frame.
        self.layers.overlay_buf = overlays;
        self.gpu.evict(TEXTURE_BUDGET, self.timing.frame);
        if let Some(bench) = &mut self.timing.bench {
            let us = |a: Instant, b: Instant| b.duration_since(a).as_secs_f64() * 1e6;
            bench.note_parts(FrameParts {
                plan_upload: us(now, t_plan),
                scene: us(t_plan, t_scene),
                ink_overlays: us(t_scene, t_ink),
                gpu_draw: us(t_ink, t_draw),
                vsync_wait: self.gpu.last_wait_us,
            });
        }

        self.update_title();
        match result {
            Drawn::Skipped => self.timing.wake_at = Some(now + RETRY_AFTER),
            Drawn::Presented if animating || fading || !settled || pending > 0 || self.timing.bench.is_some() => self.window.request_redraw(),
            Drawn::Presented => {}
        }
        if self.timing.exit_at.is_some_and(|t| now >= t) {
            self.timing.exit = true;
        }
    }

    /// Note a zoom change, and say whether the zoom has held still long enough for sharp tiles.
    fn zoom_settled(&mut self, now: Instant) -> bool {
        if (self.ed.view.zoom - self.timing.last_zoom).abs() > f32::EPSILON {
            self.timing.last_zoom = self.ed.view.zoom;
            self.timing.last_zoom_change = now;
        }
        !self.ed.view.zoom_animating() && !self.ed.pinching() && now.duration_since(self.timing.last_zoom_change) >= ZOOM_SETTLE
    }

    /// Ask the render pool for the tiles the view needs and upload the ones it has finished.
    /// Returns how many tiles are still wanted.
    fn stream_tiles(&mut self, settled: bool) -> usize {
        let Some(doc) = &mut self.session.doc else { return 0 };
        let mut pending = 0;
        // Ask for what the view needs, unless the zoom is still moving: its level would change
        // every frame.
        // (With the pages overview up the document is not seen: what is wanted is a small picture of each page it shows.)
        let overview = self.ed.overview_pages();
        // (Pictures are kept by the page of the file that draws them, so that they outlast any change of the pages.)
        let to_file = |page: u32| doc.render_page(page as usize);
        let plan = if overview.is_some() { tiles::Plan::default() } else { tiles::plan(&self.ed.layout, &self.ed.view).mapped(to_file) };
        // Pages that were added are blank: there is nothing to render, so nothing to wait for.
        self.layers.visible = plan.visible.iter().copied().filter(|&k| doc.renders(k)).collect();
        if settled {
            let (gpu, failed) = (&self.gpu, &self.session.failed);
            let have = |k: Key| !doc.renders(k) || gpu.has(k) || failed.contains(&k);
            let queue = match &overview {
                Some(pages) => pages.clone().filter_map(|p| to_file(p as u32)).map(|p| Key::Thumb { page: p }).filter(|&k| !have(k)).collect(),
                None => plan.queue(have),
            };
            pending = queue.len();
            doc.set_queue(&queue);
        }
        if let Some(bench) = &mut self.timing.bench
            && self.layers.visible.iter().any(|&k| !self.gpu.has(k))
        {
            bench.note_missing_tiles();
        }
        // Take finished renders, within a time budget so a burst cannot stall the frame.
        let started = Instant::now();
        let budget = if self.ed.pending_sample_ms.is_some() { UPLOAD_BUDGET_PEN_DOWN } else { UPLOAD_BUDGET };
        while let Some(page) = doc.try_recv() {
            let key = Key::from_tag(page.request.tag);
            match &page.error {
                Some(e) => {
                    eprintln!("render failed for {key:?}: {e}");
                    self.session.failed.insert(key);
                }
                None => self.gpu.upload(key, page.width, page.height, page.rgba.as_ref(), self.timing.frame),
            }
            if started.elapsed() > budget {
                break;
            }
        }
        pending
    }

    fn update_title(&mut self) {
        let Some(doc) = &self.session.doc else { return };
        let page = self.ed.view.current_page(&self.ed.layout) + 1;
        let percent = (self.ed.view.zoom / self.ed.actual_size_zoom() * 100.0).round();
        let mark = if self.ed.ink.is_dirty() { "● " } else { "" };
        let title = format!("{mark}{} — page {page}/{} — {percent}%", doc.name, doc.page_count());
        self.set_title(title);
    }
}
