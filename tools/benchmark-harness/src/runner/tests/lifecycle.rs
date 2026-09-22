//! Tests for OCR-cohort validation, batch warmup elision, and setup/teardown lifecycle
//! error handling.

use super::support::TeardownAdapter;
use crate::adapter::FrameworkAdapter;
use crate::runner::BenchmarkRunner;
use crate::runner::helpers::{effective_batch_warmup_iterations, validate_batch_ocr_cohort, validate_ocr_cohort};
use crate::types::{BatchCapability, BatchTimingScope};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn ocr_required_fixture_cohort_is_never_silently_skipped() {
    assert!(validate_ocr_cohort(false, 0).is_ok());
    assert!(validate_ocr_cohort(true, 2).is_ok());
    let error = validate_ocr_cohort(false, 2).unwrap_err();
    assert!(error.to_string().contains("--ocr"));
    assert!(error.to_string().contains("will not silently omit"));
}

#[test]
fn native_batch_requires_homogeneous_ocr_cohort() {
    assert!(validate_batch_ocr_cohort(false, 2, 1).is_ok());
    assert!(validate_batch_ocr_cohort(true, 2, 0).is_ok());
    assert!(validate_batch_ocr_cohort(true, 2, 2).is_ok());
    let error = validate_batch_ocr_cohort(true, 3, 1).unwrap_err();
    assert!(error.to_string().contains("homogeneous OCR cohort"));
    assert!(error.to_string().contains("will not label sequential fallback"));
}

#[test]
fn cold_subprocess_batch_never_runs_discarded_warmup_iterations() {
    let cold = BatchCapability {
        entry_point: crate::types::BatchEntryPoint::DoclingJobkit,
        timing_scope: BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing: false,
    };
    let warm = BatchCapability {
        entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
        timing_scope: BatchTimingScope::WarmSteadyState,
        per_item_timing: true,
    };

    assert_eq!(effective_batch_warmup_iterations(cold, 3), 0);
    assert_eq!(effective_batch_warmup_iterations(warm, 3), 3);
}

#[tokio::test]
async fn teardown_attempts_every_framework_after_an_error() {
    let setup_calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let frameworks: Vec<Arc<dyn FrameworkAdapter>> = vec![
        Arc::new(TeardownAdapter {
            name: "first",
            setup_calls: Arc::clone(&setup_calls),
            teardown_calls: Arc::clone(&first_calls),
            fail_setup: false,
            fail_teardown: true,
        }),
        Arc::new(TeardownAdapter {
            name: "second",
            setup_calls,
            teardown_calls: Arc::clone(&second_calls),
            fail_setup: false,
            fail_teardown: false,
        }),
    ];

    let error = BenchmarkRunner::teardown_frameworks(&frameworks).await.unwrap_err();

    assert!(error.to_string().contains("first teardown failed"));
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn setup_failure_tears_down_only_previously_initialized_frameworks() {
    let first_setup = Arc::new(AtomicUsize::new(0));
    let first_teardown = Arc::new(AtomicUsize::new(0));
    let second_setup = Arc::new(AtomicUsize::new(0));
    let second_teardown = Arc::new(AtomicUsize::new(0));
    let frameworks: Vec<Arc<dyn FrameworkAdapter>> = vec![
        Arc::new(TeardownAdapter {
            name: "first",
            setup_calls: Arc::clone(&first_setup),
            teardown_calls: Arc::clone(&first_teardown),
            fail_setup: false,
            fail_teardown: false,
        }),
        Arc::new(TeardownAdapter {
            name: "second",
            setup_calls: Arc::clone(&second_setup),
            teardown_calls: Arc::clone(&second_teardown),
            fail_setup: true,
            fail_teardown: false,
        }),
    ];

    let error = BenchmarkRunner::setup_frameworks(&frameworks).await.unwrap_err();

    assert!(error.to_string().contains("second setup failed"));
    assert_eq!(first_setup.load(Ordering::SeqCst), 1);
    assert_eq!(second_setup.load(Ordering::SeqCst), 1);
    assert_eq!(first_teardown.load(Ordering::SeqCst), 1);
    assert_eq!(second_teardown.load(Ordering::SeqCst), 0);
}
