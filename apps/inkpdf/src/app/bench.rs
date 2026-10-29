//! The scripted benchmark (`--bench`) and the pen-latency report (`--stats`).

use std::time::{Duration, Instant};

use super::State;

/// Blank-page and part timings count from here, so the first load does not hide scrolling's.
const WARM_UP: Duration = Duration::from_secs(1);

/// Main-thread time per part of a frame, in microseconds.
#[derive(Clone, Copy, Default)]
pub(super) struct FrameParts {
    pub plan_upload: f64,
    pub scene: f64,
    pub ink_overlays: f64,
    pub gpu_draw: f64,
    pub vsync_wait: f64,
}

pub(super) struct Bench {
    started: Instant,
    frame_ms: Vec<f32>,
    frames_with_missing_tiles: u32,
    blank_page_frames: u32,
    zoom_base: f32,
    /// `FrameParts` summed over the frames measured.
    totals: FrameParts,
    measured_frames: u32,
}

impl Bench {
    pub fn new(now: Instant) -> Self {
        Self {
            started: now,
            frame_ms: Vec::new(),
            frames_with_missing_tiles: 0,
            blank_page_frames: 0,
            zoom_base: 1.0,
            totals: FrameParts::default(),
            measured_frames: 0,
        }
    }

    fn warmed_up(&self) -> bool {
        self.started.elapsed() >= WARM_UP
    }

    pub fn note_missing_tiles(&mut self) {
        self.frames_with_missing_tiles += 1;
    }

    pub fn note_blank_pages(&mut self) {
        if self.warmed_up() {
            self.blank_page_frames += 1;
        }
    }

    pub fn note_parts(&mut self, parts: FrameParts) {
        if self.warmed_up() {
            let t = &mut self.totals;
            t.plan_upload += parts.plan_upload;
            t.scene += parts.scene;
            t.ink_overlays += parts.ink_overlays;
            t.gpu_draw += parts.gpu_draw;
            t.vsync_wait += parts.vsync_wait;
            self.measured_frames += 1;
        }
    }
}

impl State {
    /// Advance the scripted benchmark: scroll, zoom in and out, scroll again, then report.
    pub(super) fn drive_bench(&mut self, dt: f32) {
        let Some(bench) = &mut self.timing.bench else { return };
        let ed = &mut self.ed;
        let t = bench.started.elapsed().as_secs_f32();
        let centre = ed.viewport_centre();
        match t {
            t if t < 1.0 => bench.zoom_base = ed.view.zoom,
            t if t < 4.0 => ed.pan_px(0.0, -1500.0 * dt),
            t if t < 7.0 => {
                let phase = ((t - 4.0) / 0.75) as u32;
                let target = if phase.is_multiple_of(2) { bench.zoom_base * 2.5 } else { bench.zoom_base };
                ed.view_mut().zoom_to(target, centre);
            }
            t if t < 10.0 => ed.pan_px(0.0, -2400.0 * dt),
            _ => {}
        }
        if t >= 1.0 {
            bench.frame_ms.push(dt * 1000.0);
        }
        if t >= 10.2 {
            self.report_bench();
            self.timing.exit = true;
        }
    }

    fn report_bench(&mut self) {
        let Some(bench) = self.timing.bench.take() else { return };
        let mut ms = bench.frame_ms;
        ms.sort_by(f32::total_cmp);
        let n = ms.len().max(1);
        let pct = |p: f32| ms.get(((n as f32 * p) as usize).min(n - 1)).copied().unwrap_or(0.0);
        let avg = ms.iter().sum::<f32>() / n as f32;
        println!("frames: {n}");
        println!(
            "frame time ms: avg {avg:.2}  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}",
            pct(0.5),
            pct(0.95),
            pct(0.99),
            ms.last().copied().unwrap_or(0.0)
        );
        println!("frames with a visible page showing nothing at all: {}", bench.blank_page_frames);
        println!(
            "frames with some sharp tiles still missing: {} ({:.0}%)",
            bench.frames_with_missing_tiles,
            100.0 * bench.frames_with_missing_tiles as f32 / n as f32
        );
        println!("texture memory: {} MB in {} textures", self.gpu.residency.bytes() >> 20, self.gpu.residency.len());
        let f = f64::from(bench.measured_frames.max(1));
        let p = bench.totals;
        println!(
            "main-thread microseconds per frame: plan+upload {:.0}  scene {:.0}  ink+overlays {:.0}  gpu draw {:.0}  | waiting for vsync {:.0}",
            p.plan_upload / f,
            p.scene / f,
            p.ink_overlays / f,
            p.gpu_draw / f,
            p.vsync_wait / f
        );
    }

    pub(super) fn report_latency(&mut self) {
        let mut age = std::mem::take(&mut self.timing.read_age_ms);
        age.sort_by(f32::total_cmp);
        if let (Some(&mid), Some(&high)) = (age.get(age.len() / 2), age.get((age.len() as f32 * 0.95) as usize).or(age.last())) {
            println!(
                "newest pen sample's age when the frame read it, ms: p50 {mid:.1}  p95 {high:.1}  (the rest of the latency is the frame's own work)"
            );
        }
        let mut ms = std::mem::take(&mut self.timing.latency_ms);
        if ms.is_empty() {
            println!("pen latency: no pen samples were drawn");
            return;
        }
        ms.sort_by(f32::total_cmp);
        let n = ms.len();
        let pct = |p: f32| ms[((n as f32 * p) as usize).min(n - 1)];
        println!(
            "pen sample to frame submitted, ms ({n} frames): p50 {:.1}  p95 {:.1}  p99 {:.1}  max {:.1}",
            pct(0.5),
            pct(0.95),
            pct(0.99),
            ms[n - 1]
        );
    }
}
