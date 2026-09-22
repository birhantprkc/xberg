//! Quality-ranking coverage adjustment, Pareto frontier, and per-ranking cohort-optionality tests.

use super::support::{create_test_result, pdf_metadata_with_page_count, ranking_value};
use super::*;
use crate::types::{ErrorKind, OcrStatus, QualityMetrics};

fn markdown_pdf_result(
    framework: &str,
    duration_ms: u64,
    memory_bytes: u64,
    page_count: u32,
    quality_score: f64,
) -> BenchmarkResult {
    let mut result = create_test_result(
        framework,
        "pdf",
        OcrStatus::NotUsed,
        duration_ms,
        1_000_000.0,
        memory_bytes,
    );
    result.pdf_metadata = Some(pdf_metadata_with_page_count(page_count));
    result.quality = Some(QualityMetrics {
        f1_score_text: quality_score,
        f1_score_numeric: quality_score,
        f1_score_layout: Some(quality_score),
        quality_score,
        missing_tokens: vec![],
        extra_tokens: vec![],
        correct: false,
        reading_order_score: None,
    });
    result
}

#[test]
fn quality_rankings_penalize_minority_framework_failures_without_changing_percentiles() {
    const SUCCESS_COUNT: usize = 51;
    const FAILURE_COUNT: usize = 49;
    const EXPECTED_ADJUSTED_SCORE: f64 = SUCCESS_COUNT as f64 / (SUCCESS_COUNT + FAILURE_COUNT) as f64;

    let mut results = Vec::new();
    results
        .extend((0..SUCCESS_COUNT).map(|_| markdown_pdf_result("framework-incomplete", 1_000, 100_000_000, 100, 1.0)));
    results.extend((0..FAILURE_COUNT).map(|_| {
        let mut failure = markdown_pdf_result("framework-incomplete", 1_000, 100_000_000, 100, 1.0);
        failure.success = false;
        failure.quality = None;
        failure.error_kind = ErrorKind::FrameworkError;
        failure
    }));
    results.push(markdown_pdf_result("framework-complete", 1_000, 100_000_000, 100, 0.8));

    let aggregated = aggregate_new_format(&results);
    let incomplete = &aggregated.by_framework_mode["framework-incomplete:markdown:single"]
        .overall_performance
        .as_ref()
        .expect("overall performance");
    let raw_quality = incomplete.quality.as_ref().expect("raw quality percentiles");
    assert_eq!(raw_quality.quality_score_p50, 1.0);
    assert_eq!(raw_quality.f1_layout_p50, Some(1.0));

    let comparison = &aggregated.comparison;
    for ranking in [
        &comparison.quality_ranking_markdown,
        &comparison.pdf_quality_ranking_markdown,
        &comparison.pdf_tf1_ranking_markdown,
        &comparison.pdf_sf1_ranking_markdown,
    ] {
        assert!((ranking_value(ranking, "framework-incomplete") - EXPECTED_ADJUSTED_SCORE).abs() < 1e-9);
        assert!(ranking_value(ranking, "framework-incomplete") < ranking_value(ranking, "framework-complete"));
    }
    assert!(
        comparison
            .pareto_frontier
            .iter()
            .all(|point| !point.framework_mode.contains("framework-incomplete")),
        "coverage-adjusted SF1 must keep the dominated incomplete framework off the frontier"
    );
}

#[test]
fn quality_ranking_values_preserve_full_success_scores_and_ignore_infrastructure_failures() {
    const SCORE: f64 = 0.73;
    let success = markdown_pdf_result("framework-complete", 1_000, 100_000_000, 100, SCORE);
    let mut infrastructure_failure = success.clone();
    infrastructure_failure.success = false;
    infrastructure_failure.quality = None;
    infrastructure_failure.error_kind = ErrorKind::HarnessError;

    let aggregated = aggregate_new_format(&[success, infrastructure_failure]);
    let comparison = &aggregated.comparison;
    for ranking in [
        &comparison.quality_ranking_markdown,
        &comparison.pdf_quality_ranking_markdown,
        &comparison.pdf_tf1_ranking_markdown,
        &comparison.pdf_sf1_ranking_markdown,
    ] {
        assert_eq!(ranking_value(ranking, "framework-complete"), SCORE);
    }
    assert_eq!(comparison.pareto_frontier[0].sf1, SCORE);
}

