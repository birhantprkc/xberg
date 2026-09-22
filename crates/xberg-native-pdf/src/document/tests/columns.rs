use super::super::*;
use super::common::*;
use super::pdf_fixtures::*;
use tracing::Level;

/// An extra span dropped onto a dominant-rotation page must land in the
/// same reading-order slot on every surface: the extras merge before the
/// reading-frame map. Mapping base spans around an unmapped extra files
/// the extra text after the page in md/html but mid-page in text.
#[test]
fn extra_span_shares_the_reading_frame_on_every_surface() {
    // Three 90°-rotated lines; the mapped frame puts them at
    // y' = 612 - x: ALPHA 412, BRAVO 384, CHARLIE 356. The unrotated
    // extra at (242, 100) maps to y' = 370 — between BRAVO and CHARLIE.
    let mut content: Vec<u8> = b"BT /F1 10 Tf\n".to_vec();
    for (x, text) in [(200, "ALPHA"), (228, "BRAVO"), (256, "CHARLIE")] {
        content.extend_from_slice(format!("0 1 -1 0 {x} 150 Tm ({text}) Tj\n").as_bytes());
    }
    content.extend_from_slice(b"ET");

    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; 6];
    let obj = |buf: &mut Vec<u8>, off: &mut Vec<usize>, id: usize, body: &str| {
        off[id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    buf.extend_from_slice(b"%PDF-1.4\n");
    obj(&mut buf, &mut off, 1, "<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut buf, &mut off, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut buf,
        &mut off,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>",
    );
    off[4] = buf.len();
    buf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    buf.extend_from_slice(&content);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    obj(
        &mut buf,
        &mut off,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    let xref = buf.len();
    buf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for id in 1..=5 {
        buf.extend_from_slice(format!("{:010} 00000 n \n", off[id]).as_bytes());
    }
    buf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{xref}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(buf).unwrap();
    let extra = crate::layout::TextSpan {
        text: "INSERTED".to_string(),
        bbox: crate::geometry::Rect::new(242.0, 100.0, 50.0, 10.0),
        font_size: 10.0,
        ..Default::default()
    };
    let opts = crate::converters::ConversionOptions {
        extract_tables: true,
        ..Default::default()
    };

    let slot = |out: &str, surface: &str| {
        let (a, b, ins, c) = (
            out.find("ALPHA"),
            out.find("BRAVO"),
            out.find("INSERTED"),
            out.find("CHARLIE"),
        );
        assert!(
            a.is_some() && a < b && b < ins && ins < c,
            "{surface}: expected ALPHA<BRAVO<INSERTED<CHARLIE: {out:?}"
        );
    };
    slot(
        &doc.extract_text_with_extra_spans(0, vec![extra], &opts).unwrap(),
        "extract_text_with_extra_spans",
    );
}

/// A positive gap is a genuine word boundary, never a kerning overlap.
#[test]
fn test_reliable_kerning_overlap_requires_negative_gap() {
    let mut prev = make_test_span("Hello", 0.0, 100.0, 28.0, 10.0);
    prev.char_widths = vec![6.0, 3.0, 3.0, 3.0, 6.0];
    let span = make_test_span("World", 32.0, 100.0, 28.0, 10.0);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width); // +4pt gap ~keep
    assert!(!PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap));
}

#[test]
fn topo_two_column_prose_reads_column_major() {
    let spans = two_dense_columns(true, true);
    let out = PdfDocument::topological_block_order(&spans).expect("genuine two-column prose must be reordered");
    assert_eq!(out.len(), spans.len());
    let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
    let last_left = texts.iter().rposition(|t| t.starts_with("left")).unwrap();
    let first_right = texts.iter().position(|t| t.starts_with("right")).unwrap();
    // Whole left column before whole right column (de-interleaved). ~keep
    assert!(last_left < first_right, "columns interleaved: {texts:?}");
}

