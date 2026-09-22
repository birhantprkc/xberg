//! Guardrails: data-driven per-document minimum-score and relative-reading-order contracts
//! loaded from JSON, checked against a comparison run's results.

use super::execution::DocResult;
use super::pipeline::Pipeline;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Data-driven guardrails configuration loaded from JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct GuardrailsConfig {
    pub version: String,
    pub generated_at: String,
    pub threshold_factor: f64,
    pub contracts: Vec<GuardrailContract>,
}

/// A single guardrail contract: minimum threshold for a specific document + pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailContract {
    pub doc: String,
    /// Fixture file type used to disambiguate identical document names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    pub pipeline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_sf1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tf1: Option<f64>,
    /// Exact text anchors that must occur in this relative order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relative_order: Vec<String>,
}

pub(super) const READING_ORDER_GUARDRAIL_DOC: &str = "681693";
pub(super) const READING_ORDER_GUARDRAIL_PIPELINE: &str = "native+layout+reading-order";
const READING_ORDER_GUARDRAIL_ANCHORS: &[&str] = &[
    "maintainers wanted",
    "See #182",
    "MongoKit",
    "MongoDB is a great schema-less document oriented database",
    "Philosophy",
];

/// Load a guardrails configuration from a JSON file.
pub fn load_guardrails(path: &Path) -> Result<GuardrailsConfig> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| crate::Error::Benchmark(format!("Failed to read guardrails file {}: {}", path.display(), e)))?;
    let mut config: GuardrailsConfig = serde_json::from_str(&data)
        .map_err(|e| crate::Error::Benchmark(format!("Failed to parse guardrails file: {}", e)))?;
    install_reading_order_guardrail(&mut config);
    validate_guardrails_config(&config)?;
    Ok(config)
}

fn validate_guardrails_config(config: &GuardrailsConfig) -> Result<()> {
    if !config.threshold_factor.is_finite() || !(0.0..=1.0).contains(&config.threshold_factor) {
        return Err(crate::Error::Config(
            "guardrail threshold_factor must be finite and within [0, 1]".to_string(),
        ));
    }

    let mut identities = std::collections::HashSet::new();
    for contract in &config.contracts {
        if contract.doc.trim().is_empty() {
            return Err(crate::Error::Config(
                "guardrail document name must not be empty".to_string(),
            ));
        }
        if Pipeline::parse(&contract.pipeline).is_none_or(|pipeline| pipeline.name() != contract.pipeline) {
            return Err(crate::Error::Config(format!(
                "unknown guardrail pipeline '{}'",
                contract.pipeline
            )));
        }
        if contract.min_sf1.is_none() && contract.min_tf1.is_none() && contract.relative_order.is_empty() {
            return Err(crate::Error::Config(format!(
                "guardrail {} [{}] {} has no threshold or relative-order predicate",
                contract.doc,
                contract.file_type.as_deref().unwrap_or("any"),
                contract.pipeline
            )));
        }
        for (name, value) in [("min_sf1", contract.min_sf1), ("min_tf1", contract.min_tf1)] {
            if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
                return Err(crate::Error::Config(format!(
                    "guardrail {} {} must be finite and within [0, 1]",
                    contract.doc, name
                )));
            }
        }
        if contract.relative_order.iter().any(|anchor| anchor.trim().is_empty()) {
            return Err(crate::Error::Config(format!(
                "guardrail {} relative-order anchors must not be empty",
                contract.doc
            )));
        }
        let identity = (
            contract.doc.as_str(),
            contract.file_type.as_deref(),
            contract.pipeline.as_str(),
        );
        if !identities.insert(identity) {
            return Err(crate::Error::Config(format!(
                "duplicate guardrail contract: {} [{}] {}",
                contract.doc,
                contract.file_type.as_deref().unwrap_or("any"),
                contract.pipeline
            )));
        }
    }
    Ok(())
}

pub(super) fn reading_order_anchors(doc: &str, pipeline: &str) -> Vec<String> {
    if doc != READING_ORDER_GUARDRAIL_DOC || pipeline != READING_ORDER_GUARDRAIL_PIPELINE {
        return Vec::new();
    }
    READING_ORDER_GUARDRAIL_ANCHORS
        .iter()
        .map(|anchor| (*anchor).to_string())
        .collect()
}

