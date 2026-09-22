//! Guardrail contract checking and relative-reading-order tests.

use super::*;
use crate::comparison::guardrails::{
    READING_ORDER_GUARDRAIL_DOC, READING_ORDER_GUARDRAIL_PIPELINE, check_relative_order, reading_order_anchors,
};
use std::collections::HashMap;

#[test]
fn guardrails_fail_when_contracted_scores_are_unavailable() {
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
    let config = GuardrailsConfig {
        version: "1.0".to_string(),
        generated_at: String::new(),
        threshold_factor: 0.9,
        contracts: vec![GuardrailContract {
            doc: "missing".to_string(),
            file_type: Some("pdf".to_string()),
            pipeline: "docling".to_string(),
            min_sf1: Some(0.5),
            min_tf1: Some(0.5),
            relative_order: Vec::new(),
        }],
    };

    let failures = check_guardrails(&results, &config);

    assert_eq!(failures.len(), 2);
    assert!(failures.iter().any(|failure| failure.starts_with("SF1 unavailable:")));
    assert!(failures.iter().any(|failure| failure.starts_with("TF1 unavailable:")));
}

#[test]
fn guardrails_fail_when_contracted_document_is_missing() {
    let config = GuardrailsConfig {
        version: "1.0".to_string(),
        generated_at: String::new(),
        threshold_factor: 0.9,
        contracts: vec![GuardrailContract {
            doc: "missing".to_string(),
            file_type: Some("pdf".to_string()),
            pipeline: "baseline".to_string(),
            min_sf1: Some(0.5),
            min_tf1: None,
            relative_order: Vec::new(),
        }],
    };

    let failures = check_guardrails(&[], &config);

    assert_eq!(
        failures,
        vec!["missing guardrail document: missing [pdf] baseline".to_string()]
    );
}

#[test]
fn guardrails_fail_when_contracted_pipeline_result_is_missing() {
    let results = vec![DocResult {
        name: "example".to_string(),
        file_type: "pdf".to_string(),
        results: Vec::new(),
    }];
    let config = GuardrailsConfig {
        version: "1.0".to_string(),
        generated_at: String::new(),
        threshold_factor: 0.9,
        contracts: vec![GuardrailContract {
            doc: "example".to_string(),
            file_type: Some("pdf".to_string()),
            pipeline: "baseline".to_string(),
            min_sf1: Some(0.5),
            min_tf1: None,
            relative_order: Vec::new(),
        }],
    };

    let failures = check_guardrails(&results, &config);

    assert_eq!(
        failures,
        vec!["missing guardrail pipeline result: example [pdf] baseline".to_string()]
    );
}

#[test]
fn relative_order_guardrail_rejects_out_of_order_content_independently_of_scores() {
    let config = GuardrailsConfig {
        version: "1.0".to_string(),
        generated_at: String::new(),
        threshold_factor: 0.9,
        contracts: vec![GuardrailContract {
            doc: "681693".to_string(),
            file_type: Some("pdf".to_string()),
            pipeline: "native+layout+reading-order".to_string(),
            min_sf1: Some(0.8),
            min_tf1: None,
            relative_order: reading_order_anchors(READING_ORDER_GUARDRAIL_DOC, READING_ORDER_GUARDRAIL_PIPELINE),
        }],
    };
    let make_results = |content: &str| {
        vec![DocResult {
            name: "681693".to_string(),
            file_type: "pdf".to_string(),
            results: vec![PipelineResult {
                pipeline: Pipeline::NativeReadingOrder,
                sf1: 0.9,
                tf1: 0.9,
                char_similarity: 0.9,
                order_score: 0.9,
                per_type_sf1: HashMap::new(),
                per_type_precision: HashMap::new(),
                per_type_recall: HashMap::new(),
                time_ms: 1.0,
                missing_tokens: Vec::new(),
                extra_tokens: Vec::new(),
                content: content.to_string(),
            }],
        }]
    };

    let expected = "maintainers wanted ! See #182\n# MongoKit\n\
        MongoDB is a great schema-less document oriented database.\n## Philosophy";
    assert!(check_guardrails(&make_results(expected), &config).is_empty());

    let out_of_order = "MongoDB is a great schema-less document oriented database.\n\
        ## Philosophy\nmaintainers wanted ! See #182\n# MongoKit";
    let failures = check_guardrails(&make_results(out_of_order), &config);

    assert_eq!(failures.len(), 1);
    assert!(failures[0].starts_with("relative-order regression: 681693 [pdf] native+layout+reading-order"));
}

