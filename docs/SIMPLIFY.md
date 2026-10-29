# Simplification survey: abstractions, dependency graph, cohesion

Written 2026-10-29 on `linux-android-port`, after R1 to R9. Numbers are counted from the code and `docs/deps/full.dot`.

## TL;DR

**Status: S1 to S7 are done; see "Result" below.** The rest of this page is the survey as it was written.

1. **The dependency graph is already healthy.** 35 modules, 127 edges, no cycles, nothing points up a layer, and only
   **1 of 127 edges** breaks Martin's Stable Dependencies Principle (`ink -> figure`, 0.23 vs 0.25: noise). There is no graph
   to untangle; the gains left are in **cohesion** (what lives together), not in fewer arrows.
2. **Latency is not at stake in any item below.** The pen path is `pump -> drain_into -> Editor::handle -> scene -> GPU`.
   It has 3 trait-object calls per frame (`PointerSource`) and 1 per event (`Tool`). Measured in release after R1 to R9:
   **move 0.89 µs mean, 2.2 µs p99** per event, against a frame of 8 333 µs at 120 Hz (0.01 %). Everything below is compile-time
   moves of code between files or layers.
3. **Seven changes, by effort:** S1 and S7 take minutes; S2, S3 and S6 about an hour each; S4 and S5 a couple of hours. All are
   low risk and none changes behaviour. S8 lists what *not* to do.

## How cohesion and coupling were judged

* Coupling: Martin's instability `I = Ce / (Ca + Ce)` (out-edges over all edges). A module should depend only on modules at
  least as stable as itself (SDP). Layers were checked by `architecture.rs`.
