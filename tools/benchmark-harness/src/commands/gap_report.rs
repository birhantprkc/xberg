//! `gap-report`: builds a per-document gap report from a `run` result set.

use benchmark_harness::Result;
use benchmark_harness::gap_report::GapConfig;
use std::path::PathBuf;

pub(crate) fn execute(
    results: PathBuf,
    output: Option<PathBuf>,
    baseline: String,
    layout: String,
    competitors: Vec<String>,
) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| results.clone());
    let config = GapConfig {
        baseline,
        layout,
        competitors,
    };
    benchmark_harness::gap_report::generate(&results, &output_dir, &config)?;
    Ok(())
}
