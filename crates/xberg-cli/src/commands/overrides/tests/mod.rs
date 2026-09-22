//! Unit tests for `ExtractionOverrides`, split by CLI-flag domain.
//!
//! `default_overrides` and `config_from_json` are shared fixtures used across more than
//! one domain module below; domain-local helpers (e.g. the tracing-capture helpers used
//! only by `layout`) stay in their own module instead.

use super::*;

#[cfg(any(feature = "ocr-surface", feature = "core-cli", feature = "analysis"))]
use xberg::ExtractionConfig;

fn default_overrides() -> ExtractionOverrides {
    ExtractionOverrides::default()
}

/// Mirrors `main.rs`: config file/defaults -> `--config-json` -> individual CLI flags.
#[cfg(any(feature = "ocr-surface", feature = "core-cli", feature = "analysis"))]
fn config_from_json(json: &str) -> ExtractionConfig {
    let mut config = ExtractionConfig::default();
    crate::input::apply_json_overrides(&mut config, Some(json.to_string()), None)
        .expect("--config-json should merge into the base config");
    config
}

mod candle_backends;
mod chunking;
mod layout;
mod llm_api_key;
mod misc_validation;
mod ocr_backend_options;
mod ocr_basics;
mod ocr_json_precedence;
mod ocr_precedence;
mod output_and_concurrency;
mod pdf_backend;
mod vlm;
