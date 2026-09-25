//! LLM configuration types for liter-llm integration.
//!
//! These types are always available (not feature-gated) since they are
//! pure configuration data with no runtime dependency on liter-llm.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Placeholder printed in place of a credential by the hand-written `Debug` impls
/// in this module. Mirrors liter-llm's own `ClientConfig` debug policy.
const REDACTED: &str = "[redacted]";

/// Minimum accepted value for [`LlmConfig::top_p`], liter-llm's nucleus-sampling
/// probability.
const TOP_P_MIN: f64 = 0.0;

/// Maximum accepted value for [`LlmConfig::top_p`].
const TOP_P_MAX: f64 = 1.0;

/// Minimum accepted value for [`LlmConfig::presence_penalty`] and
/// [`LlmConfig::frequency_penalty`], matching liter-llm's/OpenAI's documented range.
const PENALTY_MIN: f64 = -2.0;

/// Maximum accepted value for [`LlmConfig::presence_penalty`] and
/// [`LlmConfig::frequency_penalty`].
const PENALTY_MAX: f64 = 2.0;

/// Configuration for an LLM provider/model via liter-llm.
///
/// Each feature (VLM OCR, VLM embeddings, structured extraction) carries
/// its own `LlmConfig`, allowing different providers per feature.
///
/// # Example
///
/// ```toml
/// [structured_extraction.llm]
/// model = "openai/gpt-4o"
/// api_key = "sk-..."  # or use XBERG_LLM_API_KEY env var
/// ```
///
/// `Debug` is implemented by hand so `api_key`, header values, and the AWS
/// credentials in [`BedrockConfig`] are never printed.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
pub struct LlmConfig {
    /// Provider/model string using liter-llm routing format.
    ///
    /// Examples: `"openai/gpt-4o"`, `"anthropic/claude-sonnet-4-20250514"`,
    /// `"groq/llama-3.1-70b-versatile"`.
    pub model: String,

    /// API key for the provider. When `None`, liter-llm falls back to
    /// the provider's standard environment variable (e.g., `OPENAI_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Custom base URL override for the provider endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Request timeout in seconds. When `None`, liter-llm's built-in 60s default
    /// applies, except the VLM OCR path which uses a 300s default (a single page
    /// image transcription routinely exceeds 60s). Set explicitly to override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,

    /// Maximum retry attempts (default: 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,

    /// Sampling temperature for generation tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Nucleus sampling parameter for generation tasks, applied to individual
    /// requests built from this config. Restricts sampling to the smallest set of
    /// tokens whose cumulative probability mass is at least this value; lower is
    /// more focused. Validated to `[0.0, 1.0]` by [`LlmConfig::validate`].
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::top_p`. A request-time
    /// parameter like `temperature`/`max_tokens` above, not a client-level
    /// setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub top_p: Option<f64>,

    /// Stop sequence(s) that halt token generation, applied to individual requests
    /// built from this config.
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::stop`
    /// (`types::common::StopSequence`), which liter-llm represents as either a
    /// single string or a list of strings via an untagged enum. Always expressed
    /// here as a list — even one stop sequence is `["..."]` — so the field has a
    /// single, FFI-friendly shape across every language binding instead of a
    /// single-or-list union type. Converted to liter-llm's
    /// `StopSequence::Multiple` at each request-building call site; see
    /// `llm::client::to_stop_sequence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub stop: Option<Vec<String>>,

    /// Random seed for reproducible outputs, applied to individual requests built
    /// from this config. Provider support varies — some silently ignore it.
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::seed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub seed: Option<i64>,

    /// Presence penalty for generation tasks, applied to individual requests
    /// built from this config. Positive values discourage the model from
    /// repeating topics already present in the conversation. Validated to
    /// `[-2.0, 2.0]` by [`LlmConfig::validate`].
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::presence_penalty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub presence_penalty: Option<f64>,

