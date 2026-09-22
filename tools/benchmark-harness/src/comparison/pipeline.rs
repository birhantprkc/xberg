//! Pipeline identifier: the [`Pipeline`] enum naming every extraction configuration this
//! harness knows how to run or read (vendored) results for.

use serde::{Deserialize, Serialize};

/// Extraction pipeline identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pipeline {
    /// Native PDF text extraction (no OCR, no layout)
    Baseline,
    /// Native PDF text extraction + layout detection
    Layout,
    /// Tesseract OCR (force_ocr)
    Tesseract,
    /// Tesseract OCR + layout detection
    TesseractLayout,
    /// Tesseract OCR with single-block page segmentation (PSM 6)
    #[serde(alias = "tesseract-psm6")]
    TesseractSingleBlock,
    /// Tesseract OCR with vertical single-block page segmentation (PSM 5)
    #[serde(alias = "tesseract-vertical", alias = "tesseract-psm5")]
    TesseractVerticalBlock,
    /// Tesseract OCR with sparse-text page segmentation (PSM 11)
    #[serde(alias = "tesseract-psm11")]
    TesseractSparseText,
    /// PP-OCRv6 medium tier (force_ocr)
    #[serde(rename = "paddle-v6-medium", alias = "paddle", alias = "paddle-mobile")]
    Paddle,
    /// PP-OCRv6 medium tier + layout detection
    #[serde(
        rename = "paddle-v6-medium+layout",
        alias = "paddle-layout",
        alias = "paddle-mobile-layout"
    )]
    PaddleLayout,
    /// PP-OCRv6 small tier (force_ocr)
    #[serde(rename = "paddle-v6-small")]
    PaddleV6Small,
    /// PP-OCRv6 small tier + layout detection
    #[serde(rename = "paddle-v6-small+layout", alias = "paddle-v6-small-layout")]
    PaddleV6SmallLayout,
    /// PP-OCRv6 small + layout with the fully pinned 1024-pixel quality control profile
    #[serde(rename = "paddle-v6-small+layout+det-side-1024")]
    PaddleV6SmallLayoutDetSide1024,
    /// PP-OCRv6 small + layout with a 1536-pixel detector input limit
    #[serde(rename = "paddle-v6-small+layout+det-side-1536")]
    PaddleV6SmallLayoutDetSide1536,
    /// PP-OCRv6 small + layout with a 2048-pixel detector input limit
    #[serde(rename = "paddle-v6-small+layout+det-side-2048")]
    PaddleV6SmallLayoutDetSide2048,
    /// PP-OCRv6 small + layout with a 0.20 detector pixel threshold
    #[serde(rename = "paddle-v6-small+layout+det-db-thresh-020")]
    PaddleV6SmallLayoutDetDbThresh020,
    /// PP-OCRv6 small + layout with a 0.35 detector box threshold
    #[serde(rename = "paddle-v6-small+layout+det-db-box-thresh-035")]
    PaddleV6SmallLayoutDetDbBoxThresh035,
    /// PP-OCRv6 small + layout with a 0.30 recognition drop score
    #[serde(rename = "paddle-v6-small+layout+drop-score-030")]
    PaddleV6SmallLayoutDropScore030,
    /// PP-OCRv6 small + layout with a 0.40 recognition drop score
    #[serde(rename = "paddle-v6-small+layout+drop-score-040")]
    PaddleV6SmallLayoutDropScore040,
    /// PP-OCRv6 tiny tier (force_ocr)
    #[serde(rename = "paddle-v6-tiny")]
    PaddleV6Tiny,
    /// PP-OCRv6 tiny tier + layout detection
    #[serde(rename = "paddle-v6-tiny+layout", alias = "paddle-v6-tiny-layout")]
    PaddleV6TinyLayout,
    /// Legacy PP-OCRv5 server tier (force_ocr)
    #[serde(rename = "paddle-v5-server", alias = "paddle-server")]
    PaddleServer,
    /// Legacy PP-OCRv5 server tier + layout detection
    #[serde(rename = "paddle-v5-server+layout", alias = "paddle-server-layout")]
    PaddleServerLayout,
    /// Tesseract OCR with auto_rotate enabled
    TesseractAutoRotate,
    /// PaddleOCR with auto_rotate enabled
    PaddleAutoRotate,
    /// PaddleOCR without auto_rotate (for comparison)
    PaddleNoRotate,
    /// Sceptre OCR pinned to the ONNX Runtime inference engine (force_ocr)
    #[serde(rename = "sceptre-ort", alias = "sceptre")]
    Sceptre,
    /// Sceptre OCR (ONNX Runtime) + layout detection
    #[serde(
        rename = "sceptre-ort+layout",
        alias = "sceptre-ort-layout",
        alias = "sceptre-layout"
    )]
    SceptreLayout,
    /// Sceptre OCR (ONNX Runtime) with auto_rotate enabled
    #[serde(rename = "sceptre-ort-autorotate", alias = "sceptre-autorotate")]
    SceptreAutoRotate,
    /// Docling vendored extraction (read from file)
    Docling,
    /// PaddleOCR Python vendored extraction (read from file)
    PaddleOcrPython,
    /// RapidOCR vendored extraction (read from file)
    RapidOcr,
    /// Native PDF + layout detection + SLANeXT wired table model (forced)
    LayoutSlanetWired,
    /// Native PDF + layout detection + SLANeXT wireless table model (forced)
    LayoutSlanetWireless,
    /// Native PDF + layout detection + SLANet_plus table model
    LayoutSlanetPlus,
    /// Native PDF + layout detection + classifier-routed SLANeXT (wired/wireless auto)
    LayoutSlanetAuto,
    /// xberg-native-pdf backend text extraction (no OCR, no layout)
    Native,
    /// xberg-native-pdf backend + layout detection
    NativeLayout,
    /// xberg-native-pdf backend + layout detection + reading-order reordering
    NativeReadingOrder,
    /// Candle-based TrOCR (force_ocr, plain text)
    CandleTrocr,
    /// Candle-based PaddleOCR-VL (force_ocr, end-to-end markdown)
    CandlePaddleocrVl,
    /// Candle-based GLM-OCR vision-language backend (force_ocr)
    CandleGlmOcr,
    /// Candle-based GLM-OCR with layout detection + formula extraction
    CandleGlmOcrLayout,
    /// Candle-based GLM-OCR with layout detection + formula + chart understanding
    CandleGlmOcrLayoutChart,
    /// Candle-based DeepSeek-OCR vision-language backend (force_ocr)
    CandleDeepseekOcr,
    /// Candle-based PaddleOCR-VL 1.5 vision-language backend (force_ocr)
    CandlePaddleocrVl15,
}

