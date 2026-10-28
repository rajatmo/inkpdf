//! Helpers shared by tests.

/// A PDF made of these objects (object 1 is the catalog), with correct cross-reference offsets.
pub fn build_pdf(objs: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// A one-page 612 x 792 PDF with correct cross-reference offsets.
pub fn sample_pdf() -> Vec<u8> {
    let content = "BT /F1 24 Tf 72 700 Td (Hello) Tj ET\n";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ])
}

/// Two pages; page 1 has links of every kind the app handles or refuses:
/// a web address, an explicit go-to-page, a named destination in the `/Names` tree, one in the
/// old `/Dests` dictionary, a program launch and a javascript address (both refused).
pub fn pdf_with_links() -> Vec<u8> {
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 8 0 R >> /Dests << /oldstyle [3 0 R /Fit] >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [5 0 R 6 0 R 7 0 R 9 0 R 10 0 R 11 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 680 300 700] /A << /S /URI /URI (https://example.com/) >> >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 600 300 620] /Dest [4 0 R /XYZ 0 792 null] >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 500 300 520] /A << /S /GoTo /D (chapter.1) >> >>".to_string(),
        "<< /Names [(chapter.1) [4 0 R /Fit] (chapter.2) [3 0 R /Fit]] >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 400 300 420] /Dest /oldstyle >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 300 300 320] /A << /S /Launch /F (calc.exe) >> >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [100 200 300 220] /A << /S /URI /URI (javascript:alert\\(1\\)) >> >>".to_string(),
    ])
}

/// Writes the link sample to the path in `INKPDF_DEMO_LINKS_OUT`, for trying links in the real app:
/// `INKPDF_DEMO_LINKS_OUT=links.pdf cargo test -p inkpdf write_link_demo -- --ignored`.
#[test]
#[ignore = "writes a file for manual testing"]
fn write_link_demo() {
    let Some(out) = std::env::var_os("INKPDF_DEMO_LINKS_OUT") else { return };
    std::fs::write(out, pdf_with_links()).expect("write the demo");
}
