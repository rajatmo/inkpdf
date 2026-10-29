# The PDF backend: the seam, the upgrade rule, the numbers, and when to reconsider

Written 2026-10-29 (roadmap tasks K1 to K3). The engine is [pdfcraft](https://github.com/storytold/pdfcraft) (the owner's own repository),
pinned to one commit in the root `Cargo.toml`.

## 1. The seam

`apps/inkpdf/src/pdf/` is the only code that names `pdfcraft_*` or `hayro` (a test in `architecture.rs` fails if any other module does).
The rest of the program speaks its own types:

| Piece | What it is |
|---|---|
| `trait PdfBackend` | inspect (page geometry), blank, open, a render pool, one render now, page text, dates |
| `trait PdfDocument` | an open file: links, outline, annotations, add and delete an annotation, organize pages, save whole or as an update |
| `trait RenderPool` | renders on threads of its own behind the frame: `set_queue`, `try_recv` |
| `pagetext::PageTextView` (model) | one page's text: nearest glyph, word, line, boxes, marquee, text of a range, find |
| `pdf/pdfcraft.rs` | the one implementation |
| `pdf/contract.rs` | the conformance suite: what any backend must do |

`ink_io` is the policy for *our* annotations (which ones are ours, how strokes and shapes are written) in terms of `PdfDocument`;
`pdf_read` places raw links and outline on the layout. Neither knows the engine.

**A new backend** implements the three traits and runs `backend_contract(&TheNewOne)` (it is in `pdf/contract.rs`). That is the whole
acceptance test; it has one test per check for pdfcraft and a test that a deliberately wrong backend fails.

## 2. The upgrade rule

Move pdfcraft only with the suite green, in a commit of its own, with the numbers of section 3 re-measured beside it:

```bash
tools/pdfcraft/bump.sh <40-hex commit>     # sets every pdfcraft rev (13 lines; the winit fork's is separate), updates the lock,
                                           # runs the conformance suite and the whole test suite, prints what changed
INKPDF_MEASURE_PDF=dense.pdf cargo test -p inkpdf --lib --release measure_the_backend -- --ignored --nocapture
```

A conformance failure after a bump is information, not noise: either the engine got better (update the expectation on purpose, and say
so in the commit) or it got worse (fix it in pdfcraft, or stay). The script run on the current rev changes nothing.

## 3. The numbers (2026-10-29, pdfcraft `1dd2737`)

Linux x86_64 container, release build, one thread for the pool. **Only Linux was measured**: Windows and Android numbers belong in this
table when someone runs the test there. The document is `inkpdf-gen dense200.pdf 200 --outline` (1.7 MB, 200 pages of dense text and
vector drawing): a floor, because it has no images and no embedded fonts, so real files will be slower.

| What | Result |
|---|---|
| inspect (page geometry of 200 pages) | 5.6 ms |
| pool, 1 thread, preview + thumbnail + a sharp tile of each of 60 pages (180 requests) | median 8.3 ms, p95 17.4 ms, worst 21.4 ms; 1.5 s in all |
| one page at 1.0, parse of the file included | median 13.6 ms, p95 24 ms |
| one page at 2.0, parse included | median 19.5 ms, p95 23 ms |
| page text plus a find, parse included | median 3.7 ms, p95 5.8 ms |
| peak memory of all of the above | 32 MB |
| release binary (Linux), code size | 11.8 MiB of code, **about 3.3 MiB (28 %) in pdfcraft and what it is built on** (`hayro_*`, `pdfcraft_*`, `lopdf`, `vello_cpu`, `moxcms`, `pic_scale`, `fearless_simd`), measured with `cargo bloat --crates`; the whole file is 22 MiB |
| Android APK (arm64, release) | 6.2 MB |

## 4. What the suite pinned down (behaviour worth knowing)

* `word_at` and `line_at` return the **first and last glyph, both included** (not a half-open range).
* **Two columns are read line by line**: a line of the left column, a tab, the same line of the right column, then the next line
  (`"Left column text\tRight column text\ncontinues down here\talso continues"`). A selection across columns therefore interleaves them.
  The text tools (roadmap X4) must either live with it or ask pdfcraft for column reading; the suite will say when that changes.
* Text of a page that is turned in the file is in the page **as displayed** (the line of a quarter-turned page runs down it).
* `duplicate_pages` with no place goes right after the page it copies; the program always gives a place.
* A render is premultiplied RGBA with the page painted opaque white where nothing is drawn.
* A request's `tag` comes back as it was, **except that bit 60 is pdfcraft's** (a print preview, which ignores the reading colours): the
  seam asserts it is clear. (The dark-pages task must use another bit.)
* `render_now` and `page_text` parse the file on every call; the pool and (later) the text indexer keep a renderer per file.
* A link's target is handed over raw (a launch or a script is listed as an address); refusing it is the program's `links::safe_url`.

## 5. What we depend on, and the risks

| Risk | State |
|---|---|
| Pre-1.0 and moving | pinned by commit; the seam plus the suite make a bump a measured act |
| One maintainer | it is the owner's own repository, so a fix can be made at the source |
| Vendored patches | `hayro*`, `lopdf` and `vello_common` come from pdfcraft's `vendor/` at the same commit; the lines must agree (the script sets them together) |
| Size | about 28 % of the code (section 3); an Android APK of 6.2 MB |
| Fidelity on real files | unknown beyond the corpus: **collect a few real-world PDFs (with images and embedded fonts) into the corpus** and the measurements before relying on the numbers above |

## 6. Options, and when to reconsider

| Option | Cost | Verdict |
|---|---|---|
| Stay on pdfcraft, with the seam and the suite | low | **the choice** |
| Vendor the pdfcraft source into this repository | a bigger repository, merges to carry | if the other repository stops being under our control |
| A second pure-Rust backend behind the seam | the three traits, then the suite | only if a trigger below fires; it must pass `backend_contract` |
| Platform renderers (Android `PdfRenderer`, Windows.Data.Pdf), pdfium, MuPDF | not pure Rust, no text or annotation access on some | **ruled out** (the owner's rule: pure Rust only) |
| An own renderer | years: fonts, CMaps, JBIG2, JPEG 2000, transparency and shadings, a spec of about 1000 pages | **not planned** |

**Reconsider** (write to the owner, with the number) when any of these is true:

1. a class of real documents renders wrong, and a fix in pdfcraft has not landed within two of its releases;
2. a tile takes more than **50 ms** median (8 ms today) or a 200-page file takes more than **200 ms** to parse and draw a page (14 ms today)
   on the slowest supported device (measure on the tablet first);
3. a typical 200-page file needs more than **512 MB** (32 MB today);
4. pdfcraft's share of the code passes **40 %**, or the Android APK passes **30 MB** (6.2 MB today);
5. the repository has had no commit for **12 months** while a bug that matters to us is open;
6. a licence change.
