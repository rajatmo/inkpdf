# InkPDF: notes for Claude sessions

InkPDF is a pen-first PDF annotator in Rust, Windows first (see `README.md`, `apps/inkpdf/ARCHITECTURE.md`). On the
`linux-android-port` branch it also builds and runs on Linux and type-checks for Android: read `docs/PORTING.md` before
touching `platform/` or anything per-OS.

- **The plan lives in `ROADMAP.md`.** Read its sections 1 to 3 first, then do one task from section 6 (for example "do task F1").
- On `main` the crate does not compile off Windows: verification is the GitHub Actions run on `windows-latest`
  (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`). Work on a branch, keep changes small, read the CI log.
  On the port branch Linux runs the same checks, and Windows can be cross-checked from Linux
  (`cargo clippy -p inkpdf --all-targets --target x86_64-pc-windows-msvc`).
- Layering is enforced by `apps/inkpdf/src/architecture.rs`: a new module goes in its `LAYERS` table and in `ARCHITECTURE.md`,
  and `python3 tools/deps/graph.py` redraws `docs/deps/` (CI runs it with `--check`).
- Input promise: every tool works with only the left mouse button and keyboard (desktop) or the stylus tip (tablet).
- Do not bump the version, push to `main`, or touch the pinned pdfcraft revision unless the task says so.
