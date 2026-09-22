//! pages/sec, cpu-seconds, batch-size, and system-load derived-metric tests.

use super::support::{create_test_result, pdf_metadata_with_page_count};
use super::*;
use crate::system_load::SystemLoad;
use crate::types::{ErrorKind, OcrStatus};

#[test]
fn pages_per_sec_percentile_from_single_file_pdf_metadata() {
    let mut result = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        2_000,
        1_000_000.0,
        10_000_000,
    );
    result.pdf_metadata = Some(pdf_metadata_with_page_count(20));

    let percentiles = calculate_percentiles(&[&result]);

    let pages_per_sec = percentiles.pages_per_sec.expect("pages_per_sec must be populated");
    assert_eq!(pages_per_sec.p50, 10.0);
    // n=1: p95/p99 suppressed rather than fabricated as equal to the single sample
    // (Defect S1).
    assert_eq!(pages_per_sec.sample_count, 1);
    assert_eq!(pages_per_sec.p95, None);
    assert_eq!(pages_per_sec.p99, None);
}

#[test]
fn pages_per_sec_is_none_without_any_page_count_data() {
    let result = create_test_result(
        "xberg-markdown-baseline",
        "docx",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );

    let percentiles = calculate_percentiles(&[&result]);

    assert!(percentiles.pages_per_sec.is_none());
}

#[test]
fn pages_per_sec_sums_page_counts_across_one_batch_invocation() {
    let capability = crate::types::BatchCapability {
        entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
        timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing: true,
    };

    let mut doc_a = create_test_result(
        "xberg-markdown-baseline-batch",
        "pdf",
        OcrStatus::NotUsed,
        4_000,
        1_000_000.0,
        10_000_000,
    );
    doc_a.framework_capabilities.batch_support = true;
    doc_a.framework_capabilities.batch_capability = Some(capability);
    doc_a.framework_capabilities.batch_performance_sample = Some(true);
    doc_a.framework_capabilities.batch_sample_id = Some("batch-1".to_string());
    doc_a.pdf_metadata = Some(pdf_metadata_with_page_count(12));

    let mut doc_b = create_test_result(
        "xberg-markdown-baseline-batch",
        "pdf",
        OcrStatus::NotUsed,
        4_000,
        1_000_000.0,
        10_000_000,
    );
    doc_b.framework_capabilities.batch_support = true;
    doc_b.framework_capabilities.batch_capability = Some(capability);
    doc_b.framework_capabilities.batch_performance_sample = Some(false);
    doc_b.framework_capabilities.batch_sample_id = Some("batch-1".to_string());
    doc_b.pdf_metadata = Some(pdf_metadata_with_page_count(8));

    let percentiles = calculate_percentiles(&[&doc_a, &doc_b]);

    let pages_per_sec = percentiles
        .pages_per_sec
        .expect("pages_per_sec must be populated for the batch");
    assert_eq!(pages_per_sec.p50, 5.0);
    assert_eq!(percentiles.performance_sample_count, 1);
}

#[test]
fn cpu_seconds_percentile_aggregates_from_performance_samples() {
    let mut r1 = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    r1.metrics.cpu_seconds = 1.0;
    let mut r2 = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    r2.metrics.cpu_seconds = 2.0;
    let mut r3 = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    r3.metrics.cpu_seconds = 3.0;

    let percentiles = calculate_percentiles(&[&r1, &r2, &r3]);

    assert_eq!(percentiles.cpu_seconds.p50, 2.0);
}

#[test]
fn batch_size_is_one_for_single_file_mode() {
    let result = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );

    let percentiles = calculate_percentiles(&[&result]);

    assert_eq!(percentiles.batch_size, Some(1));
}

#[test]
fn batch_size_reflects_documents_per_batch_invocation() {
    let capability = crate::types::BatchCapability {
        entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
        timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing: true,
    };
    let mut results = [
        create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        ),
        create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        ),
        create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        ),
        create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        ),
    ];
    for (index, result) in results.iter_mut().enumerate() {
        result.framework_capabilities.batch_support = true;
        result.framework_capabilities.batch_capability = Some(capability);
        result.framework_capabilities.batch_performance_sample = Some(index == 0);
        result.framework_capabilities.batch_sample_id = Some("batch-of-4".to_string());
    }

    let refs: Vec<&BenchmarkResult> = results.iter().collect();
    let percentiles = calculate_percentiles(&refs);

    assert_eq!(percentiles.performance_sample_count, 1);
    assert_eq!(percentiles.batch_size, Some(4));
}

/// Defect S3 regression: in single-file mode, `batch_size` must reflect real batch grouping
/// (none — every row has no `batch_sample_id`) rather than the coarse `total_sample_count /
/// performance_sample_count` ratio. With 10 single-file results and only 5 timing-eligible
/// (5 reclassified to `EmptyContent`, which is not timing-eligible), the old ratio published a
/// fictitious `batch_size` of `round(10 / 5) == 2` — a framework that never batches anything
/// would report shipping 2 documents per invocation. It must report `Some(1)`.
#[test]
fn batch_size_is_one_for_single_file_mode_despite_partial_timing_eligibility() {
    let mut results = Vec::new();
    for _ in 0..5 {
        results.push(create_test_result(
            "framework1",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        ));
    }
    for _ in 0..5 {
        let mut failed = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 0, 0.0, 0);
        failed.success = false;
        failed.error_kind = ErrorKind::EmptyContent;
        failed.error_message = Some("empty".to_string());
        results.push(failed);
    }

    let refs: Vec<&BenchmarkResult> = results.iter().collect();
    let percentiles = calculate_percentiles(&refs);

    assert_eq!(percentiles.total_sample_count, 10);
    assert_eq!(percentiles.performance_sample_count, 5);
    assert_eq!(percentiles.batch_size, Some(1));
}

#[test]
fn system_load_aggregates_contention_across_results() {
    let mut idle = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    idle.system_load = Some(SystemLoad {
        load_avg_1m: 1.0,
        load_avg_5m: 1.0,
        load_avg_15m: 1.0,
        logical_cores: 10,
        physical_cores: 10,
    });

    let mut busy = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    busy.system_load = Some(SystemLoad {
        load_avg_1m: 12.0,
        load_avg_5m: 12.0,
        load_avg_15m: 12.0,
        logical_cores: 10,
        physical_cores: 10,
    });

    let percentiles = calculate_percentiles(&[&idle, &busy]);

    let system_load = percentiles.system_load.expect("system_load must be populated");
    assert_eq!(system_load.total_sample_count, 2);
    assert_eq!(
        system_load.contended_sample_count, 1,
        "only the busy sample (load_per_core 1.2 > 0.7 threshold) should count as contended"
    );
    assert!((system_load.load_per_core_p50 - 0.65).abs() < 1e-9);
}

#[test]
fn system_load_is_none_without_any_captured_snapshot() {
    let result = create_test_result(
        "xberg-markdown-baseline",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );

    let percentiles = calculate_percentiles(&[&result]);

    assert!(percentiles.system_load.is_none());
}
