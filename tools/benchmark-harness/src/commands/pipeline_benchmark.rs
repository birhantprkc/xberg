//! `pipeline-benchmark`: 6-path pipeline benchmark across the document corpus.

use benchmark_harness::pipeline_benchmark::{
    PipelineBenchmarkConfig, default_paths, print_pipeline_table, print_triage_blocks, run_pipeline_benchmark,
    write_json_output_with_config,
};
use benchmark_harness::{CohortManifest, Result};
use clap::Args;
use std::path::PathBuf;

use super::parse_pipeline_names;

/// `pipeline-benchmark` subcommand arguments -- pulled out of the `Commands` enum (via the
/// tuple-variant `PipelineBenchmark(PipelineBenchmarkArgs)`) purely to keep `Commands` under the
/// file's type-length limit; the flag grammar itself is unchanged.
#[derive(Args)]
pub(crate) struct PipelineBenchmarkArgs {
    /// Directory containing fixture JSON files
    #[arg(short, long)]
    pub(crate) fixtures: PathBuf,

    /// Exact ordered cohort manifest. Relative paths are resolved from the current
    /// directory when present, otherwise from the fixture directory. ~keep
    #[arg(long, conflicts_with_all = ["doc", "group"])]
    pub(crate) cohort: Option<PathBuf>,

    /// Pipeline paths to run. Paddle presets include paddle-v6-{medium,small,tiny}[+layout]
    /// and paddle-v5-server[+layout], plus paddle-{auto,no}rotate. Tesseract PSM presets are
    /// tesseract-{vertical-block,single-block,sparse-text} (PSM 5, 6, and 11), plus tesseract-autorotate.
    /// Sceptre presets (pinned to the ONNX Runtime inference engine) are sceptre-ort[+layout]
    /// and sceptre-ort-autorotate. ~keep
    #[arg(long, value_delimiter = ',')]
    pub(crate) paths: Option<Vec<String>>,

    /// Also run documents whose name contains one of these strings; unions with --group
    #[arg(long, value_delimiter = ',')]
    pub(crate) doc: Option<Vec<String>>,

    /// Run a named benchmark group (hotspot, smoke, promotion, holdout, tables, structure, lists)
    #[arg(long)]
    pub(crate) group: Option<String>,

    /// Dump outputs to /tmp/xberg_pipeline/
    #[arg(long)]
    pub(crate) dump_outputs: bool,

    /// Write JSON results to this file
    #[arg(long)]
    pub(crate) json_output: Option<PathBuf>,

    /// Sort results by metric for triage (sf1, tf1, time)
    #[arg(long, default_value = "sf1")]
    pub(crate) sort_by: String,

    /// Show only the bottom N worst-performing documents
    #[arg(long)]
    pub(crate) bottom_n: Option<usize>,

    /// Print per-block-type F1 breakdown for triage
    #[arg(long)]
    pub(crate) triage_blocks: bool,

    /// Generate per-pipeline flamegraph SVGs in this directory
    #[arg(long)]
    pub(crate) profile_dir: Option<PathBuf>,
}

pub(crate) fn parse_sort_metric(value: &str) -> Result<benchmark_harness::pipeline_benchmark::SortMetric> {
    benchmark_harness::pipeline_benchmark::SortMetric::parse(value).ok_or_else(|| {
        benchmark_harness::Error::Config(format!(
            "unknown sort metric '{value}': expected one of: sf1, tf1, time"
        ))
    })
}

