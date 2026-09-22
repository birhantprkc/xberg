//! End-to-end tests that xberg's `--max-concurrent`/`--max-threads` budgets are forwarded
//! correctly in both native-batch and single-file mode, and withheld from non-xberg adapters.

use crate::adapter::FrameworkAdapter;
use crate::types::OutputFormat;
use std::time::Duration;

use super::super::SubprocessAdapter;
use super::support::test_batch_capability;

#[cfg(unix)]
#[tokio::test]
async fn xberg_batch_passes_distinct_concurrency_and_thread_limits() {
    let script = r#"
            concurrent=""
            threads=""
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --max-concurrent) concurrent="$2"; shift 2 ;;
                    --max-threads) threads="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            [ "$concurrent" = "3" ] && [ "$threads" = "7" ] || exit 64
            sleep 0.02
            printf '{"results":[{"content":"ok"}],"total_ms":0,"per_file_ms":[1]}'
        "#;
    let adapter = SubprocessAdapter::with_batch_capability(
        "xberg-test",
        "sh",
        vec!["-c".to_string(), script.to_string(), "worker-budget-probe".to_string()],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    )
    .with_batch_workers(3)
    .with_xberg_max_threads(7);
    let file = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[file.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].success);
}

#[cfg(unix)]
#[tokio::test]
async fn xberg_batch_defaults_thread_limit_to_worker_limit() {
    let script = r#"
            concurrent=""
            threads=""
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --max-concurrent) concurrent="$2"; shift 2 ;;
                    --max-threads) threads="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            [ "$concurrent" = "7" ] && [ "$threads" = "7" ] || exit 64
            sleep 0.02
            printf '{"results":[{"content":"ok"}],"total_ms":0,"per_file_ms":[1]}'
        "#;
    let adapter = SubprocessAdapter::with_batch_capability(
        "xberg-test",
        "sh",
        vec!["-c".to_string(), script.to_string(), "legacy-budget-probe".to_string()],
        vec![],
        vec!["pdf".to_string()],
        test_batch_capability(true),
    )
    .with_batch_workers(7);
    let file = tempfile::NamedTempFile::new().unwrap();

    let results = adapter
        .extract_batch(
            &[file.path()],
            Duration::from_secs(1),
            &[false],
            &[None],
            OutputFormat::Markdown,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].success);
}

#[cfg(unix)]
#[tokio::test]
async fn xberg_single_passes_explicit_thread_limit() {
    let script = r#"
            threads=""
            while [ "$#" -gt 1 ]; do
                case "$1" in
                    --max-threads) threads="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            [ "$threads" = "7" ] || exit 64
            sleep 0.02
            printf '{"content":"ok"}'
        "#;
    let adapter = SubprocessAdapter::new(
        "xberg-test",
        "sh",
        vec!["-c".to_string(), script.to_string(), "single-budget-probe".to_string()],
        vec![],
        vec!["pdf".to_string()],
    )
    .with_xberg_max_threads(7);
    let file = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract(file.path(), Duration::from_secs(1), false, None, OutputFormat::Markdown)
        .await
        .unwrap();

    assert!(result.success);
}

#[cfg(unix)]
#[tokio::test]
async fn xberg_single_without_explicit_budget_preserves_cli_auto_threads() {
    let script = r#"
            while [ "$#" -gt 1 ]; do
                [ "$1" != "--max-threads" ] || exit 64
                shift
            done
            printf '{"content":"ok"}'
        "#;
    let adapter = SubprocessAdapter::new(
        "xberg-test",
        "sh",
        vec![
            "-c".to_string(),
            script.to_string(),
            "single-auto-budget-probe".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
    )
    .with_batch_workers(7);
    let file = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract(file.path(), Duration::from_secs(1), false, None, OutputFormat::Markdown)
        .await
        .unwrap();

    assert!(result.success);
}

#[cfg(unix)]
#[tokio::test]
async fn non_xberg_single_ignores_xberg_thread_limit() {
    let script = r#"
            while [ "$#" -gt 1 ]; do
                [ "$1" != "--max-threads" ] || exit 64
                shift
            done
            printf '{"content":"ok"}'
        "#;
    let adapter = SubprocessAdapter::new(
        "docling",
        "sh",
        vec![
            "-c".to_string(),
            script.to_string(),
            "non-xberg-budget-probe".to_string(),
        ],
        vec![],
        vec!["pdf".to_string()],
    )
    .with_xberg_max_threads(7);
    let file = tempfile::NamedTempFile::new().unwrap();

    let result = adapter
        .extract(file.path(), Duration::from_secs(1), false, None, OutputFormat::Markdown)
        .await
        .unwrap();

    assert!(result.success);
}
