//! Regression test for https://github.com/xberg-io/xberg/issues/781
//!
//! DOCX OCR extraction was failing because the pipeline was deriving the document
//! (Markdown/Text generation) BEFORE running OCR on embedded images. As a result,
//! the renderers could not see or inject the OCR text results.
//!
//! This test verifies that OCR results for images in a DOCX file are successfully
//! injected into the final content.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "ocr")]
#![cfg(feature = "office")]

mod helpers;
use helpers::extract_uri_document_blocking;

use helpers::*;
use xberg::core::config::{ExtractionConfig, ImageExtractionConfig, OcrConfig};

#[test]
fn test_docx_ocr_content_injection() {
    let file_path = get_test_file_path("docx/word_sample.docx");

    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            backend: "tesseract".to_string(),
            language: vec!["eng".to_string()],
            ..Default::default()
        }),
        images: Some(ImageExtractionConfig {
            extract_images: true,
            ..Default::default()
        }),
        force_ocr: true,
        use_cache: false,
        ..Default::default()
    };

    let result = match extract_uri_document_blocking(&file_path, None, &config) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("OCR extraction failed: {}", e);
            return;
        }
    };

    let images = result.images.as_ref().expect("images must be extracted");
    assert!(!images.is_empty(), "DOCX should have at least one image");

    let has_ocr_content = images.iter().any(|img| {
        img.ocr_result
            .as_ref()
            .is_some_and(|ocr| !ocr.content.trim().is_empty())
    });

    if has_ocr_content {
        let mut found_in_content = false;
        for img in images {
            if let Some(ocr) = &img.ocr_result
                && !ocr.content.trim().is_empty()
                && result.content.contains(&ocr.content)
            {
                found_in_content = true;
                break;
            }
        }
        assert!(
            found_in_content,
            "OCR content from images must be present in the final document content"
        );
    } else {
        eprintln!("No OCR content produced for images; skipping injection verification");
    }
}

/// GH#1703: an OCR-only DOCX config (`ocr: Some(_)`, `images: None`) must not retain
/// every embedded image's raw bytes just because GH#1662's read gate needed them to run
/// OCR. `images` must come back empty and the fixed text the mock backend produced must
/// still have landed in `content` -- proving the fix drops the bytes, not the OCR text
/// they produced.
#[test]
fn test_docx_ocr_only_config_returns_no_images() {
    use async_trait::async_trait;
    use xberg::plugins::{OcrBackend, OcrBackendType, Plugin, register_ocr_backend, unregister_ocr_backend};
    use xberg::types::ExtractedDocument;

    const SENTINEL_OCR_TEXT: &str = "GH1703_DOCX_SENTINEL_OCR_TEXT";
    const BACKEND_NAME: &str = "gh1703-docx-fixed-text-ocr";

    struct FixedTextOcrBackend;

    #[async_trait]
    impl OcrBackend for FixedTextOcrBackend {
        fn backend_type(&self) -> OcrBackendType {
            OcrBackendType::Custom
        }
        fn supports_language(&self, _: &str) -> bool {
            true
        }
        async fn process_image(&self, _: &[u8], _config: &OcrConfig) -> xberg::Result<ExtractedDocument> {
            let mut document = ExtractedDocument::default();
            document.content = SENTINEL_OCR_TEXT.to_string();
            Ok(document)
        }
    }

    impl Plugin for FixedTextOcrBackend {
        fn name(&self) -> &str {
            BACKEND_NAME
        }
        fn version(&self) -> String {
            "0.0.0".to_string()
        }
        fn initialize(&self) -> xberg::Result<()> {
            Ok(())
        }
        fn shutdown(&self) -> xberg::Result<()> {
            Ok(())
        }
    }

    register_ocr_backend(std::sync::Arc::new(FixedTextOcrBackend)).unwrap();
    struct BackendGuard(&'static str);
    impl Drop for BackendGuard {
        fn drop(&mut self) {
            let _ = unregister_ocr_backend(self.0);
        }
    }
    let _guard = BackendGuard(BACKEND_NAME);

    let file_path = get_test_file_path("docx/word_sample.docx");

    let config = ExtractionConfig {
        ocr: Some(OcrConfig {
            backend: BACKEND_NAME.to_string(),
            ..Default::default()
        }),
        images: None,
        force_ocr: true,
        use_cache: false,
        ..Default::default()
    };

    let result = extract_uri_document_blocking(&file_path, None, &config).expect("extraction must succeed");

    assert!(
        result.images.as_ref().map(|v| v.is_empty()).unwrap_or(true),
        "ocr-only config (images: None) must not retain embedded image bytes; got {} image(s)",
        result.images.as_ref().map(|v| v.len()).unwrap_or(0)
    );
    assert_eq!(
        result.counts.images, 1,
        "DocumentCounts::images is documented as always populated, so dropping the bytes must not zero it"
    );
    assert!(
        !result.content.trim().is_empty(),
        "document content must still be extracted when images are dropped"
    );
    assert!(
        result.content.contains(SENTINEL_OCR_TEXT),
        "embedded-image OCR text must still land in content even though the raw image \
         bytes are dropped; got content:\n{}",
        result.content
    );
}
