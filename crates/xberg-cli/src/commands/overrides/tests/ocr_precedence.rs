#[cfg(feature = "ocr-surface")]
use super::super::*;
#[cfg(feature = "ocr-surface")]
use super::default_overrides;
#[cfg(feature = "ocr-surface")]
use xberg::ExtractionConfig;

#[cfg(feature = "ocr-surface")]
#[test]
fn should_enable_ocr_when_cli_true_overrides_loaded_disable() {
    let mut config = ExtractionConfig {
        disable_ocr: true,
        ..ExtractionConfig::default()
    };
    let overrides = ExtractionOverrides {
        ocr: Some(true),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    assert!(
        !config.disable_ocr,
        "explicit --ocr true should override loaded disable_ocr=true"
    );
    assert!(config.ocr.expect("--ocr true should configure OCR").enabled);
}

#[cfg(feature = "ocr-surface")]
#[test]
fn should_preserve_loaded_disable_ocr_when_ocr_flag_is_absent() {
    let mut config = ExtractionConfig {
        disable_ocr: true,
        ..ExtractionConfig::default()
    };

    default_overrides().apply(&mut config);

    assert!(config.disable_ocr, "an absent OCR flag should preserve loaded config");
}

#[cfg(feature = "ocr-surface")]
#[test]
fn should_clear_loaded_ocr_routing_when_cli_false_hard_disables_ocr() {
    let mut config = ExtractionConfig {
        force_ocr: true,
        ocr_strategy: xberg::OcrStrategy::ScannedPages { min_confidence: 0.8 },
        force_ocr_pages: Some(vec![2, 4]),
        ..ExtractionConfig::default()
    };
    let overrides = ExtractionOverrides {
        ocr: Some(false),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    assert!(!config.force_ocr, "--ocr false should override loaded force_ocr=true");
    assert_eq!(config.ocr_strategy, xberg::OcrStrategy::Auto);
    assert!(
        config.force_ocr_pages.is_none(),
        "--ocr false should clear forced page selection"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn should_reject_ocr_false_with_force_ocr_true() {
    let overrides = ExtractionOverrides {
        ocr: Some(false),
        force_ocr: Some(true),
        ..default_overrides()
    };

    assert_eq!(
        overrides
            .validate()
            .expect_err("disabling and forcing OCR should conflict")
            .to_string(),
        "--ocr false cannot be combined with --force-ocr true"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn should_reject_ocr_false_with_scanned_pages() {
    let overrides = ExtractionOverrides {
        ocr: Some(false),
        ocr_scanned_pages: true,
        ..default_overrides()
    };

    assert_eq!(
        overrides
            .validate()
            .expect_err("disabling OCR and selecting scanned pages should conflict")
            .to_string(),
        "--ocr false cannot be combined with --ocr-scanned-pages"
    );
}

#[cfg(feature = "ocr-surface")]
#[test]
fn should_reject_contradictory_ocr_and_disable_ocr_values() {
    for (ocr, disable_ocr) in [(true, true), (false, false)] {
        let overrides = ExtractionOverrides {
            ocr: Some(ocr),
            disable_ocr: Some(disable_ocr),
            ..default_overrides()
        };

        assert_eq!(
            overrides
                .validate()
                .expect_err("contradictory OCR flags should be rejected")
                .to_string(),
            "--ocr and --disable-ocr specify contradictory values"
        );
    }
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_validate_invalid_ocr_backend() {
    let overrides = ExtractionOverrides {
        ocr_backend: Some("invalid-backend".to_string()),
        ..default_overrides()
    };
    let err = overrides.validate().unwrap_err();
    assert!(err.to_string().contains("Invalid OCR backend"));
}

#[cfg(feature = "ocr-surface")]
#[test]
fn test_validate_valid_ocr_backends() {
    for backend in &["tesseract", "paddle-ocr", "sceptre"] {
        let overrides = ExtractionOverrides {
            ocr_backend: Some(backend.to_string()),
            ..default_overrides()
        };
        assert!(overrides.validate().is_ok(), "Expected backend '{backend}' to be valid");
    }
}