* Cohesion: *does everything in this module change for the same reason?* (Parnas 1972; Stevens, Myers, Constantine 1974;
  Martin's Common Closure Principle). Signs of low cohesion: a module that imports a crate no other module of its layer
  needs, a file whose parts have separate clients, a name like `generic`.

Fan-in and fan-out (top of the list; `docs/deps/full.svg` has the rest):

| Module | out | in | Reading |
|---|---|---|---|
| `editor` | 20 | 2 | the session: it must know the tools, the chrome and the document state; high out is its job |
| `app` | 18 | 0 | the shell, same |
| `toolbar` | 12 | 2 | draws; reads model types |
| `tools` | 11 | 2 | strategies over `workspace` |
| `ink` | 3 | 10 | the most depended-on model: stable, small surface |
| `view` | 0 | 9 | pure base, ideal |

## Findings

### S1. Two identical enums and an identity function  *(10 minutes, zero risk)*

`input::PenTool { Draw, Erase, Lasso }` and `input::Role { Draw, Erase, Lasso }` are the same type; `role()` converts one to
the other variant by variant. `PenTool` also shares a name with `tools::PenTool` (the pen drawing tool), which misleads a reader.
*Fix:* delete `PenTool`; `PenButtons::tool()` returns `Role`; delete `role()`.

### S2. PDF parsing sits in the model layer  *(small, low risk)*

`links.rs` (model) holds the in-memory link model (hit, remap, restore) **and** a 155-line PDF reader (`read`, `NamedDests`,
`target_of`). `outline.rs` (model) holds the model **and** `read(&Document)`. `coords::PageGeom::from_info` (base) takes a
`pdfcraft_render::PageInfo`. So the model and base layers import the PDF stack, though `ink_io` already is the adapter that
reads and writes PDF annotations. Separately, `links::safe_url` (a policy: only http, https, mailto) is called by all three
platform `open_url` implementations (`platform -> links` is the only reason that edge exists).
*Fix:* move the readers next to `ink_io` (as `pdf_read` or inside `ink_io`), give `PageGeom` a plain constructor that `doc` calls,
and call `safe_url` **once** in the shell before `Platform::open_url`. *Effect:* base, model and domain depend on no PDF crate; a
platform cannot forget the check; fewer duplicated lines (3 call sites become 1).

### S3. `input` holds three things  *(small, low risk)*

| Part | Lines | What it is |
|---|---|---|
| vocabulary + meaning: `event`, `mod` (Device, Phase, Mode, Role, Intent), `gestures`, `palm`, `predict` | ~750 | the domain's idea of input |
| `motion.rs` | 712 | the decoder from Android's `MotionEvent` fields to events: **adapter code**, kept here only so it tests on Linux |
| `record.rs`, `contract.rs` | 580 | stream tooling (record/replay, the stream validator) |

*Fix:* move `motion.rs` to `platform/` as an always-compiled module (not cfg'd, so its tests still run everywhere). Keep
`record`/`contract` where they are (they speak the vocabulary and nothing else). Also rename `platform/generic.rs` (171 lines:
XDG parsing, a stub clipboard, motion prefs, an error box) into what it is: `unix_dirs.rs` for folder discovery and
`stubs.rs` for the no-op services. A module called `generic` has no single reason to change.

### S4. `doc.rs` mixes three jobs  *(medium, low risk)*

1028 lines: (a) **page table bookkeeping**: `Slot`, `Buried`, `apply(PageEdit)`, `doc_of_file`, the mapping between pages on screen
and pages of the file, about 20 of its tests; (b) the **render pool** (`render_page`, `set_queue`, `try_recv`); (c) **open and save**.
(a) is pure logic that needs no renderer, yet its tests build real PDFs. *Fix:* extract a `PageTable` (model layer), leaving `Doc`
as render pool plus save. The page-edit feature is already spread over `pages`, `ink/store`, `editor/pages`, `doc`, `links`, `outline`
(each remaps its own data): giving it one pure home makes that spread explicit and testable.

### S5. The shell does three jobs through one `impl State`  *(medium, low risk)*

`app` is 2 300 lines in 12 files, all `impl State`: (a) window and event loop (`mod`, `frame`, `translate`); (b) the **document
workflow** (`files.rs`: `Then`, open, save, close, serving `Request`s; 300 lines); (c) **presentation**: turning editor state into GPU
data (`ink_layer`, `text_layer`, `overview`, `pages`; about 900 lines). After R6 the fields of (c) already live in `UiLayers`.
*Fix:* make `UiLayers` own the methods of (c) (`sync_committed(&mut self, ed: &Editor, gpu: &mut Gpu)`), so the presenter is one
struct testable without a window, and `State` is only the loop and the workflow. No behaviour or order of calls changes.

### S6. `text.rs` is font, layout and rasteriser  *(small, mechanical)*

731 lines in a base module: font loading (`set_font`, `Face`), text layout (`wrap`, `measure`, `caret_at`, `index_at`,
`selection_rects`: used by tools and the editor), and the scanline rasteriser (`raster`: used only by the shell and thumbnails).
Different clients, different reasons to change. *Fix:* `text/{font, layout, raster}.rs`; no API change.

### S7. Tooling: the graph overstates one edge  *(5 minutes, zero risk)*

`tools/deps/graph.py` draws `platform -> editor`, which comes from `platform/contract.rs`, a `#[cfg(test)]` module.
`architecture.rs` already skips test code; make the script do the same, so the diagram and the test agree.

### S8. Do not do: more abstraction in the editor

* **A `Modal` trait for prompt, Save As and overview.** After R2 the three modals dispatch through one `Editor::modal()` match.
  A trait would force the overview (which needs the tools, the ink, the graves and the clock) to be handed a wide context, which
  would be a god-parameter. The enum match is clearer and costs a jump, not a vtable.
* **Splitting `Platform` into `Environment` and `Services`** (Interface Segregation). One client (`State`) uses all 12 methods.
  Splitting for its own sake adds a type and no isolation.
* **A render-model boundary between `Editor` and the shell.** The shell reads 40 distinct editor members; a frozen `Frame` struct
  would copy per frame what is read in place today. The `Deref<Workspace>` is a leak, but its price (an allocation or copy on the
  pen path, or a big read-only view type) is worse than the leak.
* **More tool indirection.** Adding a tool touches 8 files that name tools (`tool`, `workspace`, `settings`, `editor/routing`, four in
  `toolbar`). Four of them are icons and panels, which are per-tool by nature. Leave it.

## Order, effort and what each one buys

| # | Change | Effort | Risk | Buys |
|---|---|---|---|---|
| S1 | one enum for pen roles | 10 min | none | -1 type, -1 fn, no name clash |
| S7 | graph script skips test code | 5 min | none | diagram matches the test |
| S2 | PDF readers to the adapter, `safe_url` once | 1 h | low | pure inner layers; one URL check |
| S3 | `motion.rs` to `platform/`; rename `generic.rs` | 1 h | low | `input` is only the vocabulary |
| S6 | split `text.rs` | 30 min | low | three clients, three files |
| S4 | extract `PageTable` | 2 h | low | page logic testable with no PDF |
| S5 | `UiLayers` owns the presenter | 2 h | low | `State` is loop + workflow |

## Result (S1 to S7 done, 2026-10-29)

All seven landed, one commit each, with Linux tests (713), clippy `-D warnings` on Linux, the Windows cross-target and Android, the
Linux smoke test and the graph check green.

| Measure | Before | After |
|---|---|---|
| modules / edges in `docs/deps/full.dot` | 35 / 127 | 38 / 133 |
| edges that break the Stable Dependencies Principle | 1 (`ink -> figure`) | 1 (same) |
| editor cost per pen move, release (mean / p99) | 0.89 / 2.2 µs | 0.75 / 2.4 µs (same within noise) |
| inner layers that import a PDF, GPU or windowing crate | `links`, `outline`, `coords` | none, and a test says so |
| `platform` modules that check a URL | 3 | 0 (the shell checks once) |

**The edge count went up, not down** (I had predicted about -2). Splitting a module into two gives the new one its own edges: `pdf_read`
adds 7, `page_table` adds 3, and fewer than the 4 removed come out. That is the honest trade: more, smaller boxes with the same
direction of every arrow, and each box now has one reason to change. What got better is not the count but what the layering test can
now assert: *nothing below the adapters touches a file, the environment, the OS, a PDF crate, `wgpu` or `winit`* (it also found that
`figure.rs` had never been scanned, because the test looked at the `figure/` directory only; fixed).

What each change did:

* **S1** `PenTool` and `role()` are gone; `PenButtons::role()` returns `Role`.
* **S7** `graph.py` and `architecture.rs` both skip `#[cfg(test)]` modules; the drawn `platform -> editor` edge went away.
* **S2** `pdf_read` (adapter) reads links, outline and page geometry; `links` and `outline` keep only the model; the shell calls
  `links::safe_url` once before `Platform::open_url`.
* **S3** `input/motion.rs` is `platform/motion_event.rs`; `platform/generic.rs` is `unix_dirs.rs` and `stubs.rs`; the platform view of
  the graph was brought up to date (it still mentioned `rfd` and `save_file_dialog`).
* **S6** `text/{font, layout, raster}.rs`, same public API.
* **S4** `page_table` (model) holds the page bookkeeping; six new tests need no PDF; `Doc` keeps the renderer, the bytes and the save.
* **S5** `UiLayers` owns the presenters (`sync_committed`, `sync_ui`, `sync_texts`, `overview_frame`, `scene`); they take a `Paint`
  (editor, GPU, document, frame number) built by `paint!(self)`, so `State` is the loop and the document workflow.

## References

Parnas, *On the Criteria To Be Used in Decomposing Systems into Modules*, CACM 1972. Stevens, Myers and Constantine, *Structured
Design*, IBM Systems Journal 1974 (cohesion and coupling). Chidamber and Kemerer, *A Metrics Suite for Object Oriented Design*, IEEE
TSE 1994 (LCOM). Martin, *Agile Software Development*, 2002 (SDP, CCP, SRP), and *Clean Architecture*, 2017. Lakos, *Large-Scale C++
Software Design*, 1996 (levelization). Meyer, *Object-Oriented Software Construction*, 1988 (open/closed).
