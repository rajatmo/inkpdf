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
| Pen pressure / eraser / barrel / hover / history | yes | **Wayland: yes** (L2, the tablet protocol; untried on a real tablet); X11: basic mouse and touch | yes (the S Pen's one button erases) |
| Touch: pan, pinch, taps on the buttons | yes | basic | yes |
| Palm rejection | touch is off while the pen is near | same rule (with basic input there is no hover) | yes |
| Clipboard for text boxes | yes | the system's (L3: arboard, Wayland data-control or X11; in-memory where neither is reachable) | the system's, through the Java side (A4; untried on a device) |
| Save As | in-app card, opens on Documents | in-app card, opens on Documents (XDG) | the system's picker (Storage Access Framework; A4, untried on a device): `DocId::Uri` through `platform/saf.rs` |
| Error box | yes | a card over the page (L3) | a card over the page |
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

**L2. Linux pen. Done on Wayland (2026-10-31); X11 is not done (the owner chose Wayland only), and nothing has been tried on a real tablet.**
What there is: `platform/tablet_frames.rs` (pure: what a tablet tool says, frame by frame, becomes the program's pointer events: tested on every OS with scripted
frames and a 200-round storm that the stream keeps `input::check`), `platform/linux/tablet.rs` (the binding: a second event queue on winit's own
Wayland connection, `zwp_tablet_manager_v2`, read in `pump` right after the display wait, only tools over our own surface, touches that winit
reports handed through) and `platform/linux/tablet/fake_compositor.rs` (a compositor of our own, `wayland-server` on a socket pair, serving a tablet and
a scripted pen to the real binding: hover, press, moves, lift, leave, scale, the eraser end, the side button and a pen over another window). Tried
against WSLg's compositor, which has no tablet protocol: the reader says so and the pen is the mouse, as before. The design notes that follow were
the plan; `PointerSource::set_scale` and `Platform::install_pointer(&dyn WindowHandles)` came of it.

The plan was: `platform/linux/tablet.rs`, a `PointerSource` over `zwp_tablet_manager_v2`.
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

**L3. Linux polish. Done (2026-10-31)** except the Flatpak (the AppImage was chosen: `tools/linux/build-appimage.sh`) and a HiDPI check on a real screen:
the clipboard is arboard (`platform/linux/clipboard.rs`; Xvfb runs its round-trip test, a session with neither Wayland data-control nor X11 keeps the program's
own), an error is a card over the page where the system has no message box (`toolbar/error_card.rs`, `Platform::has_error_box`), and animations follow
GNOME's `enable-animations` unless `INKPDF_MOTION` says otherwise. The plan was: Clipboard (Wayland: the fork already carries a clipboard in `platform::wayland::Clipboard`; X11:
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

**A3. Android UI fit. Done (2026-10-31; watched on the phone emulator, not on a device).** What was found on the emulator: the soft keyboard
does not resize the window (`client` stays 1080x2400) and covered the lowest 40% of it, so a text box typed low on the page was out of sight.
`Host.java` now watches the window's insets and tells Rust how many pixels the keyboard covers (`Bridge::keyboard_height`,
`Platform::keyboard_covered`); the shell looks at it while the keyboard is up or on its way (a bounded 60 ms look for 1.5 s after it is asked for,
so an idle window still costs nothing); `editor/keyboard.rs` brings the text box or form field being typed above it (`View::scroll_to_clear`),
lets the scroll go past the end of the document by the room it takes (`View::set_bottom_room`) and puts the view back when it goes, unless the person
scrolled meanwhile. Cut-outs: the activity no longer forces `LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES` with the bars hidden, so the window is never
drawn under a camera hole (the toolbars would have been); with `targetSdk` 34 the system also keeps it inside the bars. (Raising `targetSdk` to 35
makes Android draw edge to edge: the insets would then have to be passed down as the keyboard's height is.) Touch targets: the buttons are never
under 30 points (folding pills take over before they shrink: 1.4.0), 30 dp being above the 24 dp Material minimum but under its 48 dp advice; a
phone and a tablet are checked by `tools/e2e` with real touches and a pen.

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

## Build all three on one Windows PC

Windows and Android build natively, Android with Android Studio's own SDK, NDK and Java (`tools/android/build-apk.ps1`, the twin of
`build-apk.sh`); Linux builds in WSL (Ubuntu). Once:

* in Android Studio, SDK Manager > SDK Tools: **NDK (Side by side)** (r27 or newer);
* on Windows: `rustup target add aarch64-linux-android x86_64-linux-android`;
* in WSL, from the repository folder: `bash tools/linux/setup-dev.sh` (apt packages as CI's `linux` job and a C toolchain, and Rust;
  asks for sudo for apt; `--android` also sets up Android inside WSL, for a Linux-only machine).

Then, from PowerShell, `.\tools\cross-build.ps1` (or `-Only windows,linux,android`) runs each platform's tests the way CI does and
builds the Windows release, an APK with arm64 and x86_64 libraries (the x86_64 one runs in Android Studio's emulator), and the Linux
portable tarball. The Linux build stays inside WSL (`~/inkpdf-build-linux`); the packages land in `inkpdf-dist` beside the repository. Shell scripts are kept with LF line endings (`.gitattributes`), which bash in WSL
needs.

Cross-checking Windows from Linux (no Windows needed for compile errors): `rustup target add x86_64-pc-windows-msvc`,
then `cargo clippy -p inkpdf --all-targets --target x86_64-pc-windows-msvc -- -D warnings`.
