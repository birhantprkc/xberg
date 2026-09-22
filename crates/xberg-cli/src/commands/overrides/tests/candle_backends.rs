#[cfg(feature = "ocr-surface")]
use super::super::ocr::{DEFAULT_OCR_LANGUAGE, DEFAULT_PADDLE_OCR_LANGUAGE, default_language_for_backend};
#[cfg(feature = "ocr-surface")]
use super::super::*;
#[cfg(feature = "ocr-surface")]
use super::default_overrides;
#[cfg(feature = "ocr-surface")]
use xberg::ExtractionConfig;

/// The three candle VLM backends declare identical `supported_languages()` sets, so
/// they must all default to the same short code. `candle-deepseek-ocr` was missing
/// from `PADDLE_LANGUAGE_BACKENDS`, defaulting to `"eng"` where its siblings use
/// `"en"`. Against unfixed code the deepseek row below returns `"eng"`.
#[cfg(feature = "ocr-surface")]
#[test]
fn every_candle_vlm_backend_defaults_to_the_same_short_language_code() {
    for backend in ["candle-paddleocr-vl", "candle-glm-ocr", "candle-deepseek-ocr"] {
        assert_eq!(
            default_language_for_backend(backend),
            DEFAULT_PADDLE_OCR_LANGUAGE,
            "{backend} must default to the short ISO 639-1 code its siblings use"
        );
    }
    assert_eq!(
        default_language_for_backend("tesseract"),
        DEFAULT_OCR_LANGUAGE,
        "a backend outside the family must keep the ISO 639-3 default"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_apply_ocr_preserves_candle_deepseek_backend() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend: Some("candle-deepseek-ocr".to_string()),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    assert_eq!(
        config.ocr.expect("OCR config should be set").backend,
        "candle-deepseek-ocr"
    );
}
