//! Segmented-ranking (`(output_format, mode)`-scoped) and per-fixture-row losslessness tests.

use super::support::create_test_result;
use super::*;
use crate::system_load::SystemLoad;
use crate::types::{OcrStatus, PdfMetadata, QualityMetrics};

#[test]
fn comparison_order_is_deterministic_when_metrics_tie() {
    let docling = create_test_result("docling", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    let tika = create_test_result("tika", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);

    let forward = aggregate_new_format(&[docling.clone(), tika.clone()]);
    let reverse = aggregate_new_format(&[tika, docling]);

    assert_eq!(
        serde_json::to_value(forward.comparison).unwrap(),
        serde_json::to_value(reverse.comparison).unwrap()
    );
}

/// Defect regression: a markdown-batch framework and a plaintext-single-file framework must
/// never be pooled into one `throughput_ranking`. Markdown serialization cost and
/// batch-amortized process overhead are not comparable to plaintext-single-file numbers, so
/// each framework here is the sole (and therefore winning) member of its own
/// `(output_format, mode)` segment. Under the old pooled logic, the higher-throughput
/// framework would have won rank 1 globally and the other would have been rank 2.
#[test]
fn throughput_ranking_never_pools_across_output_format_and_mode_segments() {
    let markdown_batch = create_test_result(
        "framework-a-batch",
        "pdf",
        OcrStatus::NotUsed,
        1_000,
        5_000_000.0,
        10_000_000,
    );
    let mut plaintext_single =
        create_test_result("framework-b", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 10_000_000);
    plaintext_single.output_format = OutputFormat::Plaintext;

    let aggregated = aggregate_new_format(&[markdown_batch, plaintext_single]);
    let ranking = &aggregated.comparison.throughput_ranking;
    assert_eq!(ranking.len(), 2);

    let markdown_entry = ranking
        .iter()
        .find(|r| r.framework_mode.contains("framework-a"))
        .expect("markdown-batch entry present");
    let plaintext_entry = ranking
        .iter()
        .find(|r| r.framework_mode.contains("framework-b"))
        .expect("plaintext-single entry present");

    assert_eq!(markdown_entry.rank, 1, "sole member of its segment must be rank 1");
    assert_eq!(markdown_entry.relative, 1.0);
    assert_eq!(markdown_entry.output_format, OutputFormat::Markdown);
    assert_eq!(markdown_entry.mode, "batch");

    assert_eq!(plaintext_entry.rank, 1, "sole member of its segment must be rank 1");
    assert_eq!(plaintext_entry.relative, 1.0);
    assert_eq!(plaintext_entry.output_format, OutputFormat::Plaintext);
    assert_eq!(plaintext_entry.mode, "single");
}

/// Defect regression: `deltas_vs_baseline` must be computed against each entry's own
/// `(output_format, mode)` segment baseline, not a single cross-segment winner. `seg2-*` has
/// much higher raw throughput than `seg1-*`, so under the old pooled logic `seg1-low`'s delta
/// would have been computed against `seg2-high` (throughput_delta_mbs = 5.0 - 100.0 = -95.0),
/// not its own segment's `seg1-high` (throughput_delta_mbs = 5.0 - 10.0 = -5.0).
#[test]
fn deltas_vs_baseline_use_the_segment_baseline_of_their_own_entry() {
    let seg1_high = create_test_result("seg1-high", "pdf", OcrStatus::NotUsed, 1_000, 10_000_000.0, 5_000_000);
    let seg1_low = create_test_result("seg1-low", "pdf", OcrStatus::NotUsed, 1_000, 5_000_000.0, 8_000_000);
    let mut seg2_high = create_test_result("seg2-high", "pdf", OcrStatus::NotUsed, 1_000, 100_000_000.0, 2_000_000);
    seg2_high.output_format = OutputFormat::Plaintext;
    let mut seg2_low = create_test_result("seg2-low", "pdf", OcrStatus::NotUsed, 1_000, 50_000_000.0, 3_000_000);
    seg2_low.output_format = OutputFormat::Plaintext;

    let aggregated = aggregate_new_format(&[seg1_high, seg1_low, seg2_high, seg2_low]);
    let deltas = &aggregated.comparison.deltas_vs_baseline;

    let seg1_low_key = deltas
        .keys()
        .find(|k| k.contains("seg1-low"))
        .cloned()
        .expect("seg1-low delta present");
    let seg1_delta = &deltas[&seg1_low_key];
    assert_eq!(seg1_delta.throughput_delta_mbs, -5.0);
    assert_eq!(seg1_delta.throughput_delta_percent, -50.0);
    assert_eq!(seg1_delta.memory_delta_mb, 3.0);
    assert_eq!(seg1_delta.memory_delta_percent, 60.0);

    let seg2_low_key = deltas
        .keys()
        .find(|k| k.contains("seg2-low"))
        .cloned()
        .expect("seg2-low delta present");
    let seg2_delta = &deltas[&seg2_low_key];
    assert_eq!(seg2_delta.throughput_delta_mbs, -50.0);
    assert_eq!(seg2_delta.throughput_delta_percent, -50.0);
    assert_eq!(seg2_delta.memory_delta_mb, 1.0);
    assert_eq!(seg2_delta.memory_delta_percent, 50.0);

    assert!(
        !deltas.keys().any(|k| k.contains("seg1-high")),
        "the segment baseline itself must not get a delta entry"
    );
    assert!(
        !deltas.keys().any(|k| k.contains("seg2-high")),
        "the segment baseline itself must not get a delta entry"
    );
}

/// Defect regression: `memory_ranking` (lower is better) must rank ascending independently
/// within each `(output_format, mode)` segment. `mem-b-low` has more memory than
/// `mem-a-low`'s segment, yet must still land at rank 1 in its own segment because it is the
/// smallest within `mem-b-*`.
#[test]
fn memory_ranking_segments_independently_by_output_format_and_mode() {
    let mem_a_low = create_test_result("mem-a-low", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 2_000_000);
    let mem_a_high = create_test_result("mem-a-high", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 9_000_000);
    let mut mem_b_low = create_test_result("mem-b-low", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 5_000_000);
    mem_b_low.output_format = OutputFormat::Plaintext;
    let mut mem_b_high = create_test_result("mem-b-high", "pdf", OcrStatus::NotUsed, 1_000, 1_000_000.0, 20_000_000);
    mem_b_high.output_format = OutputFormat::Plaintext;

    let aggregated = aggregate_new_format(&[mem_a_low, mem_a_high, mem_b_low, mem_b_high]);
    let ranking = &aggregated.comparison.memory_ranking;
    assert_eq!(ranking.len(), 4);

    let find = |needle: &str| {
        ranking
            .iter()
            .find(|r| r.framework_mode.contains(needle))
            .unwrap_or_else(|| panic!("expected a ranking entry containing {needle:?}, got {ranking:?}"))
    };

    let a_low = find("mem-a-low");
    assert_eq!(a_low.rank, 1);
    assert_eq!(a_low.value, 2.0);
    assert_eq!(a_low.relative, 1.0);

    let b_low = find("mem-b-low");
    assert_eq!(
        b_low.rank, 1,
        "mem-b-low is the smallest within its own segment despite exceeding mem-a-low"
    );
    assert_eq!(b_low.value, 5.0);
    assert_eq!(b_low.relative, 1.0);

    let a_high = find("mem-a-high");
    assert_eq!(a_high.rank, 2);
    assert_eq!(a_high.value, 9.0);
    assert_eq!(a_high.relative, 4.5);

    let b_high = find("mem-b-high");
    assert_eq!(b_high.rank, 2);
    assert_eq!(b_high.value, 20.0);
    assert_eq!(b_high.relative, 4.0);
}

/// A successful sample with zero throughput is dropped from the `throughput` percentile
/// calculation (unchanged pre-v2.8.0 behavior), but the exclusion must now be visible via
/// `throughput_excluded_sample_count` instead of silent.
#[test]
fn zero_throughput_successful_sample_is_counted_as_excluded() {
    let zero = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 0.0, 10_000_000);
    let positive = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);

    let percentiles = calculate_percentiles(&[&zero, &positive]);

    assert_eq!(percentiles.successful_sample_count, 2);
    assert_eq!(percentiles.throughput_excluded_sample_count, 1);
    assert!(percentiles.throughput.p50 > 0.0);
}

