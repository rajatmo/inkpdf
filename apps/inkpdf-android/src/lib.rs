//! InkPDF's Android entry point. The program is the `inkpdf` library; this is the shared library the APK holds, with the one
//! function the NativeActivity glue calls. On every other platform it is empty (so a workspace build costs nothing).

#![cfg(target_os = "android")]

use std::ffi::{CString, c_char, c_int};
use std::io::{BufRead, BufReader};
use std::os::fd::FromRawFd;

use winit::platform::android::activity::AndroidApp;

unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

const LOG_INFO: c_int = 4;

/// Android sends the program's standard error nowhere, and the program reports what it does there (`GPU: …`, why a file did
/// not open, a panic). Read it on a thread of its own and write it to the log, so that `adb logcat -s inkpdf` shows it.
fn forward_stderr_to_logcat() {
    let mut ends = [0 as c_int; 2];
    // SAFETY: `ends` has room for the two descriptors the call makes; on success they are ours, and standard error (2) is
    // pointed at the write end, so what is written to it can be read at the other.
    let (read, ok) = unsafe {
        let ok = libc::pipe(ends.as_mut_ptr()) == 0 && libc::dup2(ends[1], 2) != -1;
        (ends[0], ok)
    };
    if !ok {
        return;
    }
    // SAFETY: `read` is the read end of the pipe just made, owned by nothing else.
    let file = unsafe { std::fs::File::from_raw_fd(read) };
    let tag = CString::new("inkpdf").expect("no NUL in the tag");
    let _ = std::thread::Builder::new().name("inkpdf-logcat".into()).spawn(move || {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(text) = CString::new(line) {
                // SAFETY: both strings are NUL-ended and live across the call.
                unsafe { __android_log_write(LOG_INFO, tag.as_ptr(), text.as_ptr()) };
            }
        }
    });
}

/// The entry point the NativeActivity glue calls on its own thread when the activity starts.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    forward_stderr_to_logcat();
    std::panic::set_hook(Box::new(|info| eprintln!("InkPDF panicked: {info}")));
    eprintln!("InkPDF {} starting", env!("CARGO_PKG_VERSION"));
    inkpdf::run_android(app);
}