/// `framework-fast` dominates `framework-dominated` on all three Pareto axes (higher
/// pages/sec, higher SF1, lower peak-RSS), so `framework-dominated` must be excluded from
/// the frontier. `framework-balanced` trades pages/sec for SF1 and memory against
/// `framework-fast` (neither dominates the other), so both survive.
#[test]
fn pareto_frontier_excludes_the_dominated_candidate() {
    let results = vec![
        markdown_pdf_result("framework-fast", 1_000, 500_000_000, 100, 0.70),
        markdown_pdf_result("framework-balanced", 2_000, 200_000_000, 100, 0.90),
        markdown_pdf_result("framework-dominated", 4_000, 900_000_000, 100, 0.60),
    ];

    let aggregated = aggregate_new_format(&results);
    let frontier = &aggregated.comparison.pareto_frontier;
    let frontier_keys: std::collections::HashSet<&str> =
        frontier.iter().map(|point| point.framework_mode.as_str()).collect();

    assert_eq!(
        frontier.len(),
        2,
        "expected exactly fast + balanced on the frontier: {frontier:?}"
    );
    assert!(frontier_keys.iter().any(|k| k.contains("framework-fast")));
    assert!(frontier_keys.iter().any(|k| k.contains("framework-balanced")));
    assert!(
        !frontier_keys.iter().any(|k| k.contains("framework-dominated")),
        "framework-dominated must be excluded: framework-fast strictly dominates it on all three axes, got {frontier:?}"
    );

    let fast_point = frontier
        .iter()
        .find(|point| point.framework_mode.contains("framework-fast"))
        .expect("fast point present");
    assert_eq!(fast_point.pages_per_sec, 100.0);
    assert_eq!(fast_point.sf1, 0.70);
    assert_eq!(fast_point.peak_memory_mb, 500.0);
}

#[test]
fn pareto_frontier_excludes_plaintext_frameworks() {
    let mut plaintext = markdown_pdf_result("framework-plaintext", 1_000, 100_000_000, 100, 0.95);
    plaintext.output_format = OutputFormat::Plaintext;
    plaintext.quality.as_mut().unwrap().f1_score_layout = None;

    let aggregated = aggregate_new_format(&[plaintext]);

    assert!(
        aggregated.comparison.pareto_frontier.is_empty(),
        "a plaintext-only framework has no SF1 term and must never appear on the frontier"
    );
}

