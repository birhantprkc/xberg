//! Fill/stroke color-space (gray/RGB/CMYK/Lab/Separation/DeviceN) tests split out of `extractors/text/tests.rs` for file size. ~keep

use super::super::*;
use super::extraction_and_rotation::create_test_font;

/// With merge_tm_tj_runs = false, each Tm operator starts a fresh span.
#[test]
fn test_merge_tm_tj_runs_disabled_splits() {
    let mut extractor = TextExtractor::new();
    extractor.merging_config = SpanMergingConfig {
        merge_tm_tj_runs: false,
        ..SpanMergingConfig::legacy()
    };
    let font = create_test_font();
    extractor.add_font("F1".to_string(), font);

    let stream = b"BT /F1 12 Tf 1 0 0 1 100 700 Tm (A) Tj 1 0 0 1 107 700 Tm (B) Tj 1 0 0 1 114 700 Tm (C) Tj ET";
    let spans = extractor.extract_text_spans(stream).unwrap();

    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert!(
        text.contains('A') && text.contains('B') && text.contains('C'),
        "All chars must be extracted even with merging disabled, got: {:?}",
        text
    );

    // With merge disabled, each Tm flushes the buffer, so we get more spans
    // than with merging enabled (post-processing merge_adjacent_spans may combine
    // some, but at minimum we should get spans >= 1; the key invariant is that
    // the span count here is NOT reduced by the Tm-continuation shortcut) ~keep
    assert!(
        spans.len() >= 2,
        "merge_tm_tj_runs=false should not batch same-line runs; expected >= 2 spans, got {}",
        spans.len()
    );
}

#[test]
fn test_extract_with_zero_font_size() {
    let mut extractor = TextExtractor::new();
    let font = create_test_font();
    extractor.add_font("F1".to_string(), font);

    // Zero font size is technically valid in PDF ~keep
    let stream = b"BT /F1 0 Tf 100 700 Td (X) Tj ET";
    let result = extractor.extract(stream);
    assert!(result.is_ok());
}

#[test]
fn test_extract_with_negative_font_size() {
    let mut extractor = TextExtractor::new();
    let font = create_test_font();
    extractor.add_font("F1".to_string(), font);

    // Negative font size inverts text ~keep
    let stream = b"BT /F1 -12 Tf 100 700 Td (X) Tj ET";
    let result = extractor.extract(stream);
    assert!(result.is_ok());
}

#[test]
fn test_extract_with_very_large_coordinate() {
    let mut extractor = TextExtractor::new();
    let font = create_test_font();
    extractor.add_font("F1".to_string(), font);

    let stream = b"BT /F1 12 Tf 99999 99999 Td (X) Tj ET";
    let chars = extractor.extract(stream).unwrap();
    assert_eq!(chars.len(), 1);
}

#[test]
fn test_set_fill_color_device_gray() {
    let mut extractor = TextExtractor::new();
    let font = create_test_font();
    extractor.add_font("F1".to_string(), font);

    // cs sets color space, then sc sets color components ~keep
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor { components: vec![0.5] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.5).abs() < 0.01);
    assert!((state.fill_color_rgb.1 - 0.5).abs() < 0.01);
    assert!((state.fill_color_rgb.2 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_fill_color_device_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.2, 0.4, 0.6],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.2).abs() < 0.01);
    assert!((state.fill_color_rgb.1 - 0.4).abs() < 0.01);
    assert!((state.fill_color_rgb.2 - 0.6).abs() < 0.01);
}

#[test]
fn test_set_fill_color_device_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceCMYK".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.0, 0.0, 0.0, 1.0], // the K ink ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.1373).abs() < 0.01);
    assert!((state.fill_color_rgb.1 - 0.1216).abs() < 0.01);
    assert!((state.fill_color_rgb.2 - 0.1255).abs() < 0.01);
    assert!(state.fill_color_cmyk.is_some());
}

#[test]
fn test_set_fill_color_lab() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "Lab".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![50.0, 20.0, -10.0],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    // Lab simplified to grayscale: L/100 ~keep
    assert!((state.fill_color_rgb.0 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_fill_color_iccbased_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.1, 0.2, 0.3],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.1).abs() < 0.01);
    assert!((state.fill_color_rgb.1 - 0.2).abs() < 0.01);
    assert!((state.fill_color_rgb.2 - 0.3).abs() < 0.01);
}

#[test]
fn test_set_fill_color_iccbased_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor { components: vec![0.7] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.7).abs() < 0.01);
}

#[test]
fn test_set_fill_color_iccbased_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![1.0, 0.0, 0.0, 0.0], // cyan ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.fill_color_cmyk.is_some());
}

#[test]
fn test_set_fill_color_separation() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "Separation".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.8], // tint ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    // gray = 1.0 - tint = 0.2 ~keep
    assert!((state.fill_color_rgb.0 - 0.2).abs() < 0.01);
}

#[test]
fn test_set_fill_color_devicen_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.0, 0.0, 0.0, 0.5], // 4-component DeviceN ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.fill_color_cmyk.is_some());
}

