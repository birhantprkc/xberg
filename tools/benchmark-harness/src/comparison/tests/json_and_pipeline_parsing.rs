//! Comparison-JSON output and `Pipeline` name parse/serialize round-trip tests.

use super::*;
use crate::comparison::extraction_config::{
    TESSERACT_PSM_SINGLE_BLOCK, TESSERACT_PSM_SPARSE_TEXT, TESSERACT_PSM_VERTICAL_BLOCK, apply_fixture_ocr_language,
};
use crate::corpus::CorpusDocument;
use std::collections::HashMap;

#[test]
fn comparison_json_preserves_unavailable_aggregate_scores() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("comparison.json");
    let results = vec![DocResult {
        name: "missing".to_string(),
        file_type: "pdf".to_string(),
        results: vec![PipelineResult {
            pipeline: Pipeline::Docling,
            sf1: f64::NAN,
            tf1: f64::NAN,
            char_similarity: f64::NAN,
            order_score: f64::NAN,
            per_type_sf1: HashMap::new(),
            per_type_precision: HashMap::new(),
            per_type_recall: HashMap::new(),
            time_ms: f64::NAN,
            missing_tokens: Vec::new(),
            extra_tokens: Vec::new(),
            content: String::new(),
        }],
    }];

    write_comparison_json(&results, &output_path).unwrap();
    let output: serde_json::Value = serde_json::from_slice(&std::fs::read(output_path).unwrap()).unwrap();

    assert!(output["by_format"]["pdf"]["pipelines"][0]["avg_sf1"].is_null());
    assert!(output["by_format"]["pdf"]["pipelines"][0]["avg_tf1"].is_null());
    assert_eq!(output["by_format"]["pdf"]["pipelines"][0]["sf1_count"], 0);
    assert_eq!(output["by_format"]["pdf"]["pipelines"][0]["tf1_count"], 0);
    assert!(output["overall"]["pipelines"][0]["avg_sf1"].is_null());
    assert!(output["overall"]["pipelines"][0]["avg_tf1"].is_null());
    assert_eq!(output["overall"]["pipelines"][0]["sf1_count"], 0);
    assert_eq!(output["overall"]["pipelines"][0]["tf1_count"], 0);
}