#[test]
fn no_throughput_exclusions_when_all_samples_are_positive() {
    let a = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    let b = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 2_000_000.0, 10_000_000);

    let percentiles = calculate_percentiles(&[&a, &b]);

    assert_eq!(percentiles.throughput_excluded_sample_count, 0);
}

/// `disk_sizes` keeps only the last-seen `installation_size` per framework (documented
/// last-writer-wins behavior). When two results for the same framework disagree, the
/// conflict must now be surfaced in `metadata.disk_size_conflicts` instead of silently
/// overwritten with no trace.
#[test]
fn conflicting_installation_size_for_same_framework_is_recorded() {
    let mut first = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    first.framework_capabilities.installation_size = Some(DiskSizeInfo {
        size_bytes: 1_000,
        package_bytes: 1_000,
        system_deps_bytes: 0,
        model_bytes: 0,
        method: "binary_size".to_string(),
        description: "first measurement".to_string(),
        system_deps_detail: HashMap::new(),
    });

    let mut second = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    second.framework_capabilities.installation_size = Some(DiskSizeInfo {
        size_bytes: 2_000,
        package_bytes: 2_000,
        system_deps_bytes: 0,
        model_bytes: 0,
        method: "binary_size".to_string(),
        description: "second measurement".to_string(),
        system_deps_detail: HashMap::new(),
    });

    let aggregated = aggregate_new_format(&[first, second]);

    assert_eq!(aggregated.disk_sizes["framework-x"].size_bytes, 2_000);
    assert_eq!(aggregated.metadata.disk_size_conflicts.len(), 1);
    assert!(aggregated.metadata.disk_size_conflicts[0].contains("framework-x"));
}

