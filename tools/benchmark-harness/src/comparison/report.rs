//! Reporting: the stderr comparison/per-format tables, the JSON results dump, noise-detection
//! summary, and diagnostic-diff mode for low-scoring documents.

use super::execution::{ComparisonConfig, DocResult};
use crate::Result;
use crate::corpus::{self, CorpusDocument, CorpusFilter};
use serde::Serialize;
use std::collections::HashMap;

/// The finite values of `values`, dropping any `NaN`/non-finite entries — the exclusion rule
/// every average in this module applies before computing a mean.
fn finite_values(values: impl Iterator<Item = f64>) -> Vec<f64> {
    values.filter(|v| v.is_finite()).collect()
}

/// Mean of `values`, or `NaN` when empty. Callers pass an already-`finite_values`-filtered slice.
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        f64::NAN
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Print a formatted comparison table to stderr.
pub fn print_comparison_table(results: &[DocResult]) {
    if results.is_empty() {
        eprintln!("No results to display.");
        return;
    }

    let pipeline_names = comparison_table_pipeline_names(results);
    print_comparison_table_header(&pipeline_names);
    print_comparison_table_rows(results);
    print_comparison_table_average_row(results, &pipeline_names);

    for (i, name) in pipeline_names.iter().enumerate() {
        let failed = results.iter().filter(|r| !r.results[i].tf1.is_finite()).count();
        if failed > 0 {
            eprintln!("  {}: {} timeouts/errors (excluded from averages)", name, failed);
        }
    }
}

/// The pipeline-name column headers shared by [`print_comparison_table`] and
/// [`write_comparison_json`]/[`print_per_format_summary`]'s equivalents: every document ran the
/// same pipeline set, so the first document's order defines the columns.
fn comparison_table_pipeline_names(results: &[DocResult]) -> Vec<&str> {
    results
        .first()
        .map(|r| r.results.iter().map(|pr| pr.pipeline.name()).collect())
        .unwrap_or_default()
}

fn print_comparison_table_header(pipeline_names: &[&str]) {
    eprint!("{:<25}", "Document");
    for name in pipeline_names {
        eprint!(
            " {:>10} {:>10} {:>8}",
            format!("{} SF1", name),
            format!("{} TF1", name),
            format!("{} ms", name),
        );
    }
    eprintln!();
    eprintln!("{}", "-".repeat(25 + pipeline_names.len() * 30));
}

fn print_comparison_table_rows(results: &[DocResult]) {
    for doc in results {
        eprint!("{:<25}", doc.name);
        for pr in &doc.results {
            let time_str = if !pr.time_ms.is_finite() || pr.time_ms <= 0.0 {
                "---".to_string()
            } else {
                format!("{:.0}", pr.time_ms)
            };
            eprint!(
                " {:>10} {:>10} {:>8}",
                format_percentage(pr.sf1),
                format_percentage(pr.tf1),
                time_str
            );
        }
        eprintln!();
    }
}

fn print_comparison_table_average_row(results: &[DocResult], pipeline_names: &[&str]) {
    eprintln!("{}", "-".repeat(25 + pipeline_names.len() * 30));
    eprint!("{:<25}", "AVERAGE");
    for (i, _) in pipeline_names.iter().enumerate() {
        let avg_sf1 = mean(&finite_values(results.iter().map(|r| r.results[i].sf1)));
        let avg_tf1 = mean(&finite_values(results.iter().map(|r| r.results[i].tf1)));
        let times = finite_values(
            results
                .iter()
                .map(|r| r.results[i].time_ms)
                .filter(|t| t.is_finite() && *t > 0.0),
        );
        let avg_time = mean(&times);
        let time_str = if !avg_time.is_finite() {
            "---".to_string()
        } else {
            format!("{:.0}", avg_time)
        };
        eprint!(
            " {:>10} {:>10} {:>8}",
            format_percentage(avg_sf1),
            format_percentage(avg_tf1),
            time_str
        );
    }
    eprintln!();
}

fn format_percentage(value: f64) -> String {
    if value.is_finite() {
        format!("{:.1}%", value * 100.0)
    } else {
        "---".to_string()
    }
}