#[test]
fn topo_tight_leading_two_columns_stay_separate() {
    // Two dense columns whose gutter (18 pt) is NARROWER than med_h (font
    // size 20): the same-row gap (18) is below med_h*1.0 (20), so WITHOUT the
    // Item 4 gutter veto the union-find fuses left+right into one block and
    // the side_by_side gate then declines → None → row-major interleave. With
    // the veto the two columns stay separate and read column-major. ~keep
    let mut spans = Vec::new();
    for k in 0..8 {
        let y = 200.0 - k as f32 * 24.0;
        spans.push(make_test_span(
            &format!("left column body sentence number {k} here"),
            0.0,
            y,
            90.0,
            20.0,
        ));
        spans.push(make_test_span(
            &format!("right column body sentence number {k} here"),
            108.0,
            y,
            90.0,
            20.0,
        )); // gutter 108-90 = 18 (< med_h 20) ~keep
    }
    let out =
        PdfDocument::topological_block_order(&spans).expect("tight-gutter two columns must stay separate and reorder");
    let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
    let last_left = texts.iter().rposition(|t| t.starts_with("left")).unwrap();
    let first_right = texts.iter().position(|t| t.starts_with("right")).unwrap();
    assert!(last_left < first_right, "tight-gutter columns fused: {texts:?}");
}

