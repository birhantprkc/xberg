//! Tests for native-batch execution: fixed-batch partitioning against a real cohort,
//! repeated-iteration sample bookkeeping, and single/batch quality-score parity.

use super::support::{RecordingBatchAdapter, write_ordered_cohort};
use crate::adapter::FrameworkAdapter;
use crate::config::{BenchmarkConfig, BenchmarkMode};
use crate::registry::AdapterRegistry;
use crate::runner::BenchmarkRunner;
use crate::runner::execution::BatchIterationTask;
use crate::types::OutputFormat;
use std::sync::Arc;

#[tokio::test]
async fn fixed_batches_preserve_manifest_order_across_native_calls() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = write_ordered_cohort(temp.path(), &["pdf", "pdf", "pdf", "pdf"]);
    let batches = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(RecordingBatchAdapter {
            batches: Arc::clone(&batches),
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::Batch,
        warmup_iterations: 0,
        benchmark_iterations: 1,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);
    runner.load_cohort(temp.path(), &manifest).unwrap();
    runner.set_fixed_batch_size(2).unwrap();

    runner.run(&["recording".to_string()]).await.unwrap();

    assert_eq!(
        *batches.lock().unwrap(),
        [
            vec!["d.pdf".to_string(), "b.pdf".to_string()],
            vec!["a.pdf".to_string(), "c.pdf".to_string()]
        ]
    );
}

#[tokio::test]
async fn fixed_batches_allow_partial_after_adapter_filter() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = write_ordered_cohort(temp.path(), &["pdf", "txt", "pdf", "pdf"]);
    let batches = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(RecordingBatchAdapter {
            batches: Arc::clone(&batches),
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::Batch,
        warmup_iterations: 0,
        benchmark_iterations: 1,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);
    runner.load_cohort(temp.path(), &manifest).unwrap();
    runner.set_fixed_batch_size(2).unwrap();

    runner.run(&["recording".to_string()]).await.unwrap();

    // The adapter supports only pdf, so the txt fixture (b.txt) is filtered out before the
    // native batch call. The 3 eligible pdfs partition into a full batch of 2 and a smaller
    // final batch of 1 (manifest fixture order d, a, c) rather than aborting the run.
    assert_eq!(
        *batches.lock().unwrap(),
        [
            vec!["d.pdf".to_string(), "a.pdf".to_string()],
            vec!["c.pdf".to_string()],
        ]
    );
}

#[tokio::test]
async fn repeated_native_batch_preserves_one_sample_per_document_and_iteration() {
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();
    let batches = Arc::new(std::sync::Mutex::new(Vec::new()));
    let config = BenchmarkConfig {
        warmup_iterations: 0,
        benchmark_iterations: 3,
        benchmark_mode: BenchmarkMode::Batch,
        ..Default::default()
    };
    let adapter: Arc<dyn FrameworkAdapter> = Arc::new(RecordingBatchAdapter {
        batches: Arc::clone(&batches),
    });

    let results = BenchmarkRunner::run_batch_iterations_static(BatchIterationTask {
        file_paths: vec![first.path().to_path_buf(), second.path().to_path_buf()],
        adapter,
        config: &config,
        cold_start_duration: None,
        force_ocr_flags: vec![false, false],
        ocr_languages: vec![None, None],
        output_format: OutputFormat::Markdown,
    })
    .await
    .unwrap();

    assert_eq!(batches.lock().unwrap().len(), 3);
    assert_eq!(results.len(), 2);
    for result in results {
        assert_eq!(
            result
                .iterations
                .iter()
                .map(|iteration| iteration.iteration)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(result.statistics.unwrap().sample_count, 3);
    }
}

#[tokio::test]
async fn standard_single_and_batch_runners_populate_numeric_tf1_and_sf1() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("document.pdf"), b"pdf").unwrap();
    std::fs::write(temp.path().join("ground_truth.txt"), "ok").unwrap();
    std::fs::write(temp.path().join("ground_truth.md"), "ok").unwrap();
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(
        &fixture_path,
        serde_json::json!({
            "document": "document.pdf",
            "file_type": "pdf",
            "file_size": 3,
            "ground_truth": {
                "text_file": "ground_truth.txt",
                "markdown_file": "ground_truth.md",
                "source": "manual"
            }
        })
        .to_string(),
    )
    .unwrap();

    for benchmark_mode in [BenchmarkMode::SingleFile, BenchmarkMode::Batch] {
        let mut registry = AdapterRegistry::new();
        registry
            .register(Arc::new(RecordingBatchAdapter {
                batches: Arc::new(std::sync::Mutex::new(Vec::new())),
            }))
            .unwrap();
        let config = BenchmarkConfig {
            benchmark_mode,
            measure_quality: true,
            warmup_iterations: 0,
            benchmark_iterations: 1,
            ..Default::default()
        };
        let mut runner = BenchmarkRunner::new(config, registry);
        runner.load_fixtures(&fixture_path).unwrap();

        let results = runner.run(&["recording".to_string()]).await.unwrap();
        let quality = results[0]
            .quality
            .as_ref()
            .expect("quality populated by standard runner");
        assert_eq!(quality.f1_score_text, 1.0);
        assert_eq!(quality.f1_score_numeric, 1.0);
        assert_eq!(quality.f1_score_layout, Some(1.0));
    }
}
