//! Registry-routing and `parse_options` tests for the Candle VLM-OCR backends.
//!
//! These tests verify that:
//! - `OcrBackendRegistry::new` seeds each backend under its canonical name.
//! - Unknown backend names return `None` from the registry (via `list()`).
//! - Network-gated e2e tests (tagged `#[ignore]`) drive real inference through the
//!   unified `OcrBackend::process_image` interface with device=auto to respect GPU
//!   acceleration when built with `candle-cuda`.
//!
//! `parse_options` is a private inherent method on each backend, so it cannot be
//! exercised from this external test crate. Its defaults + non-object JSON
//! resilience + empty-object-defaults behaviour is covered by the `#[cfg(test)]`
//! unit tests inside each backend module instead (`glm_ocr_backend.rs`,
//! `deepseek_ocr_backend.rs`, `paddleocr_vl_backend.rs`).
//!
//! Models are gated uniformly via `XBERG_REQUIRE_MODELS`:
//! - When unset or falsy: tests skip gracefully if required weights/env vars are missing.
//! - When set to "1", "true", or "yes": tests panic if weights cannot be obtained.
//!
//! Run just the registry tests (no network required):
//! ```
//! cargo test -p xberg --features candle-vlm-ocr --test candle_backends
//! ```
//!
//! Run e2e tests with local weights (GLM/TrOCR auto-download):
//! ```
//! XBERG_REQUIRE_MODELS=1 \
//! cargo test -p xberg --features candle-vlm-ocr --test candle_backends -- --ignored --nocapture
//! ```
//!
//! Run e2e tests with local-weight models (supply via environment):
//! ```
//! XBERG_REQUIRE_MODELS=1 \
//! XBERG_DEEPSEEK_OCR_MODEL_PATH=/models/deepseek \
//! XBERG_PADDLEOCR_VL_MODEL_PATH=/models/paddleocr-vl \
//! cargo test -p xberg --features candle-vlm-ocr --test candle_backends -- --ignored --nocapture
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "candle-ocr")]

use xberg::core::config::OcrConfig;
use xberg::plugins::registry::OcrBackendRegistry;

fn new_registry_names() -> Vec<String> {
    let registry = OcrBackendRegistry::new();
    registry.list()
}

/// Determine whether missing required weights should cause a panic or a skip.
///
/// When `XBERG_REQUIRE_MODELS` is set to a truthy value ("1", "true", "yes"),
/// missing weights cause a panic, so CI test failures are visible. Otherwise,
/// missing weights cause a graceful skip for local development without model cache.
fn require_models() -> bool {
    matches!(
        std::env::var("XBERG_REQUIRE_MODELS").as_deref(),
        Ok("1" | "true" | "yes")
    )
}

/// Check for a required local model path environment variable.
///
/// - If the env var is set: return the path.
/// - If unset and `XBERG_REQUIRE_MODELS` is truthy: panic with a helpful message.
/// - If unset and `XBERG_REQUIRE_MODELS` is falsy: return None (for graceful skip).
///
/// Only the local-weight models call this; gate it so single-model feature
/// builds (e.g. the per-model GPU matrix) don't see it as dead code.
#[cfg(any(feature = "candle-deepseek-ocr", feature = "candle-paddleocr-vl"))]
fn check_local_model_path(env_var: &str, model_name: &str) -> Option<String> {
    match std::env::var(env_var) {
        Ok(p) => Some(p),
        Err(_) => {
            if require_models() {
                panic!(
                    "{} model path required: set {} env var pointing to local model weights",
                    model_name, env_var
                );
            } else {
                println!("{} not set — skipping {} e2e test", env_var, model_name);
                None
            }
        }
    }
}

/// The global registry seeds a "candle-glm-ocr" backend when the feature is on.
#[cfg(feature = "candle-glm-ocr")]
#[test]
fn registry_resolves_candle_glm_ocr_backend() {
    let names = new_registry_names();
    assert!(
        names.contains(&"candle-glm-ocr".to_string()),
        "Expected 'candle-glm-ocr' in registry; got: {:?}",
        names,
    );
}

