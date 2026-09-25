//! GH#1752 residual — a full-page raster with visible glyphs is capped at 0.65, below the 0.70
//! default threshold, whenever the bilevel-codec + scanner-producer + visible-text conjunction
//! is the only thing separating it from an OCR sidecar. The reporter's deterministic reproducer
//! is a full-page CCITT G4 scan of typed text with a scanner `/Producer` and a tiny visible
//! Bates-stamp-style text stamp (`ABC000123`, 9 characters) in the corner. The fix (header-sized
//! native text counting toward the "no substantive visible text" signal, same as GH#1779) must
//! lift this well above the threshold, since a 9-character stamp is header-sized by any measure.
//!
//! Fixtures built from the issue's own `make_repro_1752.py` generator (shortened Gettysburg
//! Address text; functionally identical to the issue's reproducer, byte-different since the
//! generator embeds the local system font rather than the issue's exact fixed bytes).

#![cfg(feature = "pdf")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::DEFAULT_SCANNED_MIN_CONFIDENCE;
use xberg::{ExtractionConfig, FormatMetadata, PdfConfig};

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
fn ccitt_scan_with_scanner_producer_and_stamp_clears_the_default_threshold() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1752-ccitt-scanner-stamp.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE + SEPARATION_MARGIN,
        "a full-page CCITT scan with a scanner producer and a 9-character stamp must clear the \
         default threshold with margin, not sit at the 0.65 ceiling: got {confidence}"
    );
    assert!(
        selected,
        "the scanned-pages strategy must select this page at the default threshold"
    );
}

#[test]
fn authoring_producer_control_still_clears_the_threshold() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1752-control-authoring-producer.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE,
        "the scanner-producer bonus is a minor 0.05 nudge, not the deciding signal here: an \
         authoring producer with the same header-sized stamp must still clear the threshold: \
         got {confidence}"
    );
    assert!(selected);
}

#[test]
fn flate_1bit_control_still_clears_the_threshold() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1752-control-flate-1bit.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE,
        "the bilevel-codec bonus is a minor 0.10 nudge, not the deciding signal here: the same \
         bitmap as 1-bit FlateDecode with the same header-sized stamp must still clear the \
         threshold: got {confidence}"
    );
    assert!(selected);
}
