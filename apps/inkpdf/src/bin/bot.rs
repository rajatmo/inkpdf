//! A test driver that sends real pen and touch input to a running InkPDF window through Windows' synthetic pointer
//! devices (see `bot/windows.rs` for the ops). It exists only on Windows; the other platforms test their pen readers
//! with the devices of their own session (see `docs/PORTING.md`).

#[cfg(windows)]
#[path = "bot/windows.rs"]
mod windows;

fn main() {
    #[cfg(windows)]
    windows::main();
    #[cfg(not(windows))]
    {
        eprintln!("inkpdf-bot drives Windows' synthetic pointer devices, so it only runs on Windows");
        std::process::exit(1);
    }
}