#[test]
fn test_set_fill_color_devicen_single() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.3], // single-component DeviceN ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    // gray = 1.0 - 0.3 = 0.7 ~keep
    assert!((state.fill_color_rgb.0 - 0.7).abs() < 0.01);
}

#[test]
fn test_set_fill_color_unknown_space() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "CustomUnknown".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.5, 0.5],
        })
        .unwrap();
}

#[test]
fn test_set_fill_color_cal_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "CalGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor { components: vec![0.8] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.8).abs() < 0.01);
}

#[test]
fn test_set_fill_color_cal_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "CalRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColor {
            components: vec![0.9, 0.1, 0.5],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.9).abs() < 0.01);
    assert!((state.fill_color_rgb.1 - 0.1).abs() < 0.01);
    assert!((state.fill_color_rgb.2 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_device_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.4] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.4).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_device_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.1, 0.2, 0.3],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.1).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_lab() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "Lab".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![75.0, 10.0, -5.0],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.75).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_device_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceCMYK".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.0, 1.0, 0.0, 0.0], // magenta ~keep
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_iccbased_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.3] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.3).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_iccbased_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.9, 0.8, 0.7],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.9).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_iccbased_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.1, 0.2, 0.3, 0.4],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_separation() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "Separation".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.6] })
        .unwrap();

    let state = extractor.state_stack.current();
    // gray = 1.0 - 0.6 = 0.4 ~keep
    assert!((state.stroke_color_rgb.0 - 0.4).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_devicen_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.1, 0.2, 0.3, 0.4],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_devicen_single() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.5] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_cal_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "CalRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor {
            components: vec![0.5, 0.6, 0.7],
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_cal_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "CalGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.9] })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.9).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_unknown() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "UnknownCS".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColor { components: vec![0.5] })
        .unwrap();
}

#[test]
fn test_set_fill_color_n_with_pattern() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![],
            name: Some(Box::new("P1".to_string())),
        })
        .unwrap();
}

#[test]
fn test_set_fill_color_n_without_pattern_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.3],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.3).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_without_pattern_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.1, 0.2, 0.3],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.1).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_lab() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "Lab".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![80.0, 0.0, 0.0],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.8).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceCMYK".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.0, 0.0, 0.0, 0.0],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    // White (no ink) ~keep
    assert!((state.fill_color_rgb.0 - 1.0).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_iccbased() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.5, 0.6, 0.7],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_iccbased_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.9],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.9).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_iccbased_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.1, 0.2, 0.3, 0.4],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.fill_color_cmyk.is_some());
}

#[test]
fn test_set_fill_color_n_separation() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "Separation".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.4],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.6).abs() < 0.01);
}

#[test]
fn test_set_fill_color_n_devicen() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.1, 0.2, 0.3, 0.4],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.fill_color_cmyk.is_some());
}

#[test]
fn test_set_fill_color_n_devicen_single() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetFillColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetFillColorN {
            components: vec![0.2],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.fill_color_rgb.0 - 0.8).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_with_pattern() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![],
            name: Some(Box::new("P2".to_string())),
        })
        .unwrap();
}

#[test]
fn test_set_stroke_color_n_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceGray".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.6],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.6).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceRGB".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.8, 0.7, 0.6],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.8).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_lab() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "Lab".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![60.0, 0.0, 0.0],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.6).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceCMYK".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.0, 0.0, 1.0, 0.0], // yellow ~keep
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_n_iccbased_rgb() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.2, 0.3, 0.4],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.2).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_iccbased_gray() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.5],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.5).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_iccbased_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "ICCBased".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.1, 0.2, 0.3, 0.4],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_n_separation() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "Separation".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![1.0],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.0).abs() < 0.01);
}

#[test]
fn test_set_stroke_color_n_devicen_cmyk() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.5, 0.5, 0.5, 0.5],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!(state.stroke_color_cmyk.is_some());
}

#[test]
fn test_set_stroke_color_n_devicen_single() {
    let mut extractor = TextExtractor::new();
    extractor
        .execute_operator_public(Operator::SetStrokeColorSpace {
            name: "DeviceN".to_string(),
        })
        .unwrap();
    extractor
        .execute_operator_public(Operator::SetStrokeColorN {
            components: vec![0.1],
            name: None,
        })
        .unwrap();

    let state = extractor.state_stack.current();
    assert!((state.stroke_color_rgb.0 - 0.9).abs() < 0.01);
}

/// Named color space reference like "Cs1" should fall back by component
/// count rather than emitting a warn! (regression: warn spam on PDFs
/// with ICCBased color spaces registered under user-defined names).
#[test]
fn test_named_fill_color_space_fallback_gray() {
    let mut e = TextExtractor::new();
    e.execute_operator_public(Operator::SetFillColorSpace {
        name: "Cs1".to_string(),
    })
    .unwrap();
    e.execute_operator_public(Operator::SetFillColor { components: vec![0.4] })
        .unwrap();
    let state = e.state_stack.current();
    let (r, g, b) = state.fill_color_rgb;
    assert!((r - 0.4).abs() < 0.01 && (g - 0.4).abs() < 0.01 && (b - 0.4).abs() < 0.01);
}
