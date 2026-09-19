#[cfg(any(
    test,
    feature = "candle-trocr",
    feature = "candle-paddleocr-vl",
    feature = "candle-glm-ocr",
    feature = "candle-deepseek-ocr"
))]
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use xberg_candle_ocr::DevicePreference;

#[cfg(any(
    test,
    feature = "candle-trocr",
    feature = "candle-paddleocr-vl",
    feature = "candle-glm-ocr",
    feature = "candle-deepseek-ocr"
))]
use crate::XbergError;

/// Device selection shared by the typed candle backend option objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandleDevicePreference {
    #[default]
    Auto,
    Cpu,
    Cuda,
    Metal,
}

impl From<CandleDevicePreference> for DevicePreference {
    fn from(value: CandleDevicePreference) -> Self {
        match value {
            CandleDevicePreference::Auto => Self::Auto,
            CandleDevicePreference::Cpu => Self::Cpu,
            CandleDevicePreference::Cuda => Self::Cuda,
            CandleDevicePreference::Metal => Self::Metal,
        }
    }
}

impl From<DevicePreference> for CandleDevicePreference {
    fn from(value: DevicePreference) -> Self {
        match value {
            DevicePreference::Auto => Self::Auto,
            DevicePreference::Cpu => Self::Cpu,
            DevicePreference::Cuda => Self::Cuda,
            DevicePreference::Metal => Self::Metal,
        }
    }
}

/// TrOCR model variant accepted by `candle-trocr` backend options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandleTrocrVariant {
    #[default]
    BasePrinted,
    LargePrinted,
    BaseHandwritten,
    LargeHandwritten,
}

/// Task accepted by `candle-paddleocr-vl` backend options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaddleOcrVlTaskKind {
    #[default]
    Ocr,
    Table,
    Formula,
    Chart,
}

/// Task accepted by `candle-glm-ocr` backend options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlmOcrTaskKind {
    #[default]
    Ocr,
    Table,
    Formula,
    Chart,
    Caption,
}

/// Page layout mode accepted by `candle-glm-ocr` backend options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlmOcrLayoutMode {
    #[default]
    WholePage,
    Paired,
}

/// Runtime options accepted by the `candle-trocr` backend.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrocrBackendOptions {
    /// Optional model variant; the backend constructor's variant is used when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<CandleTrocrVariant>,
    /// Optional per-call device override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<CandleDevicePreference>,
    /// Optional Hugging Face cache root.
    #[serde(alias = "cache-dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
    /// Optional immutable Hugging Face model revision.
    #[serde(alias = "hf-revision", alias = "revision", skip_serializing_if = "Option::is_none")]
    pub hf_revision: Option<String>,
}

/// Runtime options accepted by the `candle-paddleocr-vl` backend.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaddleOcrVlBackendOptions {
    /// Optional per-call recognition task; the backend constructor's task is used when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<PaddleOcrVlTaskKind>,
    /// Optional local model directory, which takes precedence over `model_id`.
    #[serde(alias = "model-path", skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    /// Optional Hugging Face repository identifier.
    #[serde(alias = "model-id", skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Optional immutable Hugging Face model revision.
    #[serde(alias = "hf-revision", alias = "revision", skip_serializing_if = "Option::is_none")]
    pub hf_revision: Option<String>,
    /// Optional Hugging Face cache root.
    #[serde(alias = "cache-dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
    /// Optional per-call device override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<CandleDevicePreference>,
}

/// Runtime options accepted by the `candle-glm-ocr` backend.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlmOcrBackendOptions {
    /// Optional recognition task; the backend constructor's task is used when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<GlmOcrTaskKind>,
    /// Optional per-call device override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<CandleDevicePreference>,
    /// Optional whole-page or layout-paired dispatch mode.
    #[serde(alias = "layout-mode", skip_serializing_if = "Option::is_none")]
    pub layout_mode: Option<GlmOcrLayoutMode>,
    /// Whether chart regions use chart understanding instead of captioning.
    #[serde(alias = "enable-chart-understanding", skip_serializing_if = "Option::is_none")]
    pub enable_chart_understanding: Option<bool>,
    /// Optional Hugging Face cache root.
    #[serde(alias = "cache-dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
}