#[test]
fn topo_single_column_returns_none() {
    let mut spans = Vec::new();
    for k in 0..10 {
        let y = 200.0 - k as f32 * 12.0;
        spans.push(make_test_span(
            &format!("single column body line {k} of running text"),
            0.0,
            y,
            190.0,
            10.0,
        ));
    }
    // No side-by-side region → unchanged (row-aware path). ~keep
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

#[test]
fn topo_toc_sparse_page_number_column_returns_none() {
    // Left = chapter titles (dense), right = page numbers (sparse). The
    // text-density gate must reject it so a TOC is not read column-major
    // (which would divorce each title from its page number). ~keep
    let spans = two_dense_columns(true, false);
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

#[test]
fn topo_fragmented_table_returns_none() {
    // Two dense columns PLUS many isolated single-token fragment blocks
    // (page numbers / cell labels) the union-find cannot coalesce — the
    // signature of a structured table (chess diagram, data grid), which must
    // stay row-aware rather than be read column-major. ~keep
    let mut spans = two_dense_columns(true, true);
    for k in 0..10 {
        // Widely scattered short tokens → each its own fragment block. ~keep
        let x = 230.0 + (k as f32) * 30.0;
        let y = 205.0 - (k as f32) * 17.0;
        spans.push(make_test_span(&format!("{}", k * 7), x, y, 8.0, 10.0));
    }
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

// A page is vertical-writing only when a majority of non-empty spans carry
// WMode 1 — authoritative, so horizontal pages are never misclassified. ~keep
#[test]
fn test_page_is_vertical() {
    let v = [
        span_wmode(0.0, 0.0, 1),
        span_wmode(0.0, 0.0, 1),
        span_wmode(0.0, 0.0, 0),
    ];
    assert!(PdfDocument::page_is_vertical(&v));
    let h = [
        span_wmode(0.0, 0.0, 0),
        span_wmode(0.0, 0.0, 0),
        span_wmode(0.0, 0.0, 1),
    ];
    assert!(!PdfDocument::page_is_vertical(&h));
    // Exact tie is not a majority — stay horizontal (conservative). ~keep
    let tie = [span_wmode(0.0, 0.0, 1), span_wmode(0.0, 0.0, 0)];
    assert!(!PdfDocument::page_is_vertical(&tie));
    assert!(!PdfDocument::page_is_vertical(&[]));
    let mut blank = span_wmode(0.0, 0.0, 1);
    blank.text = "   ".to_string();
    assert!(!PdfDocument::page_is_vertical(std::slice::from_ref(&blank)));
}

// Horizontal pages use the top/bottom band; vertical pages ALSO use the
// left/right band. The side band is additive — it never removes the
// top/bottom membership a horizontal page relies on. ~keep
#[test]
fn test_in_chrome_band() {
    let (w, h) = (612.0_f32, 792.0_f32); // vband=95.04, hband=73.44 ~keep
    let top = crate::geometry::Rect::new(300.0, 780.0, 12.0, 12.0);
    let bottom = crate::geometry::Rect::new(300.0, 10.0, 12.0, 12.0);
    let middle = crate::geometry::Rect::new(300.0, 400.0, 12.0, 12.0);
    let left = crate::geometry::Rect::new(10.0, 400.0, 12.0, 12.0);
    let right = crate::geometry::Rect::new(600.0, 400.0, 12.0, 12.0);

    // Top/bottom are chrome in BOTH modes; middle is never chrome. ~keep
    for vertical in [false, true] {
        assert!(PdfDocument::in_chrome_band(&top, w, h, vertical));
        assert!(PdfDocument::in_chrome_band(&bottom, w, h, vertical));
        assert!(!PdfDocument::in_chrome_band(&middle, w, h, vertical));
    }
    // Side strips: chrome only when vertical. ~keep
    assert!(!PdfDocument::in_chrome_band(&left, w, h, false));
    assert!(!PdfDocument::in_chrome_band(&right, w, h, false));
    assert!(PdfDocument::in_chrome_band(&left, w, h, true));
    assert!(PdfDocument::in_chrome_band(&right, w, h, true));
}

// Bare page-number detection (applied only inside the margin band). ~keep
#[test]
fn test_is_bare_page_number_text() {
    for yes in ["1", "12", "999", "1000", "9999", " 7 ".trim()] {
        assert!(
            PdfDocument::is_bare_page_number_text(yes),
            "{yes:?} should be a page number"
        );
    }
    for no in ["", "0", "10000", "12345", "1a", "iv", "Page", "1.2", "-1", "1,2"] {
        assert!(
            !PdfDocument::is_bare_page_number_text(no),
            "{no:?} must NOT be a page number"
        );
    }
}

// Non-Latin folio digits are recognized as bare page numbers, bounded by
// character count (each is 2-3 UTF-8 bytes) and range-checked via the
// block-offset map (parse/to_digit are ASCII-only). ~keep
#[test]
fn test_is_bare_page_number_text_non_latin() {
    for yes in [
        "\u{0661}",                         // Arabic-Indic ١ = 1 ~keep
        "\u{0661}\u{0662}",                 // ١٢ = 12 ~keep
        "\u{06F3}",                         // Persian ۳ = 3 ~keep
        "\u{06F1}\u{06F2}\u{06F3}",         // ۱۲۳ = 123 ~keep
        "\u{0967}\u{0966}",                 // Devanagari १० = 10 ~keep
        "\u{FF11}\u{FF12}\u{FF13}\u{FF14}", // full-width １２３４ = 1234 ~keep
    ] {
        assert!(
            PdfDocument::is_bare_page_number_text(yes),
            "{yes:?} should be a non-Latin page number"
        );
    }
    for no in [
        "\u{0660}",
        // ~keep
        "\u{FF11}\u{FF10}\u{FF10}\u{FF10}\u{FF10}",
        // ~keep
        "\u{4E00}",  // CJK 一 — ideographic, intentionally excluded ~keep
        "\u{0661}a", // digit + letter ~keep
    ] {
        assert!(
            !PdfDocument::is_bare_page_number_text(no),
            "{no:?} must NOT be a page number"
        );
    }
}

// Folios paginated in non-Latin digits collapse to a shared signature, so
// the varying-literal gate (variants >= 2) can fire. ~keep
#[test]
fn test_normalize_artifact_signature_non_latin_digits() {
    // Persian "صفحه ۱" and "صفحه ۲" must share one signature. ~keep
    let s1 = PdfDocument::normalize_artifact_signature("\u{0635}\u{0641}\u{062D}\u{0647} \u{06F1}");
    let s2 = PdfDocument::normalize_artifact_signature("\u{0635}\u{0641}\u{062D}\u{0647} \u{06F2}");
    assert_eq!(s1, s2, "Persian folios must collapse to one signature");
    assert!(s1.contains('#'), "digit run must collapse to # (got {s1:?})");

    // Full-width "第１頁" / "第２頁" share a signature; multi-digit runs
    // collapse to a single #. ~keep
    let f1 = PdfDocument::normalize_artifact_signature("\u{FF11}\u{FF10}");
    assert_eq!(f1, "#", "full-width digit run collapses to a single #");

    // CJK ideographic numerals are NOT digits: "第一章" must stay intact
    // so real headings are not over-normalized. ~keep
    let heading = PdfDocument::normalize_artifact_signature("\u{7B2C}\u{4E00}\u{7AE0}");
    assert_eq!(
        heading, "\u{7B2C}\u{4E00}\u{7AE0}",
        "ideographic numerals must not collapse"
    );
}

#[test]
fn test_looks_like_stable_pagination() {
    for yes in [
        "https://doi.org/10.1234/abcd",
        "doi:10.1000/xyz",
        "Volume 14 | Article 153",
        "Vol. 7, No. 3",
        "www.frontiersin.org 1",
    ] {
        assert!(
            PdfDocument::looks_like_stable_pagination(yes),
            "{yes:?} should be stable pagination furniture"
        );
    }
    for no in [
        "Acme Regional Hospital",
        "Jane A. Doe",
        "Introduction",
        "Table 3",
        "Department of Neuroscience 2024",
        "volume of distribution", // citation keyword but NO digit ~keep
    ] {
        assert!(
            !PdfDocument::looks_like_stable_pagination(no),
            "{no:?} must NOT be classified as furniture"
        );
    }
}

#[test]
fn corrupt_optional_content_stream_warns_with_operation_identity() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << >> >>\nendobj\n",
    );
    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n[5 0 R 6 0 R]\nendobj\n");
    let corrupt = b"This is not zlib compressed data";
    let off5 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            corrupt.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(corrupt);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let valid = b"Q";
    let off6 = pdf.len();
    pdf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", valid.len()).as_bytes());
    pdf.extend_from_slice(valid);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for offset in [off1, off2, off3, off4, off5, off6] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let (result, events) = capture_events(|| doc.get_page_content_data(0));

    assert_eq!(result.unwrap().as_slice(), b"Q\n");
    let warning = events.iter().find(|event| {
        event.level == Level::WARN
            && event.target == crate::LOG_TARGET_ROOT
            && event.fields.get("operation").map(String::as_str) == Some("decode_optional_page_content")
    });
    let warning = warning.unwrap_or_else(|| panic!("missing optional-content warning: {events:#?}"));
    assert_eq!(warning.fields.get("page_index").map(String::as_str), Some("0"));
    assert_eq!(
        warning.fields.get("error_code").map(String::as_str),
        Some("decode_error")
    );
    assert!(!warning.fields.contains_key("error"));
    assert!(events.iter().all(|event| event.level != Level::ERROR));
}

#[test]
fn corrupt_mandatory_xref_stream_errors_at_open_boundary() {
    let corrupt = b"This is not zlib compressed data";
    let mut pdf = b"%PDF-1.5\n".to_vec();
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!(
            "1 0 obj\n<< /Type /XRef /Size 2 /W [1 2 1] /Length {} /Filter /FlateDecode >>\nstream\n",
            corrupt.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(corrupt);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    let (result, events) = capture_events(|| PdfDocument::from_bytes(pdf));

    assert!(result.is_err(), "a corrupt mandatory xref stream must fail open");
    let failures: Vec<_> = events
        .iter()
        .filter(|event| {
            event.level == Level::ERROR
                && event.target == format!("{}::document", crate::LOG_TARGET_ROOT)
                && event.fields.contains_key("error_code")
        })
        .collect();
    assert_eq!(failures.len(), 1, "expected one boundary error: {events:#?}");
    assert_eq!(
        events.iter().filter(|event| event.level == Level::ERROR).count(),
        1,
        "fatal xref failure must emit exactly one ERROR: {events:#?}"
    );
    assert_eq!(
        failures[0].fields.get("error_code").map(String::as_str),
        Some("invalid_pdf")
    );
    let reconstruction = events.iter().find(|event| {
        event.level == Level::WARN
            && event.target == crate::LOG_TARGET_ROOT
            && event.fields.get("operation").map(String::as_str) == Some("reconstruct_xref")
    });
    let reconstruction = reconstruction.unwrap_or_else(|| panic!("missing reconstruction context: {events:#?}"));
    assert!(reconstruction.fields.contains_key("primary_error_code"));
    assert!(reconstruction.fields.contains_key("reconstruction_error_code"));
}

/// A shared-baseline two-column prose body (academic references): each line
/// has scattered word-granular left edges in BOTH columns — so the
/// dominant-cluster-fraction gate misses it — but a single persistent
/// central gutter. The corridor accept path must route it as multi-column.
#[test]
fn test_corridor_accepts_scattered_two_column_prose() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem", 50.0, y, 35.0));
        spans.push(corridor_span("ipsumdolor", 95.0, y, 40.0));
        spans.push(corridor_span("sitametco", 140.0, y, 40.0));
        spans.push(corridor_span("consectetur", 300.0, y, 40.0));
        spans.push(corridor_span("adipiscing", 345.0, y, 40.0));
        spans.push(corridor_span("elitsedo", 390.0, y, 40.0));
    }
    assert!(
        PdfDocument::is_multi_column_page(&spans),
        "scattered-edge two-column prose with a persistent central gutter \
             must be detected as multi-column via the corridor accept path"
    );
}