    /// Frequency penalty for generation tasks, applied to individual requests
    /// built from this config. Positive values discourage the model from
    /// repeating the same tokens verbatim. Validated to `[-2.0, 2.0]` by
    /// [`LlmConfig::validate`].
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::frequency_penalty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub frequency_penalty: Option<f64>,

    /// Reasoning effort level for extended-thinking models, applied to individual
    /// requests built from this config.
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::reasoning_effort`
    /// (`types::chat::ReasoningEffort`). A request-time parameter like `temperature`/
    /// `max_tokens` above, not a client-level setting — `into_client_builder` does not
    /// map it. Accepted as a plain string — one of `"low"`, `"medium"`, `"high"`,
    /// `"minimal"`, `"max"` (case-insensitive; liter-llm's own
    /// `#[serde(rename_all = "lowercase")]` spelling) — rather than importing
    /// liter-llm's enum, because this module compiles even when the `liter-llm`
    /// feature is disabled. See `llm::client::parse_reasoning_effort` for the
    /// conversion into `liter_llm::ReasoningEffort`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub reasoning_effort: Option<String>,

    /// Provider-specific extra parameters merged into the request body (guardrails,
    /// safety settings, grounding config, etc.), applied to individual requests built
    /// from this config.
    ///
    /// Mirrors liter-llm's `ChatCompletionRequest::extra_body`. A request-time
    /// parameter like `temperature`/`max_tokens` above, not a client-level setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub extra_body: Option<serde_json::Value>,

    /// Whether liter-llm should load provider credentials from environment variables.
    ///
    /// Mirrors liter-llm's `ClientConfigBuilder::load_env`. When `None`, liter-llm's
    /// own default behavior applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_env: Option<bool>,

    /// Extra HTTP headers sent with every request to the provider.
    ///
    /// Mirrors liter-llm's `ClientConfigBuilder::header`, for gateways or providers
    /// that require custom auth/routing headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    /// Custom provider configurations, in addition to liter-llm's built-in providers.
    ///
    /// Mirrors liter-llm's `LlmConfig::providers`, for OpenAI-compatible gateways
    /// and self-hosted model servers that are not in the built-in provider catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub providers: Option<Vec<LlmProviderConfig>>,

    /// Response cache configuration.
    ///
    /// Mirrors liter-llm's `LlmConfig::cache`. Only takes effect when liter-llm's
    /// `tower` feature is compiled in; otherwise the value is accepted but unused.
    ///
    /// Boxed for the same reason as `bedrock` (a4579589ac): `LlmConfig` is the payload
    /// of `EmbeddingModelType::Llm` and `RerankerModelType::Llm`, whose other variants
    /// are tens of bytes. Inlining this and the two sub-configs below pushed that
    /// variant to 480 bytes and tripped `clippy::large_enum_variant` on the
    /// `--features full` leg. ~keep
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub cache: Option<Box<LlmCacheConfig>>,

    /// Budget enforcement configuration.
    ///
    /// Mirrors liter-llm's `LlmConfig::budget`. Only takes effect when liter-llm's
    /// `tower` feature is compiled in; otherwise the value is accepted but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub budget: Option<Box<LlmBudgetConfig>>,

    /// Per-model rate limiting configuration.
    ///
    /// Mirrors liter-llm's `LlmConfig::rate_limit`. Only takes effect when liter-llm's
    /// `tower` feature is compiled in; otherwise the value is accepted but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub rate_limit: Option<Box<LlmRateLimitConfig>>,

    /// Enable per-request cost tracking.
    ///
    /// Mirrors liter-llm's `LlmConfig::cost_tracking`. Only takes effect when
    /// liter-llm's `tower` feature is compiled in; otherwise the value is accepted
    /// but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub cost_tracking: Option<bool>,

    /// Enable OpenTelemetry-compatible tracing spans.
    ///
    /// Mirrors liter-llm's `LlmConfig::tracing`. Only takes effect when liter-llm's
    /// `tower` feature is compiled in; otherwise the value is accepted but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub tracing: Option<bool>,