/// Runtime options accepted by the `candle-deepseek-ocr` backend.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeepseekOcrBackendOptions {
    /// Local DeepSeek-OCR model directory. The backend requires this option.
    #[serde(alias = "model-path", skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    /// Optional per-call device override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<CandleDevicePreference>,
    /// DeepSeek-OCR model generation, either 1 or 2. Defaults to 2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

#[cfg(any(
    test,
    feature = "candle-trocr",
    feature = "candle-paddleocr-vl",
    feature = "candle-glm-ocr",
    feature = "candle-deepseek-ocr"
))]
pub(crate) fn parse_backend_options<T>(value: Option<&serde_json::Value>, backend_name: &str) -> crate::Result<T>
where
    T: DeserializeOwned + Default,
{
    let Some(value) = value else {
        return Ok(T::default());
    };
    if !value.is_object() {
        return Err(XbergError::validation(format!(
            "invalid {backend_name} backend_options: expected a JSON object"
        )));
    }
    // The PDF OCR route stamps `source_dpi` and `page_rotation_degrees` into every backend's
    // shared `backend_options` object regardless of which backend will read it (xberg#1672,
    // `extractors::pdf::ocr::pipeline::ocr_config_with_page_rotation_hint`), and
    // `OcrConfig::backend_options`'s own contract says unknown keys are silently ignored. No
    // candle backend option struct declares either field, so strip exactly those two known
    // pipeline hint keys before the `deny_unknown_fields` deserialize below -- which keeps
    // reporting a real typo in a candle-specific option (any other unknown key) as the
    // validation error it is.
    let mut value = value.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.remove(crate::core::config::ocr::SOURCE_DPI_BACKEND_OPTION);
        obj.remove(crate::core::config::ocr::PAGE_ROTATION_DEGREES_BACKEND_OPTION);
    }
    serde_json::from_value(value)
        .map_err(|error| XbergError::validation(format!("invalid {backend_name} backend_options: {error}")))
}

#[cfg(any(
    test,
    feature = "candle-trocr",
    feature = "candle-paddleocr-vl",
    feature = "candle-glm-ocr",
    feature = "candle-deepseek-ocr"
))]
pub(crate) fn validate_optional_non_empty(
    value: Option<&str>,
    backend_name: &str,
    field_name: &str,
) -> crate::Result<()> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(XbergError::validation(format!(
            "invalid {backend_name} backend_options.{field_name}: value must not be empty"
        )));
    }
    Ok(())
}

/// Identifier used by [`CandleOcrConfig::backend_name`] for its legacy model pair.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandleModelId {
    #[default]
    Trocr,
    PaddleocrVl,
}

/// Legacy common configuration for TrOCR and PaddleOCR-VL.
///
/// Use the backend-specific option objects for new code. This type remains for
/// 1.x compatibility and can produce the backend name and common runtime options.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CandleOcrConfig {
    pub model: CandleModelId,
    pub device: DevicePreference,
    /// Optional explicit Hugging Face Hub cache root. When unset, the standard
    /// Hugging Face environment-variable conventions are used.
    #[serde(alias = "cache-dir")]
    pub cache_dir: Option<PathBuf>,
    /// Optional immutable Hugging Face revision for caller-selected models.
    #[serde(alias = "hf-revision", alias = "revision")]
    pub hf_revision: Option<String>,
    /// Retained for source compatibility. Current candle OCR engines do not
    /// expose per-call token limits; use backend defaults.
    #[serde(alias = "max-new-tokens")]
    pub max_new_tokens: u32,
    /// Retained for source compatibility. Current candle OCR engines use their
    /// model-specific decoding strategy and do not consume this value.
    pub temperature: f32,
}

