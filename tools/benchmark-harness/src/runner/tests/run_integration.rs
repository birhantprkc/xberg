//! End-to-end `run()` integration tests: empty/unregistered framework selection, batch-mode
//! precondition rejection, warmup fatal/non-fatal handling, and iteration-failure propagation.

use super::support::{
    FailedWarmupAdapter, NonFatalWarmupFailureAdapter, SequenceAdapter, TaskErrorAdapter, TeardownAdapter,
};
use crate::Fixture;
use crate::adapter::FrameworkAdapter;
use crate::config::{BenchmarkConfig, BenchmarkMode};
use crate::registry::AdapterRegistry;
use crate::runner::BenchmarkRunner;
use crate::runner::execution::{BatchIterationTask, SingleIterationTask};
use crate::types::{ErrorKind, OutputFormat};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn test_run_with_no_frameworks() {
    let config = BenchmarkConfig::default();
    let registry = AdapterRegistry::new();
    let mut runner = BenchmarkRunner::new(config, registry);

    let result = runner.run(&[]).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No frameworks available"));
}

#[tokio::test]
async fn test_run_fails_for_requested_unregistered_framework() {
    let config = BenchmarkConfig::default();
    let registry = AdapterRegistry::new();
    let mut runner = BenchmarkRunner::new(config, registry);

    let error = runner.run(&["missing".to_string()]).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requested framework 'missing' is not registered")
    );
}

