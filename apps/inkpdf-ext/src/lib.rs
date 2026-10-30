//! The built-in extensions of InkPDF. Each is a folder here with a `mod.rs` that has `pub fn extension() -> inkpdf_api::Extension`;
//! `build.rs` finds them, so a new one touches no file of another and none of the program's. See `docs/EXTENSIONS.md`.

include!(concat!(env!("OUT_DIR"), "/extensions.rs"));
