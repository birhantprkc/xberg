//! Embed command implementation.

use anyhow::{Context, Result};

use crate::{WireFormat, style};

/// Provider selection and credentials for [`embed_command`], bundled so the command
/// function itself stays within the CLI's parameter-count limit.
pub struct EmbedProviderOptions {
    pub preset: String,
    pub provider: String,
    pub llm_model: Option<String>,
    pub llm_api_key: Option<String>,
    pub plugin_name: Option<String>,
}

/// Validate that every input text is present and non-empty.
fn validate_texts(texts: &[String]) -> Result<()> {
    if texts.is_empty() {
        anyhow::bail!("No texts provided for embedding. Provide --text or pipe text via stdin.");
    }
    for (i, t) in texts.iter().enumerate() {
        if t.is_empty() {
            anyhow::bail!("Text at position {} is empty. All texts must be non-empty.", i + 1);
        }
    }
    Ok(())
}

/// Build the `llm` provider's embedding config from `--model`/`--api-key`.
fn resolve_llm_config(
    llm_model: Option<&str>,
    llm_api_key: Option<String>,
) -> Result<(xberg::EmbeddingConfig, String)> {
    let model = llm_model.ok_or_else(|| {
        anyhow::anyhow!("--model is required when --provider is 'llm' (e.g., --model openai/text-embedding-3-small)")
    })?;

    let llm_config = xberg::LlmConfig {
        model: model.to_string(),
        api_key: llm_api_key,
        ..Default::default()
    };

    let config = xberg::EmbeddingConfig {
        model: xberg::EmbeddingModelType::Llm {
            llm: Box::new(llm_config),
        },
        show_download_progress: true,
        ..Default::default()
    };

    Ok((config, model.to_string()))
}

/// Build the `local` (ONNX preset) provider's embedding config from `--preset`.
fn resolve_local_config(preset: &str) -> Result<(xberg::EmbeddingConfig, String)> {
    let _preset_info = xberg::get_embedding_preset(preset).with_context(|| {
        format!(
            "Unknown embedding preset '{}'. Available: {:?}",
            preset,
            xberg::list_embedding_presets()
        )
    })?;

    let config = xberg::EmbeddingConfig {
        model: xberg::EmbeddingModelType::Preset {
            name: preset.to_string(),
        },
        show_download_progress: true,
        ..Default::default()
    };

    Ok((config, preset.to_string()))
}

/// Build the `plugin` provider's embedding config from `--plugin`, validating that the
/// named backend is registered.
fn resolve_plugin_config(plugin_name: Option<&str>) -> Result<(xberg::EmbeddingConfig, String)> {
    let name = plugin_name.ok_or_else(|| {
        anyhow::anyhow!(
            "--plugin NAME is required when --provider is 'plugin'. Register a backend via xberg::plugins::register_embedding_backend first."
        )
    })?;
    if name.is_empty() {
        anyhow::bail!("--plugin NAME must not be empty.");
    }

    let available = xberg::plugins::list_embedding_backends().context("Failed to read embedding backend registry")?;
    if !available.iter().any(|n| n == name) {
        anyhow::bail!(
            "Embedding backend '{}' is not registered. Available backends: {}",
            name,
            if available.is_empty() {
                "(none registered)".to_string()
            } else {
                available.join(", ")
            }
        );
    }

    let config = xberg::EmbeddingConfig {
        model: xberg::EmbeddingModelType::Plugin { name: name.to_string() },
        ..Default::default()
    };

    Ok((config, name.to_string()))
}

/// Resolve `options` into an [`xberg::EmbeddingConfig`] plus a human-readable model label,
/// dispatching on `options.provider`.
fn resolve_embedding_config(options: &EmbedProviderOptions) -> Result<(xberg::EmbeddingConfig, String)> {
    match options.provider.as_str() {
        "llm" => resolve_llm_config(options.llm_model.as_deref(), options.llm_api_key.clone()),
        "local" | "" => resolve_local_config(&options.preset),
        "plugin" => resolve_plugin_config(options.plugin_name.as_deref()),
        other => anyhow::bail!(
            "Unknown embedding provider '{}'. Valid providers: 'local' (default, ONNX), 'llm' (liter-llm), or 'plugin' (in-process backend).",
            other
        ),
    }
}

/// Print the generated embeddings in the requested wire format.
#[expect(
    clippy::print_stdout,
    reason = "embedding vectors are the command's stdout result output"
)]
fn print_embeddings(format: WireFormat, embeddings: &[Vec<f32>], model_label: &str, text_count: usize) -> Result<()> {
    let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);

    match format {
        WireFormat::Json => {
            let output = serde_json::json!({
                "embeddings": embeddings,
                "model": model_label,
                "dimensions": dimensions,
                "count": embeddings.len(),
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).context("Failed to serialize embeddings to JSON")?
            );
        }
        WireFormat::Toon => {
            let output = serde_json::json!({
                "embeddings": embeddings,
                "model": model_label,
                "dimensions": dimensions,
                "count": embeddings.len(),
            });
            println!(
                "{}",
                serde_toon::to_string(&output).context("Failed to serialize embeddings to TOON")?
            );
        }
        WireFormat::Text => {
            for (i, embedding) in embeddings.iter().enumerate() {
                if text_count > 1 {
                    println!("{}", style::dim(&format!("# text {}", i + 1)));
                }
                let values: Vec<String> = embedding.iter().map(|v| format!("{v}")).collect();
                println!("{}", values.join(","));
            }
        }
    }

    Ok(())
}

/// Execute the embed command: generate embeddings for input texts.
///
/// When `options.provider` is `"local"` (default), uses the ONNX preset model.
/// When `options.provider` is `"llm"`, uses liter-llm with the specified model and API key.
/// When `options.provider` is `"plugin"`, dispatches to a pre-registered in-process embedding backend.
pub fn embed_command(texts: Vec<String>, options: EmbedProviderOptions, format: WireFormat) -> Result<()> {
    validate_texts(&texts)?;

    let (config, model_label) = resolve_embedding_config(&options)?;

    let embeddings = xberg::embed_texts(texts.clone(), &config).context("Failed to generate embeddings")?;

    print_embeddings(format, &embeddings, &model_label, texts.len())
}
