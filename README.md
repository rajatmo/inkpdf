# InkPDF

A fast, pen-first PDF annotator for Windows. Open a PDF, write on it with a stylus, save it back. No account,
no library, no sync: just the page and your pen.

![InkPDF: pen strokes, selecting, colouring, duplicating, deleting and erasing from the bar under the tools, the settings unfolding from it, a shape edited by its anchors, typed text, the desktop/tablet mode button, and the pages overview: every page with the table of contents beside it, a jump to a chapter, pages chosen, duplicated, undone and dragged to a new place, all with the interface's motion: presses spreading, the panels folding, the cards fading in and out](docs/demo.gif)

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
desktop PC starts in desktop mode; `--mode desktop|tablet` starts in one. A button at the top right, left of
full screen, shows the mode (a monitor, or a tablet in blue) and switches it whenever you like; if the machine is
then turned into a tablet or back, InkPDF looks again and follows.

| Tool | Stylus | Mouse, desktop mode | Mouse, tablet mode | Touch | Keyboard |
|---|---|---|---|---|---|
| **Pen** | draw; eraser end erases strokes; barrel button lassos | as a stylus: left button draws, right erases, middle lassos | scroll | pan, zoom | desktop: works on the selection (below) |
| **Shape** | select, draw, erase, lasso | as a stylus | scroll | pan, zoom | desktop: works on the selection (below) |
| **Text** | desktop: none; tablet: place, edit and move text | click to place, edit and move text | scroll | pan, zoom | types, moves the caret, selects |
| **Pointer** | a red dot and fading trail, for presenting | as a stylus (the dot follows the mouse) | scroll | pan, zoom | none |

In desktop mode the keyboard also has Ctrl+Z (undo), Ctrl+Y or Ctrl+Shift+Z (redo) and Ctrl+S (save). The
wheel scrolls in both modes.

### Select, erase, copy, restyle: with whatever you have

With a tool chosen, every item it made can be created, selected, moved, restyled, duplicated, copied, pasted,
erased and deleted, and each of those undone, with only what the mode guarantees: at a desk the left mouse
button and the keyboard, as a tablet the tip of the stylus. The stylus's barrel button and eraser end and the
mouse's right and middle buttons are shortcuts, never the only way.

A bar under the tools has the buttons: **draw, select, erase** (what the plain tip, or the left mouse button,
does), then **duplicate, copy, paste, delete**, the buttons for a **point** of the shape being edited (add one,
take it away, smooth or sharpen it), the **five colours** and a **settings** button. With select chosen, a tap on an
item or a loop round several selects them, and a drag moves them. A colour tapped in the bar is the tool's colour and
the colour of what is selected, in one tap; the settings button unfolds the tool's panel (thickness, smoothing, the
shape) from the bar as one card, and what is chosen there changes what is selected too. (A tool's own button only
chooses the tool; on a narrow window the five colours are one dot that opens the panel.) A tool works on what it made: the pen on handwriting, the shape tool on shapes, the text tool on text
boxes, so a loop drawn round a shape with the pen does not pick it up.

At a desk the keyboard does the same: Ctrl+A selects all of the tool's items on the page, Ctrl+C, Ctrl+X, Ctrl+V and
Ctrl+D (duplicate), Delete, Esc (let go), the arrows (Shift for a bigger step), and Backspace takes away the
point of a shape that was pressed last. In a text box the caret goes where it is pressed, the arrows, Home and End
move it (with Shift they select, and so does a drag), and Ctrl+A, C, X and V work on the text with the Windows
clipboard, which also takes the text of a copied box and gives a new box the text copied anywhere.

Shapes are lines, polylines, polygons, splines, rectangles, ellipses and 2D/3D axes, each edited by the
anchors that suit it (nodes, tangent handles, diagonals, bounding corners, axis ends). Line and curve are
drawn by tapping: a tap adds a node, a double tap ends the stroke, a double tap near the first node closes
it. A finger also follows links and, pulled past the last page, adds a blank page. The + button adds a page
after the one you are looking at, and the bin button next to it deletes the one you are looking at, at once and
without asking (the last page stays). Every change to the pages is a step of the history: undo brings a deleted page
back with its ink and links, until the file is saved.

The pages button (four small pages, at the end of the left toolbar) opens the **pages overview**: a card over the
document with a picture of every page, ink and all, its number, and, if the PDF has an outline, its table of contents
on the left (the section being read is lit; tap an entry to go there, its arrow to fold it). Tap a page to go to it.
The check mark is edit mode: tap pages to choose them, then duplicate, move them earlier or later, or delete them from the
bar under the pages; or drag a page (or all the chosen ones) to where it should go. The number under each page opens
its menu (add a page before or after, duplicate, delete). Everything works with the mouse, the stylus tip or a finger,
and each step undoes. See [apps/inkpdf/installer/readme.txt](apps/inkpdf/installer/readme.txt) for the full user guide.

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
cargo test                                    # unit and stress tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

The PDF engine comes from [pdfcraft](https://github.com/storytold/pdfcraft), pinned by commit in
[Cargo.toml](Cargo.toml), so the first build fetches it from GitHub.

Nothing is built or written inside the repository. By default the build output goes to `inkpdf-build`, the
installer and portable zip to `inkpdf-dist`, and the end-to-end scratch files to `inkpdf-e2e`, all next to the
repository folder (`.cargo/config.toml` and the scripts say so). Set `CARGO_TARGET_DIR`, `INKPDF_DIST` and
`INKPDF_E2E_DIR` to put them elsewhere.

Packaging (both write to `inkpdf-dist`, beside the repository):

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

`tools/e2e/run.ps1` goes one step further and drives the *built app* with a synthetic stylus and finger, the real
mouse and keyboard, and checks the pixels and the window title. It is deliberately small, two scripts and four
app starts, about half a minute: it only proves what the other tests cannot see (that each device reaches the app,
that things draw on screen, that pages and the saved file are right, that the mode button and the posture message
work). It takes over the mouse and keyboard while it runs, so leave the machine alone. Windows' synthetic pen
injector drops the eraser flag, so the eraser end is covered by the unit and stress tests only; please try it on
real hardware.

## Authors

Written by Rajat Mandal with [Claude](https://www.anthropic.com/claude) (Anthropic).

## Licence

Copyright (c) 2026 Rajat Mandal. MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). Built on
pdfcraft (same licences); see [NOTICE](NOTICE).