/// The global registry seeds a "candle-deepseek-ocr" backend when the feature is on.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
#[test]
fn registry_resolves_candle_deepseek_ocr_backend() {
    let names = new_registry_names();
    assert!(
        names.contains(&"candle-deepseek-ocr".to_string()),
        "Expected 'candle-deepseek-ocr' in registry; got: {:?}",
        names,
    );
}

/// The global registry seeds a "candle-paddleocr-vl" backend when the feature is on.
#[cfg(feature = "candle-paddleocr-vl")]
#[test]
fn registry_resolves_candle_paddleocr_vl_backend() {
    let names = new_registry_names();
    assert!(
        names.contains(&"candle-paddleocr-vl".to_string()),
        "Expected 'candle-paddleocr-vl' in registry; got: {:?}",
        names,
    );
}

/// An unknown candle backend name is not present in the registry.
#[test]
fn registry_returns_none_for_unknown_candle_backend() {
    let names = new_registry_names();
    assert!(
        !names.contains(&"candle-doesnotexist".to_string()),
        "Expected 'candle-doesnotexist' to be absent from registry; got: {:?}",
        names,
    );
}

// Network-gated e2e tests — #[ignore] until env vars are set.

/// End-to-end DeepSeek-OCR extraction through `OcrBackend::process_image`.
///
/// Requires `XBERG_DEEPSEEK_OCR_MODEL_PATH` to point to a local model directory.
/// Uses device=auto to respect GPU acceleration when built with `candle-cuda`.
/// Skip cleanly when the variable is absent (unless XBERG_REQUIRE_MODELS=1).
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
#[tokio::test]
#[ignore = "requires XBERG_DEEPSEEK_OCR_MODEL_PATH env var pointing to local model weights"]
async fn candle_deepseek_ocr_e2e_extraction() {
    let model_path = match check_local_model_path("XBERG_DEEPSEEK_OCR_MODEL_PATH", "DeepSeek-OCR") {
        Some(p) => p,
        None => return,
    };

    use xberg::candle_ocr::DeepseekOcrBackend;
    use xberg::plugins::OcrBackend as _;

    let image_bytes = include_bytes!("../../../fixtures/images/test_hello_world.png");

    let backend = DeepseekOcrBackend::new();

    let config = OcrConfig {
        backend_options: Some(serde_json::json!({"model_path": model_path})),
        ..Default::default()
    };

    let result = backend
        .process_image(image_bytes, &config)
        .await
        .expect("DeepseekOcrBackend::process_image should succeed");

    assert!(
        !result.content.is_empty(),
        "DeepSeek-OCR extraction returned empty content"
    );
    assert_eq!(
        result.mime_type.as_ref(),
        "text/markdown",
        "DeepSeek-OCR must emit text/markdown"
    );

    println!(
        "DeepSeek-OCR result ({} chars): {}",
        result.content.len(),
        result.content
    );
}

/// Build a minimal, valid single-page Letter (612x792 pt) PDF whose content stream shows
/// `lines` as Helvetica text, one per line starting at the top margin.
///
/// Hand-rolled rather than pulled in via a PDF-writing dependency: object byte offsets are
/// computed as the buffer is built (never by hand-counting), which is the usual source of a
/// broken minimal PDF.
#[cfg(all(feature = "candle-deepseek-ocr", feature = "pdf", not(target_arch = "wasm32")))]
fn build_letter_prose_pdf(lines: &[String]) -> Vec<u8> {
    let mut content = String::from("BT\n/F1 11 Tf\n72 720 Td\n14 TL\n");
    for (index, line) in lines.iter().enumerate() {
        let escaped = line.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
        if index > 0 {
            content.push_str("T*\n");
        }
        content.push_str(&format!("({escaped}) Tj\n"));
    }
    content.push_str("ET");

    let mut pdf = Vec::new();
    let mut offsets = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content.as_bytes());
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n");
    pdf.extend_from_slice(format!("{xref_offset}\n").as_bytes());
    pdf.extend_from_slice(b"%%EOF");

    pdf
}