/// Print a per-format summary table to stderr, grouping documents by file_type.
pub fn print_per_format_summary(results: &[DocResult]) {
    if results.is_empty() {
        return;
    }

    let pipeline_names = comparison_table_pipeline_names(results);
    let by_format = group_by_file_type(results);

    eprintln!("\nPer-Format Summary:");

    eprint!("{:<12} {:>5}", "Format", "Count");
    for name in &pipeline_names {
        eprint!("  {:>10} {:>10}", format!("{} SF1", name), format!("{} TF1", name));
    }
    eprintln!();
    let line_width = 12 + 5 + pipeline_names.len() * 22;
    eprintln!("{}", "\u{2500}".repeat(line_width));

    for (format, docs) in &by_format {
        eprint!("{:<12} {:>5}", format, docs.len());
        for (i, _) in pipeline_names.iter().enumerate() {
            let avg_sf1 = mean(&finite_values(docs.iter().map(|d| d.results[i].sf1)));
            let avg_tf1 = mean(&finite_values(docs.iter().map(|d| d.results[i].tf1)));
            eprint!(
                "  {:>10} {:>10}",
                format_percentage(avg_sf1),
                format_percentage(avg_tf1)
            );
        }
        eprintln!();
    }
}

/// Group `results` by `file_type`, preserving `BTreeMap`'s sorted-key order (matches the original
/// per-format loop order, which iterated a `BTreeMap` built the same way).
fn group_by_file_type(results: &[DocResult]) -> std::collections::BTreeMap<&str, Vec<&DocResult>> {
    let mut by_format: std::collections::BTreeMap<&str, Vec<&DocResult>> = std::collections::BTreeMap::new();
    for doc in results {
        by_format.entry(&doc.file_type).or_default().push(doc);
    }
    by_format
}

/// Per-format aggregation entry for JSON output.
#[derive(Debug, Clone, Serialize)]
struct FormatSummary {
    count: usize,
    pipelines: Vec<FormatPipelineSummary>,
}

/// Per-pipeline averages within a format group.
#[derive(Debug, Clone, Serialize)]
struct FormatPipelineSummary {
    pipeline: String,
    avg_sf1: f64,
    avg_tf1: f64,
    sf1_count: usize,
    tf1_count: usize,
}

/// Overall summary for JSON output.
#[derive(Debug, Clone, Serialize)]
struct OverallSummary {
    total_documents: usize,
    pipelines: Vec<FormatPipelineSummary>,
}

/// Top-level JSON output structure.
#[derive(Debug, Clone, Serialize)]
struct ComparisonJsonOutput {
    documents: Vec<DocResult>,
    by_format: std::collections::BTreeMap<String, FormatSummary>,
    overall: OverallSummary,
}

/// Build one format group's (or the overall run's) per-pipeline average summaries.
fn build_pipeline_summaries(docs: &[&DocResult], pipeline_names: &[String]) -> Vec<FormatPipelineSummary> {
    pipeline_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let sf1_vals = finite_values(docs.iter().map(|d| d.results[i].sf1));
            let tf1_vals = finite_values(docs.iter().map(|d| d.results[i].tf1));
            FormatPipelineSummary {
                pipeline: name.clone(),
                avg_sf1: mean(&sf1_vals),
                avg_tf1: mean(&tf1_vals),
                sf1_count: sf1_vals.len(),
                tf1_count: tf1_vals.len(),
            }
        })
        .collect()
}

/// Write full comparison results (documents + per-format + overall) to a JSON file.
pub fn write_comparison_json(results: &[DocResult], path: &std::path::Path) -> Result<()> {
    let pipeline_names: Vec<String> = results
        .first()
        .map(|r| r.results.iter().map(|pr| pr.pipeline.name().to_string()).collect())
        .unwrap_or_default();

    let mut by_format_map: std::collections::BTreeMap<String, Vec<&DocResult>> = std::collections::BTreeMap::new();
    for doc in results {
        by_format_map.entry(doc.file_type.clone()).or_default().push(doc);
    }

    let by_format: std::collections::BTreeMap<String, FormatSummary> = by_format_map
        .iter()
        .map(|(format, docs)| {
            let pipelines = build_pipeline_summaries(docs, &pipeline_names);
            (
                format.clone(),
                FormatSummary {
                    count: docs.len(),
                    pipelines,
                },
            )
        })
        .collect();

    let all_docs: Vec<&DocResult> = results.iter().collect();
    let overall_pipelines = build_pipeline_summaries(&all_docs, &pipeline_names);

    let output = ComparisonJsonOutput {
        documents: results.to_vec(),
        by_format,
        overall: OverallSummary {
            total_documents: results.len(),
            pipelines: overall_pipelines,
        },
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(crate::Error::Io)?;
    }
    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| crate::Error::Benchmark(format!("Failed to serialize comparison JSON: {}", e)))?;
    std::fs::write(path, json).map_err(crate::Error::Io)?;

    Ok(())
}

