# Porting InkPDF to Linux and Android

Branch `linux-android-port`. `main` is untouched and still the Windows app. Written 2026-10-29.

**One idea:** every OS gets its own small *pointer source*; everything above it is shared and unchanged.

```
 pen / touch hardware
        │
 ┌──────┴────────────────────────────────────────────┐
 │ platform/windows   WM_POINTER + history   (done)  │   one PointerSource per OS,
 │ platform/linux     winit basic input     (L2: Wayland tablet-v2, X11 XI2)
 │ platform/android   winit basic input     (A2: MotionEvent + history)
 └──────┬────────────────────────────────────────────┘
        │  PointerEvent { device, phase, samples[], pen buttons }   ← same type on every OS
   input → editor → tools → ink → scene → gpu (wgpu: DX12 | Vulkan)
```

## Status

| | Windows | Linux (X11, Wayland) | Android |
|---|---|---|---|
| Builds | yes (checked by cross-compiling from Linux, clippy `-D warnings`, tests build) | **yes** | type-checks (`aarch64-linux-android`) |
| 612 unit/editor/storm tests | CI (as before) | **pass** | not run |
| Window + GPU + render | as before | **runs** (X11 on Xvfb, Wayland on headless Weston, software Vulkan) | no packaging yet |
| Mouse draws, Ctrl+S saves an Ink annotation | as before | **yes** (`tools/e2e/linux-smoke.sh`) | no |
| Pen pressure / eraser / barrel / history | yes | **not yet** (L2) | not yet (A2) |
| Clipboard for text boxes | yes | in-memory only (L3) | in-memory only (A4) |
| Save As | in-app card, opens on Documents | in-app card, opens on Documents (XDG) | in-app card, opens on shared Documents; needs the SAF for real saving (A4) |
| Error box | yes | terminal | terminal |

Not verified by me, because the container has no pen, no touch screen and no Windows: any real pen input, and the
Windows tests *running* (they only build here). CI on `windows-latest` is still the arbiter for Windows.

The picture of all this, generated from the code: `docs/deps/platform.svg` (routing per OS) and `docs/deps/skeleton.svg`
(the module graph); regenerate with `python3 tools/deps/graph.py`.

## What changed (and what did not)

* `win_pointer`, `native`, `posture` moved, unchanged apart from paths, to `platform/windows/` (git sees renames).
* `platform/mod.rs` is the only surface the shell uses: `PointerSource` (`pump` / `pending` / `drain` /
  `take_posture_change` / `uninstall`), `install_pointer`, `save_file_dialog`, `error_box`, `open_url`,
  `motion_prefs`, `detect_mode`, `SystemClipboard`, `report_fatal`, `attach_parent_console`.
* `app` holds `Option<Box<dyn PointerSource>>` instead of a bool plus free functions. `None` is the old fallback
  (winit mouse/touch), which is what Linux and Android use today.
* Per-OS differences found elsewhere, now `cfg`-split: `clock.rs` (QPC on Windows, `Instant` elsewhere), `text.rs`
  (font lookup: Windows fonts folder / Linux and Android font folders, Liberation Sans is Arial-metric),
  `gpu/device.rs` (DX12 / Vulkan), `settings.rs` (`%APPDATA%` / `$XDG_CONFIG_HOME`), `doc.rs` (a read-only file is
  refused on Linux too, and a saved file keeps its permissions), `inkpdf-bot` (Windows-only driver, gated).
* Cargo: winit gets X11 + Wayland features on Linux and NativeActivity on Android; the `rfd` dependency is gone (Save As is the app's own card).
  Windows dependency features are exactly as before.
* Layering test: `platform` replaces `win_pointer`/`native`/`posture` in `LAYERS`; the session still may not name it.

**Windows behaviour is unchanged by construction** (same code, same call order, `cfg(windows)` paths identical).
Check it yourself on return: `cargo test`, `tools/e2e/run.ps1`, `tools/e2e/latency.ps1` (p50 ≈ 2.6 ms, p95 ≈ 4.3 ms).

## Latency: why the pointer source is separate

Time from nib to photon:

```
L  =  τ_dev + τ_os + τ_wait + τ_draw + τ_scan
```

`τ_dev`, `τ_os` belong to the hardware and OS; `τ_scan ≈ ½ refresh` on average. The app owns `τ_wait` (how long a
sample sits before the frame reads it) and `τ_draw`. The ink then trails the nib by `v · L` (a pen at 0.5 m/s with
L = 20 ms is 10 mm behind), and prediction by `τ_pred` (12 ms here) cancels `v · τ_pred` of it. So per OS:

1. **Late sampling:** wait for the display first, *then* read the pen → `τ_wait ≈ 0`. On Windows `pump()` is a
   `PeekMessage` over the pointer messages. A Linux/Android source must offer the same `pump()`: read the socket or
   queue *now*, bypassing the event loop.
2. **Keep the OS's batch:** the pen reports faster than the display refreshes; keep every point (`samples[]`), as
   `GetPointerPenInfoHistory` does, so strokes stay smooth at 200 to 1000 Hz.
3. **Nothing else in the pen-down path** (no animation, no tile upload beyond 1 ms).

Key literature on why this matters: Ng et al., *Designing for Low-Latency Direct-Touch Input*, UIST 2012; Jota et al.,
*How Fast is Fast Enough?*, CHI 2013 (direct-touch dragging performance falls with latency, at tens of ms already);
Annett et al., *How Low Should We Go? Understanding the Perception of Latency While Inking*, Graphics Interface 2014
(inking shows latency perception at much lower values than touch); Deber et al., *How Much Faster is Fast Enough?*,
CHI 2015 (people notice improvements of a few ms).

History in one line each: WinTab (Wacom, 1990s) → `WM_POINTER` (Windows 8) with input history; X11 XInput2 (2009)
gave multi-device, valuator-rich input; Wayland's `zwp_tablet_v2` (late 2010s) is the tablet protocol compositors speak;
Android has had `MotionEvent` historical samples and `TOOL_TYPE_STYLUS` for a long time.

## Next steps (each is one session; do in order)

**L1. Done:** build, run and test on Linux; smoke test and CI jobs.

**L2. Linux pen (the main task).** `platform/linux/tablet.rs`, a `PointerSource` over `zwp_tablet_manager_v2`.
* Share winit's Wayland connection (`wayland-backend` `from_foreign_display` with the display pointer from
  raw-window-handle; the fork already builds `wayland-backend` with `dlopen`). A second connection cannot work: tablet
  events are delivered for *our* `wl_surface`.
