//! Regression coverage for GH#1770: a PDF that places every glyph in its own
//! fixed-pitch cell (one `Tm` + one `Tj` per character, the way fixed-pitch
//! form/receipt printers emit text) must keep the word gaps that ground truth
//! (`pdftotext`) reports, even when a word gap is a bare EMPTY cell with no
//! space glyph.
//!
//! The fixture below is a direct Rust port of the reproducer from the issue
//! body: Courier 12, one glyph per Tm+Tj, glyph advance/cell pitch 7.2pt
//! (600/1000 em at 12pt), a word gap rendered as exactly one skipped cell.

use xberg_native_pdf::document::PdfDocument;

/// Render `lines` as Courier 12 text where every non-space character gets its
/// own `Tm`+`Tj` at a fixed 7.2pt-per-column pitch; a space in the source
/// line becomes a skipped column (an empty cell), never a drawn glyph.
fn fixed_pitch_pdf(lines: &[&str]) -> Vec<u8> {
    const FONT_SIZE: f32 = 12.0;
    const CELL_PITCH_PT: f32 = 7.2; // Courier 12 glyph advance: 600/1000 em ~keep

    let mut ops = vec![format!("BT /F1 {FONT_SIZE} Tf")];
    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch != ' ' {
                let x = 72.0 + col as f32 * CELL_PITCH_PT;
                let y = 700 - row * 20;
                ops.push(format!("1 0 0 1 {x:.1} {y} Tm ({ch}) Tj"));
            }
        }
    }
    ops.push("ET".to_string());
    let stream = ops.join("\n").into_bytes();

    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_vec(),
        {
            let mut v = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
            v.extend_from_slice(&stream);
            v.extend_from_slice(b"\nendstream");
            v
        },
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_vec(),
    ];

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objs.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}

/// Reproduces GH#1770 exactly: two lines of a fixed-pitch form where a word
/// gap is one empty cell and no space glyph is ever drawn. `extract_text`
/// must still read the words apart, matching `pdftotext`.
#[test]
fn fixed_pitch_single_cell_word_gap_keeps_spaces() {
    let doc = PdfDocument::from_bytes(fixed_pitch_pdf(&["INVOICE NUMBER AND DATE", "ACCOUNT HOLDER NAME"]))
        .expect("open fixture");
    let text = doc.extract_text(0).expect("extract_text");

    assert_eq!(text.trim(), "INVOICE NUMBER AND DATE\nACCOUNT HOLDER NAME");
}

/// Sanity check that the fixture genuinely exercises the single-glyph-per-Tm
/// merge path and not some unrelated span-count artifact: intra-word glyphs
/// (zero-gap cells) must still fuse into one word, not scatter into single
/// letters. If this failed, the positive assertion above could pass
/// vacuously by every glyph staying separate with spaces between all of them.
#[test]
fn fixed_pitch_intra_word_glyphs_stay_fused() {
    let doc = PdfDocument::from_bytes(fixed_pitch_pdf(&["INVOICE"])).expect("open fixture");
    let text = doc.extract_text(0).expect("extract_text");

    assert_eq!(text.trim(), "INVOICE");
}
