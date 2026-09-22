//! Tests for cohort-manifest path resolution and the cohort/regular-fixture load lifecycle.

use super::support::write_ordered_cohort;
use crate::config::BenchmarkConfig;
use crate::registry::AdapterRegistry;
use crate::runner::BenchmarkRunner;
use crate::runner::helpers::resolve_cohort_manifest_path;
use std::path::Path;

#[test]
fn cohort_manifest_path_preserves_absolute_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cohort.json");

    assert_eq!(resolve_cohort_manifest_path(Path::new("fixtures"), &path), path);
}

#[test]
fn cohort_manifest_path_preserves_existing_cwd_relative_path() {
    let current_dir = std::env::current_dir().unwrap();
    let cwd_temp = tempfile::tempdir_in(&current_dir).unwrap();
    let fixture_root = tempfile::tempdir().unwrap();
    let manifest = cwd_temp.path().join("cohort.json");
    std::fs::write(&manifest, "cwd").unwrap();
    let relative = manifest.strip_prefix(&current_dir).unwrap();
    let fixture_relative = fixture_root.path().join(relative);
    std::fs::create_dir_all(fixture_relative.parent().unwrap()).unwrap();
    std::fs::write(fixture_relative, "fixture root").unwrap();

    assert_eq!(resolve_cohort_manifest_path(fixture_root.path(), relative), relative);
}

#[test]
fn provenance_uses_loaded_fixture_relative_manifest_after_cwd_collision() {
    let current_dir = std::env::current_dir().unwrap();
    let cwd_temp = tempfile::tempdir_in(&current_dir).unwrap();
    let fixture_root = tempfile::tempdir().unwrap();
    let original_manifest = write_ordered_cohort(fixture_root.path(), &["pdf", "pdf", "pdf", "pdf"]);
    let relative_manifest = cwd_temp.path().strip_prefix(&current_dir).unwrap().join("ordered.json");
    let nested_manifest = fixture_root.path().join(&relative_manifest);
    std::fs::create_dir_all(nested_manifest.parent().unwrap()).unwrap();
    std::fs::rename(original_manifest, &nested_manifest).unwrap();
    assert!(!relative_manifest.exists());

    let mut runner = BenchmarkRunner::new(BenchmarkConfig::default(), AdapterRegistry::new());
    let manifest = runner.load_cohort(fixture_root.path(), &relative_manifest).unwrap();
    std::fs::write(&relative_manifest, "cwd collision created after cohort load").unwrap();
    let provenance = runner
        .capture_provenance(
            &[],
            fixture_root.path(),
            Some(&manifest),
            Some(&relative_manifest),
            Some(manifest.batch_size),
            &[],
        )
        .unwrap();
    let fixture_digest = blake3::hash(&std::fs::read(&nested_manifest).unwrap())
        .to_hex()
        .to_string();
    let cwd_digest = blake3::hash(&std::fs::read(&relative_manifest).unwrap())
        .to_hex()
        .to_string();

    assert_eq!(manifest.name, "ordered");
    assert_eq!(runner.fixture_count(), 4);
    assert_eq!(
        provenance.corpus.cohort_manifest_blake3.as_deref(),
        Some(fixture_digest.as_str())
    );
    assert_ne!(fixture_digest, cwd_digest);
}

#[test]
fn failed_cohort_load_clears_path_and_names_resolved_path() {
    let current_dir = std::env::current_dir().unwrap();
    let cwd_temp = tempfile::tempdir_in(&current_dir).unwrap();
    let fixture_root = tempfile::tempdir().unwrap();
    let manifest = write_ordered_cohort(fixture_root.path(), &["pdf", "pdf", "pdf", "pdf"]);
    let missing = cwd_temp.path().strip_prefix(&current_dir).unwrap().join("missing.json");
    assert!(!missing.exists());
    let resolved_missing = fixture_root.path().join(&missing);
    let mut runner = BenchmarkRunner::new(BenchmarkConfig::default(), AdapterRegistry::new());
    runner.load_cohort(fixture_root.path(), &manifest).unwrap();
    assert!(runner.cohort_manifest_path.is_some());

    let error = runner.load_cohort(fixture_root.path(), &missing).unwrap_err();

    assert!(runner.cohort_manifest_path.is_none());
    assert!(error.to_string().contains(&resolved_missing.display().to_string()));

    let invalid_fixture_manifest = fixture_root.path().join("missing-fixture.json");
    std::fs::write(
        &invalid_fixture_manifest,
        serde_json::json!({
            "schema_version": 1,
            "name": "missing-fixture",
            "batch_size": 1,
            "fixtures": ["absent.json"]
        })
        .to_string(),
    )
    .unwrap();
    let error = runner
        .load_cohort(fixture_root.path(), &invalid_fixture_manifest)
        .unwrap_err();

    assert!(runner.cohort_manifest_path.is_none());
    assert!(
        error
            .to_string()
            .contains(&invalid_fixture_manifest.display().to_string())
    );
}

#[test]
fn loading_regular_fixtures_clears_cohort_path() {
    let fixture_root = tempfile::tempdir().unwrap();
    let manifest = write_ordered_cohort(fixture_root.path(), &["pdf", "pdf", "pdf", "pdf"]);
    let mut runner = BenchmarkRunner::new(BenchmarkConfig::default(), AdapterRegistry::new());
    runner.load_cohort(fixture_root.path(), &manifest).unwrap();
    assert!(runner.cohort_manifest_path.is_some());

    runner.load_fixtures(&fixture_root.path().join("d.json")).unwrap();

    assert!(runner.cohort_manifest_path.is_none());
}
