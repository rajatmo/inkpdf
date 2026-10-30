//! The built-in extensions of InkPDF. Each is a folder here with a `mod.rs` that has `pub fn extension() -> inkpdf_api::Extension`;
//! `build.rs` finds them, so a new one touches no file of another and none of the program's. See `docs/EXTENSIONS.md`.

include!(concat!(env!("OUT_DIR"), "/extensions.rs"));

/// The template extensions are made from, compiled in tests so that it cannot rot.
#[cfg(test)]
#[path = "../template/mod.rs"]
mod template;

#[cfg(test)]
mod tests {
    #[test]
    fn the_template_is_a_well_formed_extension() {
        let e = super::template::extension();
        // (Its name is a placeholder the script fills in: `TEMPLATE_ID` is upper case, which a real name may not be.)
        assert_eq!(e.id, "TEMPLATE_ID");
        assert_eq!(inkpdf_api::Extension { id: "template", label: "Template", ..e }.problem(), None);
        let tool = (e.create)();
        assert!(tool.policy(true).stylus && !tool.busy());
    }
}
