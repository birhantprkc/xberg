use super::*;
use serde_json::json;

#[cfg(feature = "api")]
fn assert_named_object_schema<T: utoipa::PartialSchema>(expected: serde_json::Value) {
    let schema = serde_json::to_value(T::schema()).expect("schema must serialize");
    assert_eq!(schema, expected);
}

#[cfg(feature = "api")]
#[test]
fn should_describe_binding_dtos_as_named_object_schemas() {
    assert_named_object_schema::<PresentationHyperlink>(json!({
        "type": "object",
        "required": ["url", "label"],
        "properties": {
            "url": {"type": "string"},
            "label": {"type": ["string", "null"]}
        }
    }));
    assert_named_object_schema::<PixelDimensions>(json!({
        "type": "object",
        "required": ["width", "height"],
        "properties": {
            "width": {"type": "integer"},
            "height": {"type": "integer"}
        }
    }));
    assert_named_object_schema::<ImageDpi>(json!({
        "type": "object",
        "required": ["horizontal", "vertical"],
        "properties": {
            "horizontal": {"type": "number"},
            "vertical": {"type": "number"}
        }
    }));
}

#[test]
fn should_serialize_presentation_hyperlink_as_named_object() {
    let legacy = json!(["https://xberg.io", "Xberg"]);
    let named_json = json!({"url": "https://xberg.io", "label": "Xberg"});
    let hyperlink: PresentationHyperlink = serde_json::from_value(legacy).expect("legacy hyperlink must deserialize");
    let named: PresentationHyperlink =
        serde_json::from_value(named_json.clone()).expect("named hyperlink must deserialize");

    assert_eq!(hyperlink.url, "https://xberg.io");
    assert_eq!(hyperlink.label.as_deref(), Some("Xberg"));
    assert_eq!(named, hyperlink);
    assert_eq!(
        serde_json::to_value(hyperlink).expect("hyperlink must serialize"),
        named_json
    );
    assert_eq!(
        serde_json::to_value(named).expect("named hyperlink must serialize"),
        named_json
    );
}

#[test]
fn should_serialize_missing_hyperlink_label_as_named_null() {
    let legacy = json!(["https://xberg.io", null]);
    let named_json = json!({"url": "https://xberg.io", "label": null});
    let positional: PresentationHyperlink = serde_json::from_value(legacy).expect("legacy null label must deserialize");
    let named: PresentationHyperlink =
        serde_json::from_value(json!({"url": "https://xberg.io"})).expect("omitted named label must deserialize");

    assert_eq!(named, positional);
    assert_eq!(
        serde_json::to_value(positional).expect("hyperlink must serialize"),
        named_json
    );
    assert_eq!(
        serde_json::to_value(named).expect("named hyperlink must serialize"),
        named_json
    );
}

#[test]
fn should_still_accept_legacy_positional_array_for_pixel_dimensions() {
    let dimensions: PixelDimensions =
        serde_json::from_str("[1200, 800]").expect("legacy pixel dimensions must deserialize");

    assert_eq!(
        dimensions,
        PixelDimensions {
            width: 1200,
            height: 800
        }
    );
}

#[test]
fn should_still_accept_legacy_positional_array_for_image_dpi() {
    let dpi: ImageDpi = serde_json::from_str("[72.0, 96.0]").expect("legacy image DPI must deserialize");

    assert_eq!(
        dpi,
        ImageDpi {
            horizontal: 72.0,
            vertical: 96.0
        }
    );
}

