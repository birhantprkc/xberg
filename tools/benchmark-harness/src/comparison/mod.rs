//! Framework comparison: run multiple extraction pipelines on the corpus and
//! compare quality (SF1, TF1) against ground truth with optional guardrails.
//!
//! Replaces the logic previously in `crates/xberg/tests/framework_comparison.rs`.
//! Uses canonical scoring from [`crate::quality`].
//!
//! # Pipeline semantics
//!
//! Each [`Pipeline`] variant represents a distinct extraction configuration:
//!
//! - **Baseline / Layout**: native PDF text extraction; `Layout` adds layout
//!   detection for heading/table identification without OCR.
//! - **Tesseract / TesseractLayout**: Tesseract OCR with `force_ocr`; the
//!   `Layout` suffix adds layout detection on top.
//! - **Paddle***: explicitly pinned PaddleOCR model version and tier, with
//!   optional layout detection; the label-to-model mapping is benchmark identity. ~keep
//! - **Sceptre***: Sceptre OCR pinned to the ONNX Runtime inference engine
//!   (`force_ocr`), with `Layout`/`AutoRotate` variants mirroring the Tesseract/Paddle
//!   pattern. The tract engine is deliberately not exposed here — it is reserved for the
//!   bounded diagnostic matrix reachable only from the separate `XbergPipeline` enum. ~keep
//! - **Docling / PaddleOcrPython / RapidOcr**: vendored pipelines whose
//!   outputs are pre-computed and read from disk (no live extraction).
//! - **LayoutSlanet***: layout detection with explicit SLANeXT table model
//!   variants (wired, wireless, plus, auto).
//!
//! # Guardrail thresholds
//!
//! When `guardrails` is enabled, the comparison enforces per-document minimum
//! SF1 and TF1 thresholds. Thresholds are set to approximately 90% of observed
//! scores (a floor) to catch regressions without false-positive failures from
//! run-to-run variance. The guardrail table is updated manually after
//! significant quality improvements.
//!
//! # Module layout
//!
//! [`pipeline`] names every extraction configuration; [`extraction_config`] builds an
//! `xberg::ExtractionConfig` for one; [`scoring`] scores extracted content against ground truth;
//! [`execution`] runs a pipeline (live or vendored) and the full document x pipeline sweep;
//! [`report`] prints/writes results; [`guardrails`] loads and checks per-document score/order
//! contracts. This file holds the top-level orchestration ([`run_with_guardrails`]).

mod execution;
mod extraction_config;
mod guardrails;
mod pipeline;
mod report;
mod scoring;

#[cfg(test)]
mod tests;

use crate::Result;
use std::path::Path;

// Re-exports: keep every item nameable at `crate::comparison::…` exactly as it was before the
// split, so no other file in the crate needs to change.
pub use execution::{ComparisonConfig, DocResult, PipelineResult, extract_pipeline, run_comparison};
pub use extraction_config::build_extraction_config;
pub use guardrails::{GuardrailContract, GuardrailsConfig, check_guardrails, load_guardrails};
pub use pipeline::Pipeline;
pub use report::{print_comparison_table, print_per_format_summary, write_comparison_json};
pub use scoring::{StructuralBreakdown, read_vendored_cached, score_document};

use report::{print_noise_summary, run_diagnostics};

/// Run comparison with guardrails and return exit code (0 = pass, 1 = fail).
pub async fn run_with_guardrails(config: &ComparisonConfig) -> Result<i32> {
    let results = run_comparison(config).await?;
    print_comparison_table(&results);
    print_per_format_summary(&results);

    if let Some(ref json_path) = config.json_output {
        write_comparison_json(&results, json_path)?;
        eprintln!("\nComparison JSON written to: {}", json_path.display());
    }

    if config.guardrails {
        let guardrails_path = config
            .guardrails_file
            .as_deref()
            .unwrap_or_else(|| Path::new("guardrails.json"));
        let mut guardrails_config = load_guardrails(guardrails_path)?;
        guardrails_config.contracts.retain(|contract| {
            let pipeline_selected = config
                .pipelines
                .iter()
                .any(|pipeline| pipeline.name() == contract.pipeline);
            let document_selected = config
                .name_filter
                .as_deref()
                .is_none_or(|filter| contract.doc.contains(filter));
            pipeline_selected && document_selected
        });
        eprintln!(
            "Loaded {} guardrail contracts from {} (v{}, factor {})",
            guardrails_config.contracts.len(),
            guardrails_path.display(),
            guardrails_config.version,
            guardrails_config.threshold_factor,
        );
        if guardrails_config.contracts.is_empty() {
            eprintln!("\nGUARDRAIL FAILURE: no contracts apply to the selected pipelines and document filter");
            return Ok(1);
        }
        let failures = check_guardrails(&results, &guardrails_config);
        if !failures.is_empty() {
            eprintln!("\nGUARDRAIL FAILURES:");
            for f in &failures {
                eprintln!("  {}", f);
            }
            return Ok(1);
        }
        eprintln!("\nAll guardrails passed.");
    }

    if config.noise {
        print_noise_summary(&results);
    }

    if config.diagnose {
        run_diagnostics(config, &results)?;
    }

    Ok(0)
}
