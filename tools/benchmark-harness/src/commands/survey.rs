//! `survey`: corpus-wide extraction survey with stats.

use benchmark_harness::Result;
use benchmark_harness::survey::{SurveyConfig, print_survey_table, run_survey};
use std::path::PathBuf;

pub(crate) async fn execute(fixtures: PathBuf, types: Option<Vec<String>>) -> Result<()> {
    let config = SurveyConfig {
        fixtures_dir: fixtures,
        file_types: types,
    };

    let results = run_survey(&config).await?;
    print_survey_table(&results);
    Ok(())
}