    /// Cooldown duration after transient errors, in seconds.
    ///
    /// Mirrors liter-llm's `LlmConfig::cooldown_secs`. Only takes effect when
    /// liter-llm's `tower` feature is compiled in; otherwise the value is accepted
    /// but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub cooldown_secs: Option<u64>,

    /// Background health check interval, in seconds.
    ///
    /// Mirrors liter-llm's `LlmConfig::health_check_secs`. Only takes effect when
    /// liter-llm's `tower` feature is compiled in; otherwise the value is accepted
    /// but unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub health_check_secs: Option<u64>,

    /// AWS Bedrock settings (region, cross-region routing, explicit credentials).
    ///
    /// Only consulted for `bedrock/`-prefixed models. When `None` — or when an
    /// individual field inside it is `None` — liter-llm falls back to the standard
    /// AWS environment variables and the default credential chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub bedrock: Option<Box<BedrockConfig>>,

    /// Managed OAuth2/STS credential provider for auth modes liter-llm cannot express via a
    /// static `api_key` — Azure AD, Vertex AI OAuth2, Vertex AI Application Default
    /// Credentials, and AWS STS Web Identity (EKS IRSA) for Bedrock.
    ///
    /// Mirrors liter-llm's `client::ClientConfigBuilder::credential_provider`, which takes an
    /// `Arc<dyn liter_llm::auth::CredentialProvider>` trait object — that cannot appear in a
    /// serde DTO. Every [`CredentialProviderConfig`] variant is plain data instead, so it
    /// round-trips through TOML/JSON/YAML and every language binding like the rest of
    /// `LlmConfig`.
    ///
    /// Managed credential providers are unavailable on `wasm32`, where liter-llm uses
    /// browser HTTP rather than its native authentication modules. [`LlmConfig::validate`]
    /// rejects a configured provider on that target instead of silently ignoring it.
    ///
    /// GitHub Copilot's device-flow provider has no variant here: it takes no configuration at
    /// all (`liter_llm::auth::github_copilot::GithubCopilotCredentialProvider::new` accepts only
    /// an HTTP client) and drives an interactive terminal prompt, so it cannot be expressed as
    /// data. A Rust embedder who needs it — or any other fully custom `CredentialProvider` — can
    /// call `xberg::llm::client::create_client_with_credential_provider` directly with a
    /// `liter-llm` dependency of their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub credential_provider: Option<Box<CredentialProviderConfig>>,

    /// Maximum number of simultaneously in-flight requests to the LLM provider this
    /// config resolves to.
    ///
    /// This is a real, global bound on *provider* concurrency, not a per-extraction
    /// allowance: `xberg::llm::client::create_client` shares one process-wide client
    /// instance per distinct resolved config, so every concurrent extraction that
    /// resolves to the same config shares the one in-flight-request limit this value
    /// configures, instead of each minting its own (GH#1465). `None` means unlimited.
    ///
    /// PDF and image OCR batch sizing are **not** derived from this field, even when the
    /// configured OCR backend or `vlm_fallback` policy can reach a VLM — those call sites
    /// mix CPU-bound raster/OCR work with, at most, occasional remote requests, so they
    /// size their batches from the general thread budget
    /// ([`super::ConcurrencyConfig::max_threads`]) unconditionally (GH#1465). Captioning is
    /// the one feature that *additionally* uses this value to bound its own
    /// per-extraction async request fan-out (issuing only VLM requests, with no CPU
    /// batching to protect), on top of the global provider-side limit described above;
    /// that per-extraction bound clamps a value below 1 up to 1. The global provider-side
    /// limit does not clamp: `Some(0)` reaches liter-llm as-is, and liter-llm rejects it
    /// when building the client (zero permitted in-flight requests is never useful), so it
    /// surfaces as a `create_client` error rather than a silent clamp to 1.
    ///
    /// This field is intentionally last to preserve positional constructor
    /// compatibility in generated language bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
    pub max_concurrency: Option<usize>,

