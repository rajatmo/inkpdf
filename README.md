# InkPDF

A fast, pen-first PDF annotator. Open a PDF, write on it with a stylus, save it back. No account, no library, no sync:
just the page and your pen.

![InkPDF: pen strokes, selecting, colouring, duplicating, deleting and erasing from the bar under the tools, the settings unfolding from it, a shape edited by its anchors, typed text, the desktop/tablet mode button, and the pages overview: every page with the table of contents beside it, a jump to a chapter, pages chosen, duplicated, undone and dragged to a new place, all with the interface's motion: presses spreading, the panels folding, the cards fading in and out](docs/demo.gif)

- **Ink that keeps up with the pen.** The program reads the pen as late as it can, so the stroke follows the tip
  (about 2.5 ms from pen sample to frame on a 60 Hz screen).
- **Your notes are real PDF annotations.** Ink and shapes are saved as standard Ink annotations and typed text as
  Free Text annotations, so any PDF reader shows them. Nothing else is kept, and nothing is uploaded.
- **Light on the machine.** Pages are drawn on the GPU, and an idle window costs no CPU.
- **Select, mark and search the text.** Select a PDF's text and copy it; highlight, underline or strike it out (saved as standard
  annotations); find any word with Ctrl+F.
- **Light or dark.** The interface follows the system's look or your choice, and the pages can go dark too (dark paper, light text;
  your file is never changed).
- **Pen, finger, mouse or keyboard.** Every feature works with just the left mouse button and keyboard at a desk,
  or just the stylus tip on a tablet.

## Where it runs

| | Status | Get it |
|---|---|---|
| **Windows 10/11** | the main platform: full pen support (pressure, eraser end, barrel button) | installer or portable zip |
| **Linux** (X11, Wayland) | works; the pen is read as basic mouse/touch for now (no pressure yet) | portable tarball |
| **Android** (arm64 tablets and phones) | test build: S Pen and finger work; it starts on a blank page, as everywhere; Open and Save use the system's file picker; copy, paste and links go through the system (written but not yet tried on a device) | APK |

## Get started

**Windows.** Download `InkPDF-Setup-<version>.exe` (per user, no admin rights) or `InkPDF-Portable-<version>.zip`
(unzip anywhere and run `InkPDF.exe`; with `portable.txt` beside it, settings are kept beside it too) from the
[releases page](../../releases). Both are unsigned, so SmartScreen may ask once.

**Linux.** Download `InkPDF-Portable-<version>-linux-x86_64.tar.gz`, unpack it and run `./inkpdf`. It needs a Vulkan
driver (`mesa-vulkan-drivers` on most distributions) and, on X11, `libxkbcommon-x11`.

**Android.** Download the APK (the CI run's `inkpdf-android-test-apk` artifact, or build it with
[tools/android](tools/android/README.md)) and install it. Documents are saved in the app's own folder
(`Android/data/app.inkpdf.test/files/Documents`). The folder button at the left of the toolbar opens a PDF through the system's file picker, Save As uses its "create document" screen, and "Open with InkPDF" from a file manager works.

**Using it.**

1. It opens on a blank page the size of your screen: start writing. To work on a PDF, drag it onto the window,
   use "Open with", or run `inkpdf file.pdf`.
2. Pick a tool in the middle bar (select text, pen, shapes, text, lasso, eraser, pointer), a colour and a thickness. The lasso
   picks up anything drawn (a loop, or a tap); a bar under the tools has duplicate, copy, paste and delete for what is picked up.
   The eraser takes away whatever it passes over.
3. Press the disk button (or Ctrl+S) to save. A new page asks where to save it, in the system's own Save As window (it opens in your Documents folder);
   a PDF you opened is updated in place.
4. The pages button opens an overview of every page and the table of contents: jump to a page, reorder, duplicate or
   delete pages. Every step can be undone.
5. The I-beam tool selects the page's text: drag over it, press twice for a word, three times for a line, then copy it or mark it from
   the bar under the tools. The magnifier (or Ctrl+F) searches: every place is lit on the page and listed.
6. The button at the top right, left of the mode button, is the look: each press goes from the system's to light to dark, and drops a card
   down with one switch, "Pages follow the theme", which makes the pages dark too when the look is dark. Both are remembered.

The full guide is [apps/inkpdf/installer/readme.txt](apps/inkpdf/installer/readme.txt).

## Pen, finger, mouse

InkPDF picks a **mode** from how the machine is used: **desktop** (keyboard and mouse in front of you) or **tablet**
(a slate). The button at the top right shows the mode and switches it; `--mode desktop|tablet` starts in one.

