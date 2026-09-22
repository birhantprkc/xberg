//! One module per CLI subcommand, dispatched from `main.rs`'s `match cli.command`.
//!
//! Splitting the dispatch this way keeps each subcommand's implementation independently
//! reviewable and testable, and keeps `main.rs` itself to the thin startup + dispatch shape.

pub(crate) mod cohort_contract;
pub(crate) mod compare;
pub(crate) mod consolidate;
pub(crate) mod embed_benchmark;
pub(crate) mod fixtures;
pub(crate) mod gap_report;
pub(crate) mod measure_sizes;
pub(crate) mod model_benchmark;
pub(crate) mod pipeline_benchmark;
pub(crate) mod run;
pub(crate) mod split_benchmark;
pub(crate) mod survey;
pub(crate) mod validate_artifacts;
pub(crate) mod validate_gt;

/// Parses a `--pipelines`/`--paths`-style comma-separated pipeline name list, shared by
/// `compare` and `pipeline-benchmark`.
///
/// Silently dropping a typo can turn a requested comparison into a successful no-op, so an
/// unknown name is always rejected rather than skipped. ~keep
pub(crate) fn parse_pipeline_names(
    names: &[String],
    argument: &str,
) -> benchmark_harness::Result<Vec<benchmark_harness::comparison::Pipeline>> {
    names
        .iter()
        .map(|name| {
            benchmark_harness::comparison::Pipeline::parse(name).ok_or_else(|| {
                benchmark_harness::Error::Config(format!("unknown pipeline '{name}' supplied to {argument}"))
            })
        })
        .collect()
}
