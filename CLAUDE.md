# InkPDF: notes for Claude sessions

InkPDF is a pen-first PDF annotator in Rust, Windows first (see `README.md`, `apps/inkpdf/ARCHITECTURE.md`). It also builds and
runs on Linux and builds as an Android app (`apps/inkpdf-android`): read `docs/PORTING.md` before touching `platform/` or anything
per-OS.

- **The plan lives in `ROADMAP.md`.** Read its sections 1 to 3 first, then do one task from section 6 (for example "do task F3" or "do task K1").
- Verification is the GitHub Actions run (jobs `check` on `windows-latest`, then `linux`, `android` and the portable builds):
  `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`. Work on a branch, keep changes small, read the CI log. The same
  checks run on Linux here (`cargo test -p inkpdf --lib`, `cargo clippy -p inkpdf --all-targets -- -D warnings`), and Windows and
  Android can be cross-checked from Linux (`cargo clippy -p inkpdf --all-targets --target x86_64-pc-windows-msvc`,
  `cargo clippy -p inkpdf-android --target aarch64-linux-android`). A change that touches `docs/deps` needs
  `python3 tools/deps/graph.py` run last.
- Layering is enforced by `apps/inkpdf/src/architecture.rs`: a new module goes in its `LAYERS` table and in `ARCHITECTURE.md`,
  and `python3 tools/deps/graph.py` redraws `docs/deps/` (CI runs it with `--check`).
- Input promise: every tool works with only the left mouse button and keyboard (desktop) or the stylus tip (tablet).
- Do not bump the version, push to `main`, or touch the pinned pdfcraft revision unless the task says so.
