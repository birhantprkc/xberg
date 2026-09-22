//! Tests for `parse_output`'s JSON envelope handling and `effective_timeout` clamping.

use crate::Error;
use std::time::Duration;

use super::super::SubprocessAdapter;

#[test]
fn test_parse_output_empty_error_no_content() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"error": "", "_extraction_time_ms": 0}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::EmptyContent(_)),
        "Expected EmptyContent, got: {:?}",
        err
    );
    assert!(err.to_string().contains("No content extracted"));
}

#[test]
fn test_parse_output_nonempty_error() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"error": "something went wrong"}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::FrameworkError(_)),
        "Expected FrameworkError, got: {:?}",
        err
    );
    assert!(err.to_string().contains("something went wrong"));
}

#[test]
fn test_parse_output_valid_content() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"content": "Hello, world!", "_extraction_time_ms": 42.5}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert_eq!(parsed["content"], "Hello, world!");
    assert_eq!(parsed["_extraction_time_ms"], 42.5);
}

#[test]
fn test_parse_output_flattens_peak_memory_bytes_from_nested_xberg_envelope() {
    // Mirrors the real shape `xberg extract --format json` emits (see
    // `crates/xberg-cli/src/output.rs::ExtractEnvelope`): the document is nested under
    // `result`, with `extraction_time_ms` and `peak_memory_bytes` as top-level siblings. ~keep
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{
            "result": {"content": "Hello, world!", "metadata": {}},
            "extraction_time_ms": 42.5,
            "peak_memory_bytes": 41631744
        }"#;
    let result = adapter.parse_output(output);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert_eq!(parsed["content"], "Hello, world!");
    assert_eq!(parsed["_extraction_time_ms"], 42.5);
    assert_eq!(parsed["_peak_memory_bytes"], 41_631_744);
}

#[test]
fn test_parse_output_omits_peak_memory_key_when_xberg_envelope_does_not_report_it() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{
            "result": {"content": "Hello, world!", "metadata": {}},
            "extraction_time_ms": 42.5
        }"#;
    let result = adapter.parse_output(output);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert!(
        parsed.get("_peak_memory_bytes").is_none(),
        "must not fabricate a _peak_memory_bytes key when the subprocess never reported one"
    );
}

#[test]
fn test_parse_output_missing_content_nonzero_time() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"_extraction_time_ms": 150.0}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::Benchmark(_)),
        "Expected Benchmark error, got: {:?}",
        err
    );
    assert!(err.to_string().contains("missing required 'content' field"));
}

#[test]
fn test_max_timeout_clamps_config_timeout() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()])
        .with_max_timeout(Duration::from_secs(120));
    let effective = adapter.effective_timeout(Duration::from_secs(900));
    assert_eq!(effective, Duration::from_secs(120));
}

#[test]
fn test_max_timeout_passes_lower_config() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()])
        .with_max_timeout(Duration::from_secs(120));
    let effective = adapter.effective_timeout(Duration::from_secs(60));
    assert_eq!(effective, Duration::from_secs(60));
}

#[test]
fn test_max_timeout_none_uses_config() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let effective = adapter.effective_timeout(Duration::from_secs(900));
    assert_eq!(effective, Duration::from_secs(900));
}

#[test]
fn test_with_max_timeout_builder() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()])
        .with_max_timeout(Duration::from_secs(300));
    assert_eq!(adapter.max_timeout, Some(Duration::from_secs(300)));
}

#[test]
fn test_parse_output_empty_string_content() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"content": "", "_extraction_time_ms": 5.0}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::EmptyContent(_)),
        "Expected EmptyContent, got: {:?}",
        err
    );
    assert!(err.to_string().contains("empty content"));
}

#[test]
fn test_parse_output_whitespace_only_content() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = "{\"content\": \"  \\n  \", \"_extraction_time_ms\": 10.0}";
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::EmptyContent(_)),
        "Expected EmptyContent, got: {:?}",
        err
    );
}

#[test]
fn test_parse_output_python_side_timeout() {
    let adapter = SubprocessAdapter::new("test", "echo", vec![], vec![], vec!["pdf".to_string()]);
    let output = r#"{"error": "extraction timed out after 150s", "_extraction_time_ms": 150000.0}"#;
    let result = adapter.parse_output(output);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "Expected Timeout, got: {:?}", err);
    assert!(err.to_string().contains("timed out"));
}
