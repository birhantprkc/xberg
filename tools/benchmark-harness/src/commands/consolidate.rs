//! `consolidate`: merges per-framework `results.json`/`provenance.json` directories into one
//! ranked `aggregated.json`.

use benchmark_harness::{Result, load_run_provenance, load_run_results};
use std::collections::HashSet;
use std::path::PathBuf;

fn print_framework_summaries(aggregated: &benchmark_harness::NewConsolidatedResults) {
    for agg in aggregated.by_framework_mode.values() {
        if let Some(cs) = &agg.cold_start {
            tracing::info!(
                framework = %agg.framework,
                mode = %agg.mode,
                file_type_count = agg.by_file_type.len(),
                cold_start_p50_ms = cs.p50_ms,
                "framework summary"
            );
        } else {
            tracing::info!(
                framework = %agg.framework,
                mode = %agg.mode,
                file_type_count = agg.by_file_type.len(),
                "framework summary"
            );
        }
    }
}

pub(crate) fn execute(inputs: Vec<PathBuf>, output: PathBuf) -> Result<()> {
    if inputs.is_empty() {
        return Err(benchmark_harness::Error::Benchmark(
            "No input directories specified".to_string(),
        ));
    }

    println!("Loading benchmark results from {} directory(ies)...", inputs.len());

    let mut all_results = Vec::new();
    let mut all_provenance = Vec::new();
    for input in &inputs {
        if !input.is_dir() {
            return Err(benchmark_harness::Error::Benchmark(format!(
                "Input path is not a directory: {}",
                input.display()
            )));
        }
        println!("  Loading from: {}", input.display());
        let run_results = load_run_results(input)?;
        println!("    Loaded {} results", run_results.len());
        all_results.extend(run_results);
        all_provenance.extend(load_run_provenance(input)?);
    }

    println!("\nAggregating {} results...", all_results.len());
    let mut aggregated = benchmark_harness::aggregate_new_format(&all_results);
    aggregated.run_provenance = all_provenance;
    benchmark_harness::aggregate::apply_pinned_cohort_comparison(&mut aggregated)?;
    println!(
        "  Aggregated {} frameworks across {} file types",
        aggregated.by_framework_mode.len(),
        aggregated
            .by_framework_mode
            .values()
            .flat_map(|fm| fm.by_file_type.keys())
            .collect::<HashSet<_>>()
            .len()
    );

    print_framework_summaries(&aggregated);

    std::fs::create_dir_all(&output).map_err(benchmark_harness::Error::Io)?;

    let output_file = output.join("aggregated.json");
    let json = serde_json::to_string_pretty(&aggregated)
        .map_err(|e| benchmark_harness::Error::Benchmark(format!("Failed to serialize results: {}", e)))?;
    std::fs::write(&output_file, json).map_err(benchmark_harness::Error::Io)?;
    println!("\nResults written to: {}", output_file.display());

    Ok(())
}
