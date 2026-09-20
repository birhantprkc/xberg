//! DeepSeek-OCR backend plugin for the Xberg OCR pipeline.
//!
//! This module wraps the candle-based DeepSeek-OCR engine in the `OcrBackend`
//! trait, making it available to the extraction pipeline.
//!
//! # Engine pool design
//!
//! Calls with identical engine configuration share an engine instance to avoid
//! redundant weight loading.

use async_trait::async_trait;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use crate::Result;
use crate::candle_ocr::config::{
    CandleDeepseekOcrDtype, DeepseekOcrBackendOptions, parse_backend_options, validate_optional_non_empty,
};
use crate::core::config::OcrConfig;
use crate::engine_cache::EngineCache;
use crate::plugins::{OcrBackend, OcrBackendType, Plugin};
use crate::types::ExtractedDocument;
use xberg_candle_ocr::DType;
use xberg_candle_ocr::Device;
use xberg_candle_ocr::DevicePreference;
use xberg_candle_ocr::models::DeepseekOCREngine;

/// Pick the floating-point precision that matches the actual compute device.
///
/// A BF16 checkpoint loaded as F32 doubles the weight footprint for no accuracy benefit -- on
/// an L4 (24 GB) this was the 16.5 GB reported in #1674 for a 3B-parameter model whose weights
/// are 6.67 GB in their native BF16 form. Metal's BF16 kernel coverage is incomplete, so F16 is
/// the safe default there instead; CPU inference stays F32 for broad portability.
fn default_dtype_for(device: &Device) -> DType {
    match device {
        Device::Cuda(_) => DType::BF16,
        Device::Metal(_) => DType::F16,
        Device::Cpu => DType::F32,
    }
}