/// Greedy word wrap of `text` into lines of at most `max_chars` characters.
#[cfg(all(feature = "candle-deepseek-ocr", feature = "pdf", not(target_arch = "wasm32")))]
fn wrap_words(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Width and height from a PNG's IHDR chunk (bytes 16..24: signature(8) + length(4) + "IHDR"(4)).
#[cfg(all(feature = "candle-deepseek-ocr", feature = "pdf", not(target_arch = "wasm32")))]
fn png_dimensions(png_bytes: &[u8]) -> (u32, u32) {
    let width = u32::from_be_bytes(png_bytes[16..20].try_into().expect("PNG must have an IHDR width"));
    let height = u32::from_be_bytes(png_bytes[20..24].try_into().expect("PNG must have an IHDR height"));
    (width, height)
}

/// Network-free regression check that the synthetic Letter-page PDF fixture used by
/// `candle_deepseek_ocr_reads_a_full_letter_page_of_prose` actually renders, and renders to
/// exactly the 1275x1650 px size that `crop_grid_for_selects_2x3_for_a_letter_page_at_150dpi`
/// (`deepseek_ocr/engine.rs`) proves selects a (2, 3) local-crop grid. Catches a broken PDF
/// fixture or a page-size regression without needing the network-gated test above.
#[cfg(all(feature = "candle-deepseek-ocr", feature = "pdf", not(target_arch = "wasm32")))]
#[test]
fn letter_prose_pdf_fixture_renders_at_the_size_that_selects_a_2x3_crop_grid() {
    use xberg::render_pdf_page_to_png;

    let lines = vec!["Sample prose line for rendering-size verification.".to_string()];
    let pdf_bytes = build_letter_prose_pdf(&lines);
    let png_bytes =
        render_pdf_page_to_png(&pdf_bytes, 0, Some(150), None).expect("synthetic Letter-page PDF must render");

    assert_eq!(
        png_dimensions(&png_bytes),
        (1275, 1650),
        "Letter page at 150 dpi must render to 1275x1650 px"
    );
}

/// End-to-end DeepSeek-OCR page-coverage regression test for #1674: a full Letter page of
/// prose, rendered at 150 dpi (1275x1650 px -- the size `crop_grid_for_selects_2x3_for_a_letter_page_at_150dpi`
/// in `deepseek_ocr/engine.rs` proves selects a (2, 3) local-crop grid), must come back
/// substantially whole rather than the 15-25% page coverage reported in the issue.
///
/// Uses the auto-download default model id (no `backend_options.model_path`), so this also
/// exercises `model_stager::ensure_deepseek_ocr` end to end. Device/dtype come from
/// `XBERG_CANDLE_DEVICE` / `XBERG_CANDLE_DTYPE` (e.g. `metal` / `f16` on Apple Silicon);
/// unset defaults to `auto`/`auto` (device auto-detect, dtype derived from the resolved
/// device).
#[cfg(all(feature = "candle-deepseek-ocr", feature = "pdf", not(target_arch = "wasm32")))]
#[tokio::test]
#[ignore = "downloads the 6.7 GB DeepSeek-OCR checkpoint from HuggingFace Hub on first run"]
async fn candle_deepseek_ocr_reads_a_full_letter_page_of_prose() {
    use xberg::candle_ocr::DeepseekOcrBackend;
    use xberg::plugins::OcrBackend as _;
    use xberg::render_pdf_page_to_png;

    const PARAGRAPHS: [&str; 6] = [
        "The quick brown fox jumps over the lazy dog near the riverbank every single morning before \
         the sun has fully risen above the eastern hills, and the old town clock strikes seven while \
         the bakery on the corner opens its doors to the first customers of the day, filling the \
         crisp autumn air with the warm scent of fresh bread and roasted coffee beans.",
        "Delivery trucks rumble slowly down the cobblestone street past the small bookshop that has \
         stood on this corner for nearly a century, its window display changing with the seasons but \
         its hand-painted sign never once repainted, a fact the owner mentions to anyone who lingers \
         long enough at the counter to hear the story told from the beginning.",
        "By nine the market square fills with vendors unfolding canvas awnings over crates of pears, \
         late tomatoes and bundled herbs, while a violinist near the fountain tunes against the noise \
         of the tram bell and the schoolchildren cutting through on their way to the river path that \
         curves north toward the mill and the footbridge beyond it.",
        "In the afternoon the light shifts to the west side of the street and the cafe tables move \
         with it, chairs scraping across the flagstones as regulars trade seats for shade, and the \
         last of the morning bread is marked down and sold in paper bags to students who eat it on \
         the steps of the library before the reading room opens again at four.",
        "Evening brings the ferry horn from the lower harbour and the slow return of the fishing \
         boats, their decks stacked with grey crates that the dock crew passes hand to hand into \
         the cold store, while gulls settle on the breakwater and the harbourmaster logs each \
         arrival in a ledger he still keeps in pencil despite the terminal on his desk.",
        "After dark the square empties except for the couple who run the late kiosk, selling \
         newspapers, matches and the last sandwiches of the day to the night shift heading for \
         the station, and the town settles into the quiet hum of the streetlamps until the bakery \
         ovens are lit again a little before five and the whole cycle begins once more.",
    ];
    // ~keep: 11 pt Helvetica averages ~5.5 pt per character, so a 468 pt text column holds
    // about 85 characters; longer lines run off the page unrendered and the model correctly
    // reads only their visible heads (447 chars for four 484-char lines), which is exactly the
    // fixture defect this wrapping exists to prevent.
    const MAX_LINE_CHARS: usize = 80;
    let lines: Vec<String> = PARAGRAPHS
        .iter()
        .flat_map(|paragraph| wrap_words(paragraph, MAX_LINE_CHARS))
        .collect();
    let source_chars: usize = lines.iter().map(|l| l.len()).sum();
    assert!(
        source_chars > 1900,
        "test fixture prose should be roughly 2000 chars, got {source_chars}"
    );

    let pdf_bytes = build_letter_prose_pdf(&lines);
    let png_bytes =
        render_pdf_page_to_png(&pdf_bytes, 0, Some(150), None).expect("synthetic Letter-page PDF must render");

    let mut backend_options = serde_json::json!({});
    if let Ok(device) = std::env::var("XBERG_CANDLE_DEVICE") {
        backend_options["device"] = serde_json::json!(device);
    }
    if let Ok(dtype) = std::env::var("XBERG_CANDLE_DTYPE") {
        backend_options["dtype"] = serde_json::json!(dtype);
    }

    let backend = DeepseekOcrBackend::new();
    let config = OcrConfig {
        backend_options: Some(backend_options),
        ..Default::default()
    };

    let result = backend
        .process_image(&png_bytes, &config)
        .await
        .expect("DeepseekOcrBackend::process_image should succeed on a rendered Letter page");

    println!(
        "DeepSeek-OCR Letter-page result ({} chars, source was {} chars):\n{}",
        result.content.len(),
        source_chars,
        result.content
    );

    assert!(
        result.content.len() > 1500,
        "expected substantial page coverage (>1500 chars) from a ~{source_chars}-char source page, got {} chars",
        result.content.len()
    );
}

/// End-to-end PaddleOCR-VL extraction through `OcrBackend::process_image`.
///
/// Requires `XBERG_PADDLEOCR_VL_MODEL_PATH` to point to a local model directory.
/// Uses device=auto to respect GPU acceleration when built with `candle-cuda`.
/// Skip cleanly when the variable is absent (unless XBERG_REQUIRE_MODELS=1).
#[cfg(feature = "candle-paddleocr-vl")]
#[tokio::test]
#[ignore = "requires XBERG_PADDLEOCR_VL_MODEL_PATH env var pointing to local model weights"]
async fn candle_paddleocr_vl_e2e_extraction() {
    let model_path = match check_local_model_path("XBERG_PADDLEOCR_VL_MODEL_PATH", "PaddleOCR-VL") {
        Some(p) => p,
        None => return,
    };

    use xberg::candle_ocr::PaddleOcrVlBackend;
    use xberg::plugins::OcrBackend as _;
    use xberg_candle_ocr::models::PaddleOcrVlTask;

    let image_bytes = include_bytes!("../../../fixtures/images/test_hello_world.png");

    let backend = PaddleOcrVlBackend::new(PaddleOcrVlTask::default());

    let config = OcrConfig {
        backend_options: Some(serde_json::json!({"model_path": model_path})),
        ..Default::default()
    };

    let result = backend
        .process_image(image_bytes, &config)
        .await
        .expect("PaddleOcrVlBackend::process_image should succeed");

    assert!(
        !result.content.is_empty(),
        "PaddleOCR-VL extraction returned empty content"
    );
    assert!(
        result.content.to_lowercase().contains("hello world"),
        "PaddleOCR-VL should read the fixture text, got: {}",
        result.content
    );
    assert_eq!(
        result.mime_type.as_ref(),
        "text/markdown",
        "PaddleOCR-VL must emit text/markdown"
    );

    println!(
        "PaddleOCR-VL result ({} chars): {}",
        result.content.len(),
        result.content
    );
}

/// End-to-end GLM-OCR extraction through `OcrBackend::process_image`.
///
/// GLM-OCR auto-downloads weights from HuggingFace Hub (~3 GB on first run, cached).
/// Uses device=auto to respect GPU acceleration when built with `candle-cuda`.
/// When XBERG_REQUIRE_MODELS=1: fails if download or inference fails (for CI).
/// Otherwise: skips gracefully if the download/inference fails (for local dev).
#[cfg(feature = "candle-glm-ocr")]
#[tokio::test]
#[ignore = "downloads ~3 GB of GLM-OCR weights from HuggingFace Hub on first run"]
async fn candle_glm_ocr_e2e_extraction() {
    use xberg::candle_ocr::GlmOcrBackend;
    use xberg::candle_ocr::glm_ocr_backend::LayoutMode;
    use xberg::plugins::OcrBackend as _;
    use xberg_candle_ocr::models::GlmOcrTask;

    let image_bytes = include_bytes!("../../../fixtures/images/test_hello_world.png");

    let backend = GlmOcrBackend::new(GlmOcrTask::default(), LayoutMode::default());

    let config = OcrConfig::default();

    let result = match backend.process_image(image_bytes, &config).await {
        Ok(r) => r,
        Err(e) => {
            if require_models() {
                panic!("GLM-OCR inference failed (XBERG_REQUIRE_MODELS=1): {}", e);
            } else {
                println!("GLM-OCR inference failed (dev mode); skipping: {}", e);
                return;
            }
        }
    };

    assert!(!result.content.is_empty(), "GLM-OCR extraction returned empty content");
    assert_eq!(
        result.mime_type.as_ref(),
        "text/markdown",
        "GLM-OCR must emit text/markdown"
    );

    println!("GLM-OCR result ({} chars): {}", result.content.len(), result.content);
}

/// End-to-end TrOCR extraction through `OcrBackend::process_image`.
///
/// TrOCR auto-downloads weights from HuggingFace Hub (~1.5 GB on first run, cached).
/// Uses device=auto to respect GPU acceleration when built with `candle-cuda`.
/// When XBERG_REQUIRE_MODELS=1: fails if download or inference fails (for CI).
/// Otherwise: skips gracefully if the download/inference fails (for local dev).
#[cfg(feature = "candle-trocr")]
#[tokio::test]
#[ignore = "downloads ~1.5 GB of TrOCR weights from HuggingFace Hub on first run"]
async fn candle_trocr_e2e_extraction() {
    use xberg::candle_ocr::TrocrBackend;
    use xberg::plugins::OcrBackend as _;
    use xberg_candle_ocr::models::TrocrVariant;

    let image_bytes = include_bytes!("../../../fixtures/images/test_hello_world.png");

    let backend = TrocrBackend::new(TrocrVariant::default());

    let config = OcrConfig::default();

    let result = match backend.process_image(image_bytes, &config).await {
        Ok(r) => r,
        Err(e) => {
            if require_models() {
                panic!("TrOCR inference failed (XBERG_REQUIRE_MODELS=1): {}", e);
            } else {
                println!("TrOCR inference failed (dev mode); skipping: {}", e);
                return;
            }
        }
    };

    assert!(!result.content.is_empty(), "TrOCR extraction returned empty content");
    assert_eq!(result.mime_type.as_ref(), "text/plain", "TrOCR must emit text/plain");

    println!("TrOCR result ({} chars): {}", result.content.len(), result.content);
}