#[test]
fn should_serialize_preprocessing_metadata_with_named_nested_types() {
    let legacy = json!({
        "original_dimensions": [1200, 800],
        "original_dpi": [72.0, 96.0],
        "target_dpi": 300,
        "scale_factor": 2.0,
        "auto_adjusted": true,
        "final_dpi": 288,
        "new_dimensions": [2400, 1600],
        "resample_method": "LANCZOS3",
        "dimension_clamped": false,
        "calculated_dpi": 288,
        "skipped_resize": false,
        "resize_error": null
    });
    let named = json!({
        "original_dimensions": {"width": 1200, "height": 800},
        "original_dpi": {"horizontal": 72.0, "vertical": 96.0},
        "target_dpi": 300,
        "scale_factor": 2.0,
        "auto_adjusted": true,
        "final_dpi": 288,
        "new_dimensions": {"width": 2400, "height": 1600},
        "resample_method": "LANCZOS3",
        "dimension_clamped": false,
        "calculated_dpi": 288,
        "skipped_resize": false,
        "resize_error": null
    });
    let metadata: ImagePreprocessingMetadata =
        serde_json::from_value(legacy).expect("legacy preprocessing metadata must deserialize");
    let named_dimensions: PixelDimensions =
        serde_json::from_value(json!({"width": 1200, "height": 800})).expect("named pixel dimensions must deserialize");
    let named_dpi: ImageDpi = serde_json::from_value(json!({"horizontal": 72.0, "vertical": 96.0}))
        .expect("named image DPI must deserialize");

    assert_eq!(
        metadata.original_dimensions,
        PixelDimensions {
            width: 1200,
            height: 800
        }
    );
    assert_eq!(
        metadata.original_dpi,
        ImageDpi {
            horizontal: 72.0,
            vertical: 96.0
        }
    );
    assert_eq!(
        metadata.new_dimensions,
        Some(PixelDimensions {
            width: 2400,
            height: 1600
        })
    );
    assert_eq!(
        serde_json::to_value(metadata).expect("preprocessing metadata must serialize"),
        named
    );
    assert_eq!(
        serde_json::to_value(named_dimensions).expect("named pixel dimensions must serialize"),
        json!({"width": 1200, "height": 800})
    );
    assert_eq!(
        serde_json::to_value(named_dpi).expect("named image DPI must serialize"),
        json!({"horizontal": 72.0, "vertical": 96.0})
    );
}

/// This is the public-facing `TesseractConfig` (re-exported as `crate::types::
/// TesseractConfig`), not `crate::ocr::types::TesseractConfig` (the internal,
/// engine-facing struct with its own separate `Default` impl). The two defaults must
/// agree: `extractors::image::apply_default_tesseract_psm` and related call sites
/// construct *this* struct's default and convert it into the internal one, bypassing
/// the internal struct's own `Default` — so a stale value here silently overrides the
/// internal default for every standalone image OCR call, even after the internal
/// default is changed.
///
/// Against the unfixed code this struct's `language_model_ngram_on` default is
/// `false`, disagreeing with `crate::ocr::types::TesseractConfig::default()`'s `true`
/// (see that struct's doc comment for why `true` is the deliberate, documented
/// default), so this assertion fails with `false` instead of `true`.
#[test]
fn test_tesseract_config_default_matches_internal_ngram_default() {
    let config = TesseractConfig::default();

    assert!(
        config.language_model_ngram_on,
        "public TesseractConfig::default() must match crate::ocr::types::TesseractConfig::default() \
         for language_model_ngram_on (true), or standalone image OCR silently gets the stale value"
    );
}

/// GH#1784: a config written for the old `bool` field must still deserialize, resolving to
/// what its doc comment described (`true`, "adaptive thresholding", now actually works).
#[test]
fn should_deserialize_legacy_bool_thresholding_method_for_backward_compatibility() {
    let legacy_false: TesseractConfig =
        serde_json::from_value(json!({"thresholding_method": false})).expect("legacy false must deserialize");
    let legacy_true: TesseractConfig =
        serde_json::from_value(json!({"thresholding_method": true})).expect("legacy true must deserialize");

    assert_eq!(legacy_false.thresholding_method, 0, "false must map to Otsu");
    assert_eq!(legacy_true.thresholding_method, 1, "true must map to LeptonicaOtsu");
}

/// The integer form (0-2) round-trips; an omitted field defaults to Otsu (`0`).
#[test]
fn should_deserialize_integer_thresholding_method_and_default_when_omitted() {
    for method in 0..=2 {
        let config: TesseractConfig =
            serde_json::from_value(json!({"thresholding_method": method})).expect("integer form must deserialize");
        assert_eq!(config.thresholding_method, method);
    }
    let omitted: TesseractConfig = serde_json::from_value(json!({})).expect("empty object must deserialize");
    assert_eq!(omitted.thresholding_method, 0);
}
