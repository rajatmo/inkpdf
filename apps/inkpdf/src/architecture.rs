//! The layering of the code, as tests: each module belongs to a layer, may use modules of its own layer and the ones
//! below it and nothing above, and no two modules may use each other, directly or through others. The tests read the
//! sources, so a stray `use` in the wrong direction fails the build of the tests, not a review.
//!
//! ```text
//!   adapter   app, gpu, platform, doc, ink_io, tiles, thumbnail   (window, GPU, files, OS)
//!   session   editor            routes input to the tool in use; owns the chrome
//!   tools     tools             what a press on the page does; one file each
//!   ui        toolbar           where the buttons are, how they look
//!   domain    tool, input, requests, workspace, selection, settings
//!   model     figure, ink, laser, links, outline, pages
//!   base      plane, motion, scene, clock, geometry, colour, coords, view, text, smooth, clipboard
//! ```
//!
//! The one thing the layer table cannot say yet is in [`KNOWN_EXCEPTIONS`]: edges that point up and are still there.
//! Each is on the list to be removed, and the list may only shrink: a test fails when an exception stops being true,
//! so fixing one means deleting its line.

use std::collections::{BTreeMap, BTreeSet};
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

/// The lines of `text` that are part of the program, with their numbers (from 0): not the items that exist only for
/// tests (`#[cfg(test)]` on a module, a function, a `use`), wherever in the file they are. Comments are left in; the
/// callers skip them.
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("#[cfg(test)]") {
            // Past it and any more attributes, then the item it belongs to: to the end of its braces, or its `;`.
            i += 1;
            let (mut depth, mut opened) = (0i32, false);
            while i < lines.len() {
                let item = lines[i];
                if !opened && item.trim_start().starts_with("#[") {
                    i += 1;
                    continue;
                }
                depth += item.matches('{').count() as i32 - item.matches('}').count() as i32;
                opened |= item.contains('{');
                i += 1;
                if (opened && depth <= 0) || (!opened && item.trim_end().ends_with(';')) {
                    break;
                }
            }
            continue;
        }
        out.push((i, lines[i]));
        i += 1;
    }
    out
}

/// The lines of `file` that are part of the program, not comments, that mention any of `forbidden`.
fn offences(file: &Path, forbidden: &[&str]) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    production_lines(&text)
        .into_iter()
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

// ── the module graph ─────────────────────────────────────────────────────────────────────────

/// Layers, lowest first: a module may use its own layer and the ones before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    /// Plain arithmetic and data, knowing nothing of the program.
    Base,
    /// What a drawing is made of, and how it is stored and changed.
    Model,
    /// What the tools work on and what the devices mean.
    Domain,
    /// Where the buttons are and how they look.
    Ui,
    /// What a press on the page does.
    Tools,
    /// Routing input to the tool in use, and the window's own controls.
    Session,
    /// The window, the GPU, files, the operating system.
    Adapter,
}

/// Every top-level module of the program, and its layer. A new module is placed here or the test fails.
const LAYERS: [(&str, Layer); 36] = [
    ("plane", Layer::Base),
    ("motion", Layer::Base),
    ("scene", Layer::Base),
    ("clock", Layer::Base),
    ("geometry", Layer::Base),
    ("colour", Layer::Base),
    ("coords", Layer::Base),
    ("view", Layer::Base),
    ("text", Layer::Base),
    ("smooth", Layer::Base),
    ("clipboard", Layer::Base),
    ("figure", Layer::Model),
    ("ink", Layer::Model),
    ("laser", Layer::Model),
    ("links", Layer::Model),
    ("outline", Layer::Model),
    ("pages", Layer::Model),
    ("tool", Layer::Domain),
    ("browse", Layer::Domain),
    ("input", Layer::Domain),
    ("requests", Layer::Domain),
    ("workspace", Layer::Domain),
    ("selection", Layer::Domain),
    ("settings", Layer::Domain),
    ("store", Layer::Domain),
    ("toolbar", Layer::Ui),
    ("tools", Layer::Tools),
    ("editor", Layer::Session),
    ("app", Layer::Adapter),
    ("gpu", Layer::Adapter),
    ("platform", Layer::Adapter),
    ("doc", Layer::Adapter),
    ("path_store", Layer::Adapter),
    ("ink_io", Layer::Adapter),
    ("tiles", Layer::Adapter),
    ("thumbnail", Layer::Adapter),
];

/// Edges that point up the layers and are still there, `(user, used)`, each to be removed (see the plan in
/// `ARCHITECTURE.md`). There are none now; an edge that points up has to be added here, with a reason, or fixed.
const KNOWN_EXCEPTIONS: [(&str, &str); 0] = [];

fn layer_of(module: &str) -> Option<Layer> {
    LAYERS.iter().find(|(m, _)| *m == module).map(|(_, l)| *l)
}

/// The top-level modules of the program (not its tests, its binaries or the entry point).
fn modules() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(src()).expect("src").flatten() {
        let path = entry.path();
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let is_module = path.is_dir() || path.extension().is_some_and(|e| e == "rs");
        if is_module && !matches!(name.as_str(), "main" | "lib" | "bin" | "stress" | "architecture" | "testutil") {
            found.insert(name);
        }
    }
    found
}

/// Which top-level modules each one uses, from the `crate::name` paths in its program lines.
fn dependencies() -> BTreeMap<String, BTreeSet<String>> {
    let all = modules();
    let mut out: BTreeMap<String, BTreeSet<String>> = all.iter().map(|m| (m.clone(), BTreeSet::new())).collect();
    for file in sources(&src()) {
        let rel = file.strip_prefix(src()).unwrap_or(&file);
        let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().trim_end_matches(".rs").to_string()).unwrap_or_default();
        let Some(uses) = out.get_mut(&top) else { continue };
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for (_, line) in production_lines(&text) {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(!line.contains("crate::{"), "{}: a grouped `use crate::{{…}}` hides which modules it uses: {line}", rel.display());
            for (at, _) in line.match_indices("crate::") {
                let name: String = line[at + "crate::".len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if name != top && all.contains(&name) {
                    uses.insert(name);
                }
            }
        }
    }
    out
}