/// A short-cell numeric table shares one column gap but has tiny cells
/// (mean chars per line well below 20). The prose guard must reject it so
/// the table is NOT routed to XY-cut (which would reorder its cells).
#[test]
fn test_corridor_rejects_short_cell_table() {
    let mut spans = Vec::new();
    // Scattered left edges (so the bimodal-line-start detector does NOT
    // fire and the dominant-cluster gate fails — i.e. control reaches the
    // corridor path), but every cell is a short numeric token so the
    // per-line mean char count stays well under the prose floor of 20. ~keep
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("12", 50.0, y, 12.0));
        spans.push(corridor_span("34", 95.0, y, 12.0));
        spans.push(corridor_span("56", 140.0, y, 12.0));
        spans.push(corridor_span("78", 300.0, y, 12.0));
        spans.push(corridor_span("90", 345.0, y, 12.0));
        spans.push(corridor_span("12", 390.0, y, 12.0));
    }
    assert!(
        !PdfDocument::is_multi_column_page(&spans),
        "short-cell numeric table must NOT be routed as multi-column \
             (grid-row discriminator rejects ≥2-gap rows)"
    );
}

/// Part 1a: a SHORT-line two-column verse body (Bible / lexicon) — one
/// short fragment per column, one central gutter per line — used to be
/// rejected by the raw `mean_chars <= 20` floor. It must now be admitted via
/// the corridor's short-line path (single gap/line, balanced, central).
#[test]
fn test_corridor_accepts_short_verse_two_column() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Bereshit", 50.0, y, 45.0));
        spans.push(corridor_span("barahem", 300.0, y, 40.0));
    }
    // Call the corridor directly (bypass the upstream bimodal/histogram
    // gates) with a no-op degenerate-CTM filter. ~keep
    assert!(
        PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "short-verse two-column body (1 gutter/line, balanced) must be admitted"
    );
}

