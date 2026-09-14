//! GH#1633: `s` (close-and-stroke) was parsed as `Operator::Other`, so the
//! stroke was never painted and the path was never cleared — leaving the
//! abandoned geometry for the next fill to paint over.
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![cfg(feature = "pdf")]

/// Build a 200x200 one-page PDF: a stroked frame terminated by `terminator`,
/// a white label fill inside it, then a black square that is always painted
/// (so a blank page cannot be mistaken for an empty fixture).
fn frame_and_fill_pdf(terminator: &[u8]) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(b"2 w 0 0 0 RG\n20 20 m 180 20 l 180 180 l 20 180 l 20 20 l ");
    content.extend_from_slice(terminator);
    content.extend_from_slice(b"\n1 1 1 rg\n60 90 80 30 re f\n0 0 0 rg\n90 40 20 20 re f\n");

    let objects: [Vec<u8>; 4] = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << >> >>".to_vec(),
        [
            format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(),
            content,
            b"endstream".to_vec(),
        ]
        .concat(),
    ];

    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref
        )
        .as_bytes(),
    );
    pdf
}

/// Fraction of pixels darker than luma 250 — the issue's own ink measure.
fn ink(png: &[u8]) -> f64 {
    let image = image::load_from_memory(png).expect("decode png").to_luma8();
    let dark = image.pixels().filter(|pixel| pixel.0[0] < 250).count();
    dark as f64 / image.pixels().len() as f64
}

#[test]
fn close_stroke_renders_identically_to_its_explicit_h_s_expansion() {
    let with_s =
        xberg::render_pdf_page_to_png(&frame_and_fill_pdf(b"s"), 0, Some(150), None).expect("render the `s` variant");
    let with_h_s = xberg::render_pdf_page_to_png(&frame_and_fill_pdf(b"h S"), 0, Some(150), None)
        .expect("render the `h S` variant");

    let (ink_s, ink_h_s) = (ink(&with_s), ink(&with_h_s));
    println!("ink(s)={ink_s:.5} ink(h S)={ink_h_s:.5}");

    // Before the fix `s` measured 0.011 (no frame, the white label fill had
    // flooded its interior) against 0.041 for `h S`. ~keep
    assert!(
        ink_s > 0.02,
        "`s` painted almost nothing (ink {ink_s:.5}) — the frame was never stroked"
    );
    assert_eq!(with_s, with_h_s, "`s` must render byte-identically to `h S`");
}