fn edge_list(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<(String, String)> {
    graph.iter().flat_map(|(from, to)| to.iter().map(move |t| (from.clone(), t.clone()))).collect()
}

#[test]
fn every_module_is_placed_in_a_layer() {
    let placed: BTreeSet<&str> = LAYERS.iter().map(|(m, _)| *m).collect();
    let found = modules();
    let unplaced: Vec<&String> = found.iter().filter(|m| !placed.contains(m.as_str())).collect();
    assert!(unplaced.is_empty(), "modules with no layer in architecture.rs: {unplaced:?}");
    let gone: Vec<&&str> = placed.iter().filter(|m| !found.contains(**m)).collect();
    assert!(gone.is_empty(), "layers listed for modules that are not there: {gone:?}");
}

#[test]
fn a_module_uses_only_its_own_layer_and_the_ones_below_apart_from_the_known_exceptions() {
    let bad: Vec<String> = edge_list(&dependencies())
        .into_iter()
        .filter(|(from, to)| layer_of(to) > layer_of(from) && !KNOWN_EXCEPTIONS.contains(&(from.as_str(), to.as_str())))
        .map(|(from, to)| format!("{from} ({:?}) uses {to} ({:?})", layer_of(&from), layer_of(&to)))
        .collect();
    assert!(bad.is_empty(), "layering broken:\n{}", bad.join("\n"));
}

#[test]
fn the_known_exceptions_are_all_still_true() {
    let edges = edge_list(&dependencies());
    for (from, to) in KNOWN_EXCEPTIONS {
        assert!(edges.contains(&(from.to_string(), to.to_string())), "{from} no longer uses {to}: delete its line from KNOWN_EXCEPTIONS");
        assert!(layer_of(to) > layer_of(from), "{from} -> {to} does not point up the layers: it is no exception");
    }
}

#[test]
fn no_module_uses_another_that_uses_it_back() {
    let graph = dependencies();
    // A module is in a cycle if it can reach itself.
    let reaches = |from: &str| -> BTreeSet<String> {
        let (mut seen, mut todo) = (BTreeSet::new(), vec![from.to_string()]);
        while let Some(m) = todo.pop() {
            for next in graph.get(&m).into_iter().flatten() {
                if seen.insert(next.clone()) {
                    todo.push(next.clone());
                }
            }
        }
        seen
    };
    let cyclic: Vec<String> =
        graph.keys().filter(|m| reaches(m).contains(*m)).map(|m| format!("{m} (reaches itself through {:?})", graph[m])).collect();
    assert!(cyclic.is_empty(), "modules in a cycle:\n{}", cyclic.join("\n"));
}

#[test]
fn what_looks_at_the_sources_sees_what_the_program_is_made_of_and_not_what_only_tests_are() {
    let text = "use a;\n#[cfg(test)]\nmod tests;\nfn real() {\n    use crate::x;\n}\n#[cfg(test)]\n#[allow(dead_code)]\nfn only_for_tests() {\n    use crate::y;\n    if true {\n    }\n}\n#[cfg(test)]\nuse crate::z;\nfn also_real() {}\n#[cfg(test)]\nmod tests {\n    use crate::w;\n}\n";
    let kept: Vec<&str> = production_lines(text).into_iter().map(|(_, l)| l).collect();
    assert_eq!(kept, ["use a;", "fn real() {", "    use crate::x;", "}", "fn also_real() {}"]);
}
/// Windowing, GPU and OS-input crates, and the shell's modules.
const ADAPTERS: [&str; 7] = ["winit", "wgpu", "crate::app", "crate::gpu", "crate::platform", "crate::thumbnail", "crate::tiles"];

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
        "outline.rs",
        "pages.rs",
        "workspace.rs",
        "selection.rs",
        "clipboard.rs",
        "browse.rs",
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
fn nothing_below_the_adapters_touches_the_disk_the_environment_or_an_operating_system() {
    // The inner layers are pure: the same input gives the same output on every machine. What they need from the world (a font,
    // a settings folder, a clock's source) is handed to them by the adapters, which ask `platform`.
    let forbidden =
        ["std::fs", "std::env", "std::process", "std::net", "windows_sys", "cfg(windows)", "cfg(not(windows))", "cfg(unix", "cfg(target_os", "cfg(not(target_os"];
    let inner: Vec<String> = LAYERS
        .iter()
        .filter(|(_, layer)| *layer != Layer::Adapter)
        .map(|(module, _)| if src().join(module).is_dir() { (*module).to_string() } else { format!("{module}.rs") })
        .collect();
    assert_free_of(&inner.iter().map(String::as_str).collect::<Vec<_>>(), &forbidden);
}

#[test]
fn the_session_knows_nothing_of_the_shell() {
    // (Not even winit: the session speaks the program's own input vocabulary, `input::InputEvent`, which the shell translates
    // into, so any host can drive it.)
    assert_free_of(&["editor"], &ADAPTERS);
}

#[test]
fn only_the_session_chooses_by_tool_so_a_tool_added_is_one_file_and_one_line() {
    // The shell (app) draws and talks to the tools through the trait; it never names a tool's type, so adding
    // a tool does not touch it.
    assert_free_of(&["app"], &["PenTool", "ShapeTool", "TextTool", "PointerTool"]);
}
