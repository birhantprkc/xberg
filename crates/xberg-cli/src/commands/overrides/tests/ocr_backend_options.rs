#[cfg(feature = "ocr-surface")]
use super::super::*;
#[cfg(feature = "ocr-surface")]
use super::default_overrides;
#[cfg(feature = "ocr-surface")]
use xberg::ExtractionConfig;

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_parsed_and_applied() {
    let mut config = ExtractionConfig::default();
    let backend_options_json = r#"{"task":"chart","layout_mode":"whole_page"}"#;
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend_options: Some(backend_options_json.to_string()),
        ..default_overrides()
    };

    assert!(overrides.validate().is_ok());

    overrides.apply(&mut config);
    let ocr = config.ocr.unwrap();

    assert!(ocr.backend_options.is_some());
    let opts = ocr.backend_options.unwrap();
    assert!(opts.is_object());
    assert_eq!(opts.get("task").and_then(|v| v.as_str()), Some("chart"));
    assert_eq!(opts.get("layout_mode").and_then(|v| v.as_str()), Some("whole_page"));
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_invalid_json_fails_validation() {
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend_options: Some("not-valid-json".to_string()),
        ..default_overrides()
    };
    assert!(overrides.validate().is_err());
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_not_object_fails_validation() {
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend_options: Some(r#"["array", "not", "object"]"#.to_string()),
        ..default_overrides()
    };
    assert!(overrides.validate().is_err());
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_threaded_into_config() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend: Some("candle-glm-ocr".to_string()),
        ocr_backend_options: Some(r#"{"layout_mode":"whole_page"}"#.to_string()),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let ocr = config.ocr.unwrap();
    assert_eq!(ocr.backend, "candle-glm-ocr");
    let opts = ocr.backend_options.expect("backend_options should be Some");
    assert_eq!(opts, serde_json::json!({"layout_mode": "whole_page"}));
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_validate_rejects_non_object_backend_options() {
    let overrides = ExtractionOverrides {
        ocr_backend_options: Some(r#"["not","an","object"]"#.to_string()),
        ..default_overrides()
    };
    let err = overrides.validate().unwrap_err();
    assert!(
        err.to_string().contains("--ocr-backend-options must be a JSON object"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_validate_rejects_invalid_json_backend_options() {
    let overrides = ExtractionOverrides {
        ocr_backend_options: Some("not-json".to_string()),
        ..default_overrides()
    };
    let err = overrides.validate().unwrap_err();
    assert!(
        err.to_string().contains("invalid --ocr-backend-options JSON"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_ocr_backend_options_none_when_absent() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ocr_backend: Some("candle-glm-ocr".to_string()),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let ocr = config.ocr.unwrap();
    assert!(ocr.backend_options.is_none());
}