#[test]
fn comparison_json_counts_metric_samples_independently() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("comparison.json");
    let result = |name: &str, sf1: f64, tf1: f64| DocResult {
        name: name.to_string(),
        file_type: "txt".to_string(),
        results: vec![PipelineResult {
            pipeline: Pipeline::Docling,
            sf1,
            tf1,
            char_similarity: tf1,
            order_score: sf1,
            per_type_sf1: HashMap::new(),
            per_type_precision: HashMap::new(),
            per_type_recall: HashMap::new(),
            time_ms: 1.0,
            missing_tokens: Vec::new(),
            extra_tokens: Vec::new(),
            content: String::new(),
        }],
    };
    let results = vec![result("markdown", 0.6, 0.5), result("text-only", f64::NAN, 0.9)];

    write_comparison_json(&results, &output_path).unwrap();
    let output: serde_json::Value = serde_json::from_slice(&std::fs::read(output_path).unwrap()).unwrap();
    let summary = &output["by_format"]["txt"]["pipelines"][0];

    assert_eq!(summary["sf1_count"], 1);
    assert_eq!(summary["tf1_count"], 2);
    assert!((summary["avg_sf1"].as_f64().unwrap() - 0.6).abs() < f64::EPSILON);
    assert!((summary["avg_tf1"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
    assert_eq!(output["overall"]["pipelines"][0]["sf1_count"], 1);
    assert_eq!(output["overall"]["pipelines"][0]["tf1_count"], 2);
}

#[test]
fn test_pipeline_parse_roundtrip() {
    let all_names = [
        "baseline",
        "layout",
        "tesseract",
        "tesseract+layout",
        "paddle-v6-medium",
        "paddle-v6-medium+layout",
        "paddle-v6-small",
        "paddle-v6-small+layout",
        "paddle-v6-small+layout+det-side-1024",
        "paddle-v6-small+layout+det-side-1536",
        "paddle-v6-small+layout+det-side-2048",
        "paddle-v6-small+layout+det-db-thresh-020",
        "paddle-v6-small+layout+det-db-box-thresh-035",
        "paddle-v6-small+layout+drop-score-030",
        "paddle-v6-small+layout+drop-score-040",
        "paddle-v6-tiny",
        "paddle-v6-tiny+layout",
        "paddle-v5-server",
        "paddle-v5-server+layout",
        "tesseract-autorotate",
        "paddle-autorotate",
        "tesseract-single-block",
        "tesseract-vertical-block",
        "tesseract-sparse-text",
        "paddle-norotate",
        "docling",
        "paddleocr-python",
        "rapidocr",
        "layout+slanet-wired",
        "layout+slanet-wireless",
        "layout+slanet-plus",
        "layout+slanet-auto",
        "native",
        "native+layout",
        "sceptre-ort",
        "sceptre-ort+layout",
        "sceptre-ort-autorotate",
        "candle-trocr",
        "candle-paddleocr-vl",
        "candle-glm-ocr",
        "candle-deepseek-ocr",
        "candle-paddleocr-vl-15",
    ];
    for name in all_names {
        let pipeline = Pipeline::parse(name).unwrap_or_else(|| panic!("Failed to parse pipeline '{name}'"));
        let roundtrip = pipeline.name();
        let reparsed = Pipeline::parse(roundtrip).unwrap_or_else(|| panic!("Failed to reparse pipeline '{roundtrip}'"));
        assert_eq!(pipeline, reparsed, "Roundtrip failed for '{name}' -> '{roundtrip}'");
    }
}

#[test]
fn legacy_paddle_names_parse_to_canonical_presets() {
    let aliases = [
        ("paddle", Pipeline::Paddle),
        ("paddle-mobile", Pipeline::Paddle),
        ("paddle+layout", Pipeline::PaddleLayout),
        ("paddle-layout", Pipeline::PaddleLayout),
        ("paddle-mobile+layout", Pipeline::PaddleLayout),
        ("paddle-server", Pipeline::PaddleServer),
        ("paddle-server+layout", Pipeline::PaddleServerLayout),
    ];

    for (alias, expected) in aliases {
        assert_eq!(
            Pipeline::parse(alias),
            Some(expected),
            "failed to preserve alias '{alias}'"
        );
    }
    assert_eq!(Pipeline::parse("paddle").expect("alias").name(), "paddle-v6-medium");
    assert_eq!(
        Pipeline::parse("paddle+layout").expect("alias").name(),
        "paddle-v6-medium+layout"
    );
}

#[test]
fn tesseract_segmentation_presets_pin_psm_and_language() {
    let cases = [
        (Pipeline::TesseractSingleBlock, TESSERACT_PSM_SINGLE_BLOCK),
        (Pipeline::TesseractVerticalBlock, TESSERACT_PSM_VERTICAL_BLOCK),
        (Pipeline::TesseractSparseText, TESSERACT_PSM_SPARSE_TEXT),
    ];

    for (pipeline, expected_psm) in cases {
        let config = build_extraction_config(pipeline);
        let ocr = config.ocr.expect("Tesseract preset must configure OCR");
        let tesseract = ocr
            .tesseract_config
            .expect("segmentation preset must configure Tesseract");

        assert!(config.force_ocr);
        assert!(config.layout.is_none());
        assert_eq!(ocr.backend, "tesseract");
        assert_eq!(ocr.language, ["eng"]);
        assert_eq!(tesseract.language, ["eng"]);
        assert_eq!(tesseract.psm, Some(expected_psm));
    }
}

#[test]
fn tesseract_segmentation_aliases_use_canonical_names() {
    let cases = [
        (
            "tesseract-psm5",
            Pipeline::TesseractVerticalBlock,
            "tesseract-vertical-block",
        ),
        (
            "tesseract-vertical",
            Pipeline::TesseractVerticalBlock,
            "tesseract-vertical-block",
        ),
        (
            "tesseract-psm6",
            Pipeline::TesseractSingleBlock,
            "tesseract-single-block",
        ),
        (
            "tesseract-psm11",
            Pipeline::TesseractSparseText,
            "tesseract-sparse-text",
        ),
    ];

    for (alias, expected, canonical) in cases {
        assert_eq!(Pipeline::parse(alias), Some(expected));
        assert_eq!(expected.name(), canonical);
        assert_eq!(
            serde_json::from_str::<Pipeline>(&format!(r#""{alias}""#)).expect("deserialize alias"),
            expected
        );
    }
}

#[test]
fn fixture_language_updates_vertical_segmentation_without_changing_psm() {
    let mut metadata = HashMap::new();
    metadata.insert("ocr_language".to_string(), serde_json::json!("jpn_vert"));
    let doc = CorpusDocument {
        name: "vertical-japanese".to_string(),
        document_path: std::path::PathBuf::new(),
        file_type: "jpeg".to_string(),
        file_size: 0,
        ground_truth_text: None,
        ground_truth_markdown: None,
        metadata,
        fixture_path: std::path::PathBuf::new(),
    };
    let mut config = build_extraction_config(Pipeline::TesseractVerticalBlock);

    apply_fixture_ocr_language(&mut config, &doc);

    let ocr = config.ocr.expect("Tesseract preset must configure OCR");
    let tesseract = ocr
        .tesseract_config
        .expect("segmentation preset must configure Tesseract");
    assert_eq!(ocr.language, ["jpn_vert"]);
    assert_eq!(tesseract.language, ["jpn_vert"]);
    assert_eq!(tesseract.psm, Some(TESSERACT_PSM_VERTICAL_BLOCK));
}

#[test]
fn paddle_v6_serialization_uses_canonical_names() {
    let serialized = serde_json::to_string(&Pipeline::PaddleLayout).expect("serialize pipeline");
    assert_eq!(serialized, r#""paddle-v6-medium+layout""#);
    assert_eq!(
        serde_json::from_str::<Pipeline>(r#""paddle-layout""#).expect("deserialize legacy name"),
        Pipeline::PaddleLayout
    );
}
