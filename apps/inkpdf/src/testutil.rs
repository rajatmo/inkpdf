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

/// One page with a form: a text field `name` (a box 200 wide at the top), a check box `agree`, a group of two radio buttons `size` (small and large), a
/// combo box `colour` (Red, Green, Blue), a read-only text field `locked` and a push button `send`, which cannot be filled.
pub fn pdf_with_form() -> Vec<u8> {
    let off = "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 0 >>\nstream\n\nendstream";
    // (The look of a box that is on: a black square.)
    let on = "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 18 >>\nstream\n0 g 4 4 12 12 re f\nendstream";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [7 0 R 8 0 R 10 0 R 11 0 R 12 0 R 15 0 R 16 0 R] >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Fields [7 0 R 8 0 R 9 0 R 12 0 R 15 0 R 16 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [100 700 300 720] /V () /DA (/Helv 12 Tf 0 g) /P 3 0 R /F 4 >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /Rect [100 650 120 670] /V /Off /AS /Off /P 3 0 R /F 4 \
         /AP << /N << /Yes 13 0 R /Off 14 0 R >> >> >>"
            .to_string(),
        "<< /FT /Btn /Ff 32768 /T (size) /Kids [10 0 R 11 0 R] /V /Off >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /Parent 9 0 R /Rect [100 600 120 620] /AS /Off /P 3 0 R /F 4 /AP << /N << /Small 13 0 R /Off 14 0 R >> >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Widget /Parent 9 0 R /Rect [200 600 220 620] /AS /Off /P 3 0 R /F 4 /AP << /N << /Large 13 0 R /Off 14 0 R >> >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (colour) /Opt [(Red) (Green) (Blue)] /V () /Rect [100 550 300 570] \
         /DA (/Helv 12 Tf 0 g) /P 3 0 R /F 4 >>"
            .to_string(),
        on.to_string(),
        off.to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 1 /T (locked) /Rect [100 500 300 520] /V (fixed) /DA (/Helv 12 Tf 0 g) /P 3 0 R /F 4 >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (send) /Rect [100 450 200 470] /DA (/Helv 12 Tf 0 g) /P 3 0 R /F 4 >>".to_string(),
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

/// Four pages and an outline: `Chapter 1` (page 1) with `1.1 Basics` (page 2) and `1.2 More` (page 3) under it,
/// `Chapter 2` (page 4), and `Appendix`, which goes nowhere.
pub fn pdf_with_outline() -> Vec<u8> {
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Outlines /First 8 0 R /Last 12 0 R /Count 5 >>".to_string(),
        "<< /Title (Chapter 1) /Parent 7 0 R /First 9 0 R /Last 10 0 R /Next 11 0 R /Count 2 /Dest [3 0 R /Fit] >>".to_string(),
        "<< /Title (1.1 Basics) /Parent 8 0 R /Next 10 0 R /Dest [4 0 R /Fit] >>".to_string(),
        "<< /Title (1.2 More) /Parent 8 0 R /Prev 9 0 R /Dest [5 0 R /Fit] >>".to_string(),
        "<< /Title (Chapter 2) /Parent 7 0 R /Prev 8 0 R /Next 12 0 R /Dest [6 0 R /Fit] >>".to_string(),
        "<< /Title (Appendix) /Parent 7 0 R /Prev 11 0 R >>".to_string(),
    ])
}
