//! `validate-gt`: validates ground truth files and optionally fixes HTML artifacts.

use benchmark_harness::Result;
use benchmark_harness::validate_gt::{ValidateGtConfig, ValidateGtReport, validate_ground_truth};
use std::path::PathBuf;

fn print_totals(report: &ValidateGtReport) {
    println!("=== Ground Truth Validation Report ===\n");
    println!("Total fixtures:       {}", report.total_fixtures);
    println!("With text GT:         {}", report.with_text_gt);
    println!("With markdown GT:     {}", report.with_markdown_gt);
    println!("Missing text GT:      {}", report.missing_text_gt);
    println!("Missing markdown GT:  {}", report.missing_markdown_gt);
}

fn print_issue_lists(report: &ValidateGtReport, fix: bool) {
    if !report.small_gt_files.is_empty() {
        println!("\nSmall GT files (<10 bytes):");
        for (path, size) in &report.small_gt_files {
            println!("  {} ({} bytes)", path, size);
        }
    }

    if !report.html_issues.is_empty() {
        println!("\nHTML issues in markdown GT ({} file(s)):", report.html_issues.len());
        for (path, tags) in &report.html_issues {
            let preview: Vec<&str> = tags.iter().take(3).map(|s| s.as_str()).collect();
            let suffix = if tags.len() > 3 {
                format!(" (and {} more)", tags.len() - 3)
            } else {
                String::new()
            };
            println!("  {} - {} tag(s): {}{}", path, tags.len(), preview.join(", "), suffix);
        }
    }

    if !report.noisy_gt_files.is_empty() {
        println!(
            "\nNoisy GT files ({} file(s) with Warning/Error noise issues):",
            report.noisy_gt_files.len()
        );
        for (path, count) in &report.noisy_gt_files {
            println!("  {} ({} issue(s))", path, count);
        }
    }

    if !report.low_diversity_gt.is_empty() {
        println!(
            "\nLow diversity GT files ({} file(s) with no headings for >100 byte docs):",
            report.low_diversity_gt.len()
        );
        for path in &report.low_diversity_gt {
            println!("  {}", path);
        }
    }

    if fix && report.fixes_applied > 0 {
        println!("\nFixes applied: {}", report.fixes_applied);
    }

    if !report.load_failures.is_empty() {
        println!(
            "\nFixtures that failed to load ({} — e.g. missing/unreadable ground truth):",
            report.load_failures.len()
        );
        for (path, error) in report.load_failures.iter().take(10) {
            println!("  {path}: {error}");
        }
        if report.load_failures.len() > 10 {
            println!("  ... and {} more", report.load_failures.len() - 10);
        }
    }

    if report.html_issues.is_empty()
        && report.small_gt_files.is_empty()
        && report.noisy_gt_files.is_empty()
        && report.low_diversity_gt.is_empty()
        && report.load_failures.is_empty()
    {
        println!("\nAll ground truth files are valid.");
    }
}

pub(crate) async fn execute(fixtures: PathBuf, fix: bool, strict: bool) -> Result<()> {
    let config = ValidateGtConfig {
        fixtures_dir: fixtures,
        fix,
        strict,
    };

    let report = validate_ground_truth(&config)?;

    print_totals(&report);
    print_issue_lists(&report, fix);

    if strict && !report.load_failures.is_empty() {
        return Err(benchmark_harness::Error::Benchmark(format!(
            "{} fixture(s) failed to load their ground truth. If these are reference-corpus \
             documents, the .corpus-cache was not restored (run restore-corpus-cache.sh).",
            report.load_failures.len()
        )));
    }

    Ok(())
}