impl CandleOcrConfig {
    /// Backend registry name corresponding to [`Self::model`].
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        match self.model {
            CandleModelId::Trocr => "candle-trocr",
            CandleModelId::PaddleocrVl => "candle-paddleocr-vl",
        }
    }

    /// Common runtime options understood by both legacy model choices.
    #[must_use]
    pub fn backend_options(&self) -> serde_json::Value {
        let device = CandleDevicePreference::from(self.device);
        let mut options = serde_json::Map::new();
        options.insert("device".to_string(), serde_json::json!(device));
        if let Some(cache_dir) = &self.cache_dir {
            options.insert(
                "cache_dir".to_string(),
                serde_json::Value::String(cache_dir.to_string_lossy().into_owned()),
            );
        }
        if let Some(hf_revision) = &self.hf_revision {
            options.insert(
                "hf_revision".to_string(),
                serde_json::Value::String(hf_revision.clone()),
            );
        }
        serde_json::Value::Object(options)
    }
}

impl Default for CandleOcrConfig {
    fn default() -> Self {
        Self {
            model: CandleModelId::default(),
            device: DevicePreference::Auto,
            cache_dir: None,
            hf_revision: None,
            max_new_tokens: 4096,
            temperature: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_canonical_snake_case_candle_options() {
        let trocr = TrocrBackendOptions {
            variant: Some(CandleTrocrVariant::LargeHandwritten),
            device: Some(CandleDevicePreference::Cuda),
            cache_dir: Some("/models/trocr".to_string()),
            hf_revision: Some("trocr-revision".to_string()),
        };
        assert_eq!(
            serde_json::to_value(trocr).unwrap(),
            serde_json::json!({
                "variant": "large-handwritten",
                "device": "cuda",
                "cache_dir": "/models/trocr",
                "hf_revision": "trocr-revision"
            })
        );

        let paddle = PaddleOcrVlBackendOptions {
            task: Some(PaddleOcrVlTaskKind::Chart),
            model_path: Some("/models/paddle".to_string()),
            model_id: Some("example/paddle".to_string()),
            hf_revision: Some("paddle-revision".to_string()),
            cache_dir: Some("/cache/paddle".to_string()),
            device: Some(CandleDevicePreference::Metal),
        };
        assert_eq!(
            serde_json::to_value(paddle).unwrap(),
            serde_json::json!({
                "task": "chart",
                "model_path": "/models/paddle",
                "model_id": "example/paddle",
                "hf_revision": "paddle-revision",
                "cache_dir": "/cache/paddle",
                "device": "metal"
            })
        );

        let glm = GlmOcrBackendOptions {
            task: Some(GlmOcrTaskKind::Caption),
            device: Some(CandleDevicePreference::Cpu),
            layout_mode: Some(GlmOcrLayoutMode::Paired),
            enable_chart_understanding: Some(true),
            cache_dir: Some("/cache/glm".to_string()),
        };
        assert_eq!(
            serde_json::to_value(glm).unwrap(),
            serde_json::json!({
                "task": "caption",
                "device": "cpu",
                "layout_mode": "paired",
                "enable_chart_understanding": true,
                "cache_dir": "/cache/glm"
            })
        );

        let deepseek = DeepseekOcrBackendOptions {
            model_path: Some("/models/deepseek".to_string()),
            device: Some(CandleDevicePreference::Auto),
            version: Some(3),
        };
        assert_eq!(
            serde_json::to_value(deepseek).unwrap(),
            serde_json::json!({
                "model_path": "/models/deepseek",
                "device": "auto",
                "version": 3
            })
        );
    }

    #[test]
    fn should_accept_legacy_candle_option_key_aliases() {
        let value = serde_json::json!({
            "cache-dir": "/legacy-cache",
            "hf-revision": "legacy-revision"
        });
        let options: TrocrBackendOptions = parse_backend_options(Some(&value), "candle-trocr").unwrap();
        assert_eq!(options.cache_dir.as_deref(), Some("/legacy-cache"));
        assert_eq!(options.hf_revision.as_deref(), Some("legacy-revision"));

        let revision_alias = serde_json::json!({"revision": "short-alias"});
        let options: PaddleOcrVlBackendOptions =
            parse_backend_options(Some(&revision_alias), "candle-paddleocr-vl").unwrap();
        assert_eq!(options.hf_revision.as_deref(), Some("short-alias"));
    }

    #[test]
    fn should_reject_invalid_candle_backend_options_with_context() {
        let invalid_device = serde_json::json!({"device": "quantum"});
        let error = parse_backend_options::<TrocrBackendOptions>(Some(&invalid_device), "candle-trocr")
            .unwrap_err()
            .to_string();
        assert!(error.contains("candle-trocr backend_options"));
        assert!(error.contains("unknown variant `quantum`"));

        let invalid_shape = serde_json::json!(["cpu"]);
        let error = parse_backend_options::<GlmOcrBackendOptions>(Some(&invalid_shape), "candle-glm-ocr")
            .unwrap_err()
            .to_string();
        assert!(error.contains("candle-glm-ocr backend_options"));

        let error = validate_optional_non_empty(Some("  "), "candle-paddleocr-vl", "model_id")
            .unwrap_err()
            .to_string();
        assert!(error.contains("candle-paddleocr-vl backend_options.model_id"));

        let unknown_field = serde_json::json!({"model-path": "/models/deepseek", "versoin": 2});
        let error = parse_backend_options::<DeepseekOcrBackendOptions>(Some(&unknown_field), "candle-deepseek-ocr")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `versoin`"));
        assert!(error.contains("candle-deepseek-ocr backend_options"));
    }

    /// xberg#1672: the PDF OCR page path stamps `source_dpi` and `page_rotation_degrees` into
    /// the shared `backend_options` object for every backend
    /// (`extractors::pdf::ocr::pipeline::ocr_config_with_page_rotation_hint`).
    /// `OcrConfig::backend_options`'s own contract says unknown keys are silently ignored, and no
    /// candle backend option struct declares either field, so both keys must be tolerated here
    /// too -- not just any unknown key, which `should_reject_invalid_candle_backend_options_with_context`
    /// above still rejects.
    ///
    /// Fails on unfixed code: `deny_unknown_fields` rejects `source_dpi` with `unknown field
    /// source_dpi, expected one of ...` for every one of the four candle backends.
    #[test]
    fn should_ignore_pdf_pipeline_hint_keys_in_candle_backend_options() {
        let hints = serde_json::json!({"source_dpi": 150.0, "page_rotation_degrees": 270});

        let trocr: TrocrBackendOptions = parse_backend_options(Some(&hints), "candle-trocr").unwrap();
        assert_eq!(trocr, TrocrBackendOptions::default());

        let paddle: PaddleOcrVlBackendOptions = parse_backend_options(Some(&hints), "candle-paddleocr-vl").unwrap();
        assert_eq!(paddle, PaddleOcrVlBackendOptions::default());

        let glm: GlmOcrBackendOptions = parse_backend_options(Some(&hints), "candle-glm-ocr").unwrap();
        assert_eq!(glm, GlmOcrBackendOptions::default());

        let deepseek: DeepseekOcrBackendOptions = parse_backend_options(Some(&hints), "candle-deepseek-ocr").unwrap();
        assert_eq!(deepseek, DeepseekOcrBackendOptions::default());

        // A hint alongside a real option must still combine correctly, not disappear with the
        // rest of the object.
        let mixed = serde_json::json!({"source_dpi": 150.0, "task": "table"});
        let paddle_mixed: PaddleOcrVlBackendOptions =
            parse_backend_options(Some(&mixed), "candle-paddleocr-vl").unwrap();
        assert_eq!(paddle_mixed.task, Some(PaddleOcrVlTaskKind::Table));
    }

    #[test]
    fn should_convert_legacy_config_to_runtime_backend_options() {
        let config = CandleOcrConfig {
            model: CandleModelId::PaddleocrVl,
            device: DevicePreference::Cuda,
            cache_dir: Some(PathBuf::from("/legacy-cache")),
            hf_revision: Some("legacy-revision".to_string()),
            max_new_tokens: 128,
            temperature: 0.75,
        };
        assert_eq!(
            config.backend_options(),
            serde_json::json!({
                "device": "cuda",
                "cache_dir": "/legacy-cache",
                "hf_revision": "legacy-revision"
            })
        );
        assert_eq!(config.backend_name(), "candle-paddleocr-vl");
    }
}