fn install_reading_order_guardrail(config: &mut GuardrailsConfig) {
    if let Some(contract) = config.contracts.iter_mut().find(|contract| {
        contract.doc == READING_ORDER_GUARDRAIL_DOC
            && contract.pipeline == READING_ORDER_GUARDRAIL_PIPELINE
            && contract.file_type.as_deref().is_none_or(|file_type| file_type == "pdf")
    }) {
        contract.file_type.get_or_insert_with(|| "pdf".to_string());
        if contract.relative_order.is_empty() {
            contract.relative_order =
                reading_order_anchors(READING_ORDER_GUARDRAIL_DOC, READING_ORDER_GUARDRAIL_PIPELINE);
        }
        return;
    }
    config.contracts.push(GuardrailContract {
        doc: READING_ORDER_GUARDRAIL_DOC.to_string(),
        file_type: Some("pdf".to_string()),
        pipeline: READING_ORDER_GUARDRAIL_PIPELINE.to_string(),
        min_sf1: None,
        min_tf1: None,
        relative_order: reading_order_anchors(READING_ORDER_GUARDRAIL_DOC, READING_ORDER_GUARDRAIL_PIPELINE),
    });
}

/// Check guardrails from a loaded config, returning a list of failure messages (empty = all passed).
pub fn check_guardrails(results: &[DocResult], config: &GuardrailsConfig) -> Vec<String> {
    let mut failures = Vec::new();

    for contract in &config.contracts {
        let doc = match resolve_guardrail_document(results, contract) {
            Ok(Some(doc)) => doc,
            Ok(None) => {
                failures.push(format!(
                    "missing guardrail document: {} [{}] {}",
                    contract.doc,
                    contract.file_type.as_deref().unwrap_or("any"),
                    contract.pipeline
                ));
                continue;
            }
            Err(failure) => {
                failures.push(failure);
                continue;
            }
        };
        let Some(pr) = doc.results.iter().find(|pr| pr.pipeline.name() == contract.pipeline) else {
            failures.push(format!(
                "missing guardrail pipeline result: {} [{}] {}",
                contract.doc, doc.file_type, contract.pipeline
            ));
            continue;
        };
        let target = format!("{} [{}] {}", contract.doc, doc.file_type, contract.pipeline);

        failures.extend(check_guardrail_score("SF1", pr.sf1, contract.min_sf1, &target));
        failures.extend(check_guardrail_score("TF1", pr.tf1, contract.min_tf1, &target));

        if let Err(reason) = check_relative_order(&pr.content, &contract.relative_order) {
            failures.push(format!("relative-order regression: {target} {reason}"));
        }
    }

    failures
}

fn resolve_guardrail_document<'a>(
    results: &'a [DocResult],
    contract: &GuardrailContract,
) -> std::result::Result<Option<&'a DocResult>, String> {
    let matching_docs: Vec<&DocResult> = results
        .iter()
        .filter(|result| {
            result.name == contract.doc
                && contract
                    .file_type
                    .as_deref()
                    .is_none_or(|file_type| result.file_type == file_type)
        })
        .collect();
    if matching_docs.len() <= 1 {
        return Ok(matching_docs.first().copied());
    }

    let mut file_types: Vec<&str> = matching_docs.iter().map(|doc| doc.file_type.as_str()).collect();
    file_types.sort_unstable();
    file_types.dedup();
    Err(format!(
        "ambiguous legacy guardrail target: {} {} matches file types {}",
        contract.doc,
        contract.pipeline,
        file_types.join(", ")
    ))
}

fn check_guardrail_score(metric: &str, value: f64, minimum: Option<f64>, target: &str) -> Option<String> {
    let minimum = minimum?;
    if !value.is_finite() {
        return Some(format!("{metric} unavailable: {target} has no finite score"));
    }
    (value < minimum).then(|| {
        format!(
            "{metric} regression: {target} {metric} {:.1}% < minimum {:.1}%",
            value * 100.0,
            minimum * 100.0
        )
    })
}

pub(super) fn check_relative_order(content: &str, anchors: &[String]) -> std::result::Result<(), String> {
    let normalized_content = normalize_order_text(content);
    let mut remainder = normalized_content.as_str();
    for anchor in anchors {
        if anchor.is_empty() {
            return Err("contains an empty anchor".to_string());
        }
        let normalized_anchor = normalize_order_text(anchor);
        let Some(position) = remainder.find(&normalized_anchor) else {
            return Err(format!("missing or out-of-order anchor {anchor:?}"));
        };
        remainder = &remainder[position + normalized_anchor.len()..];
    }
    Ok(())
}

fn normalize_order_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(escaped) = characters.peek()
            && escaped.is_ascii_punctuation()
        {
            normalized.push(characters.next().expect("peeked escaped punctuation"));
            continue;
        }
        match character {
            '\u{00a0}' => normalized.push(' '),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2212}' => normalized.push('-'),
            _ => normalized.push(character),
        }
    }
    normalized
}
