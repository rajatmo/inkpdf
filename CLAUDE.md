# InkPDF: notes for Claude sessions

InkPDF is a Windows-only, pen-first PDF annotator in Rust (see `README.md`, `apps/inkpdf/ARCHITECTURE.md`).

- **The plan lives in `ROADMAP.md`.** Read its sections 1 to 3 first, then do one task from section 6 (for example "do task F1").
- The crate does not compile off Windows: verification is the GitHub Actions run on `windows-latest`
  (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`). Work on a branch, keep changes small, read the CI log.
- Layering is enforced by `apps/inkpdf/src/architecture.rs`: a new module goes in its `LAYERS` table and in `ARCHITECTURE.md`.
- Input promise: every tool works with only the left mouse button and keyboard (desktop) or the stylus tip (tablet).
- Do not bump the version, push to `main`, or touch the pinned pdfcraft revision unless the task says so.
