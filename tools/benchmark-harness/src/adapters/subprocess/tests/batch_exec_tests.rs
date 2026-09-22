//! End-to-end tests for native-batch and single-file extraction: cardinality validation,
//! throughput/timing bookkeeping, per-item failure attribution, and timing-capability
//! cross-checks.

use crate::adapter::FrameworkAdapter;
use crate::types::{BatchCapability, BatchEntryPoint, ErrorKind, OcrStatus, OutputFormat};
use std::time::Duration;

use super::super::SubprocessAdapter;
use super::super::support::bytes_per_second;
use super::support::test_batch_capability;

#[test]
fn throughput_uses_total_bytes_over_makespan() {
    assert_eq!(bytes_per_second(4_000, Duration::from_secs(2)), 2_000.0);
    assert_eq!(bytes_per_second(4_000, Duration::ZERO), 0.0);
}

#[tokio::test]
async fn batch_rejects_force_ocr_cardinality_mismatch() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "echo",
        vec![],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    );
    let input = tempfile::NamedTempFile::new().unwrap();
    let error = adapter
        .extract_batch(
            &[input.path()],
            Duration::from_secs(1),
            &[],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("force_ocr cardinality mismatch"));
}

#[tokio::test]
async fn batch_rejects_non_native_adapter_without_spawning_single_file_commands() {
    let adapter = SubprocessAdapter::new(
        "single-only",
        "command-that-must-not-run",
        vec![],
        vec![],
        vec!["pdf".to_string()],
    );
    let input = tempfile::NamedTempFile::new().unwrap();

    let error = adapter
        .extract_batch(
            &[input.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("verified native batch API"));
}

#[cfg(unix)]
#[tokio::test]
async fn single_throughput_uses_wall_duration() {
    let adapter = SubprocessAdapter::new(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.05; printf '{\"content\":\"ok\",\"_extraction_time_ms\":1}'".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
    );
    let mut input = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut input, &[0; 100]).unwrap();

    let result = adapter
        .extract(
            input.path(),
            Duration::from_secs(1),
            false,
            None,
            OutputFormat::Markdown,
        )
        .await
        .unwrap();
    let expected = bytes_per_second(result.file_size, result.duration);
    assert!((result.metrics.throughput_bytes_per_sec - expected).abs() < f64::EPSILON);
    assert_eq!(result.extraction_duration, Some(Duration::from_millis(1)));
    assert_eq!(
        result.framework_capabilities.supported_extensions,
        vec!["pdf".to_string()]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn batch_capable_single_zero_byte_result_is_marked_as_a_process_sample() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "docling",
        "sh",
        vec!["-c".to_string(), "printf '{\"content\":\"ok\"}'".to_string()],
        vec![],
        vec!["pdf".to_string()],
        BatchCapability {
            entry_point: BatchEntryPoint::DoclingJobkit,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        },
    );
    let input = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract(
            input.path(),
            Duration::from_secs(1),
            false,
            None,
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(result.metrics.throughput_bytes_per_sec, 0.0);
    assert_eq!(result.framework_capabilities.batch_performance_sample, Some(true));
    assert!(result.is_performance_sample());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn measured_command_drains_output_larger_than_pipe_capacity() {
    const CONTENT_BYTES: usize = 300_000;

    let adapter = SubprocessAdapter::new(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "printf '{\"content\":\"'; yes x | head -c 600000 | tr -d '\\n'; printf '\"}'".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
    );
    let input = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract(
            input.path(),
            Duration::from_secs(5),
            false,
            None,
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.extracted_text.as_deref().map(str::len), Some(CONTENT_BYTES));
}

#[cfg(unix)]
#[tokio::test]
async fn batch_rejects_output_cardinality_mismatch() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.02; printf '[{\"content\":\"only one\"}]'".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(false),
    );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();
    let error = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("batch output cardinality mismatch"));
}

