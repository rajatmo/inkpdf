# InkPDF

A fast, pen-first PDF annotator for Windows. Open a PDF, write on it with a stylus, save it back. No account,
no library, no sync: just the page and your pen.

- **Low latency.** The frame waits for the display, *then* reads the pen, so the ink carries the newest
  samples; the live stroke is extended a few milliseconds ahead of the pen. Pen sample to frame submitted is
  about 2.5 ms median on a 60 Hz panel.
- **Real PDF annotations.** Ink and shapes are saved as standard Ink annotations and typed text as Free Text
  annotations, so any PDF reader shows them. Nothing else is kept.
- **Light on the machine.** Rendering is on the GPU (wgpu, DirectX 12), tiles stream in the background, and
  an idle window costs no CPU.
- **Windows only**, on purpose: the pen is read from Windows pointer messages (pressure, eraser end, barrel
  button, batched history), which other toolkits flatten.

## Tools and input

Each kind of input does one thing, and what it does depends on the tool and on the **mode**. InkPDF picks the
mode itself from how the machine is being used: **desktop** (a keyboard and mouse in front of you) or
**tablet** (a convertible turned into a slate, or a tablet). It reads the posture Windows reports, so a
desktop PC is always in desktop mode; `--mode desktop|tablet` forces one.

| Tool | Stylus | Mouse, desktop mode | Mouse, tablet mode | Touch | Keyboard |
|---|---|---|---|---|---|
| **Pen** | draw; eraser end erases strokes; barrel button lassos | as a stylus: left button draws, right erases, middle lassos | scroll | pan, zoom | none |
| **Shape** | select, draw, erase, lasso | as a stylus | scroll | pan, zoom | none |
| **Text** | desktop: none; tablet: place, edit and move text | click to place, edit and move text | scroll | pan, zoom | types |
| **Pointer** | a red dot and fading trail, for presenting | as a stylus (the dot follows the mouse) | scroll | pan, zoom | none |

In desktop mode the keyboard also has Ctrl+Z (undo), Ctrl+Y or Ctrl+Shift+Z (redo) and Ctrl+S (save). The
wheel scrolls in both modes.

Shapes are lines, polylines, polygons, splines, rectangles, ellipses and 2D/3D axes, each edited by the
anchors that suit it (nodes, tangent handles, diagonals, bounding corners, axis ends). Line and curve are
drawn by tapping: a tap adds a node, a double tap ends the stroke, a double tap near the first node closes
it. A finger also follows links and, pulled past the last page, adds a blank page. See
[apps/inkpdf/installer/readme.txt](apps/inkpdf/installer/readme.txt) for the full user guide.

## Install

Download the installer (`InkPDF-Setup-<version>.exe`, per-user, no admin rights) or the portable zip
(`InkPDF-Portable-<version>.zip`, one exe with the C runtime built in; with `portable.txt` beside it the
settings are kept beside it too) from the releases page. Both are unsigned, so SmartScreen may ask once.

```
inkpdf [file.pdf] [--fullscreen] [--smoothing 0..1] [--gpu integrated|discrete|auto] [--mode desktop|tablet|auto]
```

## Build

You need Windows 10/11, a recent stable Rust (1.95 or newer; edition 2024), and the Visual Studio build tools
with a Windows SDK.

```powershell
cargo run --release -p inkpdf                 # run it
cargo test                                    # 396 unit and stress tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

The PDF engine comes from [pdfcraft](https://github.com/storytold/pdfcraft), pinned by commit in
[Cargo.toml](Cargo.toml), so the first build fetches it from GitHub. Build outputs are large; consider
`CARGO_TARGET_DIR` outside any cloud-synced folder.

Packaging (both write to `apps/inkpdf/installer/Output/`):

```powershell
winget install JRSoftware.InnoSetup              # once
apps\inkpdf\installer\build.ps1                  # InkPDF-Setup-<version>.exe (with the Explorer thumbnail handler)
apps\inkpdf\installer\build-portable.ps1         # InkPDF-Portable-<version>.zip (builds with the C runtime linked in)
```

The version comes from `apps/inkpdf/Cargo.toml`. The icons are drawn in TikZ; their SVGs are checked in, and
`apps/inkpdf/icons/build.ps1` rebuilds them (needs MiKTeX or TeX Live with `dvisvgm`).

## How it is organised

```
apps/inkpdf/        the app
  src/tools/          one file per tool behind a Tool trait: pen, shape (anchors, draft), text, pointer
  src/editor/         the session: routes input to the tool in use, toolbar, panel, prompt, history
  src/app/            the shell: window, frames, files, GPU overlay drawing
  src/gpu/            wgpu renderer (pages, ink capsules, UI panels)
  src/ink/, figure/   the ink store with undo, and editable shapes
  src/bin/            inkpdf-bot (synthetic pen/touch for tests), inkpdf-gen (test PDFs)
  installer/          Inno Setup script, portable packaging, icon
apps/inkpdf-thumb/  Explorer thumbnail handler (a COM DLL that asks inkpdf.exe to draw the page)
tools/e2e/          real-input checks that drive the built app
```

[apps/inkpdf/ARCHITECTURE.md](apps/inkpdf/ARCHITECTURE.md) explains the layers, the patterns, the device rules
and the latency work. The layering is enforced by a test (`src/architecture.rs`).

## Testing

`cargo test` runs the unit tests, the tool contract suite (the same hostile event streams against every
tool), the layering test, and seeded stress storms. `INKPDF_STRESS=10 cargo test --release` runs the storms
ten times larger.

`tools/e2e/run-all.ps1` goes further and drives the *built app* with a synthetic stylus and finger, the real
mouse and keyboard, and checks pixels and the window's changed mark. It takes over the mouse and keyboard for
several minutes, so run it on a machine you are not using. Windows' synthetic pen injector drops the eraser
flag, so the eraser end is covered by the unit and stress tests only; please try it on real hardware.

## Authors

Written by Rajat Mandal with [Claude](https://www.anthropic.com/claude) (Anthropic).

## Licence

Copyright (c) 2026 Rajat Mandal. MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). Built on
pdfcraft (same licences); see [NOTICE](NOTICE).
