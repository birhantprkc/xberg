//! GH#1793 — `IMAGE_COVERAGE_MIN` zeroes any page whose images cover less than 80% of it,
//! before any other signal runs. An image-only page carrying a small native header/footer
//! (real-world reporter range: 22-76% coverage, median 66%, 41-92 native glyphs) is therefore
//! unreachable at any `scanned-pages` threshold. The fix must let such a page reach scoring
//! without also flagging a genuine text page that merely has a large figure on it.
//!
//! Fixtures built from the issue's own `make_repros.py` generator.

#![cfg(feature = "pdf")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::DEFAULT_SCANNED_MIN_CONFIDENCE;
use xberg::{ExtractionConfig, FormatMetadata, PdfConfig};

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
fn a_75_percent_coverage_image_with_header_sized_text_is_selected() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1793-image-75pct-with-header.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE,
        "75% image coverage with only header/footer text must clear the default threshold: \
         got {confidence}"
    );
    assert!(
        selected,
        "the scanned-pages strategy must select this page at the default threshold"
    );
}

#[test]
fn a_50_percent_coverage_image_with_header_sized_text_is_selected() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1793-image-50pct-with-header.pdf"));
    assert!(
        confidence >= DEFAULT_SCANNED_MIN_CONFIDENCE,
        "50% image coverage with only header/footer text must clear the default threshold \
         (the reporter's real corpus ranged 22-76%): got {confidence}"
    );
    assert!(
        selected,
        "the scanned-pages strategy must select this page at the default threshold"
    );
}

/// The regression this fix must not introduce: a page with a real, substantial native text
/// layer (12 lines, not header-sized) sitting next to a 50%-coverage figure is body text with
/// a figure, not a scan, however low its image coverage floor is set.
#[test]
fn native_text_with_a_figure_is_never_flagged_as_scanned() {
    let (confidence, selected) = scanned_confidence(&fixture("gh1793-control-native-text-with-figure.pdf"));
    assert_eq!(
        confidence, 0.0,
        "a page of real native text next to a figure must score exactly 0.0, not merely below \
         threshold: got {confidence}"
    );
    assert!(
        !selected,
        "this page must never be selected by the scanned-pages strategy"
    );
}
