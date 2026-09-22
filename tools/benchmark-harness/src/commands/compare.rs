//! `compare`: multi-pipeline quality comparison on the document corpus.

use benchmark_harness::Result;
use benchmark_harness::comparison::{ComparisonConfig, Pipeline, run_with_guardrails};
use clap::Args;
use std::path::PathBuf;

use super::parse_pipeline_names;

/// `compare` subcommand arguments -- pulled out of the `Commands` enum (via the tuple-variant
/// `Compare(CompareArgs)`) purely to keep `Commands` under the file's type-length limit; the
/// flag grammar itself is unchanged.
#[derive(Args)]
pub(crate) struct CompareArgs {
    /// Directory containing fixture JSON files
    #[arg(short, long)]
    pub(crate) fixtures: PathBuf,

    /// Pipelines to compare. Paddle presets include paddle-v6-{medium,small,tiny}[+layout]
    /// and paddle-v5-server[+layout], plus paddle-{auto,no}rotate. Opt-in Paddle quality sweeps use the
    /// paddle-v6-small+layout+{det-side-*,det-db-*,drop-score-*} prefix. Tesseract PSM presets are
    /// tesseract-{vertical-block,single-block,sparse-text} (PSM 5, 6, and 11), plus tesseract-autorotate.
    /// Sceptre presets (pinned to the ONNX Runtime inference engine) are sceptre-ort[+layout]
    /// and sceptre-ort-autorotate.
    #[arg(long, value_delimiter = ',')]
    pub(crate) pipelines: Option<Vec<String>>,

    /// Dump extraction outputs to /tmp/xberg_compare/
    #[arg(long)]
    pub(crate) dump_outputs: bool,

    /// Enable quality guardrails (fail on regressions)
    #[arg(long)]
    pub(crate) guardrails: bool,

    /// Path to guardrails JSON config file (used when --guardrails is set)
    #[arg(long, default_value = "guardrails.json")]
    pub(crate) guardrails_file: PathBuf,

    /// Only run documents whose name contains this string
    #[arg(long)]
    pub(crate) filter: Option<String>,

    /// Only run documents whose metadata.category exactly matches this string
    #[arg(long)]
    pub(crate) category: Option<String>,

    /// Write full comparison results to JSON file
    #[arg(long)]
    pub(crate) json_output: Option<PathBuf>,

    /// Run noise detection on extracted outputs
    #[arg(long)]
    pub(crate) noise: bool,

    /// Enable diagnostic diff mode for poor-scoring documents
    #[arg(long)]
    pub(crate) diagnose: bool,

    /// SF1 threshold below which to diagnose (default 0.8)
    #[arg(long, default_value = "0.8")]
    pub(crate) diagnose_threshold: f64,
}

pub(crate) async fn execute(args: CompareArgs) -> Result<()> {
    let selected_pipelines = match args.pipelines {
        Some(names) => parse_pipeline_names(&names, "compare --pipelines")?,
        None => vec![Pipeline::Baseline, Pipeline::Layout],
    };

    let config = ComparisonConfig {
        fixtures_dir: args.fixtures,
        pipelines: selected_pipelines,
        dump_outputs: args.dump_outputs,
        guardrails: args.guardrails,
        guardrails_file: Some(args.guardrails_file),
        name_filter: args.filter,
        category_filter: args.category,
        json_output: args.json_output,
        noise: args.noise,
        diagnose: args.diagnose,
        diagnose_threshold: args.diagnose_threshold,
    };

    let exit_code = run_with_guardrails(&config).await?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
