//! End-to-end integration test for `GlmOcrBackend` through the `OcrBackend` trait.
//!
//! Constructs the backend directly via its public constructor and drives
//! `process_image` through the trait surface, verifying that the full wiring
//! from `xberg::candle_ocr::GlmOcrBackend` down to `xberg-candle-ocr`
//! produces coherent output.
//!
//! Run with:
//! `cargo test -p xberg --features candle-glm-ocr --test glm_ocr_backend -- --ignored --nocapture`
//!
//! `glm_ocr_paired_keeps_hex_identifier_intact` additionally needs the `pdf` feature (it
//! renders a synthetic PDF page to feed the backend):
//! `cargo test -p xberg --features candle-glm-ocr,pdf --test glm_ocr_backend -- --ignored --nocapture`

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "candle-glm-ocr")]

use xberg::candle_ocr::{GlmOcrBackend, glm_ocr_backend::LayoutMode};
use xberg::core::config::OcrConfig;
use xberg::plugins::OcrBackend;
use xberg_candle_ocr::models::GlmOcrTask;

/// End-to-end test driving `GlmOcrBackend` through the `OcrBackend` trait.
///
/// Downloads ~3GB of GLM-OCR model weights on first run (cached in
/// ~/.cache/huggingface). Subsequent runs use cached weights.
#[tokio::test]
#[ignore = "downloads ~3GB of GLM-OCR weights from HuggingFace Hub"]
async fn glm_ocr_backend_process_image_returns_hello_world() {
    let image_bytes = include_bytes!("../../../fixtures/images/test_hello_world.png");

    let backend = GlmOcrBackend::new(GlmOcrTask::default(), LayoutMode::WholePage);

    let config = OcrConfig::default();

    eprintln!("Calling GlmOcrBackend::process_image through OcrBackend trait...");
    let result = backend
        .process_image(image_bytes, &config)
        .await
        .expect("GlmOcrBackend::process_image should succeed");

    eprintln!("process_image returned successfully");
    eprintln!("Content length: {} chars", result.content.len());
    eprintln!("MIME type: {}", result.mime_type);
    eprintln!("Content:\n{}", result.content);

    assert!(!result.content.is_empty(), "Extracted content should not be empty");

    let lower = result.content.to_lowercase();
    assert!(
        lower.contains("hello") || lower.contains("world"),
        "Expected content to contain \"hello\" or \"world\"; got {:?}",
        result.content
    );

    let run = longest_repeated_ngram_run(&result.content, 3);
    assert!(
        run < 5,
        "Detected degenerate-repeat output (longest 3-gram run = {}): {}...",
        run,
        &result.content[..200.min(result.content.len())]
    );

    eprintln!("\n✓ GlmOcrBackend end-to-end test passed!");
}

/// GH#1675: `apply_repetition_penalty`'s old default (1.1) scaled per OCCURRENCE over the
/// whole decode history, exponentially suppressing hex digits and `-` inside a long hyphenated
/// identifier until argmax drifted onto a re-emitted group. Renders a synthetic page carrying
/// a UUID-shaped header plus ordinary prose at 150 dpi and asserts the identifier survives OCR
/// intact: exactly one occurrence of the full id, and no 4-hex group repeated.
#[cfg(all(feature = "pdf", not(target_arch = "wasm32")))]
#[tokio::test]
#[ignore = "downloads ~3GB of GLM-OCR weights from HuggingFace Hub"]
async fn glm_ocr_paired_keeps_hex_identifier_intact() {
    use xberg::render_pdf_page_to_png;

    const DOCUMENT_ID: &str = "3f2a9c1e-7b4d-4e8a-9c6f-2d1b8e5a4f7c";
    const HEADER_FONT_SIZE: u32 = 9;
    let body_lines = [
        "This page carries ordinary prose beneath the identifier header.",
        "The paragraph continues for a second line of unrelated text.",
        "And a third line so the region has a realistic amount of body copy.",
    ];

    let pdf_bytes = build_header_and_prose_pdf(&format!("Document ID: {DOCUMENT_ID}"), HEADER_FONT_SIZE, &body_lines);
    let png_bytes =
        render_pdf_page_to_png(&pdf_bytes, 0, Some(150), None).expect("synthetic header+prose PDF must render");

    let backend = GlmOcrBackend::new(GlmOcrTask::default(), LayoutMode::WholePage);
    let config = OcrConfig::default();

    let result = backend
        .process_image(&png_bytes, &config)
        .await
        .expect("GlmOcrBackend::process_image should succeed");

    eprintln!("Content:\n{}", result.content);

    let occurrences = result.content.matches(DOCUMENT_ID).count();
    assert_eq!(
        occurrences, 1,
        "expected the UUID to appear exactly once, found {occurrences} in: {}",
        result.content
    );

    for group in DOCUMENT_ID.split('-') {
        let group_occurrences = result.content.matches(group).count();
        assert_eq!(
            group_occurrences, 1,
            "4-hex group {group:?} repeated ({group_occurrences} occurrences) in: {}",
            result.content
        );
    }
}

/// Build a minimal single-page Letter (612x792 pt) PDF: one `header` line at `header_size` pt,
/// then `body_lines` as ordinary 11pt Helvetica prose beneath it.
///
/// Hand-rolled rather than pulled in via a PDF-writing dependency, matching
/// `candle_backends.rs`'s `build_letter_prose_pdf`: object byte offsets are computed as the
/// buffer is built, never by hand-counting -- the usual source of a broken minimal PDF.
#[cfg(all(feature = "pdf", not(target_arch = "wasm32")))]
fn build_header_and_prose_pdf(header: &str, header_size: u32, body_lines: &[&str]) -> Vec<u8> {
    fn escape(text: &str) -> String {
        text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
    }

    let mut content = format!("BT\n/F1 {header_size} Tf\n72 720 Td\n({}) Tj\n", escape(header));
    content.push_str("/F1 11 Tf\n0 -24 Td\n14 TL\n");
    for (index, line) in body_lines.iter().enumerate() {
        if index > 0 {
            content.push_str("T*\n");
        }
        content.push_str(&format!("({}) Tj\n", escape(line)));
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

/// Count the longest run of identical consecutive N-grams in `text`. Catches
/// degenerate generations where a model loops on the same phrase indefinitely.
fn longest_repeated_ngram_run(text: &str, n: usize) -> usize {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < n * 2 {
        return 0;
    }
    let mut max_run = 0usize;
    for start in 0..tokens.len() - n + 1 {
        let pattern = &tokens[start..start + n];
        let mut run = 1usize;
        let mut next = start + n;
        while next + n <= tokens.len() && &tokens[next..next + n] == pattern {
            run += 1;
            next += n;
        }
        max_run = max_run.max(run);
    }
    max_run
}
