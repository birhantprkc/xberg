//! Tests for [`super`], split by what they exercise. Every submodule is a group of `#[test]`
//! functions moved verbatim out of the former single `comparison.rs` test module.

use super::*;

mod extraction_config_presets;
mod fixture_language_and_vendored;
mod guardrails_tests;
mod json_and_pipeline_parsing;
mod scoring_and_timeout;
