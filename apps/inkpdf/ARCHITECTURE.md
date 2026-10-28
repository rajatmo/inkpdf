# InkPDF architecture (v0.2.0)

A fast, pen-first PDF annotator for Windows. The application layer was rewritten in 0.2.0 from the plan
below; the 0.1.11 source is in this repository's history. The layering rule and the tool contract are
enforced by tests (`src/architecture.rs`, `src/editor/tests/contract.rs`).

## What was wrong with 0.1.x

| Smell | Where | Principle broken | Now |
|---|---|---|---|
| `Editor` was one 40-field struct doing routing, every tool, selection, text, drafts, prompt, toolbar | `editor/` | **S** | tools own their state; `Editor` routes and owns the window's chrome |
| `match self.tool` / `if tool == Text` in ~15 places; a new tool edited all of them | `pen.rs`, `mouse.rs`, … | **O** | one `ToolSet` match; everything else calls the `Tool` trait |
| One shared `Act` enum for every tool's gesture; the renderer inspected it to paint overlays | `act.rs`, `ink_layer.rs` | **O**, **D** | each tool has a private gesture enum and paints itself through `Painter`; `Act` is only the chrome's |
| Device rules scattered as `if`s | `pen.rs`, `mouse.rs` | **S**, **O** | `Tool::policy()` returns `InputPolicy` (data); one routing function reads it |
| Public `want_*` flags the shell polled and reset | `Editor` | **I** | `Request` queue the shell drains |
| Tests reached into `Editor` internals | `editor/tests` | **D** | tests use accessors (`ed.draft()`, `ed.tools.shape.controlling()`) |

## Layers (dependencies point inward only)

```
 adapters  ─────────►  session  ─────────►  tools  ─────────►  domain
 app (winit shell),     editor/ (Editor:     tools/ (Tool trait,  ink, figure, geometry, smooth,
 gpu, win_pointer,      routing, chrome,     Pen, Shape, Text,    text, laser, view, colour, links,
 doc, native            history, prompt)     Pointer, gestures)   workspace, requests, input
```

* **domain** — no window, GPU, file or clock. `workspace.rs` is the document state the tools share
  (ink, selection, view, layout, styles, redraw flag); `requests.rs` is the queue of asks to the shell.
* **tools** (`src/tools/`) — `Tool` trait (`policy`, `pointer`, `owns`, `busy`, `end`, `settle`, `leave`,
  `cancel`, `key`, `undo`, `redo`, `restyled`, `drag_offset`, `paint`); `PenTool`, `ShapeTool` (with
  `anchors` and `draft` modules), `TextTool`, `PointerTool`; shared gesture parts in `gestures.rs`
  (`Mover`, `Eraser`, `Lasso`); `paint.rs` (`Painter` trait and line helpers). A tool never names another
  tool, the toolbar, the editor, the window or the GPU.
* **session** (`src/editor/`) — `Editor` is the facade the shell talks to. `routing.rs` hands pointer
  and keyboard input to the tool in use according to its policy and exposes read accessors; `pen.rs`,
  `mouse.rs`, `touch.rs`, `navigation.rs` do the window's own work (toolbar presses, panning, links,
  pull-for-a-page); `toolbar_ui.rs`, `prompt.rs`, `commands.rs`, `select.rs` are chrome and history.
* **adapters** — everything touching the OS. The session knows winit's event *types* (the vocabulary of
  input) but never the window, GPU, files or the pointer layer.

### The layers, as enforced (`src/architecture.rs`)

Every top-level module has a layer, and the tests read the sources (production lines only, whatever the position of
the `#[cfg(test)]` items) to check the real module graph against this table. A module may use its own layer and the
ones below; no two modules may reach each other, directly or through others.

| Layer | Modules |
|---|---|
| adapter | `app`, `gpu`, `win_pointer`, `native`, `posture`, `doc`, `ink_io`, `tiles`, `thumbnail` |
| session | `editor` |
| tools | `tools` |
| ui | `toolbar` |
| domain | `tool`, `input`, `requests`, `workspace`, `selection`, `settings` |
| model | `figure`, `ink`, `laser`, `links` |
| base | `plane`, `scene`, `clock`, `geometry`, `colour`, `coords`, `view`, `text`, `smooth`, `clipboard` |

