//! Regression test for https://github.com/xberg-io/xberg/issues/1651
//!
//! A caller's configured `security_limits` never reached the Tesseract backend's image
//! decode. `TesseractConfig` had no `security_limits` field, so `config_to_tesseract`
//! could not carry `OcrConfig::security_limits` even where a route injected it, and the
//! standalone-image route never injected it in the first place. Every Tesseract OCR decode
//! therefore ran under `SecurityLimits::default()`.
//!
//! The limit is exercised in the *refusing* direction: a limit low enough to reject the
//! decode is observable, whereas a raised limit is indistinguishable from the default that
//! already permits the image. `max_content_size` is compared against the live decoded byte
//! count (`extraction/image_decode.rs::validate`), so a 64x64 image (12288 decoded RGB
//! bytes) is rejected at 5000 while its ~200 raw PNG bytes still clear every byte-size gate
//! upstream -- the limit under test is the only thing that can fail these cases. The 4096
//! pixel count is deliberately *below* 5000: GH#1761 removed a pixel-count comparison
//! against this byte budget, and these cases pass on the byte count alone either way. ~keep

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "ocr")]

use xberg::core::config::{ExtractInput, ExtractionConfig, OcrConfig};
use xberg::extractors::security::SecurityLimits;

/// Rejects the 64x64 decode (12288 live bytes) while admitting the raw PNG bytes.
const DISCRIMINATING_MAX_CONTENT_SIZE: usize = 5_000;

fn tiny_png() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]));
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .expect("encode png");
    bytes
}

fn limits() -> SecurityLimits {
    SecurityLimits {
        max_content_size: DISCRIMINATING_MAX_CONTENT_SIZE,
        ..Default::default()
    }
}

fn ocr_config() -> OcrConfig {
    OcrConfig {
        backend: "tesseract".to_string(),
        ..Default::default()
    }
}

/// Collects the refusal text from EITHER channel `extract()` can report a rejection on:
/// the outer `Err`, or a non-fatal `ExtractionResult::errors` entry. Asserting only on the
/// outer `Err` would fail for the wrong reason if the limit surfaces as a per-input error. ~keep
async fn extract_png_failure_text(config: &ExtractionConfig) -> String {
    match xberg::extract(
        ExtractInput::from_bytes(tiny_png(), "image/png", Some("gh1651.png".to_string())),
        config,
    )
    .await
    {
        Err(error) => error.to_string(),
        Ok(result) => {
            assert!(
                !result.errors.is_empty(),
                "extraction succeeded with no error on either channel: the configured limit \
                 never reached the OCR decode. results={} content_len={:?}",
                result.results.len(),
                result.results.first().map(|doc| doc.content.len())
            );
            result
                .errors
                .iter()
                .map(|item| item.message.clone())
                .collect::<Vec<_>>()
                .join(" | ")
        }
    }
}

/// Control. Without a constraining limit the same image must extract, so a failure in the
/// two cases below can only be the configured limit and never an unrelated fault. ~keep
#[tokio::test]
async fn should_run_ocr_on_the_image_when_no_constraining_limit_is_configured() {
    let config = ExtractionConfig {
        ocr: Some(ocr_config()),
        ..Default::default()
    };

    let result = xberg::extract(
        ExtractInput::from_bytes(tiny_png(), "image/png", Some("gh1651.png".to_string())),
        &config,
    )
    .await
    .expect("control: the image must extract under default limits");

    let document = result.results.first().expect("control: one document");
    assert_eq!(
        document.extraction_method,
        Some(xberg::types::ExtractionMethod::Ocr),
        "control: OCR must actually run, otherwise the decode under test is never reached and \
         the limit cases below can never pass. errors={:?}",
        result.errors
    );
}

#[tokio::test]
async fn should_reject_the_ocr_decode_when_extraction_config_security_limits_are_configured() {
    let config = ExtractionConfig {
        ocr: Some(ocr_config()),
        security_limits: Some(limits()),
        ..Default::default()
    };

    let error = extract_png_failure_text(&config).await;

    assert!(
        error.contains(&DISCRIMINATING_MAX_CONTENT_SIZE.to_string()),
        "the refusal must cite the configured limit, not a default: {error}"
    );
}

#[tokio::test]
async fn should_reject_the_ocr_decode_when_ocr_config_security_limits_are_configured() {
    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            security_limits: Some(limits()),
            ..ocr_config()
        }),
        ..Default::default()
    };

    let error = extract_png_failure_text(&config).await;

    assert!(
        error.contains(&DISCRIMINATING_MAX_CONTENT_SIZE.to_string()),
        "the refusal must cite the configured limit, not a default: {error}"
    );
}

