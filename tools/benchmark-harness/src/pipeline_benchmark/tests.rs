use std::collections::HashMap;

use super::*;

fn test_config(fixtures_dir: PathBuf) -> PipelineBenchmarkConfig {
    PipelineBenchmarkConfig {
        fixtures_dir,
        paths: vec![Pipeline::Baseline],
        doc_filter: Vec::new(),
        exact_doc_filter: Vec::new(),
        dump_outputs: false,
        json_output: None,
        sort_by: SortMetric::Sf1,
        bottom_n: None,
        triage_blocks: false,
    }
}

#[test]
fn structural_scoring_uses_content_after_50_kib() {
    let prefix = format!("{}\n\n", "plain text ".repeat(6_000));
    assert!(prefix.len() > 50 * 1024);
    let markdown = format!("{prefix}# Tail heading\n");
    let structural = score_structural_markdown(&markdown, &markdown);
    assert_eq!(structural.sf1, 1.0);
    assert_eq!(structural.per_type_sf1.get("heading"), Some(&1.0));
}

#[test]
fn aggregates_count_available_metrics_independently() {
    let pipeline_result = |sf1, tf1| PipelineResult {
        pipeline: Pipeline::Docling,
        sf1,
        tf1,
        char_similarity: tf1,
        order_score: tf1,
        per_type_sf1: HashMap::new(),
        per_type_precision: HashMap::new(),
        per_type_recall: HashMap::new(),
        time_ms: 10.0,
        missing_tokens: Vec::new(),
        extra_tokens: Vec::new(),
        content: String::new(),
    };
    let doc_result = |name: &str, sf1, tf1| PipelineDocResult {
        name: name.to_string(),
        file_type: "pdf".to_string(),
        file_size: 1,
        results: vec![pipeline_result(sf1, tf1)],
    };
    let results = vec![
        doc_result("all-metrics", 0.75, 0.75),
        doc_result("tf1-only", f64::NAN, 0.25),
    ];

    let aggregates = compute_aggregates(&results);

    assert_eq!(aggregates.len(), 1);
    assert_eq!(aggregates[0].sf1_count, Some(1));
    assert_eq!(aggregates[0].tf1_count, Some(2));
    assert_eq!(aggregates[0].mean_tf1, 0.5);
    assert_eq!(aggregates[0].p50_tf1, 0.75);
    let serialized = serde_json::to_value(&aggregates).unwrap();
    assert_eq!(serialized[0]["sf1_count"], 1);
    assert_eq!(serialized[0]["tf1_count"], 2);
    assert_eq!(
        format_coverage_line(&aggregates[0], results.len()),
        "  docling coverage: SF1 1/2 docs, TF1 2/2 docs"
    );
}

#[test]
fn aggregates_preserve_all_unavailable_scores() {
    let results = vec![PipelineDocResult {
        name: "failure".to_string(),
        file_type: "pdf".to_string(),
        file_size: 1,
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

    let aggregates = compute_aggregates(&results);
    let serialized = serde_json::to_value(&aggregates).unwrap();

    assert!(aggregates[0].mean_sf1.is_nan());
    assert!(aggregates[0].mean_tf1.is_nan());
    assert_eq!(aggregates[0].sf1_count, Some(0));
    assert_eq!(aggregates[0].tf1_count, Some(0));
    assert!(aggregates[0].p50_sf1.is_nan());
    assert!(aggregates[0].p50_tf1.is_nan());
    assert!(serialized[0]["mean_sf1"].is_null());
    assert!(serialized[0]["mean_tf1"].is_null());
    assert!(serialized[0]["p50_sf1"].is_null());
    assert!(serialized[0]["p50_tf1"].is_null());
}

#[test]
fn legacy_summary_deserializes_without_provenance() {
    let summary: PipelineRunSummary = serde_json::from_value(serde_json::json!({
        "timestamp": "2026-01-01T00:00:00Z",
        "git_sha": "abc",
        "doc_count": 0,
        "pipeline_count": 0,
        "aggregates": [{
            "pipeline": "docling",
            "mean_sf1": 0.0,
            "mean_tf1": 0.0,
            "mean_time_ms": 0.0,
            "p50_sf1": 0.0,
            "p50_tf1": 0.0,
            "p50_time_ms": 0.0,
            "p90_time_ms": 0.0
        }],
        "docs": []
    }))
    .unwrap();
    assert!(summary.provenance.hash_algorithm.is_empty());
    assert_eq!(summary.aggregates[0].sf1_count, None);
    assert_eq!(summary.aggregates[0].tf1_count, None);
    assert_eq!(
        format_coverage_line(&summary.aggregates[0], 0),
        "  docling coverage: unknown (legacy artifact)"
    );
}

#[test]
fn config_hash_tracks_exact_selection() {
    let mut config = test_config(PathBuf::from("fixtures"));
    let before = hash_config(&config);
    config.exact_doc_filter.push("fixture-a".to_string());
    assert_ne!(before, hash_config(&config));
}

#[tokio::test]
async fn empty_selection_is_an_error() {
    let fixtures = tempfile::tempdir().unwrap();
    let error = run_pipeline_benchmark(&test_config(fixtures.path().to_path_buf()))
        .await
        .unwrap_err();
    assert!(matches!(error, crate::Error::Config(_)));
}