/// Run diagnostic diff mode on documents with SF1 below the threshold.
pub(super) fn run_diagnostics(config: &ComparisonConfig, results: &[DocResult]) -> Result<()> {
    use crate::diagnostics::{diagnose_document, write_diagnostic_files};

    let filter = CorpusFilter {
        file_types: None,
        require_ground_truth: true,
        require_markdown_ground_truth: true,
        name_patterns: config.name_filter.clone().into_iter().collect(),
        category: config.category_filter.clone(),
        ..Default::default()
    };
    let docs = corpus::build_corpus(&config.fixtures_dir, &filter)?;
    let doc_map: HashMap<String, &CorpusDocument> = docs.iter().map(|d| (d.name.clone(), d)).collect();

    let mut diagnosed_count = 0;

    for doc_result in results {
        let corpus_doc = match doc_map.get(&doc_result.name) {
            Some(d) => d,
            None => continue,
        };

        let gt_text = corpus_doc
            .ground_truth_text
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();

        let gt_markdown = corpus_doc
            .ground_truth_markdown
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());

        for pr in &doc_result.results {
            if pr.sf1 < config.diagnose_threshold {
                let diag = diagnose_document(
                    &doc_result.name,
                    &doc_result.file_type,
                    pr.pipeline.name(),
                    &pr.content,
                    &gt_text,
                    gt_markdown.as_deref(),
                );

                if let Err(e) = write_diagnostic_files(&diag, gt_markdown.as_deref(), &pr.content) {
                    eprintln!(
                        "  Warning: failed to write diagnostics for {}/{}: {}",
                        doc_result.name,
                        pr.pipeline.name(),
                        e
                    );
                }

                diagnosed_count += 1;
            }
        }
    }

    if diagnosed_count > 0 {
        eprintln!(
            "\nDiagnosed {} document(s) with SF1 < {:.0}% -> /tmp/xberg_diagnose/",
            diagnosed_count,
            config.diagnose_threshold * 100.0
        );
    } else {
        eprintln!(
            "\nNo documents below SF1 threshold ({:.0}%) — no diagnostics generated.",
            config.diagnose_threshold * 100.0
        );
    }

    Ok(())
}

/// Run noise detection on all pipeline outputs and print summary.
pub(super) fn print_noise_summary(results: &[DocResult]) {
    use crate::noise_detection::{Severity, detect_noise};
    use std::collections::HashMap;

    eprintln!("\n{:=<70}", "");
    eprintln!("NOISE DETECTION SUMMARY");
    eprintln!("{:=<70}", "");

    let mut total_docs_with_noise = 0;
    let mut total_issues = 0;
    let mut kind_counts: HashMap<String, usize> = HashMap::new();
    let mut noisy_docs: Vec<(String, String, usize, usize, usize)> = Vec::new();

    for doc_result in results {
        for pr in &doc_result.results {
            if pr.content.is_empty() {
                continue;
            }
            let report = detect_noise(&pr.content);
            if report.issues.is_empty() {
                continue;
            }
            total_docs_with_noise += 1;
            total_issues += report.issues.len();
            for issue in &report.issues {
                *kind_counts.entry(format!("{:?}", issue.kind)).or_insert(0) += 1;
            }
            let errors = report.issues.iter().filter(|i| i.severity == Severity::Error).count();
            let warnings = report.issues.iter().filter(|i| i.severity == Severity::Warning).count();
            let infos = report.issues.iter().filter(|i| i.severity == Severity::Info).count();
            noisy_docs.push((
                doc_result.name.clone(),
                pr.pipeline.name().to_string(),
                errors,
                warnings,
                infos,
            ));
        }
    }

    if total_docs_with_noise == 0 {
        eprintln!("No noise detected in any extracted output.");
        return;
    }

    eprintln!(
        "{} documents with noise issues ({} total issues)",
        total_docs_with_noise, total_issues
    );

    eprintln!("\nBy kind:");
    let mut sorted_kinds: Vec<_> = kind_counts.into_iter().collect();
    sorted_kinds.sort_by_key(|b| std::cmp::Reverse(b.1));
    for (kind, count) in &sorted_kinds {
        eprintln!("  {:<30} {}", kind, count);
    }

    noisy_docs.sort_by(|a, b| b.2.cmp(&a.2).then(b.3.cmp(&a.3)));
    let show = noisy_docs.len().min(20);
    eprintln!("\nTop {} noisy documents:", show);
    eprintln!(
        "  {:<30} {:<15} {:>6} {:>6} {:>6}",
        "Document", "Pipeline", "Errors", "Warns", "Infos"
    );
    for (doc, pipeline, errors, warnings, infos) in noisy_docs.iter().take(show) {
        eprintln!(
            "  {:<30} {:<15} {:>6} {:>6} {:>6}",
            doc, pipeline, errors, warnings, infos
        );
    }
}
