//! `extraction_duration` percentile edge cases and OCR-status cohort tests.

use super::support::create_test_result;
use super::*;
use crate::stats::percentile_r7;
use crate::types::OcrStatus;
use std::time::Duration;

#[test]
fn test_empty_input() {
    let results: Vec<BenchmarkResult> = vec![];
    let aggregated = aggregate_new_format(&results);

    assert_eq!(aggregated.by_framework_mode.len(), 0);
    assert_eq!(aggregated.metadata.total_results, 0);
}

#[test]
fn test_percentile_interpolation() {
    let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let p95 = percentile_r7(&sorted, 0.95);

    assert!((p95 - 4.8).abs() < 0.01);
}

#[test]
fn test_calculate_percentiles_extraction_duration_all_present() {
    let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result1.extraction_duration = Some(Duration::from_millis(80));

    let mut result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
    result2.extraction_duration = Some(Duration::from_millis(120));

    let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
    result3.extraction_duration = Some(Duration::from_millis(160));

    let refs = vec![&result1, &result2, &result3];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
    assert!((ext_dur.p50 - 120.0).abs() < 0.1);
    // n=3 is far below MIN_SAMPLES_FOR_P95 (20): p95/p99 must be suppressed rather than
    // fabricated from an interpolation that is really just reading off the max (Defect S1).
    assert_eq!(ext_dur.sample_count, 3);
    assert_eq!(ext_dur.p95, None);
    assert_eq!(ext_dur.p99, None);
}

#[test]
fn test_calculate_percentiles_extraction_duration_all_none() {
    let result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    let result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
    let result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);

    let refs = vec![&result1, &result2, &result3];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_none());
}

#[test]
fn test_calculate_percentiles_extraction_duration_mixed() {
    let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result1.extraction_duration = Some(Duration::from_millis(80));

    let result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);

    let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
    result3.extraction_duration = Some(Duration::from_millis(160));

    let refs = vec![&result1, &result2, &result3];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
    assert!((ext_dur.p50 - 120.0).abs() < 0.1);
}

#[test]
fn test_calculate_percentiles_extraction_duration_filters_invalid() {
    let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result1.extraction_duration = Some(Duration::from_millis(80));

    let mut result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
    result2.extraction_duration = Some(Duration::from_millis(120));

    let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
    result3.extraction_duration = Some(Duration::from_millis(160));

    let refs = vec![&result1, &result2, &result3];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
    assert!(ext_dur.p50.is_finite());
    assert!(!ext_dur.p50.is_nan());
}

#[test]
fn test_calculate_percentiles_extraction_duration_with_failed_results() {
    let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result1.extraction_duration = Some(Duration::from_millis(80));

    let mut result2_failed = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 0, 0.0, 0);
    result2_failed.success = false;
    result2_failed.error_message = Some("Failed".to_string());
    result2_failed.extraction_duration = Some(Duration::from_millis(50));

    let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
    result3.extraction_duration = Some(Duration::from_millis(160));

    let refs = vec![&result1, &result2_failed, &result3];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
    assert_eq!(percentiles.successful_sample_count, 2);
    assert_eq!(percentiles.total_sample_count, 3);
    assert!((ext_dur.p50 - 120.0).abs() < 0.1);
}

#[test]
fn test_aggregate_by_ocr_status_extraction_duration() {
    let mut result_no_ocr_1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result_no_ocr_1.extraction_duration = Some(Duration::from_millis(80));

    let mut result_no_ocr_2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
    result_no_ocr_2.extraction_duration = Some(Duration::from_millis(120));

    let mut result_with_ocr = create_test_result("framework1", "pdf", OcrStatus::Used, 300, 500_000.0, 20_000_000);
    result_with_ocr.extraction_duration = Some(Duration::from_millis(250));

    let refs = vec![&result_no_ocr_1, &result_no_ocr_2, &result_with_ocr];
    let (no_ocr, with_ocr) = aggregate_by_ocr_status(&refs);

    assert!(no_ocr.is_some());
    let no_ocr_perf = no_ocr.unwrap();
    assert!(no_ocr_perf.extraction_duration.is_some());
    assert_eq!(no_ocr_perf.extraction_duration.as_ref().unwrap().p50, 100.0);

    assert!(with_ocr.is_some());
    let with_ocr_perf = with_ocr.unwrap();
    assert!(with_ocr_perf.extraction_duration.is_some());
    assert_eq!(with_ocr_perf.extraction_duration.as_ref().unwrap().p50, 250.0);
}

