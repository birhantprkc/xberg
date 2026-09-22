//! Tests for `stage_liteparse_input`'s platform-specific staging strategy.

use super::super::SubprocessAdapter;

#[test]
fn liteparse_staging_produces_a_readable_input() {
    let source = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(source.path(), b"staged input").unwrap();
    let destination_dir = tempfile::tempdir().unwrap();
    let destination = destination_dir.path().join("input.pdf");

    SubprocessAdapter::stage_liteparse_input(source.path(), &destination).unwrap();

    assert_eq!(std::fs::read(destination).unwrap(), b"staged input");
}

#[cfg(unix)]
#[test]
fn liteparse_staging_uses_symlink_on_unix() {
    let source = tempfile::NamedTempFile::new().unwrap();
    let destination_dir = tempfile::tempdir().unwrap();
    let destination = destination_dir.path().join("input.pdf");

    SubprocessAdapter::stage_liteparse_input(source.path(), &destination).unwrap();

    assert!(std::fs::symlink_metadata(destination).unwrap().file_type().is_symlink());
}

#[cfg(windows)]
#[test]
fn liteparse_staging_uses_windows_safe_link_or_copy() {
    let source = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(source.path(), b"windows input").unwrap();
    let destination_dir = tempfile::tempdir().unwrap();
    let destination = destination_dir.path().join("input.pdf");

    SubprocessAdapter::stage_liteparse_input(source.path(), &destination).unwrap();

    assert_eq!(std::fs::read(destination).unwrap(), b"windows input");
}