* `pump()` = `prepare_read` + `read` (non-blocking) + `dispatch_pending` on our own queue, right after `Gpu::acquire`.
* Events accumulate until `frame(time)`; one frame = one `Sample`, so several frames read at once become `samples[]`.
  Map: `proximity_in/out` → Hover/Leave, `down/up` → Down/Up, `pressure` / 65535, `tool type eraser` → `eraser`,
  `button` BTN_STYLUS (0x14b) → `barrel`; positions are surface-local logical, multiply by the scale factor.
* Test the translation as a pure state machine with scripted event lists (like `posture::decide`), and the binding with
  a mock compositor (`wayland-server` over a socket pair). Hardware check needs a tablet: say so in the PR.
* X11 fallback: XInput2 (`XI_Motion` with valuators for pressure; `XIGetDeviceProperty` for tool type).
* Posture: no reliable signal on Linux; keep `--mode` and the toolbar's mode button.

**L3. Linux polish.** Clipboard (Wayland: the fork already carries a clipboard in `platform::wayland::Clipboard`; X11:
`arboard`), an in-app error card (like Save As), animations setting (`gsettings`/portal), packaging (AppImage or
Flatpak; a `.desktop` file and icon), HiDPI check, the real `latency.sh` equivalent.

**A1. Android build. Done (first cut).** The program is now the library `apps/inkpdf/src/lib.rs` (`main.rs` only calls `inkpdf::run()`;
unit tests are `cargo test -p inkpdf --lib`). `apps/inkpdf-android` is the `cdylib` the APK holds (`android_main`, stderr forwarded to
logcat as tag `inkpdf`). The GPU surface is paused on `Suspended` and remade on `Resumed`. `tools/android/build-apk.sh` builds and
signs an arm64 APK (NativeActivity, no Java); CI builds it as an artifact. **Not run on a device by the author**: expect first-run
problems and read `adb logcat -s inkpdf`. Known gaps: pen pressure/eraser/barrel (A2), system bars and cutouts (A3), files from other apps
and the clipboard (A4); saving goes to the app's own `Documents` folder (`Android/data/app.inkpdf.test/files/Documents`).

**A2. Android stylus.** `platform/android/stylus.rs`: `MotionEvent` through `android-activity` (tool type
stylus/eraser/finger, pressure, `ACTION_HOVER_*`, historical pointer samples = `samples[]`, `BUTTON_STYLUS_PRIMARY`).
Palm rejection already exists in `input/palm.rs`. Latency items to evaluate on a device: unbuffered input dispatch,
`Choreographer`-aligned frames (Swappy frame pacing), Vulkan `FIFO` vs `MAILBOX`, and front-buffer rendering where the
device has it. Do not claim numbers without `--stats` on hardware.

**A3. Android UI fit.** Touch targets (the toolbar is sized for a mouse and a pen), system bars and cutouts, the
soft keyboard for the text tool (`Ime` events), default `Mode::Tablet` (already set).

**A4. Android files.** Storage Access Framework (open/save `Intent`, `content://` URIs, so `Doc` needs a bytes-based
open and save, not just a path), clipboard, `ACTION_VIEW` for links. Needs JNI (`jni` crate + `ndk-context`).

## Try it on Linux

```bash
sudo apt install mesa-vulkan-drivers libxkbcommon-x11-0 fonts-liberation   # plus a real GPU driver on a real machine
cargo run -p inkpdf --bin inkpdf -- some.pdf --mode desktop
WINIT_UNIX_BACKEND=wayland cargo run -p inkpdf --bin inkpdf     # or =x11
tools/e2e/linux-smoke.sh                                                      # needs xvfb, xdotool
cargo check -p inkpdf-android --target aarch64-linux-android                  # rustup target add aarch64-linux-android first
```

Cross-checking Windows from Linux (no Windows needed for compile errors): `rustup target add x86_64-pc-windows-msvc`,
then `cargo clippy -p inkpdf --all-targets --target x86_64-pc-windows-msvc -- -D warnings`.
