# Porting InkPDF to Linux and Android

Linux and Android live on `main` beside Windows (merged 2026-10-29). Written 2026-10-29.

**One idea:** every OS gets its own small *pointer source*; everything above it is shared and unchanged.

```
 pen / touch hardware
        │
 ┌──────┴────────────────────────────────────────────┐
 │ platform/windows   WM_POINTER + history   (done)  │   one PointerSource per OS,
 │ platform/linux     winit basic input     (L2: Wayland tablet-v2, X11 XI2)
 │ platform/android   MotionEvent + history (done)   │   behind one Platform object
 └──────┬────────────────────────────────────────────┘
        │  PointerEvent { device, phase, samples[], pen buttons }   ← same type on every OS
   input → editor → tools → ink → scene → gpu (wgpu: DX12 | Vulkan)
```

## Status

| | Windows | Linux (X11, Wayland) | Android |
|---|---|---|---|
| Builds | yes | yes | yes (arm64 APK, `tools/android`) |
| 717 unit/editor/storm tests | CI and by hand | pass | the pure parts (the decoder) run on every OS |
| Window + GPU + render | yes (DX12) | runs (X11 on Xvfb, Wayland on headless Weston, software Vulkan) | runs on a Samsung tablet (S Pen), Vulkan |
| Pen pressure / eraser / barrel / hover / history | yes | **not yet** (L2): basic mouse and touch | yes (the S Pen's one button erases) |
| Touch: pan, pinch, taps on the buttons | yes | basic | yes |
| Palm rejection | touch is off while the pen is near | same rule (with basic input there is no hover) | yes |
| Clipboard for text boxes | yes | in-memory only (L3) | the system's, through the Java side (A4; untried on a device) |
| Save As | in-app card, opens on Documents | in-app card, opens on Documents (XDG) | the system's picker (Storage Access Framework; A4, untried on a device): `DocId::Uri` through `platform/saf.rs` |
| Error box | yes | terminal | terminal |
| Portable package | zip (`build-portable.ps1`, CI) | tarball (`tools/linux/build-portable.sh`, CI) | APK (CI artifact) |

What has been tried by hand: Windows (pen, touch and the portable build), Linux on WSL, and Android on an S Pen tablet. What has not:
Linux with a real pen (there is no reader for it yet), an Android phone, Android with a mouse or a keyboard dock, and any
measurement of latency on Linux or Android (do not claim numbers without `--stats` on hardware).

The picture of all this, generated from the code: `docs/deps/platform.svg` (routing per OS) and `docs/deps/skeleton.svg`
(the module graph); regenerate with `python3 tools/deps/graph.py`.

## How it is built

* `platform/mod.rs` is the only surface the shell uses: the `Platform` trait (folders, `file_store`, font, motion and posture, URL,
  error box, text input, clipboard, `install_pointer`) built once by `platform::native()`, and `PointerSource` (`pump` / `pending` /
  `drain_into` / `take_posture_change` / `uninstall`; `pump_early`, `drain_other_into` and `late_touch` for a source that shares the OS's
  input queue with the toolkit, which Android's does). `Option<Box<dyn PointerSource>>` in the shell: `None` is the fallback to winit's
  mouse and touch, which Linux uses today. The window crosses as a `raw-window-handle` and a `Waker`.
* The pen path is unchanged in order (wait for the display, `pump`, `drain_into`, draw), and no thread, channel or lock sits between a
  source and the editor.
* Per-OS differences live in `platform/`, not scattered as `cfg`: the clock is `Instant` (Windows converts its counter stamps in
  `platform/windows/counter.rs`), the font is Arial from Windows or the built-in Liberation Sans, settings go to `Platform::config_dir`,
  files go through a `FileStore`. What is left as `cfg` in the program: the GPU backend choice (`gpu/device.rs`: DX12 on Windows, Vulkan
  elsewhere) and the read-only check in `path_store.rs`. A test forbids files, the environment, OS `cfg`, PDF, `wgpu` and `winit` below the
  adapters.
* Cargo: winit gets X11 + Wayland features on Linux and NativeActivity on Android. The program is the library `apps/inkpdf/src/lib.rs`
  (`main.rs` only calls `inkpdf::run()`), and `apps/inkpdf-android` is the `cdylib` the APK holds.
* `--record FILE` / `--replay FILE` copy and replay the input stream, so a problem on one device can be played on another.

**Windows behaviour is unchanged by design** (the same Win32 calls in the same order), and it was checked by hand on a real machine
after the port: `cargo test`, `tools/e2e/run.ps1`, `tools/e2e/latency.ps1` are still the way to check it (p50 about 2.6 ms, p95 about 4.3 ms).

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

**L1. Done:** build, run and test on Linux; smoke test, portable tarball and CI jobs.

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

**A1. Android build. Done.** The program is the library `apps/inkpdf/src/lib.rs`; `apps/inkpdf-android` is the `cdylib` the APK holds
(`android_main`, stderr forwarded to logcat as tag `inkpdf`). The GPU surface is paused on `Suspended` and remade on `Resumed`.
`tools/android/build-apk.sh` builds and signs an arm64 APK (NativeActivity, no Java); CI builds it as an artifact and runs it on an emulator.
Saving goes to the app's own `Documents` folder (`Android/data/app.inkpdf.test/files/Documents`).

**A2. Android stylus. Done, tried on an S Pen tablet.** `platform/android/reader.rs` reads the activity's input queue *before* winit does
(at winit's `new_events`, and again after the display wait), because winit turns every `MotionEvent` into a plain touch. What it takes goes
through `platform::motion_event::Decoder`, which is tested on every platform: stylus = pen with pressure and the points between events
(`samples[]`), eraser end or the stylus's first button = eraser (an S Pen has one button), second button = barrel, hover and leave, finger =
touch with its contact size, palm = rejected, mouse = the mouse; keys (Back = Escape, the soft keyboard's characters, Ctrl shortcuts). A few
events arrive in the moment between the two reads and winit reports them as touch: `late_touch` makes them the pointer they already were.
What the device taught: a screen *cancels* its touches when the pen comes near, so a cancel carries the pointer's last place (or whoever is
told of it cannot let it go, and the next one-finger pan becomes a pinch with the leftover finger); a pen digitiser and a touch screen both
number their pointers from 0, so a pointer is told by its device *and* its id; a pen whose screen does not report hover must not be called
"in range" after a lift, or touch stays off. A randomized test (`no_finger_is_left_behind_...`) drives such orders through the decoder into
the editor. Still to evaluate on a device: unbuffered input dispatch, `Choreographer`-aligned frames (Swappy), Vulkan `FIFO` vs `MAILBOX`,
front-buffer rendering.

**A3. Android UI fit.** Touch targets (the toolbar is sized for a mouse and a pen), system bars and cutouts, the
soft keyboard for the text tool (`Ime` events), default `Mode::Tablet` (already set).

**A4. Android files.** Storage Access Framework (open/save `Intent`, `content://` URIs, so `Doc` needs a bytes-based
open and save, not just a path), clipboard, `ACTION_VIEW` for links. Needs JNI (`jni` crate + `ndk-context`).

## Try it on Linux

```bash
sudo apt install mesa-vulkan-drivers libxkbcommon-x11-0 fonts-liberation   # plus a real GPU driver on a real machine
cargo run -p inkpdf --bin inkpdf -- some.pdf --mode desktop
WINIT_UNIX_BACKEND=wayland cargo run -p inkpdf --bin inkpdf     # or =x11
tools/e2e/linux-smoke.sh                                                      # needs xvfb, xdotool (also records and replays a session)
tools/linux/build-portable.sh                                                 # InkPDF-Portable-<version>-linux-x86_64.tar.gz
cargo check -p inkpdf-android --target aarch64-linux-android                  # rustup target add aarch64-linux-android first
```

Cross-checking Windows from Linux (no Windows needed for compile errors): `rustup target add x86_64-pc-windows-msvc`,
then `cargo clippy -p inkpdf --all-targets --target x86_64-pc-windows-msvc -- -D warnings`.
