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
use std::path::Path;
use std::sync::{Arc, LazyLock};

use crate::Result;
use crate::candle_ocr::config::{DeepseekOcrBackendOptions, parse_backend_options, validate_optional_non_empty};
use crate::core::config::OcrConfig;
use crate::engine_cache::EngineCache;
use crate::plugins::{OcrBackend, OcrBackendType, Plugin};
use crate::types::ExtractedDocument;
use xberg_candle_ocr::DType;
use xberg_candle_ocr::DevicePreference;
use xberg_candle_ocr::models::DeepseekOCREngine;

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
    dtype: DType,
    model_path: &str,
    version: usize,
) -> crate::Result<PooledEngine> {
    let key = EnginePoolKey::new(preference, dtype, model_path, version);

    ENGINE_POOL.get_or_try_init(key, || {
        let device = preference.select().map_err(|e| crate::XbergError::Ocr {
            message: format!("Failed to select compute device: {e}"),
            source: Some(Box::new(e)),
        })?;

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

/// DeepSeek-OCR backend using candle transformers.
///
/// A vision-language model combining SAM vision encoder, ViT/Qwen2 vision
/// transformer, CLIP projection, and language decoder for multimodal OCR.
///
/// # Configuration
///
/// DeepSeek-OCR accepts backend options for device, model path, and version:
/// ```json
/// {
///   "device": "auto",
///   "model_path": "/path/to/deepseek-ocr-model",
///   "version": 2
/// }
/// ```
///
/// - `device` (string): `"auto"` (default), `"cpu"`, `"cuda"`, `"metal"`
/// - `model_path` (string): path to the local model directory (required)
/// - `version` (integer): model version `1` or `2` (default: `2`)
#[cfg_attr(alef, alef(skip))]
pub struct DeepseekOcrBackend {
    dtype: DType,
}

impl DeepseekOcrBackend {
    /// Create a new DeepSeek-OCR backend.
    ///
    /// The data type defaults to `F32`. Use [`DeepseekOcrBackend::with_dtype`] to override.
    pub fn new() -> Self {
        Self { dtype: DType::F32 }
    }

    /// Override the floating-point precision used by the candle engine.
    pub fn with_dtype(mut self, dtype: DType) -> Self {
        self.dtype = dtype;
        self
    }

    /// Parse backend options to extract DeepSeek-OCR-specific configuration.
    ///
    /// Device selection is delegated to [`crate::candle_ocr::resolve_device_preference`]
    /// so the central `AccelerationConfig` is honoured.
    ///
    /// Returns `(model_path, device_preference, version)`.
    fn parse_options(config: &OcrConfig) -> Result<(Option<String>, DevicePreference, usize)> {
        let options: DeepseekOcrBackendOptions =
            parse_backend_options(config.backend_options.as_ref(), "candle-deepseek-ocr")?;
        validate_optional_non_empty(options.model_path.as_deref(), "candle-deepseek-ocr", "model_path")?;
        let version = options.version.unwrap_or(2);
        if !matches!(version, 1 | 2) {
            return Err(crate::XbergError::validation(format!(
                "invalid candle-deepseek-ocr backend_options.version: expected 1 or 2, got {version}"
            )));
        }
        let device = super::resolve_device_preference(config, options.device);
        Ok((options.model_path, device, version as usize))
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
    /// Returns an error if the image is empty, model_path is not provided,
    /// the model fails to initialize, or inference fails.
    async fn process_image(&self, image_bytes: &[u8], config: &OcrConfig) -> Result<ExtractedDocument> {
        if image_bytes.is_empty() {
            return Err(crate::XbergError::Validation {
                message: "Empty image data provided to DeepSeek-OCR".to_string(),
                source: None,
            });
        }

        let (model_path, device, version) = Self::parse_options(config)?;

        let model_path = model_path.ok_or_else(|| crate::XbergError::Validation {
            message: "DeepSeek-OCR requires `model_path` in backend_options".to_string(),
            source: None,
        })?;

        let image_bytes_owned = image_bytes.to_vec();
        let dtype = self.dtype;

        let content = tokio::task::spawn_blocking(move || {
            let engine = get_or_init_engine(device, dtype, &model_path, version)?;
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
            Cow::Borrowed("text/markdown"),
            image_bytes,
            config,
            "candle-deepseek-ocr",
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
        let (model_path, device, version) = DeepseekOcrBackend::parse_options(&config).unwrap();
        assert!(model_path.is_none());
        assert_eq!(device, DevicePreference::Auto);
        assert_eq!(version, 2);
    }

    #[test]
    fn test_parse_options_model_path() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"model_path": "/models/deepseek"})),
            ..Default::default()
        };
        let (model_path, _device, _version) = DeepseekOcrBackend::parse_options(&config).unwrap();
        assert_eq!(model_path.as_deref(), Some("/models/deepseek"));
    }

    #[test]
    fn test_parse_options_custom_device() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"device": "cpu"})),
            ..Default::default()
        };
        let (_model_path, device, _version) = DeepseekOcrBackend::parse_options(&config).unwrap();
        assert_eq!(device, DevicePreference::Cpu);
    }

    #[test]
    fn test_parse_options_rejects_unsupported_version() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"version": 3})),
            ..Default::default()
        };
        let error = DeepseekOcrBackend::parse_options(&config).unwrap_err().to_string();
        assert!(error.contains("backend_options.version"));
    }

    #[test]
    fn test_parse_options_accepts_supported_version() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({"version": 1})),
            ..Default::default()
        };
        let (_, _, version) = DeepseekOcrBackend::parse_options(&config).unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_parse_options_non_object_json_returns_contextual_error() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!(null)),
            ..Default::default()
        };
        let error = DeepseekOcrBackend::parse_options(&config).unwrap_err().to_string();
        assert!(error.contains("candle-deepseek-ocr backend_options"));
    }

    #[test]
    fn test_parse_options_empty_object_returns_defaults() {
        let config = OcrConfig {
            backend_options: Some(serde_json::json!({})),
            ..Default::default()
        };
        let (model_path, device, version) = DeepseekOcrBackend::parse_options(&config).unwrap();
        assert!(model_path.is_none());
        assert_eq!(device, DevicePreference::Auto);
        assert_eq!(version, 2);
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
