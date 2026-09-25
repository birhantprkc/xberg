//! Regression coverage for GH#1789: repair OCR tokens that are clearly numeric but
//! mis-punctuated -- a dropped thousands separator, a decimal point misread for a grouping
//! comma, or a single number split into two tokens at a rendering gap. The rule logic itself
//! (`repair_ocr_numeric_tokens`) has dense unit coverage next to `repair_ocr_list_markers` in
//! `crates/xberg/src/extractors/pdf/ocr/recognition_noise_tests.rs`; this file covers the
//! config wiring: `OcrConfig::numeric_repair` (default `false`) must gate the repair end to
//! end through a real OCR extraction, exactly as `ImagePreprocessingConfig::normalize_shaded_rows`
//! gates GH#1785's shaded-row normalization.
//!
//! This machine's Tesseract FAST build (bundled with the `ocr` feature) reads the shared
//! `gh1785_shaded_rows.png` fixture with zero numeric misreads (verified separately by
//! printing the raw OCR text): there is no dropped separator, misread decimal point, or split
//! token to repair on this platform/font/model combination -- the issue's own measurements
//! show the effect is small on FAST and requires the `tessdata_best` model to reproduce at
//! scale, and downloading that model is out of scope for a fixture-based regression test. That
//! makes this fixture a strong test of the property that matters most here: with the repair
//! turned on against a page Tesseract already read correctly, every already-correct value must
//! survive byte-for-byte. If `numeric_repair: true` ever starts rewriting a value this fixture
//! gets right today, this test catches it immediately.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: org logging policy exempts tests
#![cfg(feature = "ocr")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::{ExtractionConfig, OcrConfig};
use xberg::types::TesseractConfig;

const SHADED_ROWS_FIXTURE: &[u8] = include_bytes!("fixtures/ocr/gh1785_shaded_rows.png");
/// A single line, "Total 1172 units", rendered at 300 dpi with no thousands separator --
/// digits Tesseract reads correctly as-is, giving a deterministic, always-reproducible stand-in
/// for the dropped-separator failure shape without depending on `tessdata_best` (the issue's
/// own repro needs that model to reproduce the shape reliably; this fixture needs nothing
/// beyond the bundled FAST data because there is no separator to drop in the first place -- the
/// point is that `numeric_repair` adds one where none exists in a bare 4-digit integer).
const NUMBER_TEST_FIXTURE: &[u8] = include_bytes!("fixtures/ocr/gh1789_number_test.png");

/// Every already-correctly-grouped value baked into the fixture by `make_repros.py`'s
/// `SHADED_ROWS` table. Used to assert the repair changes none of them when enabled.
const ALREADY_CORRECT_VALUES: &[&str] = &[
    "48,210", "49,850", "51,344", "52,885", "54,471", "56,105", "21,406", "21,977", "22,636", "23,315", "24,015",
    "24,735", "7,812", "7,968", "8,127", "8,290", "8,456", "8,625", "3,104", "3,197", "3,293", "3,391", "3,493",
    "3,598", "80,532", "82,992", "85,400", "87,881", "90,435", "93,063", "5,103", "5,256", "5,413", "5,576", "5,743",
    "5,915",
];

fn config_with_numeric_repair(numeric_repair: bool) -> ExtractionConfig {
    ExtractionConfig {
        ocr: Some(OcrConfig {
            backend: "tesseract".to_string(),
            language: vec!["eng".to_string()],
            tesseract_config: Some(TesseractConfig {
                use_cache: false,
                ..Default::default()
            }),
            numeric_repair,
            ..Default::default()
        }),
        force_ocr: true,
        use_cache: false,
        ..Default::default()
    }
}

/// `OcrConfig::default()` must keep `numeric_repair: false`: like GH#1785's
/// `normalize_shaded_rows`, this is an opt-in repair with a stated locale assumption
/// (US/UK grouping), not a new default OCR post-processing step.
#[test]
fn numeric_repair_defaults_to_false() {
    assert!(!OcrConfig::default().numeric_repair);
}

/// The zero-corruption property, measured end to end through real OCR rather than only at
/// the unit level: enabling `numeric_repair` on a page Tesseract already reads correctly must
/// not change a single already-correct value, and in this fixture's case (no misreads to fix
/// on this platform) must not change the output at all.
#[test]
fn numeric_repair_does_not_alter_a_page_with_no_misreads_to_fix() {
    let off = extract_bytes_document_blocking(SHADED_ROWS_FIXTURE, "image/png", &config_with_numeric_repair(false))
        .expect("OCR must not error");
    let on = extract_bytes_document_blocking(SHADED_ROWS_FIXTURE, "image/png", &config_with_numeric_repair(true))
        .expect("OCR must not error");

    for value in ALREADY_CORRECT_VALUES {
        assert_eq!(
            off.content.matches(value).count(),
            on.content.matches(value).count(),
            "numeric_repair changed the count of already-correct value {value}; off:\n{}\non:\n{}",
            off.content,
            on.content
        );
    }
    assert_eq!(
        off.content, on.content,
        "this fixture has no numeric misreads on this platform's OCR build, so numeric_repair \
         must be a byte-identical no-op here"
    );
}

/// The repair actually firing end to end, through a real OCR run and the standalone-image
/// extraction path in `extractors/image.rs` -- the exact route the issue's own reproduction
/// uses (`xberg extract flat/p1.png ...`), not a synthetic string passed directly to
/// `repair_ocr_numeric_tokens`. Proves the config flag reaches the actual document content
/// Tesseract read, including through the hOCR-element-tree assembly path
/// (`build_image_internal_document_from_hocr_elements`) that a flat-string-only repair would
/// miss: an earlier version of this fix repaired `ocr_content` but not
/// `ocr_internal_document`'s elements, and the final page text is rebuilt from the latter
/// whenever hOCR paragraph structure is available (the common case) -- so this test would have
/// caught that gap.
#[test]
fn numeric_repair_adds_a_missing_thousands_separator_through_the_standalone_image_route() {
    let off = extract_bytes_document_blocking(NUMBER_TEST_FIXTURE, "image/png", &config_with_numeric_repair(false))
        .expect("OCR must not error");
    let on = extract_bytes_document_blocking(NUMBER_TEST_FIXTURE, "image/png", &config_with_numeric_repair(true))
        .expect("OCR must not error");

    assert!(
        off.content.contains("1172"),
        "fixture assumption broken: Tesseract must read the bare digits \"1172\" with no \
         separator when the repair is off; got:\n{}",
        off.content
    );
    assert!(
        on.content.contains("1,172"),
        "numeric_repair: true must add the missing thousands separator; got:\n{}",
        on.content
    );
    assert!(
        !on.content.contains("1172"),
        "the unrepaired \"1172\" token must not survive alongside the repaired \"1,172\"; got:\n{}",
        on.content
    );
}