#[tokio::test]
async fn batch_mode_rejects_non_native_adapter_before_setup() {
    let setup_calls = Arc::new(AtomicUsize::new(0));
    let teardown_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(TeardownAdapter {
            name: "single-only",
            setup_calls: Arc::clone(&setup_calls),
            teardown_calls: Arc::clone(&teardown_calls),
            fail_setup: false,
            fail_teardown: false,
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::Batch,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);

    let error = runner.run(&["single-only".to_string()]).await.unwrap_err();

    assert!(
        error.to_string().contains("verified native batch API"),
        "unexpected error: {error}"
    );
    assert_eq!(setup_calls.load(Ordering::SeqCst), 0);
    assert_eq!(teardown_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn warmup_failure_aborts_run_without_recording_cold_start() {
    use crate::fixture::Fixture;

    let temp_dir = tempfile::tempdir().unwrap();
    let document_path = temp_dir.path().join("document.pdf");
    let fixture_path = temp_dir.path().join("fixture.json");
    std::fs::write(&document_path, b"pdf").unwrap();
    let fixture = Fixture {
        document: PathBuf::from("document.pdf"),
        file_type: "pdf".to_string(),
        file_size: 3,
        expected_frameworks: vec!["xberg-failed-warmup".to_string()],
        metadata: HashMap::new(),
        ground_truth: None,
    };
    std::fs::write(&fixture_path, serde_json::to_string(&fixture).unwrap()).unwrap();

    let teardown_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(FailedWarmupAdapter {
            teardown_calls: Arc::clone(&teardown_calls),
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::SingleFile,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);
    runner.load_fixtures(&fixture_path).unwrap();

    let error = runner.run(&["xberg-failed-warmup".to_string()]).await.unwrap_err();

    assert!(error.to_string().contains("warmup failed for 'xberg-failed-warmup'"));
    assert!(error.to_string().contains("intentional warmup failure"));
    assert!(!runner.cold_start_durations.contains_key("xberg-failed-warmup"));
    assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
}

/// Defect S6 regression: the fatal-vs-non-fatal warmup split (runner.rs ~1159-1177) is fatal
/// only for `adapter.name().starts_with("xberg")`. The only existing warmup-failure test
/// (above) uses an adapter literally named "xberg-failed-warmup" and exercises just the fatal
/// branch — inverting the `starts_with("xberg")` condition would not fail any test before
/// this one. This proves the non-fatal branch: a non-xberg framework whose one-time warmup
/// fails must not abort the run, must still produce its measured result, and must be recorded
/// as having no cold-start sample rather than one silently fabricated or borrowed from
/// elsewhere.
#[tokio::test]
async fn non_fatal_warmup_failure_completes_run_with_no_cold_start_sample() {
    use crate::fixture::Fixture;

    let temp_dir = tempfile::tempdir().unwrap();
    let document_path = temp_dir.path().join("document.pdf");
    let fixture_path = temp_dir.path().join("fixture.json");
    std::fs::write(&document_path, b"pdf").unwrap();
    let fixture = Fixture {
        document: PathBuf::from("document.pdf"),
        file_type: "pdf".to_string(),
        file_size: 3,
        expected_frameworks: vec!["docling-flaky-warmup".to_string()],
        metadata: HashMap::new(),
        ground_truth: None,
    };
    std::fs::write(&fixture_path, serde_json::to_string(&fixture).unwrap()).unwrap();

    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(NonFatalWarmupFailureAdapter {
            calls: AtomicUsize::new(0),
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::SingleFile,
        warmup_iterations: 0,
        benchmark_iterations: 1,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);
    runner.load_fixtures(&fixture_path).unwrap();

    let results = runner
        .run(&["docling-flaky-warmup".to_string()])
        .await
        .expect("a non-fatal (non-xberg) warmup failure must not abort the run");

    assert_eq!(results.len(), 1, "the measured iteration must still be recorded");
    assert!(results[0].success, "the measured extraction itself succeeded");
    assert!(
        !runner.cold_start_durations.contains_key("docling-flaky-warmup"),
        "a failed warmup must not record a cold-start sample"
    );
    assert_eq!(
        results[0].cold_start_duration, None,
        "the recorded result must not carry a fabricated or borrowed cold-start duration"
    );
}

#[tokio::test]
async fn repeated_single_result_fails_if_any_iteration_fails_and_uses_success_payload() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let config = BenchmarkConfig {
        warmup_iterations: 0,
        benchmark_iterations: 2,
        ..Default::default()
    };
    let adapter: Arc<dyn FrameworkAdapter> = Arc::new(SequenceAdapter {
        calls: AtomicUsize::new(0),
    });

    let result = BenchmarkRunner::run_iterations_static(SingleIterationTask {
        file_path: file.path(),
        adapter,
        config: &config,
        cold_start_duration: None,
        force_ocr: false,
        ocr_language: None,
        output_format: OutputFormat::Markdown,
    })
    .await
    .unwrap();

    assert!(!result.success);
    assert_eq!(result.error_kind, ErrorKind::FrameworkError);
    assert_eq!(result.extracted_text.as_deref(), Some("successful payload"));
}

#[tokio::test]
async fn single_file_task_error_fails_run_instead_of_dropping_result() {
    let temp_dir = tempfile::tempdir().unwrap();
    let fixture_path = temp_dir.path().join("fixture.json");
    std::fs::write(temp_dir.path().join("document.pdf"), b"pdf").unwrap();
    let fixture = Fixture {
        document: PathBuf::from("document.pdf"),
        file_type: "pdf".to_string(),
        file_size: 3,
        expected_frameworks: vec!["task-error".to_string()],
        metadata: HashMap::new(),
        ground_truth: None,
    };
    std::fs::write(&fixture_path, serde_json::to_string(&fixture).unwrap()).unwrap();

    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::new(TaskErrorAdapter {
            calls: AtomicUsize::new(0),
        }))
        .unwrap();
    let config = BenchmarkConfig {
        benchmark_mode: BenchmarkMode::SingleFile,
        warmup_iterations: 0,
        benchmark_iterations: 1,
        ..Default::default()
    };
    let mut runner = BenchmarkRunner::new(config, registry);
    runner.load_fixtures(&fixture_path).unwrap();

    let error = runner.run(&["task-error".to_string()]).await.unwrap_err();

    assert!(error.to_string().contains("task-error"));
    assert!(error.to_string().contains("intentional task error"));
}

#[tokio::test]
async fn repeated_batch_result_fails_if_any_iteration_fails_and_uses_success_payload() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let config = BenchmarkConfig {
        warmup_iterations: 0,
        benchmark_iterations: 2,
        ..Default::default()
    };
    let adapter: Arc<dyn FrameworkAdapter> = Arc::new(SequenceAdapter {
        calls: AtomicUsize::new(0),
    });

    let results = BenchmarkRunner::run_batch_iterations_static(BatchIterationTask {
        file_paths: vec![file.path().to_path_buf()],
        adapter,
        config: &config,
        cold_start_duration: None,
        force_ocr_flags: vec![false],
        ocr_languages: vec![None],
        output_format: OutputFormat::Markdown,
    })
    .await
    .unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert_eq!(results[0].error_kind, ErrorKind::FrameworkError);
    assert_eq!(results[0].error_message.as_deref(), Some("measured iteration failed"));
    assert_eq!(results[0].extracted_text.as_deref(), Some("successful payload"));
    assert_eq!(results[0].iterations.len(), 2);
}
