#[cfg(feature = "ocr-surface")]
use super::super::*;
#[cfg(feature = "ocr-surface")]
use super::default_overrides;
#[cfg(feature = "ocr-surface")]
use xberg::ExtractionConfig;

#[cfg(feature = "ocr-surface")]
#[test]
fn test_validate_vlm_backend_requires_model() {
    let overrides = ExtractionOverrides {
        ocr_backend: Some("vlm".to_string()),
        ..default_overrides()
    };

    let error = overrides
        .validate()
        .expect_err("VLM backend without a model should fail");

    assert_eq!(
        error.to_string(),
        "--ocr-backend vlm requires --vlm-model to be specified"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_vlm_flow() {
    let mut config = ExtractionConfig::default();
    let backend_options_json = r#"{"task":"chart","layout_mode":"whole_page"}"#;
    let overrides = ExtractionOverrides {
        vlm_model: Some("openai/gpt-4o".to_string()),
        ocr_backend_options: Some(backend_options_json.to_string()),
        ..default_overrides()
    };

    assert!(overrides.validate().is_ok());

    overrides.apply(&mut config);

    let ocr = config.ocr.expect("OCR should be configured");
    assert_eq!(ocr.backend, "vlm");

    let opts = ocr.backend_options.expect("backend_options should be Some");
    assert!(opts.is_object());
    assert_eq!(opts.get("task").and_then(|v| v.as_str()), Some("chart"));
    assert_eq!(opts.get("layout_mode").and_then(|v| v.as_str()), Some("whole_page"));

    assert!(ocr.vlm_config.is_some());
    let vlm = ocr.vlm_config.unwrap();
    assert_eq!(vlm.model, "openai/gpt-4o");
}
