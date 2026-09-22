//! Resolving and propagating the shared `--api-key` / `XBERG_LLM_API_KEY` LLM API key.

use xberg::{ExtractionConfig, LlmConfig};

/// Resolve the LLM API key the CLI should propagate to every `LlmConfig` slot.
///
/// Precedence (highest first):
/// 1. The `--api-key` CLI flag (`cli_api_key`).
/// 2. The `XBERG_LLM_API_KEY` environment variable.
/// 3. `None` — keep whatever the loaded config / inline JSON / overrides set.
///
/// Returns `None` when neither the CLI flag nor the environment variable
/// supplies a non-empty value. In that case [`apply_llm_api_key`] is not
/// called and liter-llm's per-provider env-var fallback runs at request time.
///
/// The resolved source is logged at `info!` level; the key value itself is
/// never logged.
pub(crate) fn resolve_llm_api_key(cli_api_key: Option<&str>) -> Option<String> {
    if let Some(key) = cli_api_key.map(str::trim).filter(|s| !s.is_empty()) {
        tracing::info!(source = "cli_flag", "Resolved LLM API key from --api-key flag");
        return Some(key.to_string());
    }
    if let Ok(value) = std::env::var("XBERG_LLM_API_KEY")
        && !value.is_empty()
    {
        tracing::info!(source = "xberg_env", "Resolved LLM API key from XBERG_LLM_API_KEY");
        return Some(value);
    }
    None
}

/// Write `key` into every [`LlmConfig`] field of `config` whose `api_key` is
/// `None`. Existing non-`None` values (from the loaded config file or inline
/// JSON) take precedence over the resolved key — the CLI never silently
/// overrides explicit configuration.
pub(crate) fn apply_llm_api_key(config: &mut ExtractionConfig, key: &str) {
    fn fill(slot: &mut LlmConfig, key: &str) {
        if slot.api_key.is_none() {
            slot.api_key = Some(key.to_string());
        }
    }

    if let Some(ocr) = config.ocr.as_mut()
        && let Some(vlm) = ocr.vlm_config.as_mut()
    {
        fill(vlm, key);
    }

    if let Some(ext) = config.structured_extraction.as_mut() {
        fill(&mut ext.llm, key);
    }

    if let Some(chunking) = config.chunking.as_mut()
        && let Some(embedding) = chunking.embedding.as_mut()
        && let xberg::EmbeddingModelType::Llm { llm } = &mut embedding.model
    {
        fill(llm, key);
    }

    if let Some(translation) = config.translation.as_mut() {
        fill(&mut translation.llm, key);
    }

    if let Some(pc) = config.page_classification.as_mut() {
        fill(&mut pc.llm, key);
    }

    if let Some(cap) = config.captioning.as_mut() {
        fill(&mut cap.llm, key);
    }

    if let Some(sum) = config.summarization.as_mut()
        && let Some(llm) = sum.llm.as_mut()
    {
        fill(llm, key);
    }

    if let Some(ner) = config.ner.as_mut()
        && let Some(llm) = ner.llm.as_mut()
    {
        fill(llm, key);
    }
}
