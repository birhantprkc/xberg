//! Tests for the docling/mineru `version()` probe: distribution precedence and fallback.

use crate::adapter::FrameworkAdapter;
use crate::types::{BatchCapability, BatchEntryPoint};
use std::path::PathBuf;

use super::super::SubprocessAdapter;

#[cfg(unix)]
fn fake_docling_site(
    docling_distribution: Option<&str>,
    slim_distribution: Option<&str>,
    module_version: Option<&str>,
) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let site = tempfile::tempdir().unwrap();
    let package_dir = site.path().join("docling");
    std::fs::create_dir(&package_dir).unwrap();
    let module = module_version
        .map(|version| format!("__version__ = {version:?}\n"))
        .unwrap_or_default();
    std::fs::write(package_dir.join("__init__.py"), module).unwrap();

    for (name, normalized_name, version) in [
        ("docling", "docling", docling_distribution),
        ("docling-slim", "docling_slim", slim_distribution),
    ] {
        let Some(version) = version else {
            continue;
        };
        let metadata_dir = site.path().join(format!("{normalized_name}-{version}.dist-info"));
        std::fs::create_dir(&metadata_dir).unwrap();
        std::fs::write(
            metadata_dir.join("METADATA"),
            format!("Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"),
        )
        .unwrap();
    }

    let python = which::which("python3").unwrap();
    let wrapper = site.path().join("isolated-python");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\nexec \"{}\" -S \"$@\"\n", python.display()),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&wrapper, permissions).unwrap();
    (site, wrapper)
}

#[cfg(unix)]
fn docling_version_from_site(
    docling_distribution: Option<&str>,
    slim_distribution: Option<&str>,
    module_version: Option<&str>,
) -> String {
    let (site, python) = fake_docling_site(docling_distribution, slim_distribution, module_version);
    let adapter = SubprocessAdapter::with_batch_capability(
        "docling",
        python,
        vec![],
        vec![("PYTHONPATH".to_string(), site.path().to_string_lossy().into_owned())],
        vec!["pdf".to_string()],
        BatchCapability {
            entry_point: BatchEntryPoint::DoclingJobkit,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        },
    );
    adapter.version()
}

#[cfg(unix)]
#[test]
fn docling_version_prefers_full_distribution() {
    assert_eq!(
        docling_version_from_site(Some("1.2.3"), Some("2.3.4"), Some("3.4.5")),
        "1.2.3"
    );
}

#[cfg(unix)]
#[test]
fn docling_version_falls_back_to_slim_distribution() {
    assert_eq!(docling_version_from_site(None, Some("2.3.4"), Some("3.4.5")), "2.3.4");
}

#[cfg(unix)]
#[test]
fn docling_version_falls_back_to_module_version() {
    assert_eq!(docling_version_from_site(None, None, Some("3.4.5")), "3.4.5");
}

#[cfg(unix)]
#[test]
fn docling_version_is_unknown_when_all_sources_are_empty() {
    assert_eq!(docling_version_from_site(None, None, Some("")), "unknown");
}