/// The reporter's own scenario, and the only case that exercises the limit in the
/// *raising* direction. Lowering a limit is satisfied by any gate upstream of OCR, so it
/// cannot distinguish "the OCR decode honours the caller's limit" from "something earlier
/// refused first". Raising it above `SecurityLimits::default()` can only be satisfied by
/// every gate on the path, the OCR decode included. 5000x7100 RGB is 106_500_000 live
/// bytes, just over the 104_857_600 default -- the reporter's exact dimensions. ~keep
#[tokio::test]
async fn should_ocr_a_large_image_when_the_caller_raises_the_limit_above_the_default() {
    const WIDTH: u32 = 5_000;
    const HEIGHT: u32 = 7_100;
    let raised = SecurityLimits {
        max_content_size: 5 * 1024 * 1024 * 1024,
        ..Default::default()
    };

    let image = image::RgbImage::from_pixel(WIDTH, HEIGHT, image::Rgb([255, 255, 255]));
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .expect("encode large png");

    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            security_limits: Some(raised.clone()),
            ..ocr_config()
        }),
        security_limits: Some(raised),
        ..Default::default()
    };

    let result = xberg::extract(
        ExtractInput::from_bytes(bytes, "image/png", Some("gh1651_large.png".to_string())),
        &config,
    )
    .await;

    match result {
        Ok(output) => assert!(
            output.errors.is_empty(),
            "a raised limit must admit a {WIDTH}x{HEIGHT} image, but it was refused: {:?}",
            output.errors
        ),
        Err(error) => panic!("a raised limit must admit a {WIDTH}x{HEIGHT} image, but: {error}"),
    }
}

fn pdf_fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ocr")
        .join(name)
}

/// Collects the refusal text from EVERY channel a PDF-route OCR rejection can surface on:
/// the outer `Err`, `ExtractionResult::errors`, and -- the reporter's own case -- a
/// `ProcessingWarning` on the document when a targeted page fallback fails. ~keep
async fn extract_pdf_failure_text(name: &str, config: &ExtractionConfig) -> String {
    let bytes = std::fs::read(pdf_fixture(name)).expect("fixture must exist");
    match xberg::extract(
        ExtractInput::from_bytes(bytes, "application/pdf", Some(name.to_string())),
        config,
    )
    .await
    {
        Err(error) => error.to_string(),
        Ok(result) => {
            let mut messages: Vec<String> = result.errors.iter().map(|item| item.message.clone()).collect();
            messages.extend(
                result
                    .results
                    .iter()
                    .flat_map(|doc| doc.processing_warnings.iter())
                    .map(|warning| warning.message.to_string()),
            );
            assert!(
                !messages.is_empty(),
                "PDF extraction succeeded with no error or warning on any channel: the configured \
                 limit never reached the OCR decode. content={:?}",
                result.results.first().map(|doc| doc.content.clone())
            );
            messages.join(" | ")
        }
    }
}

/// The reporter's real case: a mixed native/scanned PDF whose scanned pages go through the
/// targeted OCR fallback. The limit is set on `OcrConfig` ONLY, deliberately: every page
/// render gate on this route reads `ExtractionConfig::security_limits`, so leaving that
/// `None` keeps the render under the default and makes the Tesseract decode the only gate
/// that can cite `5000`. Before the fix `config_to_tesseract` dropped the value and the
/// page OCR'd successfully. ~keep
#[tokio::test]
async fn should_reject_the_targeted_pdf_fallback_decode_when_ocr_config_security_limits_are_configured() {
    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            security_limits: Some(limits()),
            ..ocr_config()
        }),
        ..Default::default()
    };

    let text = extract_pdf_failure_text("mixed_native_scanned.pdf", &config).await;

    assert!(
        text.contains(&DISCRIMINATING_MAX_CONTENT_SIZE.to_string()),
        "the refusal must cite the OcrConfig limit, not a default: {text}"
    );
}

/// Same channel on the image-only route (`pipeline.rs`'s full-document scanned-page path),
/// which prepares its `OcrConfig` at a different site from the mixed route above. ~keep
#[tokio::test]
async fn should_reject_the_scanned_pdf_decode_when_ocr_config_security_limits_are_configured() {
    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            security_limits: Some(limits()),
            ..ocr_config()
        }),
        ..Default::default()
    };

    let text = extract_pdf_failure_text("scanned_hello.pdf", &config).await;

    assert!(
        text.contains(&DISCRIMINATING_MAX_CONTENT_SIZE.to_string()),
        "the refusal must cite the OcrConfig limit, not a default: {text}"
    );
}