#[test]
fn pages_per_sec_ranking_orders_by_median_descending() {
    let mut fast = create_test_result(
        "framework-fast",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    fast.pdf_metadata = Some(pdf_metadata_with_page_count(100));

    let mut slow = create_test_result(
        "framework-slow",
        "pdf",
        OcrStatus::NotUsed,
        4_000,
        1_000_000.0,
        10_000_000,
    );
    slow.pdf_metadata = Some(pdf_metadata_with_page_count(100));

    let aggregated = aggregate_new_format(&[fast, slow]);
    let ranking = &aggregated.comparison.pages_per_sec_ranking;

    assert_eq!(ranking.len(), 2);
    assert!(ranking[0].framework_mode.contains("framework-fast"));
    assert_eq!(ranking[0].rank, 1);
    assert_eq!(ranking[0].value, 100.0);
    assert!(ranking[1].framework_mode.contains("framework-slow"));
    assert_eq!(ranking[1].rank, 2);
    assert_eq!(ranking[1].value, 25.0);
}

#[test]
fn cpu_seconds_ranking_orders_ascending_lowest_first() {
    let mut lean = create_test_result(
        "framework-lean",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    lean.metrics.cpu_seconds = 0.5;

    let mut heavy = create_test_result(
        "framework-heavy",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    heavy.metrics.cpu_seconds = 4.0;

    let aggregated = aggregate_new_format(&[lean, heavy]);
    let ranking = &aggregated.comparison.cpu_seconds_ranking;

    assert_eq!(ranking.len(), 2);
    assert!(ranking[0].framework_mode.contains("framework-lean"));
    assert_eq!(ranking[0].rank, 1);
    assert_eq!(ranking[0].value, 0.5);
    assert!(ranking[1].framework_mode.contains("framework-heavy"));
    assert_eq!(ranking[1].rank, 2);
    assert_eq!(ranking[1].value, 4.0);
    assert!(
        ranking[1].relative > 1.0,
        "the higher-CPU framework's relative value should exceed the lowest-CPU baseline"
    );
}

/// Defect S2 regression: `cpu_seconds == 0.0` is `integrate_cpu_core_seconds`'s
/// below-measurement-resolution floor (real for native single-file liteparse/xberg — see
/// `monitoring.rs`), not a legitimate "measured zero CPU time" sample. A framework whose only
/// row reports `0.0` must be excluded from `cpu_seconds_ranking` entirely (it would otherwise
/// win rank 1 with a physically impossible value) and recorded in `unranked_frameworks`
/// instead of vanishing silently. The remaining, genuinely-measured rows must still rank with
/// a well-defined `relative` scaled against the smallest positive value among them.
#[test]
fn cpu_seconds_ranking_excludes_zero_floor_and_records_it_as_unranked() {
    let mut zero_cost = create_test_result(
        "framework-zero-cost",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    zero_cost.metrics.cpu_seconds = 0.0;

    let mut light = create_test_result(
        "framework-light",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    light.metrics.cpu_seconds = 2.0;

    let mut heavy = create_test_result(
        "framework-heavy",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        1_000_000.0,
        10_000_000,
    );
    heavy.metrics.cpu_seconds = 8.0;

    let aggregated = aggregate_new_format(&[zero_cost, light, heavy]);
    let ranking = &aggregated.comparison.cpu_seconds_ranking;

    // Only the two genuinely-measured (positive) frameworks rank; the 0.0-floor framework is
    // excluded, not ranked first.
    assert_eq!(ranking.len(), 2);
    assert!(!ranking.iter().any(|r| r.framework_mode.contains("framework-zero-cost")));

    let relatives: HashMap<&str, f64> = ranking
        .iter()
        .map(|r| (r.framework_mode.as_str(), r.relative))
        .collect();
    let light_relative = *relatives
        .iter()
        .find(|(k, _)| k.contains("framework-light"))
        .map(|(_, v)| v)
        .expect("light framework present");
    let heavy_relative = *relatives
        .iter()
        .find(|(k, _)| k.contains("framework-heavy"))
        .map(|(_, v)| v)
        .expect("heavy framework present");
    assert_eq!(light_relative, 1.0);
    assert_eq!(heavy_relative, 4.0);

    // The exclusion is recorded, not silent (Defect S4).
    let unranked = &aggregated.comparison.unranked_frameworks;
    let zero_cost_entry = unranked
        .iter()
        .find(|u| u.framework_mode.contains("framework-zero-cost"))
        .expect("zero-cost framework must be recorded in unranked_frameworks");
    assert!(zero_cost_entry.reason.contains("cpu_seconds_ranking"));
}

/// Defect S2 regression, all-zero edge case: if literally every framework's only row reports
/// `0.0` cpu_seconds, none of them clears the measurement floor, so `cpu_seconds_ranking` must
/// be empty and every framework must be recorded in `unranked_frameworks` — not silently
/// absent, and not falsely tied for first at `0.0`.
#[test]
fn cpu_seconds_ranking_all_zero_floor_excludes_every_framework() {
    let mut a = create_test_result("framework-a", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    a.metrics.cpu_seconds = 0.0;
    let mut b = create_test_result("framework-b", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    b.metrics.cpu_seconds = 0.0;

    let aggregated = aggregate_new_format(&[a, b]);
    let ranking = &aggregated.comparison.cpu_seconds_ranking;
    assert_eq!(ranking.len(), 0);

    let unranked = &aggregated.comparison.unranked_frameworks;
    assert!(unranked.iter().any(|u| u.framework_mode.contains("framework-a")));
    assert!(unranked.iter().any(|u| u.framework_mode.contains("framework-b")));
    assert!(unranked.iter().all(|u| u.reason.contains("cpu_seconds_ranking")));
}

/// Defect #8 regression: MinerU is marked `optional` (best-effort) in the release contract
/// (`bench_matrix::native_matrix`/`ocr_matrix`), but ranking output carried no flag
/// distinguishing it from a contract-verified framework. `RankedFramework::optional` must be
/// `true` for MinerU's `mineru:markdown:single` cell and `false` for a required framework in
/// the same ranking.
#[test]
fn ranked_framework_flags_optional_cohort_entries() {
    let mineru = create_test_result("mineru", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    let docling = create_test_result("docling", "pdf", OcrStatus::NotUsed, 2_000, 500_000.0, 20_000_000);

    let mut aggregated = aggregate_new_format(&[mineru, docling]);
    aggregated.comparison = comparison_for_cohort(&aggregated.by_framework_mode, crate::bench_matrix::Cohort::Native);

    let find = |ranking: &[RankedFramework], needle: &str| -> RankedFramework {
        ranking
            .iter()
            .find(|r| r.framework_mode.contains(needle))
            .unwrap_or_else(|| panic!("expected a ranking entry containing {needle:?}, got {ranking:?}"))
            .clone()
    };

    let throughput_mineru = find(&aggregated.comparison.throughput_ranking, "mineru");
    let throughput_docling = find(&aggregated.comparison.throughput_ranking, "docling");
    assert!(throughput_mineru.optional, "mineru is optional in the release contract");
    assert!(
        !throughput_docling.optional,
        "docling is a required, contract-verified framework"
    );

    let memory_mineru = find(&aggregated.comparison.memory_ranking, "mineru");
    assert!(memory_mineru.optional);

    let cpu_mineru = find(&aggregated.comparison.cpu_seconds_ranking, "mineru");
    assert!(cpu_mineru.optional);
}

#[test]
fn ranking_optionality_is_specific_to_the_active_cohort() {
    let mut tika = create_test_result("tika", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    tika.output_format = OutputFormat::Plaintext;
    let aggregated = aggregate_new_format(&[tika]);

    let native = comparison_for_cohort(&aggregated.by_framework_mode, crate::bench_matrix::Cohort::Native);
    let ocr = comparison_for_cohort(&aggregated.by_framework_mode, crate::bench_matrix::Cohort::Ocr);

    assert!(!native.throughput_ranking[0].optional, "Tika is required in native PDF");
    assert!(ocr.throughput_ranking[0].optional, "Tika is best-effort in OCR PDF");
}

/// Defect S4 regression: a framework every one of whose results failed (or was reclassified)
/// has zero usable performance samples, so it used to vanish from `throughput_ranking`,
/// `memory_ranking`, `cpu_seconds_ranking`, and `pages_per_sec_ranking` with nothing in
/// `ComparisonData` recording it was attempted at all — a reader saw a chart with only the
/// working frameworks and no hint a fifth ran and produced nothing. It must instead be
/// recorded in `unranked_frameworks`, while a genuinely-working framework in the same cohort
/// is unaffected and still ranks normally.
#[test]
fn fully_failed_framework_is_recorded_in_unranked_frameworks_not_silently_dropped() {
    let mut totally_failed = create_test_result(
        "framework-totally-failed",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );
    totally_failed.success = false;
    totally_failed.error_kind = ErrorKind::Timeout;
    totally_failed.error_message = Some("timed out".to_string());

    let working = create_test_result(
        "framework-working",
        "pdf",
        OcrStatus::NotUsed,
        100,
        1_000_000.0,
        10_000_000,
    );

    let aggregated = aggregate_new_format(&[totally_failed, working]);
    let comparison = &aggregated.comparison;

    // The fully-failed framework has no ranked entry anywhere...
    assert!(
        !comparison
            .throughput_ranking
            .iter()
            .any(|r| r.framework_mode.contains("totally-failed"))
    );
    assert!(
        !comparison
            .memory_ranking
            .iter()
            .any(|r| r.framework_mode.contains("totally-failed"))
    );
    assert!(
        !comparison
            .cpu_seconds_ranking
            .iter()
            .any(|r| r.framework_mode.contains("totally-failed"))
    );

    // ...but its absence is recorded, not silent.
    let entry = comparison
        .unranked_frameworks
        .iter()
        .find(|u| u.framework_mode.contains("totally-failed"))
        .expect("fully-failed framework must be recorded in unranked_frameworks");
    assert!(entry.reason.contains("no usable performance samples"));

    // The genuinely-working framework in the same cohort is unaffected.
    assert!(
        comparison
            .throughput_ranking
            .iter()
            .any(|r| r.framework_mode.contains("framework-working"))
    );
    assert!(
        !comparison
            .unranked_frameworks
            .iter()
            .any(|u| u.framework_mode.contains("framework-working"))
    );
}