    /// Maximum size, in bytes, of a single HTTP response body read from the LLM
    /// provider.
    ///
    /// Bounds response bodies on every non-streaming call (chat completions,
    /// embeddings, model listings, …) and the error body read on a failed
    /// request; a successful streaming response keeps its own existing frame
    /// bounds and is unaffected. `None` (the default) means unbounded, matching
    /// liter-llm's own default.
    ///
    /// Mirrors liter-llm's `client::ClientConfigBuilder::max_response_bytes`, which
    /// is native-only (`native-http`, non-`wasm32`) — see
    /// `llm::client::build_client_config`. `Some(0)` is rejected by
    /// [`LlmConfig::validate`] rather than reaching liter-llm, which would refuse it
    /// at client-build time with the same complaint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "alef-meta", alef(since = "1.2.3"))]
    pub max_response_bytes: Option<usize>,
}

impl LlmConfig {
    /// Validate the request-time sampling parameters that have a documented range:
    /// `top_p` (`[0.0, 1.0]`), `presence_penalty`, and `frequency_penalty` (both
    /// `[-2.0, 2.0]`, matching liter-llm's/OpenAI's semantics). An unset field is
    /// always valid — silence in config should never be rejected.
    ///
    /// Called from `llm::client::build_client_config` before a liter-llm
    /// client is built from this config, alongside the existing
    /// `validate_cache_backend` check in that function.
    pub fn validate(&self) -> crate::Result<()> {
        self.validate_sampling_parameters()?;
        validate_response_byte_cap(self.max_response_bytes)?;
        #[cfg(target_arch = "wasm32")]
        validate_wasm_credential_provider(self.credential_provider.as_deref())?;
        Ok(())
    }

