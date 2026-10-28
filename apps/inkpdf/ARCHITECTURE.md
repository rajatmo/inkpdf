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

## Device rules (`Tool::policy`)

| Tool | Stylus | Mouse | Touch | Keyboard |
|---|---|---|---|---|
| Pen | draw, erase, lasso | scroll | pan/zoom | none |
| Shape | select, draw, erase, lasso (tap-drawn polylines and splines) | scroll | pan/zoom | none |
| Pointer | point | scroll | pan/zoom | none |
| Text | none | scroll, place/select/move text | pan/zoom | types |

Every device still presses the window's chrome (toolbars, panel, prompt).

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
* Real input (`tools/e2e/`, run with `run-all.ps1`): scripts drive the built exe with a synthetic stylus/finger, the real mouse and keyboard and
un-all.ps1): scripts drive the built exe with a synthetic stylus/finger, the real mouse and keyboard and
  check pixels and the title's changed mark: the device-rule matrix (every tool × stylus, barrel lasso,
  mouse click/drag/wheel, finger drag/pinch, keyboard), the per-tool scenario scripts, and a seeded storm.
  Known limit: Windows' synthetic pen injector drops the eraser flag on this OS build, so the eraser end is
  covered by the unit and stress tests only.
