//! `model-benchmark`: layout model A/B comparison benchmark.

use benchmark_harness::Result;
use benchmark_harness::model_benchmark::{ModelBenchmarkConfig, print_model_table, run_model_benchmark};
use std::path::PathBuf;

pub(crate) async fn execute(fixtures: PathBuf, model_a: String, model_b: String) -> Result<()> {
    let config = ModelBenchmarkConfig {
        fixtures_dir: fixtures,
        model_a: model_a.clone(),
        model_b: model_b.clone(),
        ..Default::default()
    };

    let results = run_model_benchmark(&config).await?;
    print_model_table(&results, &model_a, &model_b);
    Ok(())
}