    fn validate_sampling_parameters(&self) -> crate::Result<()> {
        if let Some(top_p) = self.top_p {
            validate_sampling_range("top_p", top_p, TOP_P_MIN, TOP_P_MAX)?;
        }
        if let Some(presence_penalty) = self.presence_penalty {
            validate_sampling_range("presence_penalty", presence_penalty, PENALTY_MIN, PENALTY_MAX)?;
        }
        if let Some(frequency_penalty) = self.frequency_penalty {
            validate_sampling_range("frequency_penalty", frequency_penalty, PENALTY_MIN, PENALTY_MAX)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn validate_for_wasm_target(&self) -> crate::Result<()> {
        self.validate_sampling_parameters()?;
        validate_response_byte_cap(self.max_response_bytes)?;
        validate_wasm_credential_provider(self.credential_provider.as_deref())
    }
}

/// Reject `max_response_bytes: Some(0)` at config validation, before
/// `llm::client::build_client_config` ever reaches liter-llm — liter-llm's own
/// `ClientConfigBuilder::max_response_bytes` refuses a zero limit for the same reason
/// (a zero-byte cap could never read any response), so this surfaces the identical
/// failure earlier, against the field name a caller actually set. An unset limit is
/// always valid.
fn validate_response_byte_cap(limit: Option<usize>) -> crate::Result<()> {
    if limit == Some(0) {
        return Err(crate::XbergError::Validation {
            message: "Invalid LLM max_response_bytes 0: must be nonzero".to_string(),
            source: None,
        });
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
const WASM_CREDENTIAL_PROVIDER_ERROR: &str =
    "credential_provider is not supported on wasm32 targets; use api_key or browser-compatible authentication";

#[cfg(any(target_arch = "wasm32", test))]
fn validate_wasm_credential_provider(provider: Option<&CredentialProviderConfig>) -> crate::Result<()> {
    if provider.is_some() {
        return Err(crate::XbergError::validation(
            WASM_CREDENTIAL_PROVIDER_ERROR.to_string(),
        ));
    }
    Ok(())
}

/// Reject a request-time sampling parameter outside its documented `[min, max]` range,
/// naming the field, the offending value, and the accepted range in the error message.
fn validate_sampling_range(field_name: &str, value: f64, min: f64, max: f64) -> crate::Result<()> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(crate::XbergError::Validation {
            message: format!("Invalid LLM {field_name} {value}: expected a value between {min} and {max}"),
            source: None,
        })
    }
}

/// Managed credential-provider configuration for OAuth2/STS-based authentication modes liter-llm
/// cannot express via a static `api_key`. See [`LlmConfig::credential_provider`].
///
/// `Debug` is implemented by hand: [`CredentialProviderConfig::AzureAd`]'s `client_secret` is a
/// credential and must never be printed, matching [`LlmConfig`]'s own redaction policy. The
/// other variants carry no secret material — [`CredentialProviderConfig::VertexOauth2`] and
/// [`CredentialProviderConfig::BedrockWebIdentity`] reference a *file path*, never the key or
/// token itself.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub enum CredentialProviderConfig {
    /// Azure AD OAuth2 client-credentials flow (Azure OpenAI / Azure Cognitive Services).
    AzureAd {
        /// Azure AD tenant ID.
        tenant_id: String,
        /// Application (client) ID.
        client_id: String,
        /// Client secret value. Secret — never logged.
        client_secret: String,
        /// OAuth2 scope. Defaults to liter-llm's own
        /// `https://cognitiveservices.azure.com/.default` when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Google Vertex AI OAuth2 via a service-account JSON key file on disk.
    ///
    /// Points at a file path rather than embedding the key inline: the key file contains an
    /// RSA private key — stronger secret material than an API key — and `LlmConfig` must never
    /// carry that directly, matching the credential-handling policy the rest of this module
    /// follows.
    VertexOauth2 {
        /// Path to a Google service-account JSON key file (the same file
        /// `GOOGLE_APPLICATION_CREDENTIALS` would point to).
        service_account_key_file: String,
        /// OAuth2 scope. Defaults to liter-llm's own Vertex AI scope when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Google Vertex AI Application Default Credentials, resolved from the GCE/GKE/Cloud Run
    /// metadata server. Carries no secret material at all.
    VertexAdc {
        /// OAuth2 scope. Defaults to liter-llm's own Vertex AI scope when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// AWS STS `AssumeRoleWithWebIdentity` (EKS IRSA / OIDC federation) for Bedrock.
    BedrockWebIdentity {
        /// ARN of the IAM role to assume.
        role_arn: String,
        /// Path to a file containing the OIDC JWT (the same file
        /// `AWS_WEB_IDENTITY_TOKEN_FILE` would point to).
        token_file: String,
        /// STS session name. Defaults to liter-llm's own default (`"liter-llm-session"`) when
        /// unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_name: Option<String>,
        /// AWS region. Defaults to liter-llm's own default (`"us-east-1"`) when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
    },
}

impl std::fmt::Debug for CredentialProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AzureAd {
                tenant_id,
                client_id,
                scope,
                client_secret: _,
            } => f
                .debug_struct("AzureAd")
                .field("tenant_id", tenant_id)
                .field("client_id", client_id)
                .field("client_secret", &REDACTED)
                .field("scope", scope)
                .finish(),
            Self::VertexOauth2 {
                service_account_key_file,
                scope,
            } => f
                .debug_struct("VertexOauth2")
                .field("service_account_key_file", service_account_key_file)
                .field("scope", scope)
                .finish(),
            Self::VertexAdc { scope } => f.debug_struct("VertexAdc").field("scope", scope).finish(),
            Self::BedrockWebIdentity {
                role_arn,
                token_file,
                session_name,
                region,
            } => f
                .debug_struct("BedrockWebIdentity")
                .field("role_arn", role_arn)
                .field("token_file", token_file)
                .field("session_name", session_name)
                .field("region", region)
                .finish(),
        }
    }
}

/// A custom provider configuration entry, in addition to liter-llm's built-in providers.
///
/// Mirrors liter-llm's `LlmProviderConfig`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub struct LlmProviderConfig {
    /// Provider name, used to key model prefix matching.
    pub name: String,

    /// Base URL for the provider's OpenAI-compatible API.
    pub base_url: String,

