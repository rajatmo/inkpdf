//! The `inkpdf` program: everything is in the library (see `lib.rs`); this is only its entry point.

// The release build has no console window of its own (it joins the terminal it was started from,
// if any, in `run`). Debug builds keep the console for logging.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    inkpdf::run();
}
