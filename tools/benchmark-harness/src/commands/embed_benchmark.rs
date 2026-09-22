//! `embed-benchmark`: embedding throughput and batch-size benchmark across all presets.

use benchmark_harness::Result;

pub(crate) fn execute() -> Result<()> {
    benchmark_harness::embed_benchmark::run_embed_benchmark();
    Ok(())
}