impl Pipeline {
    pub fn name(&self) -> &'static str {
        match self {
            Pipeline::Baseline => "baseline",
            Pipeline::Layout => "layout",
            Pipeline::Tesseract => "tesseract",
            Pipeline::TesseractLayout => "tesseract+layout",
            Pipeline::TesseractSingleBlock => "tesseract-single-block",
            Pipeline::TesseractVerticalBlock => "tesseract-vertical-block",
            Pipeline::TesseractSparseText => "tesseract-sparse-text",
            Pipeline::Paddle => "paddle-v6-medium",
            Pipeline::PaddleLayout => "paddle-v6-medium+layout",
            Pipeline::PaddleV6Small => "paddle-v6-small",
            Pipeline::PaddleV6SmallLayout => "paddle-v6-small+layout",
            Pipeline::PaddleV6SmallLayoutDetSide1024 => "paddle-v6-small+layout+det-side-1024",
            Pipeline::PaddleV6SmallLayoutDetSide1536 => "paddle-v6-small+layout+det-side-1536",
            Pipeline::PaddleV6SmallLayoutDetSide2048 => "paddle-v6-small+layout+det-side-2048",
            Pipeline::PaddleV6SmallLayoutDetDbThresh020 => "paddle-v6-small+layout+det-db-thresh-020",
            Pipeline::PaddleV6SmallLayoutDetDbBoxThresh035 => "paddle-v6-small+layout+det-db-box-thresh-035",
            Pipeline::PaddleV6SmallLayoutDropScore030 => "paddle-v6-small+layout+drop-score-030",
            Pipeline::PaddleV6SmallLayoutDropScore040 => "paddle-v6-small+layout+drop-score-040",
            Pipeline::PaddleV6Tiny => "paddle-v6-tiny",
            Pipeline::PaddleV6TinyLayout => "paddle-v6-tiny+layout",
            Pipeline::PaddleServer => "paddle-v5-server",
            Pipeline::PaddleServerLayout => "paddle-v5-server+layout",
            Pipeline::TesseractAutoRotate => "tesseract-autorotate",
            Pipeline::PaddleAutoRotate => "paddle-autorotate",
            Pipeline::PaddleNoRotate => "paddle-norotate",
            Pipeline::Sceptre => "sceptre-ort",
            Pipeline::SceptreLayout => "sceptre-ort+layout",
            Pipeline::SceptreAutoRotate => "sceptre-ort-autorotate",
            Pipeline::Docling => "docling",
            Pipeline::PaddleOcrPython => "paddleocr-python",
            Pipeline::RapidOcr => "rapidocr",
            Pipeline::LayoutSlanetWired => "layout+slanet-wired",
            Pipeline::LayoutSlanetWireless => "layout+slanet-wireless",
            Pipeline::LayoutSlanetPlus => "layout+slanet-plus",
            Pipeline::LayoutSlanetAuto => "layout+slanet-auto",
            Pipeline::Native => "native",
            Pipeline::NativeLayout => "native+layout",
            Pipeline::NativeReadingOrder => "native+layout+reading-order",
            Pipeline::CandleTrocr => "candle-trocr",
            Pipeline::CandlePaddleocrVl => "candle-paddleocr-vl",
            Pipeline::CandleGlmOcr => "candle-glm-ocr",
            Pipeline::CandleGlmOcrLayout => "candle-glm-ocr+layout",
            Pipeline::CandleGlmOcrLayoutChart => "candle-glm-ocr+layout+chart",
            Pipeline::CandleDeepseekOcr => "candle-deepseek-ocr",
            Pipeline::CandlePaddleocrVl15 => "candle-paddleocr-vl-15",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "baseline" => Some(Pipeline::Baseline),
            "layout" => Some(Pipeline::Layout),
            "tesseract" => Some(Pipeline::Tesseract),
            "tesseract+layout" | "tesseract-layout" => Some(Pipeline::TesseractLayout),
            "tesseract-single-block" | "tesseract-psm6" => Some(Pipeline::TesseractSingleBlock),
            "tesseract-vertical-block" | "tesseract-vertical" | "tesseract-psm5" => {
                Some(Pipeline::TesseractVerticalBlock)
            }
            "tesseract-sparse-text" | "tesseract-psm11" => Some(Pipeline::TesseractSparseText),
            "paddle" | "paddle-mobile" | "paddle-v6-medium" => Some(Pipeline::Paddle),
            "paddle+layout"
            | "paddle-layout"
            | "paddle-mobile+layout"
            | "paddle-mobile-layout"
            | "paddle-v6-medium+layout"
            | "paddle-v6-medium-layout" => Some(Pipeline::PaddleLayout),
            "paddle-v6-small" => Some(Pipeline::PaddleV6Small),
            "paddle-v6-small+layout" | "paddle-v6-small-layout" => Some(Pipeline::PaddleV6SmallLayout),
            "paddle-v6-small+layout+det-side-1024" => Some(Pipeline::PaddleV6SmallLayoutDetSide1024),
            "paddle-v6-small+layout+det-side-1536" => Some(Pipeline::PaddleV6SmallLayoutDetSide1536),
            "paddle-v6-small+layout+det-side-2048" => Some(Pipeline::PaddleV6SmallLayoutDetSide2048),
            "paddle-v6-small+layout+det-db-thresh-020" => Some(Pipeline::PaddleV6SmallLayoutDetDbThresh020),
            "paddle-v6-small+layout+det-db-box-thresh-035" => Some(Pipeline::PaddleV6SmallLayoutDetDbBoxThresh035),
            "paddle-v6-small+layout+drop-score-030" => Some(Pipeline::PaddleV6SmallLayoutDropScore030),
            "paddle-v6-small+layout+drop-score-040" => Some(Pipeline::PaddleV6SmallLayoutDropScore040),
            "paddle-v6-tiny" => Some(Pipeline::PaddleV6Tiny),
            "paddle-v6-tiny+layout" | "paddle-v6-tiny-layout" => Some(Pipeline::PaddleV6TinyLayout),
            "paddle-server" | "paddle-v5-server" => Some(Pipeline::PaddleServer),
            "paddle-server+layout" | "paddle-server-layout" | "paddle-v5-server+layout" | "paddle-v5-server-layout" => {
                Some(Pipeline::PaddleServerLayout)
            }
            "tesseract-autorotate" => Some(Pipeline::TesseractAutoRotate),
            "paddle-autorotate" => Some(Pipeline::PaddleAutoRotate),
            "paddle-norotate" => Some(Pipeline::PaddleNoRotate),
            "sceptre" | "sceptre-ort" | "sceptre_ort" => Some(Pipeline::Sceptre),
            "sceptre-ort+layout" | "sceptre-ort-layout" | "sceptre-layout" | "sceptre+layout"
            | "sceptre_ort_layout" => Some(Pipeline::SceptreLayout),
            "sceptre-ort-autorotate" | "sceptre-autorotate" | "sceptre_ort_autorotate" | "sceptre_autorotate" => {
                Some(Pipeline::SceptreAutoRotate)
            }
            "docling" => Some(Pipeline::Docling),
            "paddleocr-python" => Some(Pipeline::PaddleOcrPython),
            "rapidocr" => Some(Pipeline::RapidOcr),
            "layout+slanet-wired" | "layout-slanet-wired" => Some(Pipeline::LayoutSlanetWired),
            "layout+slanet-wireless" | "layout-slanet-wireless" => Some(Pipeline::LayoutSlanetWireless),
            "layout+slanet-plus" | "layout-slanet-plus" => Some(Pipeline::LayoutSlanetPlus),
            "layout+slanet-auto" | "layout-slanet-auto" | "layout+slanet" | "layout-slanet" => {
                Some(Pipeline::LayoutSlanetAuto)
            }
            "native" => Some(Pipeline::Native),
            "native+layout" | "native-layout" => Some(Pipeline::NativeLayout),
            "native+layout+reading-order" | "native-layout-reading-order" => Some(Pipeline::NativeReadingOrder),
            "candle-trocr" | "candle_trocr" | "trocr" => Some(Pipeline::CandleTrocr),
            "candle-paddleocr-vl" | "candle_paddleocr_vl" | "paddleocr-vl" => Some(Pipeline::CandlePaddleocrVl),
            "candle-glm-ocr" | "candle_glm_ocr" | "glm-ocr" => Some(Pipeline::CandleGlmOcr),
            "candle-glm-ocr+layout" | "candle_glm_ocr_layout" | "glm-ocr+layout" | "glm-ocr-layout" => {
                Some(Pipeline::CandleGlmOcrLayout)
            }
            "candle-glm-ocr+layout+chart"
            | "candle_glm_ocr_layout_chart"
            | "glm-ocr+layout+chart"
            | "glm-ocr-layout-chart" => Some(Pipeline::CandleGlmOcrLayoutChart),
            "candle-deepseek-ocr" | "candle_deepseek_ocr" | "deepseek-ocr" => Some(Pipeline::CandleDeepseekOcr),
            "candle-paddleocr-vl-15" | "candle_paddleocr_vl_15" | "paddleocr-vl-15" => {
                Some(Pipeline::CandlePaddleocrVl15)
            }
            _ => None,
        }
    }

    /// All pipelines that use xberg in-process extraction.
    ///
    /// `CandleTrocr`, `CandlePaddleocrVl`, and the new Candle VLM backends
    /// (`CandleDeepseekOcr`, `CandlePaddleocrVl15`) are
    /// deliberately omitted from `all_xberg()`: they need large model
    /// downloads from HuggingFace and only build with their own feature flags,
    /// so default cross-pipeline runs do not include them.
    /// Named Paddle quality-sweep presets are also omitted so experimental configurations cannot
    /// silently expand release benchmark matrices. ~keep
    /// `CandleGlmOcr` is included because the `glm-ocr-bench` feature gates
    /// the entire harness build, making the inclusion safe.
    pub fn all_xberg() -> Vec<Pipeline> {
        vec![
            Pipeline::Baseline,
            Pipeline::Layout,
            Pipeline::Tesseract,
            Pipeline::TesseractLayout,
            Pipeline::Paddle,
            Pipeline::PaddleLayout,
            Pipeline::PaddleV6Small,
            Pipeline::PaddleV6SmallLayout,
            Pipeline::PaddleV6Tiny,
            Pipeline::PaddleV6TinyLayout,
            Pipeline::Native,
            Pipeline::NativeLayout,
            Pipeline::CandleGlmOcr,
        ]
    }
}