#[cfg(unix)]
#[tokio::test]
async fn xberg_batch_envelope_uses_reported_timings_ocr_and_honest_throughput() {
    let adapter = SubprocessAdapter::with_batch_capability(
            "test",
            "sh",
            vec![
                "-c".to_string(),
                "sleep 0.02; printf '{\"results\":[{\"content\":\"one\",\"metadata\":{\"ocr_used\":false}},{\"content\":\"two\",\"metadata\":{\"ocr_used\":true}}],\"total_ms\":2000,\"per_file_ms\":[100,200]}'"
                    .to_string(),
            ],
            vec![],
            vec!["pdf".to_string()],
            test_batch_capability(true),
        );
    let mut first = tempfile::NamedTempFile::new().unwrap();
    let mut second = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut first, &[0; 100]).unwrap();
    std::io::Write::write_all(&mut second, &[0; 300]).unwrap();
    let results = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.duration == Duration::from_secs(2)));
    assert_eq!(results[0].extraction_duration, Some(Duration::from_millis(100)));
    assert_eq!(results[1].extraction_duration, Some(Duration::from_millis(200)));
    assert!(
        results
            .iter()
            .all(|result| result.subprocess_overhead == Some(Duration::ZERO))
    );
    assert_eq!(results[0].ocr_status, OcrStatus::NotUsed);
    assert_eq!(results[1].ocr_status, OcrStatus::Used);
    assert_eq!(results[0].extracted_text.as_deref(), Some("one"));
    assert_eq!(results[1].extracted_text.as_deref(), Some("two"));
    assert_eq!(results[0].metrics.throughput_bytes_per_sec, 200.0);
    assert_eq!(results[1].metrics.throughput_bytes_per_sec, 200.0);
    assert!(
        results
            .iter()
            .all(|result| result.framework_capabilities.supported_extensions == ["pdf".to_string()])
    );
    assert_eq!(results[0].framework_capabilities.batch_performance_sample, Some(true));
    assert_eq!(results[1].framework_capabilities.batch_performance_sample, Some(false));
    assert_eq!(
        results[0].framework_capabilities.batch_sample_id,
        results[1].framework_capabilities.batch_sample_id
    );
    assert!(results[0].framework_capabilities.batch_sample_id.is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn batch_overhead_uses_process_wall_minus_reported_total() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.05; printf '{\"results\":[{\"content\":\"ok\"}],\"total_ms\":1,\"per_file_ms\":[1]}'".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    );
    let file = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract_batch(
            &[file.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap()
        .remove(0);

    assert_eq!(
        result.subprocess_overhead,
        Some(result.duration.saturating_sub(Duration::from_millis(1)))
    );
    assert!(result.subprocess_overhead > Some(Duration::ZERO));
}

#[cfg(unix)]
#[tokio::test]
async fn batch_array_does_not_infer_overhead_from_per_item_timing() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "printf '[{\"content\":\"ok\",\"_extraction_time_ms\":1}]'".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    );
    let file = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract_batch(
            &[file.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap()
        .remove(0);

    assert_eq!(result.extraction_duration, Some(Duration::from_millis(1)));
    assert_eq!(result.subprocess_overhead, None);
}

#[cfg(unix)]
#[tokio::test]
async fn batch_envelope_preserves_unavailable_per_item_timings() {
    let capability = BatchCapability {
        entry_point: BatchEntryPoint::DoclingJobkit,
        timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing: false,
    };
    let adapter = SubprocessAdapter::with_batch_capability(
            "docling",
            "sh",
            vec![
                "-c".to_string(),
                "sleep 0.02; printf '{\"results\":[{\"content\":\"one\"},{\"content\":\"two\"}],\"total_ms\":10,\"per_file_ms\":[null,null]}'"
                    .to_string(),
            ],
            vec![],
            vec!["pdf".to_string()],
            capability,
        );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert!(results.iter().all(|result| result.extraction_duration.is_none()));
    assert!(
        results
            .iter()
            .all(|result| { result.framework_capabilities.batch_capability == Some(capability) })
    );
}

#[cfg(unix)]
#[tokio::test]
async fn batch_rejects_numeric_timing_when_capability_declares_unavailable() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "docling",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.02; printf '{\"results\":[{\"content\":\"one\"}],\"total_ms\":10,\"per_file_ms\":[1]}'"
                .to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        BatchCapability {
            entry_point: BatchEntryPoint::DoclingJobkit,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        },
    );
    let input = tempfile::NamedTempFile::new().unwrap();

    let error = adapter
        .extract_batch(
            &[input.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("unavailable but returned numeric"));
}

#[cfg(unix)]
#[tokio::test]
async fn batch_requires_numeric_timing_when_capability_declares_per_item() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "xberg-test",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.02; printf '{\"results\":[{\"content\":\"one\"}],\"total_ms\":10,\"per_file_ms\":[null]}'"
                .to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    );
    let input = tempfile::NamedTempFile::new().unwrap();

    let error = adapter
        .extract_batch(
            &[input.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("declares per-item batch timing"));
}

