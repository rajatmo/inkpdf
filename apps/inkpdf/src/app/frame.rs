//! One frame: wait for the display, read the pen, step the view, stream tiles, draw.

use std::time::{Duration, Instant};

use crate::gpu::{Drawn, InkScene};
use crate::input::InputEvent;
use crate::pdf::PageLook;
use crate::tiles::{self, Key};

use super::bench::FrameParts;
use super::{State, ink_layer, paint};

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
            // (The buffers are kept from frame to frame, and the source's queue keeps its room: no allocation in a steady stream.)
            let (mut pointers, mut others) = (std::mem::take(&mut self.timing.pointer_buf), std::mem::take(&mut self.timing.other_buf));
            pointer.drain_into(&mut pointers);
            pointer.drain_other_into(&mut others);
            let events = pointers.drain(..).map(InputEvent::Pointer).chain(others.drain(..));
            for ev in events {
                if let Some(recorder) = &mut self.recorder {
                    recorder.push(&ev);
                }
                self.ed.handle(ev);
            }
            (self.timing.pointer_buf, self.timing.other_buf) = (pointers, others);
        }
        self.play_recording();
    }

    /// Hand the editor what a recording has come to the time of, and close the program a moment after the last of it.
    fn play_recording(&mut self) {
        let (due, finished) = match &mut self.player {
            Some(player) => (player.due(crate::clock::now_ms()), player.finished()),
            None => return,
        };
        for recorded in due {
            self.feed(recorded.event);
        }
        if !finished {
            self.window.request_redraw();
        } else if self.timing.exit_at.is_none() {
            self.timing.exit_at = Some(Instant::now() + Duration::from_millis(500));
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
        let fading = self.ed.tick_tools(pen_clock);
        let settled = self.zoom_settled(now);
        let pending = self.stream_tiles(settled) + usize::from(self.stream_text());

        let t_plan = Instant::now();
        self.layers.sync_texts(&mut paint!(self));
        let scene = self.layers.scene(&paint!(self));
        let t_scene = Instant::now();
        if scene.blank_pages > 0
            && let Some(bench) = &mut self.timing.bench
        {
            bench.note_blank_pages();
        }
        for &key in &scene.drawn {
            self.gpu.touch(key, self.timing.frame);
        }

        self.layers.sync_committed(&mut paint!(self));
        self.layers.sync_ui(&mut paint!(self));
        self.layers.overview_frame(&mut paint!(self));
        let ranges = self.layers.visible_ink_ranges(&paint!(self));
        let buf = std::mem::take(&mut self.layers.overlay_buf);
        let overlays = ink_layer::overlays(&self.ed, pen_clock, buf);
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
        let result = self.gpu.draw(
            frame,
            &scene.quads,
            &self.layers.overview.quads,
            &ink_scene,
            [self.ed.theme.window[0], self.ed.theme.window[1], self.ed.theme.window[2], 1.0],
        );
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
            Drawn::Presented if animating || fading || self.layers.overview.fading_in || !settled || pending > 0 || self.timing.bench.is_some() => {
                self.window.request_redraw()
            }
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
        // The pages are drawn in the look the user has chosen: when it changes, the old pictures are dropped and everything is asked for again.
        let look = if self.ed.dark_pages_active() { PageLook::Dark } else { PageLook::Plain };
        if doc.set_look(look) {
            self.gpu.clear_textures();
            self.session.failed.clear();
        }
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
            let key = Key::from_tag(page.tag);
            match &page.error {
                Some(e) => {
                    eprintln!("render failed for {key:?}: {e}");
                    self.session.failed.insert(key);
                }
                None => self.gpu.upload(key, page.width, page.height, page.rgba.as_slice(), self.timing.frame),
            }
            if started.elapsed() > budget {
                break;
            }
        }
        pending
    }

    /// Take the text the indexer has read, tell the editor which page of the file draws each page, and ask for the pages nearest the one in
    /// view first. `true` while there is more to read (frames go on until it is all in).
    fn stream_text(&mut self) -> bool {
        let (Some(doc), Some(indexer)) = (&self.session.doc, &self.session.indexer) else { return false };
        let changed = self.ed.set_text_files(doc.file_pages());
        while let Some((file, text)) = indexer.try_recv() {
            self.ed.add_text(file, text);
        }
        let current = self.ed.view.current_page(&self.ed.layout);
        if changed || self.ed.text.progress().0 < self.ed.text.progress().1 && !indexer.busy() {
            indexer.want(self.ed.text.wanted(current));
        }
        indexer.busy()
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
