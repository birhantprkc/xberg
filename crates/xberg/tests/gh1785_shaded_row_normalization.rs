//! Regression coverage for GH#1785: whole-page Otsu binarization drops a table row
//! whose background is shaded (a subtotal/total row rendered as dark text on a light
//! grey fill, or white text on a mid/dark grey fill) because the single whole-page
//! threshold does not survive the fill. `ImagePreprocessingConfig::normalize_shaded_rows`
//! (default `false`) adds a per-band step that stretches each shaded band to its own
//! dark-text-on-white polarity before binarization runs.
//!
//! The fixture (`gh1785_shaded_rows.png`) is the synthetic six-year forecast schedule
//! from the issue's own `make_repros.py` reproducer: a public-domain-shaped synthetic
//! page with a light-fill "Subtotal" row, no real document content. It was regenerated
//! on macOS for this repository (the issue's own script notes its output depends on the
//! installed font, Arial here vs. DejaVu Sans on the Linux box the issue was filed
//! from), so this file's exact OCR text differs from the issue's reported numbers, but
//! the presence/absence of the shaded "SUBTOTAL" rows is the property under test, not
//! exact digit accuracy.
//!
//! This mirrors the issue's own verification (`grep -ci subtotal`): with the default
//! config the light-fill "SUBTOTAL ..." rows are dropped entirely (zero occurrences);
//! with `normalize_shaded_rows: true` they are read back.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: org logging policy exempts tests
#![cfg(feature = "ocr")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::{ExtractionConfig, OcrConfig};
use xberg::types::{ImagePreprocessingConfig, TesseractConfig};

const SHADED_ROWS_FIXTURE: &[u8] = include_bytes!("fixtures/ocr/gh1785_shaded_rows.png");

/// The three light-fill "Subtotal ..." row labels baked into the fixture by
/// `make_repros.py`'s `SHADED_ROWS` table (kind `"sub"`).
const SUBTOTAL_ROW_COUNT: usize = 3;

fn config_with_normalization(normalize_shaded_rows: bool) -> ExtractionConfig {
    ExtractionConfig {
        ocr: Some(OcrConfig {
            backend: "tesseract".to_string(),
            language: vec!["eng".to_string()],
            tesseract_config: Some(TesseractConfig {
                use_cache: false,
                preprocessing: Some(ImagePreprocessingConfig {
                    normalize_shaded_rows,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        force_ocr: true,
        use_cache: false,
        ..Default::default()
    }
}

fn subtotal_occurrences(text: &str) -> usize {
    text.to_ascii_uppercase().matches("SUBTOTAL").count()
}

/// RED before the fix, GREEN after: default preprocessing (`normalize_shaded_rows:
/// false`, the struct's `Default`) must still drop every light-fill "Subtotal" row, so
/// this test also documents the baseline the opt-in step is measured against.
#[test]
fn default_preprocessing_drops_every_shaded_subtotal_row() {
    let config = config_with_normalization(false);
    let result =
        extract_bytes_document_blocking(SHADED_ROWS_FIXTURE, "image/png", &config).expect("OCR must not error");

    assert_eq!(
        subtotal_occurrences(&result.content),
        0,
        "whole-page Otsu binarization is expected to drop every shaded SUBTOTAL row; \
         got content:\n{}",
        result.content
    );
}

/// The fix: with `normalize_shaded_rows: true`, every light-fill "Subtotal" row must be
/// present in the OCR output.
#[test]
fn normalize_shaded_rows_recovers_the_shaded_subtotal_rows() {
    let config = config_with_normalization(true);
    let result =
        extract_bytes_document_blocking(SHADED_ROWS_FIXTURE, "image/png", &config).expect("OCR must not error");

    assert_eq!(
        subtotal_occurrences(&result.content),
        SUBTOTAL_ROW_COUNT,
        "normalize_shaded_rows must recover all {SUBTOTAL_ROW_COUNT} shaded SUBTOTAL rows; \
         got content:\n{}",
        result.content
    );
}

/// `ImagePreprocessingConfig::default()` must keep `normalize_shaded_rows: false`: it is
/// an opt-in step (GH#1785's own measurement shows it can regress a mid-fill row style
/// the per-band step does not fully model), not a new default binarization behavior.
#[test]
fn normalize_shaded_rows_defaults_to_false() {
    assert!(!ImagePreprocessingConfig::default().normalize_shaded_rows);
}