* `plane` holds the point type `P` and the plane arithmetic (distances, polygon tests, simplify, thin, `clamp_into`)
  that `ink`, `figure`, `smooth`, `text` and `laser` all stand on; `ink` re-exports the common ones. `geometry` is only
  the `Shape` enum; the polyline a shape makes is `figure::shape_points`. `figure::NodeAction` is what can be done to
  a point of a shape. None of these depend on anything above them, which is what removed the cycles
  `ink`/`figure`/`geometry` and `toolbar`/`selection`/`workspace`/`requests`.
* `tool` holds which tool is in use (`Tool`), how each writes (`ToolStyle`), the colour slots and the thickness ranges,
  and `requests` holds `Choice`, the answer to the unsaved-changes prompt: plain data that the document state, the saved
  settings and the toolbar all use, so none of the first two reaches into the toolbar any more.
* There are no upward edges and no cycles left: `KNOWN_EXCEPTIONS` in the test is empty, and an edge that points up has to be
  added there, with a reason, or fixed.
* `scene` holds what is drawn as plain data (round-ended lines and rounded rectangles); the toolbar and the tools describe
  what they want in it and `gpu` draws it. `clock` is the performance-counter clock; the editor takes it as a function
  (`Editor::set_clock`), so tests can give it another.

### Simplification plan (status): done

1. Layer table, ratchet and cycle test.
2. The `ink`/`figure`/`geometry` and `toolbar`/`selection`/`workspace` cycles (the base module `plane`).
3. `Tool`, `ToolStyle`, the palette and the ranges moved out of `toolbar` (into `tool`), and `Choice` (into `requests`).
4. `toolbar` describes what it draws in `scene` types, which `gpu` draws; it no longer knows `gpu`.
5. The tools share what they have in common:
   * `tools::gestures::Pick` is erase / select by loop or tap / drag the selection, which the pen, shape and text tools
     each used to carry their own copy of;
   * `Tool::actions`, `action_enabled` and `act` let a tool offer buttons of its own to the bar (`ToolAction`), so the bar
     does not know that shapes have points;
   * `tool::Tool` carries what is true of each tool (`ALL`, `kind`, `makes_items`, `name`), and `Kind` lives beside it.
6. The editor:
   * the window's own controls are `editor::Chrome` (the prompt, the panel and its colour editor, the hover, the press under
     way), whose small state machines take the document state as an argument; the `Editor` routes input and owns one;
   * `Editor` derefs to the document state read-only outside tests, so only the editor and the tools change it, and the
     shell asks for changes in the editor's terms (`set_dpr`, `document_saved`, `requests()`, ...);
   * the editor reads the time from its own clock, not from the pointer layer.
## SOLID, concretely

* **S** — `Tool`s (one interaction style each), `InputPolicy` (who may do what), `Workspace` (shared
  document state), `Requests` (asks of the shell), `Editor` (routing + chrome).
* **O** — a new tool is one file implementing `Tool` plus one variant in `ToolSet`'s two `match`es. The
  shell names no tool type (checked by a test).
* **L** — every tool honours the contract suite (`contract.rs`): idle when new, owns only what it began,
  cancel leaves the document untouched, end/settle/leave are idempotent, devices outside the policy never
  reach it, and 4 000 hostile events leave it sound.
* **I** — a tool sees `Workspace` (not the editor), `Painter` (not the GPU), and may ask the shell only
  through `Request`.
* **D** — the renderer depends on `Painter`/`Tool::paint`, not on any tool's internals.

## Patterns

