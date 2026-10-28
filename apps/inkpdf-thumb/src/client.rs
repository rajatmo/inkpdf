//! Running `inkpdf.exe --thumbnail SIZE`: the PDF goes in on standard input, the picture comes back
//! on standard output as a header (width, then height, little-endian u32 each) followed by
//! width * height pixels of opaque BGRA.

use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long the helper gets before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// The biggest side the helper draws (see the app's thumbnail module), which bounds what is read.
const MAX_SIDE: u32 = 1024;
const MAX_OUTPUT: usize = 8 + (MAX_SIDE as usize) * (MAX_SIDE as usize) * 4;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const BELOW_NORMAL_PRIORITY: u32 = 0x0000_4000;

pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The helper's output as a picture, if it is exactly what was promised.
pub fn parse(output: &[u8]) -> Option<Picture> {
    let width = u32::from_le_bytes(output.get(0..4)?.try_into().ok()?);
    let height = u32::from_le_bytes(output.get(4..8)?.try_into().ok()?);
    let valid = (1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height);
    let pixels = output.get(8..)?;
    (valid && pixels.len() == width as usize * height as usize * 4).then(|| Picture { width, height, bgra: pixels.to_vec() })
}

/// Draw page 1 of `pdf`, `side` pixels on its longer side, with the helper at `exe`.
pub fn render(exe: &Path, pdf: Vec<u8>, side: u32) -> Option<Picture> {
    let mut command = Command::new(exe);
    command.arg("--thumbnail").arg(side.to_string()).creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY);
    parse(&run(command, pdf, TIMEOUT)?)
}

/// Run `command` with `input` on its standard input and return what it writes to standard output,
/// if it succeeds. Killed, and `None`, if it takes longer than `timeout`.
pub fn run(mut command: Command, input: Vec<u8>, timeout: Duration) -> Option<Vec<u8>> {
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let (mut stdin, stdout) = (child.stdin.take()?, child.stdout.take()?);
    // Feeding the input and collecting the output happen on their own threads, so neither can
    // block the other (or this one, which only waits for the clock).
    thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut output);
        let _ = sender.send(output);
    });
    match receiver.recv_timeout(timeout) {
        Ok(output) => child.wait().ok()?.success().then_some(output).filter(|o| o.len() <= MAX_OUTPUT),
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn picture(width: u32, height: u32, pixels: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend(std::iter::repeat_n(7, pixels));
        out
    }

    #[test]
    fn only_exactly_what_the_helper_promised_is_accepted() {
        let ok = parse(&picture(3, 2, 3 * 2 * 4)).expect("a picture");
        assert_eq!((ok.width, ok.height, ok.bgra.len()), (3, 2, 24));
        assert!(parse(&picture(3, 2, 23)).is_none(), "too few pixels");
        assert!(parse(&picture(3, 2, 25)).is_none(), "too many");
        assert!(parse(&picture(0, 2, 0)).is_none(), "no width");
        assert!(parse(&picture(2000, 1, 8000)).is_none(), "bigger than ever asked for");
        assert!(parse(&picture(u32::MAX, u32::MAX, 4)).is_none(), "sizes whose product overflows");
        assert!(parse(&[1, 2, 3]).is_none(), "not even a header");
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn a_helper_that_answers_is_heard() {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", "more"]);
        let out = run(cmd, b"hello".to_vec(), Duration::from_secs(20)).expect("echoed back");
        assert!(String::from_utf8_lossy(&out).contains("hello"));
    }

    #[test]
    fn a_helper_that_fails_gives_nothing() {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", "exit 3"]);
        assert!(run(cmd, Vec::new(), Duration::from_secs(20)).is_none());
        assert!(run(Command::new("this-program-does-not-exist.exe"), Vec::new(), Duration::from_secs(5)).is_none());
    }

    #[test]
    fn a_helper_that_hangs_is_killed_on_time() {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", "ping -n 30 127.0.0.1 >nul"]);
        let started = Instant::now();
        assert!(run(cmd, Vec::new(), Duration::from_millis(800)).is_none());
        assert!(started.elapsed() < Duration::from_secs(5), "gave up after {:?}", started.elapsed());
    }
}