#[test]
fn agreeing_installation_size_across_results_records_no_conflict() {
    let disk_size = DiskSizeInfo {
        size_bytes: 1_000,
        package_bytes: 1_000,
        system_deps_bytes: 0,
        model_bytes: 0,
        method: "binary_size".to_string(),
        description: "measurement".to_string(),
        system_deps_detail: HashMap::new(),
    };
    let mut first = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    first.framework_capabilities.installation_size = Some(disk_size.clone());
    let mut second = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    second.framework_capabilities.installation_size = Some(disk_size);

    let aggregated = aggregate_new_format(&[first, second]);

    assert!(aggregated.metadata.disk_size_conflicts.is_empty());
}

/// Every measured field on a `BenchmarkResult` must reach its `PerFixtureRow` (the row-level
/// half of B2 losslessness; see `tests/lossless_aggregation.rs` for the provenance half and
/// the full round-trip test).
#[test]
fn per_fixture_row_carries_every_measured_field_losslessly() {
    let mut result = create_test_result("framework-x", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
    result.file_size = 4_096;
    result.metrics.baseline_memory_bytes = 111;
    result.metrics.peak_memory_delta_bytes = 222;
    result.metrics.avg_cpu_percent = 42.5;
    result.metrics.cpu_seconds = 1.25;
    result.metrics.p50_memory_bytes = 300;
    result.metrics.p95_memory_bytes = 400;
    result.metrics.p99_memory_bytes = 500;
    result.extraction_duration = Some(std::time::Duration::from_millis(77));
    result.subprocess_overhead = Some(std::time::Duration::from_millis(23));
    result.cold_start_duration = Some(std::time::Duration::from_millis(555));
    result.success = false;
    result.error_message = Some("framework exploded".to_string());
    result.quality = Some(QualityMetrics {
        f1_score_text: 0.9,
        f1_score_numeric: 0.8,
        f1_score_layout: Some(0.7),
        quality_score: 0.85,
        missing_tokens: vec![("foo".to_string(), 2)],
        extra_tokens: vec![("bar".to_string(), 1)],
        correct: false,
        reading_order_score: None,
    });
    result.pdf_metadata = Some(PdfMetadata {
        has_text_layer: true,
        detection_method: "pdftotext".to_string(),
        page_count: Some(3),
        ocr_enabled: false,
        text_quality_score: Some(0.6),
    });
    result.framework_capabilities.version = "9.9.9".to_string();
    result.framework_capabilities.ocr_support = true;
    result.framework_capabilities.async_support = true;
    result.framework_capabilities.supported_extensions = vec!["pdf".to_string()];
    result.framework_capabilities.supported_output_formats = vec![OutputFormat::Markdown];
    result.framework_capabilities.batch_capability = Some(crate::types::BatchCapability {
        entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
        timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing: true,
    });
    result.system_load = Some(SystemLoad {
        load_avg_1m: 1.0,
        load_avg_5m: 2.0,
        load_avg_15m: 3.0,
        logical_cores: 8,
        physical_cores: 4,
    });
    result.iterations = vec![crate::types::IterationResult {
        iteration: 0,
        duration: std::time::Duration::from_millis(10),
        extraction_duration: None,
        metrics: result.metrics.clone(),
    }];
    result.statistics = Some(crate::types::DurationStatistics {
        mean: std::time::Duration::from_millis(100),
        median: std::time::Duration::from_millis(95),
        std_dev_ms: 5.0,
        min: std::time::Duration::from_millis(80),
        max: std::time::Duration::from_millis(150),
        p95: std::time::Duration::from_millis(140),
        p99: std::time::Duration::from_millis(148),
        sample_count: 3,
    });

    let aggregated = aggregate_new_format(&[result.clone()]);
    let row = &aggregated.per_fixture_results[0];

    assert_eq!(row.file_size, 4_096);
    assert_eq!(row.baseline_memory_bytes, 111);
    assert_eq!(row.peak_memory_delta_bytes, 222);
    assert_eq!(row.avg_cpu_percent, 42.5);
    assert_eq!(row.cpu_seconds, 1.25);
    assert_eq!(row.p50_memory_bytes, 300);
    assert_eq!(row.p95_memory_bytes, 400);
    assert_eq!(row.p99_memory_bytes, 500);
    assert_eq!(row.extraction_duration_ms, Some(77.0));
    assert_eq!(row.subprocess_overhead_ms, Some(23.0));
    assert_eq!(row.cold_start_duration_ms, Some(555.0));
    assert_eq!(row.error_message.as_deref(), Some("framework exploded"));
    let quality = row.quality.as_ref().expect("quality present");
    assert_eq!(quality.missing_tokens, vec![("foo".to_string(), 2)]);
    assert_eq!(quality.extra_tokens, vec![("bar".to_string(), 1)]);
    let pdf_metadata = row.pdf_metadata.as_ref().expect("pdf_metadata present");
    assert_eq!(pdf_metadata.text_quality_score, Some(0.6));
    assert_eq!(pdf_metadata.page_count, Some(3));
    assert_eq!(row.framework_capabilities.version, "9.9.9");
    assert!(row.framework_capabilities.batch_capability.is_some());
    let system_load = row.system_load.expect("system_load present");
    assert_eq!(system_load.load_avg_1m, 1.0);
    assert_eq!(system_load.logical_cores, 8);
    assert_eq!(row.iterations.len(), 1);
    assert_eq!(row.statistics.as_ref().expect("statistics present").sample_count, 3);
}