#[test]
fn unknown_pdf_ocr_status_is_excluded_from_ocr_cohorts() {
    let unknown = create_test_result("framework1", "pdf", OcrStatus::Unknown, 100, 1_000_000.0, 10_000_000);
    let refs = vec![&unknown];

    let (no_ocr, with_ocr) = aggregate_by_ocr_status(&refs);

    assert!(no_ocr.is_none());
    assert!(with_ocr.is_none());
}

#[test]
fn test_aggregate_new_format_extraction_duration_preserved() {
    let mut result1 = create_test_result("xberg-sync", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result1.extraction_duration = Some(Duration::from_millis(80));

    let mut result2 = create_test_result("xberg-sync", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
    result2.extraction_duration = Some(Duration::from_millis(120));

    let results = vec![result1, result2];
    let aggregated = aggregate_new_format(&results);

    let framework_mode = aggregated.by_framework_mode.get("xberg:markdown:single").unwrap();
    let pdf_stats = framework_mode.by_file_type.get("pdf").unwrap();
    let no_ocr = pdf_stats.no_ocr.as_ref().unwrap();

    assert!(no_ocr.extraction_duration.is_some());
    let ext_dur = no_ocr.extraction_duration.as_ref().unwrap();
    assert!((ext_dur.p50 - 100.0).abs() < 0.1);
}

#[test]
fn test_calculate_percentiles_extraction_duration_single_value() {
    let mut result = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result.extraction_duration = Some(Duration::from_millis(80));

    let refs = vec![&result];
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
    assert_eq!(ext_dur.p50, 80.0);
    // n=1: p95/p99 would otherwise equal p50 (the single sample doubling as its own "tail"),
    // which is exactly the fabricated-precision anti-pattern Defect S1 removes.
    assert_eq!(ext_dur.sample_count, 1);
    assert_eq!(ext_dur.p95, None);
    assert_eq!(ext_dur.p99, None);
    assert_eq!(ext_dur.std_dev, 0.0);
}

#[test]
fn test_calculate_percentiles_extraction_duration_large_dataset() {
    let mut results = vec![];
    for i in 1..=100 {
        let mut result = create_test_result("framework1", "pdf", OcrStatus::NotUsed, i * 10, 1_000_000.0, 10_000_000);
        result.extraction_duration = Some(Duration::from_millis(i * 8));
        results.push(result);
    }

    let refs: Vec<&BenchmarkResult> = results.iter().collect();
    let percentiles = calculate_percentiles(&refs);

    assert!(percentiles.extraction_duration.is_some());
    let ext_dur = percentiles.extraction_duration.as_ref().unwrap();

    assert!(ext_dur.p50 >= 400.0 && ext_dur.p50 <= 410.0);

    // n=100 clears both MIN_SAMPLES_FOR_P95 (20) and MIN_SAMPLES_FOR_P99 (100), so both
    // percentiles are reported (Defect S1 only suppresses below-threshold groups).
    assert_eq!(ext_dur.sample_count, 100);
    let p95 = ext_dur.p95.expect("n=100 supports p95");
    let p99 = ext_dur.p99.expect("n=100 supports p99");
    assert!((p95 - 760.4).abs() < 0.01, "p95 = {p95}");
    assert!((p99 - 792.08).abs() < 0.01, "p99 = {p99}");
    assert!(p95 > ext_dur.p50);
    assert!(p99 > p95);
}