/// Map a parsed `backend_options.dtype` request to a concrete [`DType`]. `Auto` defers to
/// [`default_dtype_for`], so it returns `None` here.
fn requested_dtype(value: Option<CandleDeepseekOcrDtype>) -> Option<DType> {
    match value {
        None | Some(CandleDeepseekOcrDtype::Auto) => None,
        Some(CandleDeepseekOcrDtype::F32) => Some(DType::F32),
        Some(CandleDeepseekOcrDtype::F16) => Some(DType::F16),
        Some(CandleDeepseekOcrDtype::Bf16) => Some(DType::BF16),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EnginePoolKey {
    preference: DevicePreference,
    dtype: DType,
    model_path: std::path::PathBuf,
    version: usize,
}

impl EnginePoolKey {
    fn new(preference: DevicePreference, dtype: DType, model_path: &str, version: usize) -> Self {
        Self {
            preference,
            dtype,
            model_path: model_path.into(),
            version,
        }
    }
}

/// Pooled engine value: shared reference with interior mutability for the engine.
type PooledEngine = Arc<parking_lot::Mutex<DeepseekOCREngine>>;

static ENGINE_POOL: LazyLock<EngineCache<EnginePoolKey, parking_lot::Mutex<DeepseekOCREngine>>> =
    LazyLock::new(EngineCache::unbounded);

fn get_or_init_engine(
    preference: DevicePreference,
    device: Device,
    dtype: DType,
    model_path: &str,
    version: usize,
) -> crate::Result<PooledEngine> {
    let key = EnginePoolKey::new(preference, dtype, model_path, version);

    ENGINE_POOL.get_or_try_init(key, || {
        tracing::info!(
            preference = ?preference,
            dtype = ?dtype,
            model_path = %model_path,
            "Initialising DeepSeek-OCR engine (cold start)"
        );

        let new_engine =
            DeepseekOCREngine::init(model_path, device, dtype, version).map_err(|e| crate::XbergError::Ocr {
                message: format!("DeepSeek-OCR engine initialisation failed: {e}"),
                source: Some(Box::new(e)),
            })?;
        Ok(parking_lot::Mutex::new(new_engine))
    })
}

/// Default HuggingFace repo id for DeepSeek-OCR weights: the original
/// `deepseek-ai/DeepSeek-OCR` repository, pinned to an immutable, checksum-verified
/// revision (see `model_stager::DEEPSEEK_OCR`). Used when `backend_options` provides
/// neither `model_path` nor a custom `model_id`.
const DEFAULT_MODEL_ID: &str = "deepseek-ai/DeepSeek-OCR";

/// DeepSeek-OCR backend using candle transformers.
///
/// A vision-language model combining SAM vision encoder, ViT/Qwen2 vision
/// transformer, CLIP projection, and language decoder for multimodal OCR.
///
/// # Configuration
///
/// DeepSeek-OCR accepts backend options for weight source, device, version, and dtype:
/// ```json
/// {
///   "device": "auto",
///   "model_id": "deepseek-ai/DeepSeek-OCR",
///   "version": 2,
///   "dtype": "auto"
/// }
/// ```
///
/// - `device` (string): `"auto"` (default), `"cpu"`, `"cuda"`, `"metal"`
/// - `model_id` (string): HuggingFace repo id to auto-download weights from. Defaults to
///   `deepseek-ai/DeepSeek-OCR`, pinned to a checksum-verified revision. Ignored when
///   `model_path` is set.
/// - `model_path` (string, optional): path to a local model directory. Takes precedence
///   over `model_id`. When omitted, the weights named by `model_id` are downloaded on
///   first use into the standard Hugging Face cache -- no manual staging required.
/// - `hf_revision` (string, optional): immutable commit for a custom `model_id`. The
///   default model is pinned automatically.
/// - `cache_dir` (string, optional): explicit Hugging Face Hub cache root. When omitted,
///   `HF_HUB_CACHE`, `HUGGINGFACE_HUB_CACHE`, and `HF_HOME` are honored.
/// - `version` (integer): model version `1` or `2` (default: `2`)
/// - `dtype` (string): `"auto"` (default, picks BF16 on CUDA / F16 on Metal / F32 on CPU),
///   `"f32"`, `"f16"`, or `"bf16"`. A dtype with no kernel on the selected device fails the
///   load hard rather than silently falling back -- this is not a tuning knob.
#[cfg_attr(alef, alef(skip))]
pub struct DeepseekOcrBackend {
    dtype: Option<DType>,
}

/// Parsed, defaulted `candle-deepseek-ocr` backend configuration for a single call.
#[derive(Debug)]
struct DeepseekOcrOptions {
    model_path: Option<String>,
    model_id: String,
    hf_revision: Option<String>,
    cache_dir: Option<PathBuf>,
    device: DevicePreference,
    version: usize,
    /// `None` defers to [`default_dtype_for`] once the device is resolved.
    dtype: Option<DType>,
}

impl DeepseekOcrBackend {
    /// Create a new DeepSeek-OCR backend.
    ///
    /// The data type is auto-selected per compute device (see [`default_dtype_for`]) unless
    /// overridden by [`DeepseekOcrBackend::with_dtype`] or a `backend_options.dtype` request,
    /// which takes precedence over both.
    pub fn new() -> Self {
        Self { dtype: None }
    }

    /// Force a specific floating-point precision regardless of device.
    ///
    /// A per-call `backend_options.dtype` request still takes precedence over this
    /// constructor-level override.
    pub fn with_dtype(mut self, dtype: DType) -> Self {
        self.dtype = Some(dtype);
        self
    }

    /// Parse backend options to extract DeepSeek-OCR-specific configuration.
    ///
    /// Device selection is delegated to [`crate::candle_ocr::resolve_device_preference`]
    /// so the central `AccelerationConfig` is honoured. `model_id` defaults to
    /// [`DEFAULT_MODEL_ID`] and is only consulted when `model_path` is absent.
    fn parse_options(&self, config: &OcrConfig) -> Result<DeepseekOcrOptions> {
        let options: DeepseekOcrBackendOptions =
            parse_backend_options(config.backend_options.as_ref(), "candle-deepseek-ocr")?;
        for (field, value) in [
            ("model_path", options.model_path.as_deref()),
            ("model_id", options.model_id.as_deref()),
            ("hf_revision", options.hf_revision.as_deref()),
            ("cache_dir", options.cache_dir.as_deref()),
        ] {
            validate_optional_non_empty(value, "candle-deepseek-ocr", field)?;
        }
        let version = options.version.unwrap_or(2);
        if !matches!(version, 1 | 2) {
            return Err(crate::XbergError::validation(format!(
                "invalid candle-deepseek-ocr backend_options.version: expected 1 or 2, got {version}"
            )));
        }
        Ok(DeepseekOcrOptions {
            model_path: options.model_path,
            model_id: options.model_id.unwrap_or_else(|| DEFAULT_MODEL_ID.to_string()),
            hf_revision: options.hf_revision,
            cache_dir: options.cache_dir.map(PathBuf::from),
            device: super::resolve_device_preference(config, options.device),
            version: version as usize,
            dtype: requested_dtype(options.dtype).or(self.dtype),
        })
    }
}

impl Default for DeepseekOcrBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for DeepseekOcrBackend {
    fn name(&self) -> &str {
        "candle-deepseek-ocr"
    }

    fn version(&self) -> String {
        "0.1.0".to_string()
    }

    fn initialize(&self) -> Result<()> {
        tracing::debug!("Initializing DeepSeek-OCR backend");
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Inherits the `RequiresUpright` default for `page_orientation_handling` — unmeasured, not validated (#657).
#[async_trait]
impl OcrBackend for DeepseekOcrBackend {
    /// Process an image using the DeepSeek-OCR engine.
    ///
    /// # Errors
    ///
    /// Returns [`crate::XbergError::Validation`] if `image_bytes` is empty. Returns
    /// [`crate::XbergError::Ocr`] if weight download, device selection, engine
    /// initialisation, or inference fails.
    async fn process_image(&self, image_bytes: &[u8], config: &OcrConfig) -> Result<ExtractedDocument> {
        if image_bytes.is_empty() {
            return Err(crate::XbergError::Validation {
                message: "Empty image data provided to DeepSeek-OCR".to_string(),
                source: None,
            });
        }

        let options = self.parse_options(config)?;
        let image_bytes_owned = image_bytes.to_vec();

        let content = tokio::task::spawn_blocking(move || {
            let model_path = match options.model_path {
                Some(path) => PathBuf::from(path),
                None => super::model_stager::ensure_deepseek_ocr(
                    &options.model_id,
                    options.hf_revision.as_deref(),
                    options.cache_dir.as_deref(),
                )
                .map_err(|e| crate::XbergError::Ocr {
                    message: format!("DeepSeek-OCR weight download failed: {e}"),
                    source: None,
                })?,
            };
            let model_path = model_path.to_string_lossy().into_owned();

            let device = options.device.select().map_err(|e| crate::XbergError::Ocr {
                message: format!("Failed to select compute device: {e}"),
                source: Some(Box::new(e)),
            })?;
            let dtype = options.dtype.unwrap_or_else(|| default_dtype_for(&device));
            let engine = get_or_init_engine(options.device, device, dtype, &model_path, options.version)?;
            let mut engine_guard = engine.lock();
            let output = engine_guard
                .process_image(&image_bytes_owned, None)
                .map_err(|e| crate::XbergError::Ocr {
                    message: format!("DeepSeek-OCR inference failed: {e}"),
                    source: Some(Box::new(e)),
                })?;
            Ok::<String, crate::XbergError>(output)
        })
        .await
        .map_err(|e| crate::XbergError::Ocr {
            message: format!("DeepSeek-OCR task execution failed: {e}"),
            source: None,
        })??;

        Ok(super::ocr_result::build_ocr_document(
            content,
            Vec::new(),
            image_bytes,
            config,
            super::ocr_result::OcrDocumentContext {
                mime_type: Cow::Borrowed("text/markdown"),
                backend_name: "candle-deepseek-ocr",
                // DeepSeek-OCR has no task selection; every call is plain-text OCR. ~keep
                plain_text_task: true,
            },
        ))
    }

    /// Process an image file using the DeepSeek-OCR engine.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or if inference fails.
    async fn process_image_file(&self, path: &Path, config: &OcrConfig) -> Result<ExtractedDocument> {
        let bytes = crate::core::io::read_file_async(path).await?;
        self.process_image(&bytes, config).await
    }

    fn supports_language(&self, _lang: &str) -> bool {
        true
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![
            "eng", "en", "zho", "zh", "jpn", "ja", "kor", "ko", "fra", "fr", "deu", "de", "spa", "es", "ita", "it",
            "por", "pt", "rus", "ru", "ara", "ar", "hin", "hi", "tha", "th", "vie", "vi",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Candle
    }

    fn emits_structured_markdown(&self) -> bool {
        true
    }

    /// DeepSeek-OCR reports no page-level confidence.
    fn confidence_semantics(&self) -> crate::plugins::ConfidenceSemantics {
        crate::plugins::ConfidenceSemantics::None
    }

    // Rotation handling has not been measured for this backend; it stays on the trait's
    // `RequiresUpright` default.
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;

    use super::*;

    #[test]
    fn test_deepseek_ocr_backend_creation() {
        let backend = DeepseekOcrBackend::new();
        assert_eq!(backend.name(), "candle-deepseek-ocr");
        assert_eq!(backend.backend_type(), OcrBackendType::Candle);
    }

    #[test]
    fn test_deepseek_ocr_emits_structured_markdown() {
        let backend = DeepseekOcrBackend::new();
        assert!(backend.emits_structured_markdown());
    }

    #[test]
    fn test_deepseek_ocr_language_support() {
        let backend = DeepseekOcrBackend::new();
        assert!(backend.supports_language("eng"));
        assert!(backend.supports_language("zho"));
        assert!(backend.supports_language("jpn"));
        assert!(backend.supports_language("unknown"));
    }

    #[test]
    fn test_deepseek_ocr_supported_languages() {
        let backend = DeepseekOcrBackend::new();
        let langs = backend.supported_languages();
        assert!(langs.contains(&"eng".to_string()));
        assert!(langs.contains(&"zho".to_string()));
        assert!(langs.contains(&"fra".to_string()));
    }

    #[test]
    fn test_parse_options_defaults() {
        let config = OcrConfig::default();
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert!(options.model_path.is_none());
        assert_eq!(options.device, DevicePreference::Auto);
        assert_eq!(options.version, 2);
        assert_eq!(options.dtype, None);
    }

    #[test]
    fn test_parse_options_defaults_model_id() {
        let config = OcrConfig::default();
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.model_id, DEFAULT_MODEL_ID);
        assert_eq!(DEFAULT_MODEL_ID, "deepseek-ai/DeepSeek-OCR");
    }

    #[test]
    fn test_parse_options_model_path() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"model_path": "/models/deepseek"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.model_path.as_deref(), Some("/models/deepseek"));
    }

    #[test]
    fn test_parse_options_custom_model_id_and_revision() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"model_id": "example/deepseek-ocr", "hf_revision": "abc123"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.model_id, "example/deepseek-ocr");
        assert_eq!(options.hf_revision.as_deref(), Some("abc123"));
        assert!(options.model_path.is_none());
    }

    #[test]
    fn test_parse_options_custom_device() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"device": "cpu"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.device, DevicePreference::Cpu);
    }

    #[test]
    fn test_parse_options_rejects_unsupported_version() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"version": 3})),
            ..Default::default()
        };
        let error = DeepseekOcrBackend::new()
            .parse_options(&config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("backend_options.version"));
    }

    #[test]
    fn test_parse_options_accepts_supported_version() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"version": 1})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.version, 1);
    }

    #[test]
    fn test_parse_options_non_object_json_returns_contextual_error() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!(null)),
            ..Default::default()
        };
        let error = DeepseekOcrBackend::new()
            .parse_options(&config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("candle-deepseek-ocr backend_options"));
    }

    #[test]
    fn test_parse_options_empty_object_returns_defaults() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert!(options.model_path.is_none());
        assert_eq!(options.device, DevicePreference::Auto);
        assert_eq!(options.version, 2);
        assert_eq!(options.dtype, None);
    }

    #[test]
    fn test_parse_options_explicit_dtype() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"dtype": "bf16"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.dtype, Some(DType::BF16));
    }

    #[test]
    fn test_parse_options_auto_dtype_resolves_to_none() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"dtype": "auto"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new().parse_options(&config).unwrap();
        assert_eq!(options.dtype, None);
    }

    #[test]
    fn test_parse_options_constructor_dtype_used_when_options_dtype_absent() {
        let config = OcrConfig::default();
        let options = DeepseekOcrBackend::new()
            .with_dtype(DType::F16)
            .parse_options(&config)
            .unwrap();
        assert_eq!(options.dtype, Some(DType::F16));
    }

    #[test]
    fn test_parse_options_explicit_dtype_overrides_constructor_dtype() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"dtype": "bf16"})),
            ..Default::default()
        };
        let options = DeepseekOcrBackend::new()
            .with_dtype(DType::F16)
            .parse_options(&config)
            .unwrap();
        assert_eq!(options.dtype, Some(DType::BF16));
    }

    #[test]
    fn default_dtype_for_cpu_is_f32() {
        assert_eq!(default_dtype_for(&Device::Cpu), DType::F32);
    }

    #[test]
    fn requested_dtype_maps_each_explicit_variant() {
        assert_eq!(requested_dtype(None), None);
        assert_eq!(requested_dtype(Some(CandleDeepseekOcrDtype::Auto)), None);
        assert_eq!(requested_dtype(Some(CandleDeepseekOcrDtype::F32)), Some(DType::F32));
        assert_eq!(requested_dtype(Some(CandleDeepseekOcrDtype::F16)), Some(DType::F16));
        assert_eq!(requested_dtype(Some(CandleDeepseekOcrDtype::Bf16)), Some(DType::BF16));
    }

    #[test]
    fn test_initialize_and_shutdown() {
        let backend = DeepseekOcrBackend::new();
        assert!(backend.initialize().is_ok());
        assert!(backend.shutdown().is_ok());
    }

    #[test]
    fn engine_pool_reuses_equal_configs_and_isolates_distinct_configs() {
        let original = EnginePoolKey::new(DevicePreference::Cpu, DType::F32, "/models/v1", 1);
        let equal = EnginePoolKey::new(DevicePreference::Cpu, DType::F32, "/models/v1", 1);
        let mut pool = AHashMap::new();
        pool.insert(original, 7_u8);

        assert_eq!(pool.get(&equal), Some(&7));
        assert_eq!(
            pool.get(&EnginePoolKey::new(DevicePreference::Cpu, DType::F32, "/models/v2", 1)),
            None
        );
        assert_eq!(
            pool.get(&EnginePoolKey::new(DevicePreference::Cpu, DType::F32, "/models/v1", 2)),
            None
        );
        assert_eq!(
            pool.get(&EnginePoolKey::new(DevicePreference::Auto, DType::F32, "/models/v1", 1)),
            None
        );
    }
}
