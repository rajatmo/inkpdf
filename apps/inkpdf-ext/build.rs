//! Finds the extensions: every folder `src/<name>/` that has a `mod.rs` is one, and gets a line in the list the program reads. So an
//! extension is added by adding a folder, and no file that is shared with another extension is edited (nothing for two forks to conflict on).

use std::fmt::Write;
use std::path::Path;

fn main() {
    let src = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("src");
    println!("cargo:rerun-if-changed=src");
    let mut names: Vec<String> = std::fs::read_dir(&src)
        .expect("read src")
        .flatten()
        .filter(|e| e.path().join("mod.rs").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut out = String::new();
    for name in &names {
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "the extension folder {name:?} must be lower case snake_case"
        );
        let path = src.join(name).join("mod.rs");
        writeln!(out, "#[path = {:?}]\nmod {name};", path.display().to_string()).expect("write");
    }
    writeln!(out, "/// Every extension, in the order of their folders' names.\npub fn all() -> Vec<inkpdf_api::Extension> {{\n    vec![")
        .expect("write");
    for name in &names {
        writeln!(out, "        {name}::extension(),").expect("write");
    }
    writeln!(out, "    ]\n}}").expect("write");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("out dir")).join("extensions.rs");
    std::fs::write(dest, out).expect("write the list");
}
