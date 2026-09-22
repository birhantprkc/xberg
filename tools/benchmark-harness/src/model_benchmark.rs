//! Layout model A/B benchmark: compare layout detection configurations on rendered PDF pages.
//!
//! Replaces `crates/xberg/tests/layout_model_benchmark.rs`.
//! Compares two table model configurations on cold start, inference latency, and class distribution.

use crate::Result;
use crate::corpus::{self, CorpusFilter};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use xberg::core::config::layout::TableModel;

fn parse_table_model(s: &str) -> TableModel {
    match s {
        "tatr" => TableModel::Tatr,
        "slanet_wired" => TableModel::SlanetWired,
        "slanet_wireless" => TableModel::SlanetWireless,
        "slanet_plus" => TableModel::SlanetPlus,
        "slanet_auto" => TableModel::SlanetAuto,
        "disabled" => TableModel::Disabled,
        _ => TableModel::default(),
    }
}

/// Configuration for model benchmark.
pub struct ModelBenchmarkConfig {
    pub fixtures_dir: PathBuf,
    pub model_a: String,
    pub model_b: String,
    pub max_pages: usize,
}

impl Default for ModelBenchmarkConfig {
    fn default() -> Self {
        Self {
            fixtures_dir: PathBuf::from("tools/benchmark-harness/fixtures"),
            model_a: "tatr".to_string(),
            model_b: "slanet_auto".to_string(),
            max_pages: 3,
        }
    }
}

/// Per-document model comparison result.
#[derive(Debug)]
pub struct ModelDocResult {
    pub name: String,
    pub model_a_ms: f64,
    pub model_b_ms: f64,
    pub model_a_regions: usize,
    pub model_b_regions: usize,
}

/// Wall-clock ceiling for one document under one table model.
const MODEL_EXTRACTION_TIMEOUT: Duration = Duration::from_secs(180);

/// Extract `document_path` under `table_model` and report the elapsed milliseconds.
///
/// A timeout or extraction error yields `None` alongside the elapsed time, so a document that
/// one model cannot handle still contributes its timing to the comparison.
async fn time_table_model(
    document_path: &Path,
    document_name: &str,
    table_model: &str,
) -> (Option<xberg::ExtractedDocument>, f64) {
    let extraction_config = xberg::ExtractionConfig {
        output_format: xberg::core::config::OutputFormat::Markdown,
        layout: Some(xberg::core::config::layout::LayoutDetectionConfig {
            table_model: parse_table_model(table_model),
            ..Default::default()
        }),
        ..Default::default()
    };

    let started = Instant::now();
    let result = match tokio::time::timeout(
        MODEL_EXTRACTION_TIMEOUT,
        crate::extract_xberg_file(document_path, &extraction_config),
    )
    .await
    {
        Ok(r) => r.ok(),
        Err(_) => {
            eprintln!("  TIMEOUT {}/{}", document_name, table_model);
            None
        }
    };

    (result, started.elapsed().as_secs_f64() * 1000.0)
}

/// Run model benchmark (stub — full implementation requires layout model API).
///
/// This currently extracts using the two table model configurations and measures timing.
/// A full implementation would directly invoke the ONNX models on rendered pages.
pub async fn run_model_benchmark(config: &ModelBenchmarkConfig) -> Result<Vec<ModelDocResult>> {
    let filter = CorpusFilter {
        file_types: Some(vec!["pdf".to_string()]),
        require_ground_truth: true,
        name_patterns: Vec::new(),
        max_file_size: Some(5_000_000),
        ..Default::default()
    };

    let docs = corpus::build_corpus(&config.fixtures_dir, &filter)?;
    eprintln!(
        "Model benchmark: {} documents, models: {} vs {}",
        docs.len(),
        config.model_a,
        config.model_b
    );

    let mut results = Vec::new();

    for doc in &docs {
        let (result_a, model_a_ms) = time_table_model(&doc.document_path, &doc.name, &config.model_a).await;
        let (result_b, model_b_ms) = time_table_model(&doc.document_path, &doc.name, &config.model_b).await;

        let count_headings = |content: &str| content.lines().filter(|l| l.starts_with('#')).count();

        let model_a_regions = result_a.as_ref().map(|r| count_headings(&r.content)).unwrap_or(0);
        let model_b_regions = result_b.as_ref().map(|r| count_headings(&r.content)).unwrap_or(0);

        results.push(ModelDocResult {
            name: doc.name.clone(),
            model_a_ms,
            model_b_ms,
            model_a_regions,
            model_b_regions,
        });
    }

    Ok(results)
}

/// Print model benchmark results table.
pub fn print_model_table(results: &[ModelDocResult], model_a: &str, model_b: &str) {
    eprintln!(
        "{:<25} {:>10} {:>10} {:>10} {:>10}",
        "Document",
        format!("{} ms", model_a),
        format!("{} ms", model_b),
        format!("{} rgns", model_a),
        format!("{} rgns", model_b),
    );
    eprintln!("{}", "-".repeat(70));

    for r in results {
        eprintln!(
            "{:<25} {:>10.0} {:>10.0} {:>10} {:>10}",
            if r.name.len() > 24 { &r.name[..24] } else { &r.name },
            r.model_a_ms,
            r.model_b_ms,
            r.model_a_regions,
            r.model_b_regions,
        );
    }

    let n = results.len() as f64;
    let avg_a: f64 = results.iter().map(|r| r.model_a_ms).sum::<f64>() / n;
    let avg_b: f64 = results.iter().map(|r| r.model_b_ms).sum::<f64>() / n;
    eprintln!("{}", "-".repeat(70));
    eprintln!("{:<25} {:>10.0} {:>10.0}", "AVERAGE", avg_a, avg_b);
}
