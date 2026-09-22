//! Tests for installation-size resolution and basic runner construction.

use crate::config::BenchmarkConfig;
use crate::registry::AdapterRegistry;
use crate::runner::BenchmarkRunner;
use crate::runner::helpers::resolve_installation_size;
use crate::types::DiskSizeInfo;
use std::collections::HashMap;

fn disk_size(size: u64, package: u64, model: u64) -> DiskSizeInfo {
    DiskSizeInfo {
        size_bytes: size,
        package_bytes: package,
        system_deps_bytes: 0,
        model_bytes: model,
        method: "binary_size".to_string(),
        description: "test".to_string(),
        system_deps_detail: HashMap::new(),
    }
}

fn xberg_size_map() -> HashMap<String, DiskSizeInfo> {
    let mut sizes = HashMap::new();
    // shipped binary+dylibs = 40 MB, on-demand model cache = 525 MB. ~keep
    sizes.insert("xberg-rust".to_string(), disk_size(565, 40, 525));
    sizes.insert("liteparse".to_string(), disk_size(35, 35, 0));
    sizes
}

#[test]
fn should_resolve_competitor_size_by_direct_name() {
    let sizes = xberg_size_map();
    let info = resolve_installation_size("liteparse", &sizes).unwrap();
    assert_eq!(info.size_bytes, 35);
}

#[test]
fn should_strip_batch_suffix_for_competitor_lookup() {
    let sizes = xberg_size_map();
    let info = resolve_installation_size("liteparse-batch", &sizes).unwrap();
    assert_eq!(info.size_bytes, 35);
}

#[test]
fn should_report_shipped_only_for_xberg_heuristic_rows() {
    let sizes = xberg_size_map();
    // Baseline/plaintext heuristic pipelines ship without ML models. ~keep
    for name in [
        "xberg-markdown-baseline",
        "xberg-plaintext-baseline",
        "xberg-markdown-baseline-batch",
    ] {
        let info = resolve_installation_size(name, &sizes).unwrap();
        assert_eq!(info.size_bytes, 40, "{name} should report shipped-only size");
        assert_eq!(info.model_bytes, 0, "{name} should not count model cache");
    }
}

#[test]
fn should_include_models_for_xberg_ml_rows() {
    let sizes = xberg_size_map();
    for name in [
        "xberg-markdown-layout",
        "xberg-markdown-layout-batch",
        "xberg-markdown-paddle-ocr",
    ] {
        let info = resolve_installation_size(name, &sizes).unwrap();
        assert_eq!(info.size_bytes, 565, "{name} should include model cache");
        assert_eq!(info.model_bytes, 525);
    }
}

#[test]
fn should_return_none_when_xberg_rust_unmeasured() {
    let sizes = HashMap::new();
    assert!(resolve_installation_size("xberg-markdown-baseline", &sizes).is_none());
    assert!(resolve_installation_size("unknown-framework", &sizes).is_none());
}

#[tokio::test]
async fn test_benchmark_runner_creation() {
    let config = BenchmarkConfig::default();
    let registry = AdapterRegistry::new();
    let runner = BenchmarkRunner::new(config, registry);

    assert_eq!(runner.fixture_count(), 0);
}