    /// Header name used to carry the API key (defaults to `Authorization` when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_header: Option<String>,

    /// Model name prefixes routed to this provider (e.g. `["my-provider/"]`).
    #[serde(default)]
    pub model_prefixes: Vec<String>,
}

/// Response cache configuration.
///
/// Mirrors liter-llm's `LlmCacheConfig`. Only takes effect when liter-llm's `tower`
/// feature is compiled in; otherwise the value round-trips through configuration
/// but is not consulted at request time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub struct LlmCacheConfig {
    /// Maximum number of cached entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_entries: Option<usize>,

    /// Cache entry time-to-live, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,

    /// Cache backend name (e.g. `"memory"`, or an `opendal` scheme).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,

    /// Backend-specific configuration key/value pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_config: Option<HashMap<String, String>>,
}

/// Budget enforcement configuration.
///
/// Mirrors liter-llm's `LlmBudgetConfig`. Only takes effect when liter-llm's `tower`
/// feature is compiled in; otherwise the value round-trips through configuration
/// but is not enforced at request time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub struct LlmBudgetConfig {
    /// Global spend limit in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_limit: Option<f64>,

    /// Per-model spend limits in USD, keyed by model name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_limits: Option<HashMap<String, f64>>,

    /// Enforcement mode: `"hard"` (reject over-budget requests) or `"soft"` (log only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<String>,
}

/// Per-model rate limiting configuration.
///
/// Mirrors liter-llm's `LlmRateLimitConfig`. Only takes effect when liter-llm's
/// `tower` feature is compiled in; otherwise the value round-trips through
/// configuration but is not enforced at request time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub struct LlmRateLimitConfig {
    /// Requests per minute limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<u32>,

    /// Tokens per minute limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<u64>,

    /// Rate limit window, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<u64>,
}

/// AWS Bedrock configuration for `bedrock/`-prefixed models.
///
/// Mirrors liter-llm's `BedrockConfig`. Every field is optional: anything left
/// unset falls back to the standard AWS environment variables
/// (`AWS_DEFAULT_REGION` / `AWS_REGION`, `AWS_ACCESS_KEY_ID`,
/// `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `BEDROCK_CROSS_REGION`) and the
/// default AWS credential chain. Leave the credential fields unset unless you
/// have an explicit reason to pin them.
///
/// # Example
///
/// ```toml
/// [ocr.vlm_config]
/// model = "bedrock/anthropic.claude-3-sonnet-20240229-v1:0"
///
/// [ocr.vlm_config.bedrock]
/// region = "eu-central-1"
/// cross_region_prefix = "eu"
/// ```
///
/// `Debug` is implemented by hand so the three credential fields are never printed.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "alef-meta", alef(since = "1.1.0"))]
pub struct BedrockConfig {
    /// AWS region (e.g. `"us-east-1"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// Cross-region inference profile prefix (e.g. `"us"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_region_prefix: Option<String>,

    /// Explicit AWS access key ID. Secret — never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,

    /// Explicit AWS secret access key. Secret — never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,

    /// Explicit AWS session token for temporary credentials. Secret — never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

/// Redacting `Debug`: `api_key` and every header value are credentials, so only
/// their presence (and, for headers, the header name) is printed.
impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_headers: Option<Vec<(&str, &str)>> = self
            .headers
            .as_ref()
            .map(|headers| headers.keys().map(|name| (name.as_str(), REDACTED)).collect());

        f.debug_struct("LlmConfig")
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| REDACTED))
            .field("base_url", &self.base_url)
            .field("timeout_secs", &self.timeout_secs)
            .field("max_retries", &self.max_retries)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("top_p", &self.top_p)
            .field("stop", &self.stop)
            .field("seed", &self.seed)
            .field("presence_penalty", &self.presence_penalty)
            .field("frequency_penalty", &self.frequency_penalty)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("extra_body", &self.extra_body)
            .field("load_env", &self.load_env)
            .field("headers", &redacted_headers)
            .field("providers", &self.providers)
            .field("cache", &self.cache)
            .field("budget", &self.budget)
            .field("rate_limit", &self.rate_limit)
            .field("cost_tracking", &self.cost_tracking)
            .field("tracing", &self.tracing)
            .field("cooldown_secs", &self.cooldown_secs)
            .field("health_check_secs", &self.health_check_secs)
            .field("bedrock", &self.bedrock)
            .field("credential_provider", &self.credential_provider)
            .field("max_concurrency", &self.max_concurrency)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