/// Part 1a guard: a lopsided narrow-label + wide-data table must stay
/// rejected even though it has one gap per line — its gutter sits off-centre
/// (failing the centre gate) and its columns are lopsided (failing the
/// char-mass balance), either of which is sufficient.
#[test]
fn test_corridor_rejects_label_column_table() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("1", 50.0, y, 8.0));
        spans.push(corridor_span("Descriptionlongdata", 300.0, y, 200.0));
    }
    assert!(
        !PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "lopsided narrow-label + wide-data table must be rejected (char balance)"
    );
}

/// Part 1b: a two-column prose body interleaved with a MINORITY of
/// full-width display-math / heading rows must still be detected — the
/// full-width rows are excluded from the coverage denominator. Without the
/// exclusion the coverage floor (best_size*2 >= lines) fails.
#[test]
fn test_corridor_survives_minority_display_math() {
    let mut spans = Vec::new();
    for i in 0..16 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor", 50.0, y, 120.0));
        spans.push(corridor_span("sit amet consectetur", 300.0, y, 150.0));
    }
    for i in 0..24 {
        let y = 400.0 - i as f32 * 14.0;
        spans.push(corridor_span("Section heading spanning width", 50.0, y, 400.0));
    }
    assert!(
        PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "two-column prose with a minority of full-width display rows must hold"
    );
}