#[test]
fn legacy_guardrail_json_defaults_relative_order_and_installs_active_contract() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("guardrails.json");
    std::fs::write(
        &path,
        r#"{
            "version": "1.0",
            "generated_at": "",
            "threshold_factor": 0.9,
            "contracts": [{
                "doc": "legacy",
                "pipeline": "baseline",
                "min_sf1": 0.5
            }]
        }"#,
    )
    .unwrap();

    let config = load_guardrails(&path).unwrap();
    let legacy = config
        .contracts
        .iter()
        .find(|contract| contract.doc == "legacy")
        .unwrap();
    assert!(legacy.relative_order.is_empty());
    assert!(legacy.file_type.is_none());
    let active = config
        .contracts
        .iter()
        .find(|contract| {
            contract.doc == READING_ORDER_GUARDRAIL_DOC && contract.pipeline == READING_ORDER_GUARDRAIL_PIPELINE
        })
        .unwrap();
    assert_eq!(
        active.relative_order,
        reading_order_anchors(READING_ORDER_GUARDRAIL_DOC, READING_ORDER_GUARDRAIL_PIPELINE)
    );
    assert_eq!(active.file_type.as_deref(), Some("pdf"));
}

#[test]
fn guardrail_config_rejects_unknown_pipeline_among_valid_contracts() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("guardrails.json");
    std::fs::write(
        &path,
        r#"{
            "version": "1.0",
            "generated_at": "",
            "threshold_factor": 0.9,
            "contracts": [
                {"doc": "valid", "pipeline": "baseline", "min_tf1": 0.5},
                {"doc": "invalid", "pipeline": "unknown-pipeline", "min_tf1": 0.5}
            ]
        }"#,
    )
    .unwrap();

    let error = load_guardrails(&path).unwrap_err().to_string();

    assert!(
        error.contains("unknown guardrail pipeline 'unknown-pipeline'"),
        "{error}"
    );
}

#[test]
fn guardrail_config_rejects_contract_without_predicate() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("guardrails.json");
    std::fs::write(
        &path,
        r#"{
            "version": "1.0",
            "generated_at": "",
            "threshold_factor": 0.9,
            "contracts": [{"doc": "empty", "pipeline": "baseline"}]
        }"#,
    )
    .unwrap();

    let error = load_guardrails(&path).unwrap_err().to_string();

    assert!(
        error.contains("has no threshold or relative-order predicate"),
        "{error}"
    );
}

#[test]
fn guardrails_require_file_type_for_duplicate_document_names() {
    let result_for = |file_type: &str, sf1: f64| DocResult {
        name: "shared".to_string(),
        file_type: file_type.to_string(),
        results: vec![PipelineResult {
            pipeline: Pipeline::Baseline,
            sf1,
            tf1: 1.0,
            char_similarity: 1.0,
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
    let results = vec![result_for("json", f64::NAN), result_for("pdf", 0.9)];
    let mut contract = GuardrailContract {
        doc: "shared".to_string(),
        file_type: None,
        pipeline: "baseline".to_string(),
        min_sf1: Some(0.8),
        min_tf1: None,
        relative_order: Vec::new(),
    };
    let config_for = |contract| GuardrailsConfig {
        version: "1.0".to_string(),
        generated_at: String::new(),
        threshold_factor: 0.9,
        contracts: vec![contract],
    };

    let failures = check_guardrails(&results, &config_for(contract.clone()));
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("ambiguous legacy guardrail target"));
    assert!(failures[0].contains("json, pdf"));

    contract.file_type = Some("pdf".to_string());
    assert!(check_guardrails(&results, &config_for(contract)).is_empty());
}

#[test]
fn relative_order_rejects_missing_repeated_and_empty_anchors() {
    assert_eq!(
        check_relative_order("first then third", &["first".to_string(), "second".to_string()]),
        Err("missing or out-of-order anchor \"second\"".to_string())
    );
    assert_eq!(
        check_relative_order("once", &["once".to_string(), "once".to_string()]),
        Err("missing or out-of-order anchor \"once\"".to_string())
    );
    assert_eq!(
        check_relative_order("content", &["".to_string()]),
        Err("contains an empty anchor".to_string())
    );
}

#[test]
fn relative_order_normalizes_markdown_escapes_and_unicode_hyphens() {
    let anchors = vec![
        "maintainers wanted".to_string(),
        "See #182".to_string(),
        "MongoKit".to_string(),
        "MongoDB is a great schema-less document oriented database".to_string(),
        "Philosophy".to_string(),
    ];
    let equivalent = "maintainers wanted\nSee \\#182\n# MongoKit\n\
        MongoDB is a great schema‑less document oriented database\n### Philosophy";
    assert_eq!(check_relative_order(equivalent, &anchors), Ok(()));

    let wrong_order = "maintainers wanted\n\
        MongoDB is a great schema‑less document oriented database\n\
        See \\#182\n# MongoKit\n### Philosophy";
    assert_eq!(
        check_relative_order(wrong_order, &anchors),
        Err("missing or out-of-order anchor \"MongoDB is a great schema-less document oriented database\"".to_string())
    );
}
