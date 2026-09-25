//! GH#1779 — a full-page scan with a native header/footer and a decorative background under a
//! full page of native text score identically (0.50), so no `scanned-pages` threshold separates
//! them. The fix must give the scan-with-header a score above `DEFAULT_SCANNED_MIN_CONFIDENCE`
//! and keep the decorative-background page below it, with a real margin between the two.
//!
//! Fixtures built from the issue's own `make_repros.py` generator (Linux hashes given there; the
//! generator itself notes the text-bearing files differ by host font, so only
//! `gh1779-control-decorative-background.pdf` — whose native text uses the standard Helvetica
//! font rather than a rendered image — reproduces the issue's exact sha256
//! `048d670c3eaefd4d066058279e38b23e2269f6b1a9460125bc5216ed11d1a854`).

#![cfg(feature = "pdf")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::DEFAULT_SCANNED_MIN_CONFIDENCE;
use xberg::{ExtractionConfig, FormatMetadata, PdfConfig};

/// How far apart the two classes must land around the default threshold. Not a magic
/// tolerance: it is the minimum gap that makes the separation robust to small scoring
/// changes elsewhere in the matrix (bilevel/producer bonuses are 0.05-0.10 each). ~keep
const SEPARATION_MARGIN: f64 = 0.10;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf/regressions/scan_detect")
        .join(name)
}

fn scanned_confidence(path: &std::path::Path) -> (f64, bool) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    let config = ExtractionConfig {
        disable_ocr: true,
        use_cache: false,
        enable_quality_processing: false,
        pdf_options: Some(PdfConfig {
            extract_tables: false,
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = extract_bytes_document_blocking(&bytes, "application/pdf", &config)
        .unwrap_or_else(|e| panic!("extract fixture {path:?}: {e}"));
    let Some(FormatMetadata::Pdf(pdf_metadata)) = result.metadata.format else {
        panic!("fixture {path:?} produced no PDF-specific metadata");
    };
    let confidence = pdf_metadata
        .scanned_confidence
        .unwrap_or_else(|| panic!("fixture {path:?} produced no scanned_confidence"));
    let selected = pdf_metadata.scanned_pages.is_some_and(|pages| pages.contains(&1));
    (f64::from(confidence), selected)
}

#[test]
fn scan_with_header_clears_the_default_threshold() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1779-scan-with-header.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE + SEPARATION_MARGIN,
        "a full-page scan with only header/footer-sized native text must clear the default \
         threshold with margin: got {confidence}, threshold {DEFAULT_SCANNED_MIN_CONFIDENCE}"
    );
    assert!(
        selected,
        "the scanned-pages strategy at the default threshold must select this page"
    );
}

#[test]
fn decorative_background_stays_below_the_default_threshold() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1779-control-decorative-background.pdf"));
    assert!(
        confidence <= DEFAULT_SCANNED_MIN_CONFIDENCE - SEPARATION_MARGIN,
        "a decorative full-page background under a full page of real native text must stay \
         below the default threshold with margin: got {confidence}, threshold {DEFAULT_SCANNED_MIN_CONFIDENCE}"
    );
    assert!(
        !selected,
        "the scanned-pages strategy at the default threshold must NOT OCR a page of real \
         native text merely because it sits over a full-bleed image"
    );
}

#[test]
fn the_two_classes_are_separated_by_a_real_margin() {
    let (header_confidence, _) = scanned_confidence(&fixture("gh1779-scan-with-header.pdf"));
    let (decorative_confidence, _) = scanned_confidence(&fixture("gh1779-control-decorative-background.pdf"));
    assert!(
        header_confidence - decorative_confidence >= SEPARATION_MARGIN,
        "scan-with-header ({header_confidence}) must score at least {SEPARATION_MARGIN} above \
         decorative-background ({decorative_confidence}); no single threshold can separate two \
         scores this close together"
    );
}