#[test]
fn measure_gutter_accepts_centered_two_columns() {
    // Left col →170, right col 300→450: a wide central corridor at ~235,
    // which is 0.46 of the [50,450] content width (inside 0.30..=0.70). ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor", 50.0, y, 120.0));
        spans.push(corridor_span("sit amet consectetur", 300.0, y, 150.0));
    }
    let g = PdfDocument::measure_single_central_gutter(&spans);
    assert!(g.is_some(), "centered two columns must yield a gutter");
    assert!((g.unwrap() - 235.0).abs() < 5.0, "gutter mid-x ≈ 235, got {g:?}");
}

#[test]
fn measure_gutter_rejects_single_column() {
    // One full-width column → no corridor → None (byte-identical caller). ~keep
    let mut spans = Vec::new();
    for i in 0..10 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Full width single column line", 50.0, y, 400.0));
    }
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn measure_gutter_rejects_three_column_grid() {
    // Three columns ⇒ two corridors ⇒ not a single central gutter ⇒ None
    // (grids/tables stay on their existing row-aware/structural path). ~keep
    let mut spans = Vec::new();
    for i in 0..6 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("colA", 50.0, y, 60.0));
        spans.push(corridor_span("colB", 140.0, y, 60.0));
        spans.push(corridor_span("colC", 230.0, y, 60.0));
    }
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_finds_tight_gutter_under_bridging_header() {
    // Dense two columns separated by a TIGHT ~12 pt gutter (left →291,
    // right 304→552), below the 18 pt cover-scan threshold, PLUS one
    // full-width header line that bridges the gutter (43→308). The 1-D cover
    // scan jumps its running max past the corridor and misses it; the 2-D
    // projection only counts spans that actually straddle a given x, so the
    // lone header is absorbed by the tolerance and the gutter at ~297 is
    // found. This is the PMC8129076 defect-2 case. ~keep
    let mut spans = Vec::new();
    spans.push(corridor_span("NATURE COMMUNICATIONS header line", 43.0, 760.0, 265.0));
    for i in 0..16 {
        let y = 740.0 - i as f32 * 12.0;
        spans.push(corridor_span("left column body text here ok", 43.0, y, 248.0));
        spans.push(corridor_span("right column body text here ok", 304.0, y, 248.0));
    }
    let g = PdfDocument::density_central_gutter(&spans);
    assert!(g.is_some(), "tight gutter under a bridging header must be found");
    assert!((g.unwrap() - 297.5).abs() < 8.0, "gutter mid-x ≈ 297, got {g:?}");
    // The conservative cover scan misses this (gutter < 18 pt and a header
    // bridges it), confirming the density probe adds genuinely new coverage. ~keep
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_single_column() {
    let mut spans = Vec::new();
    for i in 0..16 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("Full width single column line of text", 50.0, y, 400.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_three_column_grid() {
    let mut spans = Vec::new();
    for i in 0..12 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("colA", 50.0, y, 60.0));
        spans.push(corridor_span("colB", 140.0, y, 60.0));
        spans.push(corridor_span("colC", 230.0, y, 60.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_degenerate_ctm_content_width() {
    // Two "columns" separated by a 200,000pt gap — the signature of a
    // degenerate CTM scale factor inflating span x-coordinates, not a
    // real page (a normal page is at most a few thousand points wide).
    // Before the MAX_CONTENT_EXTENT bound, the huge empty middle region
    // was itself picked up as a single "corridor" and returned as a
    // (nonsensical) gutter position; it must now be rejected outright. ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("left col text here", 50.0, y, 60.0));
        spans.push(corridor_span("right col text here", 200_050.0, y, 60.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn classifier_gutter_rejects_degenerate_ctm_content_width() {
    // Same degenerate-CTM hazard as the density-probe test above, for
    // `classifier_column_gutter`'s independent content_w computation. ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("left col text here", 50.0, y, 60.0));
        spans.push(corridor_span("right col text here", 200_050.0, y, 60.0));
    }
    assert!(PdfDocument::classifier_column_gutter(&spans).is_none());
}

#[test]
fn block_char_density_separates_dense_from_sparse() {
    // 5 lines of prose (~21 chars/line) is DENSE; 5 lines of bare numbers
    // (~2 chars/line) is SPARSE. med_h = 10 (corridor_span height). ~keep
    let dense: Vec<_> = (0..5)
        .map(|i| corridor_span("twenty chars of text!", 50.0, 700.0 - i as f32 * 14.0, 120.0))
        .collect();
    let sparse: Vec<_> = (0..5)
        .map(|i| corridor_span("12", 50.0, 700.0 - i as f32 * 14.0, 12.0))
        .collect();
    let dref: Vec<&_> = dense.iter().collect();
    let sref: Vec<&_> = sparse.iter().collect();
    let dd = PdfDocument::block_char_density(&dref, 10.0);
    let sd = PdfDocument::block_char_density(&sref, 10.0);
    assert!(dd > 15.0, "dense density should be high, got {dd}");
    assert!(sd < 4.0, "sparse density should be low, got {sd}");
    assert!(dd > sd * 4.0, "dense must clearly exceed sparse");
}

#[test]
fn lift_marginalia_column_lifts_left_line_numbers() {
    let mut spans = Vec::new();
    for i in 0..14 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Body prose line of real text here", 100.0, y, 300.0));
        spans.push(corridor_span(&format!("{}", 118 + i), 50.0, y, 15.0));
    }
    let lifted = PdfDocument::lift_marginalia_column(&spans).expect("rail must be lifted");
    assert_eq!(lifted.len(), 14, "all 14 line numbers lifted");
    for &i in &lifted {
        assert!(
            spans[i].text.trim().chars().all(|c| c.is_ascii_digit()),
            "lifted span must be a numeral: {:?}",
            spans[i].text
        );
    }
}

#[test]
fn lift_marginalia_column_skips_dense_first_column() {
    // Genuine two-column prose: both columns are wide + multi-word, so
    // neither sits inside the narrow outer strip → no lift (byte-identical). ~keep
    let mut spans = Vec::new();
    for i in 0..14 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor sit", 50.0, y, 120.0));
        spans.push(corridor_span("amet consectetur elit", 300.0, y, 150.0));
    }
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}

#[test]
fn lift_marginalia_column_skips_abutting_label_column() {
    // A narrow short-token left column whose gutter to the body is < 18 pt
    // (an abutting table label column, not a detached rail) → no lift. ~keep
    let mut spans = Vec::new();
    for i in 0..10 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("AB", 50.0, y, 15.0));
        spans.push(corridor_span("Body prose text here long", 70.0, y, 200.0));
    }
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}

#[test]
fn lift_marginalia_column_skips_single_page_number() {
    // A single stray numeral (1 rail line) fails the ≥3-line gate → no lift. ~keep
    let mut spans = Vec::new();
    for i in 0..12 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Body prose line of real text", 100.0, y, 300.0));
    }
    spans.push(corridor_span("7", 50.0, 500.0, 12.0));
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}
