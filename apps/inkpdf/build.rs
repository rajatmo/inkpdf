//! Gives inkpdf.exe its icon and version information (shown in Explorer, the taskbar, and
//! Settings > Apps), by compiling a Windows resource.

fn main() {
    println!("cargo:rerun-if-changed=installer/app.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("installer/app.ico");
        res.set("ProductName", "InkPDF");
        res.set("FileDescription", "InkPDF - pen-first PDF annotator");
        res.set("LegalCopyright", "MIT OR Apache-2.0");
        // A resource that cannot be built must not stop the app from building.
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