| Pattern | Where |
|---|---|
| **Strategy** | `Tool` objects; `InputPolicy` as per-tool data |
| **State** | each tool's private gesture enum (`Draw`, `Drag`, `Control`, `DraftTap`, `Place`, `Grab`…) |
| **Command** | `ink::Cmd` — the undo log |
| **Facade** | `Editor` |
| **Registry** | `ToolSet::get/get_mut` (total: no `Option`) |
| **Observer (queue)** | `Request` queue drained by `app::files::service` |
| **Composition** | `Mover`, `Eraser`, `Lasso` held by Pen and Shape (and `Mover` by Text) |
| **Flyweight** | text bitmap cache (kept) |
| **Builder** | `Draft`, a polyline built node by node (kept) |
| **Incremental computation** | the live stroke's smooth curve is extended segment by segment (`curve_segments`) |
| **Ports & Adapters** | `win_pointer` (Win32 → `PointerEvent`), `gpu` (scene → wgpu), `Painter` |

## Modes and device rules (`Tool::policy(mode)`)

The rules depend on a `Mode`, which follows the machine's posture (`src/posture.rs`):

| Mode | When | Mouse | Keyboard |
|---|---|---|---|
| **Desktop** | not in slate posture, or not a machine that has one (every desktop PC) | stands in for a stylus on the pen, shape and pointer tools; text as itself | shortcuts (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z, Ctrl+S, Ctrl+A/C/X/V/D), the selection's keys and typing |
| **Tablet** | the OS reports slate posture on a portable machine with a touch or pen digitizer | scrolls only | typing (physical or Windows' touch keyboard), no shortcuts |

| Tool | Stylus | Mouse (desktop) | Mouse (tablet) | Touch | Keyboard |
|---|---|---|---|---|---|
| Pen | draw, erase (eraser end), lasso (barrel) | as a stylus: left tip, right eraser end, middle barrel | scroll | pan/zoom | desktop: the selection's keys |
| Shape | select, draw, erase, lasso (tap-drawn polylines and splines) | as a stylus | scroll | pan/zoom | desktop: the selection's keys, Backspace on a point |
| Pointer | point | as a stylus (dot follows the mouse) | scroll | pan/zoom | none |
| Text | desktop: none; tablet: place/select/move text | place/select/move text | scroll | pan/zoom | types, caret, selection |

Every device still presses the window's chrome (toolbars, panel, prompt, the bar under the tools); the wheel
always scrolls.

### Create, select, move, restyle, copy, erase, delete: whatever the device

The promise: with a tool chosen, every item it made supports all of those, with only the devices the mode
guarantees (desktop: left button and keyboard; tablet: the stylus tip). `editor/tests/capability.rs` states it per
tool and mode and runs it through those devices alone; the stylus's barrel button and eraser end and the mouse's
right and middle buttons are shortcuts and no test may need them.

* **Intent** (`input::Intent`: draw, select, erase) is what the plain tip does. `Workspace::role` is the one
  place that combines it with the pen's own buttons (a held button wins), so every tool that takes a stylus gets
  select and erase for free: the pen and shape tools in `down`, the text tool in `press_at`.
* **The selection's operations** live once, on the workspace (`selection.rs`): select all, delete, duplicate,
  copy, cut, paste, nudge, restyle. Each is one undo step (`InkStore::add_copies`, `replace_many` with its
  `Cmd::Restyle`, which coalesces a slider drag into one step). Keys, shortcuts and the buttons of the bar call
  these; nothing is implemented twice.
* **Tool scope** (`selection::Kind`): selecting, erasing, select-all and paste only touch the tool's own kind of
  item (handwriting, shapes, text boxes), so tools stay independent and a loop cannot grab what another tool made.
* **The bar** (`toolbar/context_bar.rs`, `editor/context.rs`): buttons for the intents, the selection operations, the
  points of the shape being edited (`selection::NodeAction`, done by `ShapeTool::node_action`, the same edits the
  barrel button and a double tap make), the five colours, and the settings button. It is a chrome interaction
  (`Act::Context`) like the toolbar's, so every device presses it. The tool's panel is not a separate pop-up: it
  unfolds from the bar (`ContextBar::draw` draws bar and panel as one card; `Editor::drawer` is the card's rectangle
  for hit testing, where a press does nothing), and a tool's own button only chooses the tool. On a window too narrow
  for five colours they are one dot that opens the panel (`ContextBar`'s `compact`).
* **Keys** reach a tool as `tools::Key`. `tools/shortcuts.rs::selection_key` (Delete, Escape, arrows) is shared by the
  pen, shape and text tools; Ctrl+letter shortcuts are the session's (`Editor::shortcut`, desktop only) and go to
  the text tool as `Key::Chord` while a box is being edited.
* **Text editing** is `TextEdit` (text, caret, selection anchor) in the text tool plus pure layout in `text.rs`
  (`caret_at`, `index_at`, `vertical`, `selection_rects`, `follow_wrap`). The system clipboard is a domain trait
  (`clipboard::TextClipboard`) held by the workspace; `native::SystemClipboard` is the Windows adapter, and tests use
  the in-memory one.

How it fits the layers: a tool states a `Tool::policy(mode)` (an `InputPolicy` with a `MouseUse`: `Scrolls`,
`Itself` or `AsStylus`) and never learns the mode. The editor's router hands a mouse to a stylus tool as a
stylus with an id of its own, so no tool has a mouse code path. The mode is the session's (`Editor::set_mode`
ends whatever was under way first); the shell sets it from `posture::detect()` at start and again when
Windows sends `WM_SETTINGCHANGE` "ConvertibleSlateMode", unless `--mode desktop|tablet` forced it. The toolbar's
`Item::ModeSwitch` (right pill, left of full screen) both shows the mode (`State.mode`: a monitor, or a tablet in
the accent colour) and flips it through the same `set_mode`; it is not saved, and a posture change decides again.

`SM_CONVERTIBLESLATEMODE` alone cannot be trusted: an ordinary desktop PC reports 0 ("slate"). So `posture::decide`
also needs a touch or pen digitizer and a platform role of mobile or slate (`PowerDeterminePlatformRole`). The
rule is a pure function with a test per kind of machine; the flip itself cannot be tested without a convertible.

## Latency (input → pixels)

1. **Late sampling**: wait for the display (`Gpu::acquire`), *then* drain the pointer queue.
2. **Prediction**: the live stroke is extended 12 ms from the last samples.
3. **Incremental curve**: a frame re-smooths only the last few segments of the live stroke, not all of it
   (proved equal to smoothing it whole, `smooth::tests` and `editor::tests::pen`).
4. **No per-frame allocation of the overlay buffers**: they are owned by the shell and reused.
5. **Pen-down frames do not wait for tiles**: uploads are capped at 1 ms (4 ms otherwise).
6. **Incremental GPU appends** for committed ink.

Measured with the synthetic pen (`inkpdf --stats`, 830–860 frames): pen sample → frame submitted
p50 2.6–2.8 ms, p95 4.3–4.4 ms, against 2.5 / 4.4 / 11.1 ms before the rewrite. The newest sample is
already 2.8 ms old when the frame reads it, so the frame's own work adds ~0: what remains is the input
delivery and the display clock, not code that can be sped up here. The p99 swings between runs (11–14 ms)
with whether a frame lands on a vblank boundary.

## Testing

* Domain: unit and property tests.
* Tools: behaviour tests through `Editor` (`editor/tests/*`) and the contract suite.
* Architecture: `src/architecture.rs` fails if the domain imports tools/editor/adapters, a tool imports
  the editor, the toolbar, an adapter or another tool, the session imports the GPU/shell, or the shell names
  a tool type (shown to fail on a real violation).
* Stress: seeded storms (all tools, undo, pages, save/reopen) at `INKPDF_STRESS=10`.
* Real input (`tools/e2e/`, `run.ps1`, about half a minute): two scripts drive the built exe, in four app starts in all,
  with a synthetic stylus and finger, the real mouse and keyboard, and check pixels and the title. They prove only
  what the other tests cannot see: that each device reaches the app and does its work, that the tools draw on the
  screen, that pages and the saved file come out right, and that the mode button and the posture message work.
  The rules themselves (every device in every tool and mode, every edge) are the unit and stress tests' job.
  Known limit: Windows' synthetic pen injector drops the eraser flag on this OS build, so the eraser end is
  covered by the unit and stress tests only.
