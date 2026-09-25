//! GH#1794: `render_page_capturing_glyph_drops` (`crates/xberg/src/pdf/render.rs`)
//! turned ANY non-image `xberg_native_pdf` engine warning captured during a render
//! call into a "glyph ink is missing" `ProcessingWarning`, regardless of whether a
//! glyph actually failed to paint. Two font-*load* warnings reach it this way: the
//! Type 3 glyph-name fallback (`fonts/font_dict.rs`, unconditional for every Type 3
//! font) and GH#1795's now-fixed "dictionary used where stream expected" warning.
//!
//! This builds a minimal, working one-page Type 3 font PDF — one glyph, a filled
//! rectangle, invoked from the content stream — and renders it through the same
//! public path the CLI uses. The font-load warning still fires (Type 3 fonts always
//! log it), and the render still paints real ink (the filled rectangle), so the
//! bug is entirely in the wording, not the pipeline: nothing was dropped, but the
//! pre-fix classifier claimed otherwise.

#![cfg(feature = "pdf")]

use xberg::pdf::render::{
    install_pdf_render_diagnostics, render_pdf_page_to_png, take_xberg_native_pdf_render_warnings,
};

/// A one-page PDF with a Type 3 font holding a single glyph (a filled rectangle,
/// via `d1` + `re f`), invoked once from the content stream. `FontMatrix
/// [0.001 0 0 0.001 0 0]` gives a standard 1000-unit em; `/Differences [65 /g]` maps
/// byte code 65 (`A`) to the glyph. Real ink is painted — this is not a font that
/// fails to render, it is the ordinary Type 3 shape that always logs the
/// `type3_font` diagnostic on load.
fn build_type3_font_pdf() -> Vec<u8> {
    let content_stream = b"BT /F1 24 Tf 72 700 Td (A) Tj ET";
    let glyph_proc = b"1000 0 100 100 700 700 d1\n100 100 600 600 re\nf";

    let mut pdf: Vec<u8> = Vec::new();
    macro_rules! push_bytes {
        ($s:expr) => {
            pdf.extend_from_slice($s)
        };
    }
    macro_rules! push_str {
        ($s:expr) => {
            pdf.extend_from_slice($s.as_bytes())
        };
    }

    push_bytes!(b"%PDF-1.5\n");

    let off1 = pdf.len();
    push_bytes!(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    push_bytes!(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    push_bytes!(
        b"3 0 obj\n\
         << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 792]\n\
            /Resources << /Font << /F1 5 0 R >> >>\n\
            /Contents 4 0 R >>\n\
         endobj\n"
    );

    let off4 = pdf.len();
    push_str!(format!("4 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()));
    push_bytes!(content_stream);
    push_bytes!(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    push_bytes!(
        b"5 0 obj\n\
         << /Type /Font /Subtype /Type3 /Name /T3_0 \
            /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] \
            /CharProcs << /g 6 0 R >> \
            /Encoding << /Type /Encoding /Differences [65 /g] >> \
            /FirstChar 65 /LastChar 65 /Widths [1000] \
            /Resources << /ProcSet [/PDF] >> >>\n\
         endobj\n"
    );

    let off6 = pdf.len();
    push_str!(format!("6 0 obj\n<< /Length {} >>\nstream\n", glyph_proc.len()));
    push_bytes!(glyph_proc);
    push_bytes!(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    push_str!(format!(
        "xref\n0 7\n\
         0000000000 65535 f \r\n\
         {off1:010} 00000 n \r\n\
         {off2:010} 00000 n \r\n\
         {off3:010} 00000 n \r\n\
         {off4:010} 00000 n \r\n\
         {off5:010} 00000 n \r\n\
         {off6:010} 00000 n \r\n"
    ));
    push_str!(format!(
        "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n"
    ));

    pdf
}

#[test]
fn type3_font_load_warning_is_not_reported_as_missing_glyph_ink() {
    assert!(
        install_pdf_render_diagnostics(),
        "no other component should own the tracing dispatcher in this test binary"
    );
    let _ = take_xberg_native_pdf_render_warnings();

    let pdf = build_type3_font_pdf();
    let png = render_pdf_page_to_png(&pdf, 0, Some(150), None).expect("a working Type 3 font page must render");
    assert!(!png.is_empty(), "renderer must produce page bytes");

    let warnings = take_xberg_native_pdf_render_warnings();

    // The Type 3 load diagnostic still fires and is still reported (GH#1794's fix must not
    // make the engine's diagnostics disappear), but never as a claim that ink is missing —
    // the rectangle was painted.
    let ink_missing_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("glyph ink is missing"))
        .collect();
    assert!(
        ink_missing_warnings.is_empty(),
        "a Type 3 font that painted its glyph must not be reported as missing glyph ink; got: {warnings:?}"
    );

    assert_eq!(
        warnings.len(),
        1,
        "the type3_font load diagnostic must still surface as exactly one warning; got: {warnings:?}"
    );
    let warning = &warnings[0];
    assert_eq!(warning.source, "pdf-render");
    assert!(
        warning.message.contains("did not affect the rendered output"),
        "warning must describe a font-load diagnostic, not content loss; got: {}",
        warning.message
    );
    assert!(
        warning.message.contains("Type 3"),
        "warning must still carry the engine's own cause text; got: {}",
        warning.message
    );
}
