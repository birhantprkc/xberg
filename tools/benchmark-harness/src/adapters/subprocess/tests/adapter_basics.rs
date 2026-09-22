//! Basic adapter construction/format-matching tests and OCR-language forwarding argument tests.

use crate::adapter::FrameworkAdapter;

use super::super::SubprocessAdapter;

#[test]
fn test_subprocess_adapter_creation() {
    let adapter = SubprocessAdapter::new(
        "test-adapter",
        "echo",
        vec!["test".to_string()],
        vec![],
        vec!["pdf".to_string(), "docx".to_string()],
    );
    assert_eq!(adapter.name(), "test-adapter");
}

#[test]
fn test_supports_format() {
    let adapter = SubprocessAdapter::new(
        "test",
        "echo",
        vec![],
        vec![],
        vec!["pdf".to_string(), "docx".to_string()],
    );
    assert!(adapter.supports_format("pdf"));
    assert!(adapter.supports_format("docx"));
    assert!(!adapter.supports_format("unknown"));
}

#[test]
fn ocr_language_forward_arg_only_when_flag_and_language_present() {
    let base = || SubprocessAdapter::new("ext", "echo", vec![], vec![], vec!["png".to_string()]);

    // No flag configured: never forwards, even with a fixture language.
    assert_eq!(base().ocr_language_forward_arg(Some("eng+kor")), None);

    let forwarding = base().with_ocr_language_arg("--ocr-lang");
    // Flag configured but fixture pins no language: nothing forwarded.
    assert_eq!(forwarding.ocr_language_forward_arg(None), None);
    // Emitted as a single `--flag=value` token in canonical Tesseract form.
    assert_eq!(
        forwarding.ocr_language_forward_arg(Some("eng+kor")).as_deref(),
        Some("--ocr-lang=eng+kor")
    );
    // Whitespace/formatting is canonicalized, matching the xberg path.
    assert_eq!(
        forwarding.ocr_language_forward_arg(Some(" jpn_vert ")).as_deref(),
        Some("--ocr-lang=jpn_vert")
    );
}

#[test]
fn native_batch_language_forwarding_requires_one_global_language() {
    let adapter = SubprocessAdapter::new("docling", "echo", vec![], vec![], vec!["png".to_string()])
        .with_ocr_language_arg("--ocr-lang")
        .with_ocr_language_policy(crate::adapter::OcrLanguagePolicy::AnyBatchGlobal);
    assert_eq!(
        adapter
            .batch_ocr_language_forward_arg(&[Some(" eng ".to_string()), Some("eng".to_string())])
            .unwrap()
            .as_deref(),
        Some("--ocr-lang=eng")
    );
    assert!(
        adapter
            .batch_ocr_language_forward_arg(&[Some("eng".to_string()), Some("deu".to_string())])
            .is_err()
    );
}
