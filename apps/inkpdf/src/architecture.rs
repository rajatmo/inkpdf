//! The layering of the code, as a test: each layer may use the ones below it and nothing above. It reads the
//! sources, so a stray `use` in the wrong direction fails the build of the tests, not a review.
//!
//! ```text
//!   adapters   app, gpu, win_pointer, native      (window, GPU, files, OS input)
//!   session    editor                             (routes input to the tool in use; owns the chrome)
//!   tools      tools                              (what a press on the page does; one file each)
//!   domain     workspace, requests, ink, figure, geometry, smooth, text, laser, view, input, …
//! ```

use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // (Tests are the code that looks at the layers from outside.)
            if path.file_name().is_some_and(|n| n != "tests") {
                found.extend(sources(&path));
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
    found
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The lines of `file` outside its `#[cfg(test)]` modules that mention any of `forbidden`.
fn offences(file: &Path, forbidden: &[&str]) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    // (Test modules sit at the end of a file, after the first `#[cfg(test)]`.)
    let code = text.split("#[cfg(test)]").next().unwrap_or_default();
    code.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| forbidden.iter().any(|f| line.contains(f)))
        .map(|(i, line)| format!("{}:{}: {}", file.strip_prefix(src()).unwrap_or(file).display(), i + 1, line.trim()))
        .collect()
}

/// Check that nothing in `paths` (relative to `src/`; a directory or a file) mentions `forbidden`.
fn assert_free_of(paths: &[&str], forbidden: &[&str]) {
    let mut bad = Vec::new();
    for rel in paths {
        let path = src().join(rel);
        let files = if path.is_dir() { sources(&path) } else { vec![path.clone()] };
        assert!(!files.is_empty(), "{rel} has no sources: the layering test is looking in the wrong place");
        for file in files {
            bad.extend(offences(&file, forbidden));
        }
    }
    assert!(bad.is_empty(), "layering broken:\n{}", bad.join("\n"));
}

/// Windowing, GPU and OS-input crates, and the shell's modules.
const ADAPTERS: [&str; 8] = ["winit", "wgpu", "crate::app", "crate::gpu", "crate::win_pointer", "crate::native", "crate::thumbnail", "crate::tiles"];

#[test]
fn the_domain_knows_nothing_of_tools_the_session_or_the_shell() {
    let domain = [
        "ink",
        "figure",
        "figure.rs",
        "geometry.rs",
        "smooth.rs",
        "text.rs",
        "laser.rs",
        "view.rs",
        "colour.rs",
        "coords.rs",
        "links.rs",
        "workspace.rs",
        "requests.rs",
    ];
    let domain: Vec<&str> = domain.iter().copied().filter(|p| src().join(p).exists()).collect();
    let mut forbidden = vec!["crate::tools", "crate::editor"];
    forbidden.extend(ADAPTERS);
    assert_free_of(&domain, &forbidden);
}

#[test]
fn tools_know_neither_each_other_nor_the_session_nor_the_shell() {
    let mut forbidden = vec!["crate::editor", "crate::toolbar::{", "Toolbar", "PromptView", "super::super::super"];
    forbidden.extend(ADAPTERS);
    // The tool id (`toolbar::Tool`) is how the registry names them; nothing else of the toolbar is theirs.
    forbidden.retain(|f| *f != "crate::toolbar::{");
    assert_free_of(&["tools"], &forbidden);
    // One tool may not use another: each file names only itself and the shared parts.
    for (file, others) in [
        ("tools/pen.rs", ["shape", "text", "pointer"]),
        ("tools/pointer.rs", ["pen", "shape", "text"]),
        ("tools/text.rs", ["pen", "shape", "pointer"]),
        ("tools/shape/mod.rs", ["pen", "text", "pointer"]),
    ] {
        let bad: Vec<String> =
            others.iter().flat_map(|o| offences(&src().join(file), &[&format!("super::{o}::"), &format!("super::{o};")])).collect();
        assert!(bad.is_empty(), "{file} reaches into another tool:\n{}", bad.join("\n"));
    }
}

#[test]
fn the_session_knows_nothing_of_the_shell() {
    // (It does speak winit's event types, ElementState and the like, as the vocabulary of input; what it
    // never touches is the window, the GPU, the files or the OS's pointer layer.)
    let forbidden: Vec<&str> = ADAPTERS.iter().copied().filter(|a| *a != "winit" && *a != "crate::win_pointer").collect();
    assert_free_of(&["editor"], &forbidden);
}

#[test]
fn only_the_session_chooses_by_tool_so_a_tool_added_is_one_file_and_one_line() {
    // The shell (app) draws and talks to the tools through the trait; it never names a tool's type, so adding
    // a tool does not touch it.
    assert_free_of(&["app"], &["PenTool", "ShapeTool", "TextTool", "PointerTool"]);
}
