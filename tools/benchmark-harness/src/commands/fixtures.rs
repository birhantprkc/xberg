//! `list-fixtures` and `validate`: cheap fixture-loading commands that don't run any benchmark.

use benchmark_harness::{FixtureManager, Result};
use std::path::Path;

pub(crate) fn list_fixtures(fixtures: &Path) -> Result<()> {
    let mut manager = FixtureManager::new();

    if fixtures.is_dir() {
        manager.load_fixtures_from_dir(fixtures)?;
    } else {
        manager.load_fixture(fixtures)?;
    }

    println!("Loaded {} fixture(s)", manager.len());
    for (path, fixture) in manager.fixtures() {
        println!(
            "  {} - {} ({} bytes)",
            path.display(),
            fixture.document.display(),
            fixture.file_size
        );
    }

    Ok(())
}

pub(crate) fn validate(fixtures: &Path) -> Result<()> {
    let mut manager = FixtureManager::new();

    if fixtures.is_dir() {
        manager.load_fixtures_from_dir(fixtures)?;
    } else {
        manager.load_fixture(fixtures)?;
    }

    println!("✓ All {} fixture(s) are valid", manager.len());
    Ok(())
}