| | Stylus | Finger | Mouse (desktop mode) |
|---|---|---|---|
| **Draws, selects, erases** | tip draws (with the lasso tool it loops, with the eraser tool it erases); eraser end erases and side button makes a lasso, with any tool (on Android, an S Pen's one button erases) | pans and zooms; a tap follows a link (with the Select tool a stylus tap or a mouse click does too); pulling past the last page adds a page | left draws, right erases, middle lassos; the wheel scrolls |

- **Palm rejection is simple:** touch is off while the pen is near the screen (hovering or touching) and comes back the
  moment it leaves.
- Ctrl+Z undoes, Ctrl+Y or Ctrl+Shift+Z redoes, Ctrl+S saves; Ctrl+A, C, X, V and D work on the selection (desktop mode).
- Shapes are lines, polylines, polygons, splines, rectangles, ellipses and 2D/3D axes; each is edited by anchors that suit
  it. A line or curve is drawn by tapping: a tap adds a node, a double tap ends it, and a double tap near the first node
  closes it.

## Your files and settings

- Settings (colours, thickness, last tool, light or dark) are one small text file: `%APPDATA%\InkPDF\settings.txt` on Windows,
  `~/.config/InkPDF/settings.txt` on Linux, the app's private folder on Android. With `portable.txt` beside the program
  they are kept beside it instead.
- A saved PDF is the original with your annotations added; it opens anywhere.

```
inkpdf [file.pdf] [--fullscreen] [--smoothing 0..1] [--gpu integrated|discrete|auto] [--mode desktop|tablet|auto]
       [--record FILE] [--replay FILE]
```

`--record` saves everything you do (pen, touch, mouse, keys) as text and `--replay` plays it back: handy for reporting a bug.

---

## For developers

**Build** (recent stable Rust, 1.95 or newer, edition 2024). Windows also needs the Visual Studio build tools with a Windows
SDK; Linux needs a Vulkan driver to run.

```bash
cargo run --release -p inkpdf        # run it
cargo test -p inkpdf --lib           # unit and stress tests (Linux or Windows)
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

The PDF engine is [pdfcraft](https://github.com/storytold/pdfcraft), pinned by commit in [Cargo.toml](Cargo.toml).
Build output goes beside the repository (`../inkpdf-build`; `CARGO_TARGET_DIR` overrides it), never inside it.

**Packaging**

```bash
apps/inkpdf/installer/build.ps1             # Windows installer (needs Inno Setup)
apps/inkpdf/installer/build-portable.ps1    # Windows portable zip
tools/linux/build-portable.sh               # Linux portable tarball
tools/android/build-apk.sh                  # Android APK (needs the NDK and SDK; see tools/android/README.md)
```

**How it is organised.** The pen path is *wait for the display, read the pen, hand it to the editor, draw*, and everything that
differs between operating systems sits behind one `Platform` trait.

```
apps/inkpdf/src/
  platform/   per-OS code: windows/, linux, android/ (pen readers, folders, clipboard, fonts)
  input/      the input vocabulary (events, devices, gestures, palm rejection, record/replay)
  editor/     the session: routes input to the tool in use, the bars, the prompt, Save As, the pages overview
  tools/      one file per tool behind a Tool trait: pen, shape, text, pointer
  app/        the shell: window, frames, files, turning editor state into GPU data
  gpu/        wgpu renderer (pages, ink, interface)
  ink/, figure/, text/, page_table.rs, links.rs ...   the document model and what it is made of
apps/inkpdf-android/   the Android entry point      apps/inkpdf-thumb/   Windows Explorer thumbnails
tools/                 Android and Linux packaging, end-to-end checks, dependency graphs
```

Layering is enforced by a test (`src/architecture.rs`): nothing below the adapters touches a file, the environment, an OS, a PDF
crate, `wgpu` or `winit`. More: [ARCHITECTURE.md](apps/inkpdf/ARCHITECTURE.md) (layers, patterns, latency),
[docs/PORTING.md](docs/PORTING.md) (Linux and Android), `palette.rs` (every colour, by role), [docs/PDF-BACKEND.md](docs/PDF-BACKEND.md) (the PDF engine seam, the upgrade rule, the numbers), [docs/REFACTOR.md](docs/REFACTOR.md) and
[docs/SIMPLIFY.md](docs/SIMPLIFY.md) (design reviews), [docs/deps](docs/deps) (dependency graphs), [ROADMAP.md](ROADMAP.md),
[CHANGELOG.md](CHANGELOG.md).

**Testing.** `cargo test` runs the unit tests, the tool contract suite (the same hostile event streams against every tool),
the layering test and seeded stress storms (`INKPDF_STRESS=10 cargo test --release` runs them ten times larger).
`tools/e2e/linux-smoke.sh` runs the built app on a virtual display; `tools/e2e/run.ps1` drives it on Windows with a synthetic
stylus and finger (it takes over the mouse and keyboard while it runs). Windows' synthetic pen injector drops the eraser flag, so
the eraser end is covered by unit tests only: please try it on real hardware.

## Authors

Written by Rajat Mandal with [Claude](https://www.anthropic.com/claude) (Anthropic).

## Licence

Copyright (c) 2026 Rajat Mandal. MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). Built on
pdfcraft (same licences); see [NOTICE](NOTICE). The bundled Liberation Sans font is under the SIL Open Font License
(`apps/inkpdf/assets/fonts`).