/// Redacting `Debug`: the three AWS credential fields print only their presence.
/// `region` and `cross_region_prefix` are not secrets and are printed verbatim.
impl std::fmt::Debug for BedrockConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BedrockConfig")
            .field("region", &self.region)
            .field("cross_region_prefix", &self.cross_region_prefix)
            .field("access_key_id", &self.access_key_id.as_ref().map(|_| REDACTED))
            .field("secret_access_key", &self.secret_access_key.as_ref().map(|_| REDACTED))
            .field("session_token", &self.session_token.as_ref().map(|_| REDACTED))
            .finish()
    }
}

/// Configuration for LLM-based structured data extraction.
///
/// Sends extracted document content to a VLM with a JSON schema,
/// returning structured data that conforms to the schema.
///
/// # Example
///
/// ```toml
/// [structured_extraction]
/// schema_name = "invoice_data"
/// strict = true
///
/// [structured_extraction.schema]
/// type = "object"
/// properties.vendor = { type = "string" }
/// properties.total = { type = "number" }
/// required = ["vendor", "total"]
///
/// [structured_extraction.llm]
/// model = "openai/gpt-4o"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredExtractionConfig {
    /// JSON Schema defining the desired output structure.
    pub schema: serde_json::Value,

    /// Schema name passed to the LLM's structured output mode.
    #[serde(default = "StructuredExtractionConfig::default_schema_name")]
    pub schema_name: String,

    /// Optional schema description for the LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_description: Option<String>,

    /// Enable strict mode — output must exactly match the schema.
    #[serde(default)]
    pub strict: bool,

    /// Custom Jinja2 extraction prompt template. When `None`, a default template is used.
    ///
    /// Available template variables:
    /// - `{{ content }}` — The extracted document text.
    /// - `{{ schema }}` — The JSON schema as a formatted string.
    /// - `{{ schema_name }}` — The schema name.
    /// - `{{ schema_description }}` — The schema description (may be empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// LLM configuration for the extraction.
    pub llm: LlmConfig,
}

impl StructuredExtractionConfig {
    /// Default [`Self::schema_name`] passed to the LLM's structured-output mode.
    ///
    /// Public and on the type rather than a free private `fn` because generated bindings
    /// have to call it to reproduce the default, and a private one is out of their reach.
    pub fn default_schema_name() -> String {
        "extraction".to_string()
    }
}

/// How a structured-extraction preset is dispatched to the model.
///
/// This is the preset-facing call mode (the `preferred_call_mode` field of a
/// `Preset`). The structured pipeline has a richer
/// runtime-only decision enum with skip and fallback states; this 3-variant
/// type is the stable, serializable surface presets and bindings depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CallMode {
    /// Use the extracted text only.
    #[default]
    TextOnly,
    /// Use rasterized page images only.
    VisionOnly,
    /// Provide both extracted text and page images to the model.
    TextPlusVision,
}

/// How partial results from multiple model calls (e.g. per page batch) are combined.
///
/// Canonical home for the merge strategy referenced by presets and by the
/// structured pipeline's post-processing. There is intentionally only one merge
/// type across the crate — do not introduce a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    /// Deep-merge JSON objects field by field (later calls fill missing fields).
    #[default]
    ObjectMerge,
    /// Concatenate top-level arrays across calls.
    ArrayConcat,
    /// Keep the first non-empty result; ignore subsequent calls.
    ObjectFirst,
}

#[cfg(test)]
mod tests;
