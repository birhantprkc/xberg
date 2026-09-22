//! `split-benchmark`: multi-document PDF split-boundary benchmark.

use benchmark_harness::Result;
use benchmark_harness::split_benchmark::{
    SplitBenchmarkConfig, print_split_table, print_sweep_table, run_split_benchmark, run_threshold_sweep,
};
use std::path::PathBuf;

pub(crate) async fn execute(
    fixtures: PathBuf,
    sweep: bool,
    guardrails_out: Option<PathBuf>,
    profile_out: Option<PathBuf>,
) -> Result<()> {
    let config = SplitBenchmarkConfig {
        fixtures_dir: fixtures,
        sweep,
        guardrails_out,
    };

    let results = if let Some(ref svg_path) = profile_out {
        use benchmark_harness::profiling::ProfileGuard;
        if let Some(parent) = svg_path.parent() {
            std::fs::create_dir_all(parent).map_err(benchmark_harness::Error::Io)?;
        }
        let guard = ProfileGuard::new(1000)?;
        let results = run_split_benchmark(&config).await?;
        let profiling_result = guard.finish()?;
        profiling_result.generate_flamegraph(svg_path)?;
        tracing::info!(output = %svg_path.display(), "flamegraph written");
        results
    } else {
        run_split_benchmark(&config).await?
    };

    print_split_table(&results);

    if sweep {
        let cells = run_threshold_sweep(&config).await?;
        print_sweep_table(&cells);
    }

    Ok(())
}