#[cfg(unix)]
#[tokio::test]
async fn batch_process_failure_preserves_measured_resource_stats() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "sh",
        vec![
            "-c".to_string(),
            "sleep 0.02; printf 'batch failed' >&2; exit 9".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(false),
    );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| !result.success));
    assert!(results.iter().all(|result| {
        result
            .error_message
            .as_deref()
            .is_some_and(|error| error.contains("batch failed"))
    }));
    assert!(results.iter().all(|result| result.metrics.baseline_memory_bytes > 0));
    assert!(
        results
            .iter()
            .all(|result| { result.metrics.peak_memory_bytes >= result.metrics.baseline_memory_bytes })
    );
}

#[cfg(unix)]
#[tokio::test]
async fn partial_batch_item_failure_returns_accountable_result_rows() {
    let adapter = SubprocessAdapter::with_batch_capability(
            "test",
            "sh",
            vec![
                "-c".to_string(),
                "printf '{\"results\":[{\"content\":\"ok\"},{\"error\":\"failed item\"}],\"total_ms\":10,\"per_file_ms\":[5,null]}'; exit 1"
                    .to_string(),
            ],
            vec![],
            vec!["pdf".to_string()],
            test_batch_capability(true),
        );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results[0].success);
    assert_eq!(results[0].error_kind, ErrorKind::None);
    assert!(!results[1].success);
    assert_eq!(results[1].error_kind, ErrorKind::FrameworkError);
    assert_eq!(results[1].error_message.as_deref(), Some("failed item"));
    assert_eq!(results[1].extracted_text, None);
    assert_eq!(results[0].extraction_duration, Some(Duration::from_millis(5)));
    assert_eq!(results[1].extraction_duration, None);
}

#[cfg(unix)]
#[tokio::test]
async fn mixed_batch_process_failure_applies_global_error_only_to_implicit_failure() {
    let adapter = SubprocessAdapter::with_batch_capability(
            "test",
            "sh",
            vec![
                "-c".to_string(),
                "printf '{\"results\":[{\"content\":\"ok\"},{\"error\":\"\"},{\"error\":\"explicit item error\"}],\"total_ms\":10,\"per_file_ms\":[5,null,null]}'; printf 'global process error' >&2; exit 1"
                    .to_string(),
            ],
            vec![],
            vec!["pdf".to_string()],
            test_batch_capability(true),
        );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();
    let third = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[first.path(), second.path(), third.path()],
            Duration::from_secs(1),
            &[false, false, false],
            &[None, None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
    assert!(results[0].success);
    assert_eq!(results[0].error_message, None);
    assert_eq!(results[0].error_kind, ErrorKind::None);
    assert!(!results[1].success);
    assert_eq!(
        results[1].error_message.as_deref(),
        Some(
            "Benchmark error: Batch subprocess failed with exit status: 1\nstderr: global process error\nstdout: {\"results\":[{\"content\":\"ok\"},{\"error\":\"\"},{\"error\":\"explicit item error\"}],\"total_ms\":10,\"per_file_ms\":[5,null,null]}"
        )
    );
    assert_eq!(results[1].error_kind, ErrorKind::HarnessError);
    assert!(!results[2].success);
    assert_eq!(results[2].error_message.as_deref(), Some("explicit item error"));
    assert_eq!(results[2].error_kind, ErrorKind::FrameworkError);
}

#[cfg(unix)]
#[tokio::test]
async fn batch_rejects_envelope_timing_cardinality_mismatch() {
    let adapter = SubprocessAdapter::with_batch_capability(
            "test",
            "sh",
            vec![
                "-c".to_string(),
                "sleep 0.02; printf '{\"results\":[{\"content\":\"one\"},{\"content\":\"two\"}],\"total_ms\":10,\"per_file_ms\":[1]}'"
                    .to_string(),
            ],
            vec![],
            vec!["pdf".to_string()],
            test_batch_capability(true),
        );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();

    let error = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, false],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("batch timing cardinality mismatch"));
}

#[cfg(unix)]
#[tokio::test]
async fn mixed_ocr_batch_is_rejected_as_non_comparable() {
    let adapter = SubprocessAdapter::with_batch_capability(
        "test",
        "sh",
        vec!["-c".to_string(), "exit 99".to_string()],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(false),
    );
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();

    let error = adapter
        .extract_batch(
            &[first.path(), second.path()],
            Duration::from_secs(1),
            &[false, true],
            &[None, None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("homogeneous OCR cohort"));
}