/// Resolves the `--doc`/`--cohort`/`--group` document filter. `--cohort` and `--group` are
/// mutually exclusive with each other (enforced by clap) and both resolve to an exact document
/// name list; `--doc` is always a set of name-substring patterns that unions with either.
fn resolve_doc_filter(
    fixtures: &std::path::Path,
    cohort: Option<PathBuf>,
    doc: Option<Vec<String>>,
    group: Option<String>,
) -> Result<(Vec<String>, Vec<String>)> {
    let patterns: Vec<String> = doc.unwrap_or_default();
    let mut exact_names = Vec::new();
    if let Some(manifest_path) = cohort {
        let resolved_manifest_path = if manifest_path.is_absolute() || manifest_path.exists() {
            manifest_path
        } else {
            fixtures.join(manifest_path)
        };
        let manifest = CohortManifest::from_file(&resolved_manifest_path)?;
        let docs = manifest.load_corpus(fixtures, &resolved_manifest_path)?;
        exact_names = docs
            .iter()
            .map(|doc| {
                doc.fixture_path
                    .strip_prefix(fixtures)
                    .unwrap_or(&doc.fixture_path)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        tracing::info!(
            cohort = manifest.name,
            matched_document_count = exact_names.len(),
            "benchmark cohort resolved"
        );
    } else if let Some(ref group_name) = group {
        use benchmark_harness::groups::{find_group, group_names, resolve_group_docs};
        let g = find_group(group_name).ok_or_else(|| {
            benchmark_harness::Error::Config(format!(
                "Unknown group '{}'. Available: {}",
                group_name,
                group_names().join(", ")
            ))
        })?;
        exact_names = resolve_group_docs(fixtures, g)?;
        tracing::info!(
            group = g.name,
            description = g.description,
            matched_document_count = exact_names.len(),
            "benchmark group resolved"
        );
    }
    Ok((patterns, exact_names))
}

/// Runs one pipeline at a time under a CPU profiler, writing a flamegraph SVG per pipeline into
/// `profile_dir`, then returns without running the combined table.
async fn run_profiled(
    args: &PipelineBenchmarkArgs,
    selected_paths: &[benchmark_harness::comparison::Pipeline],
    sort_metric: benchmark_harness::pipeline_benchmark::SortMetric,
    doc_filter: &[String],
    exact_doc_filter: &[String],
    prof_dir: &std::path::Path,
) -> Result<()> {
    use benchmark_harness::profiling::ProfileGuard;

    std::fs::create_dir_all(prof_dir).map_err(benchmark_harness::Error::Io)?;

    for &pipeline in selected_paths {
        let svg_path = prof_dir.join(format!("{}.svg", pipeline.name()));
        tracing::info!(
            pipeline = pipeline.name(),
            output = %svg_path.display(),
            "profiling pipeline"
        );

        let config = PipelineBenchmarkConfig {
            fixtures_dir: args.fixtures.clone(),
            paths: vec![pipeline],
            doc_filter: doc_filter.to_vec(),
            exact_doc_filter: exact_doc_filter.to_vec(),
            dump_outputs: args.dump_outputs,
            json_output: None,
            sort_by: sort_metric,
            bottom_n: None,
            triage_blocks: false,
        };

        let guard = ProfileGuard::new(1000)?;
        let results = run_pipeline_benchmark(&config).await?;
        let profiling_result = guard.finish()?;
        profiling_result.generate_flamegraph(&svg_path)?;

        print_pipeline_table(&results, sort_metric, None);
    }

    Ok(())
}

pub(crate) async fn execute(args: PipelineBenchmarkArgs) -> Result<()> {
    let selected_paths = match &args.paths {
        Some(names) => parse_pipeline_names(names, "pipeline-benchmark --paths")?,
        None => default_paths(),
    };
    let sort_metric = parse_sort_metric(&args.sort_by)?;
    let (doc_filter, exact_doc_filter) = resolve_doc_filter(
        &args.fixtures,
        args.cohort.clone(),
        args.doc.clone(),
        args.group.clone(),
    )?;

    if let Some(ref prof_dir) = args.profile_dir {
        run_profiled(
            &args,
            &selected_paths,
            sort_metric,
            &doc_filter,
            &exact_doc_filter,
            prof_dir,
        )
        .await?;
        return Ok(());
    }

    let config = PipelineBenchmarkConfig {
        fixtures_dir: args.fixtures,
        paths: selected_paths,
        doc_filter,
        exact_doc_filter,
        dump_outputs: args.dump_outputs,
        json_output: args.json_output.clone(),
        sort_by: sort_metric,
        bottom_n: args.bottom_n,
        triage_blocks: args.triage_blocks,
    };

    let results = run_pipeline_benchmark(&config).await?;
    print_pipeline_table(&results, sort_metric, args.bottom_n);

    if args.triage_blocks {
        print_triage_blocks(&results, sort_metric, args.bottom_n.unwrap_or(10));
    }

    if let Some(ref path) = args.json_output {
        write_json_output_with_config(&results, path, &config)?;
    }

    Ok(())
}
