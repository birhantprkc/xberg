use super::*;
use std::borrow::Cow;

struct MockOcrBackend {
    languages: Vec<String>,
}

impl Plugin for MockOcrBackend {
    fn name(&self) -> &str {
        "mock-ocr"
    }

    fn version(&self) -> String {
        "1.0.0".to_string()
    }

    fn initialize(&self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl OcrBackend for MockOcrBackend {
    async fn process_image(&self, _image_bytes: &[u8], _config: &OcrConfig) -> Result<ExtractedDocument> {
        Ok(ExtractedDocument {
            content: "Mocked OCR text".to_string(),
            mime_type: Cow::Borrowed("text/plain"),
            ..Default::default()
        })
    }

    fn supports_language(&self, lang: &str) -> bool {
        self.languages.iter().any(|l| l == lang)
    }

    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Custom
    }

    fn supported_languages(&self) -> Vec<String> {
        self.languages.clone()
    }
}

#[tokio::test]
async fn test_ocr_backend_process_image() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string(), "deu".to_string()],
    };

    let config = OcrConfig {
        backend: "mock".to_string(),
        language: vec!["eng".to_string()],
        ..Default::default()
    };

    let result = backend.process_image(b"fake image data", &config).await.unwrap();
    assert_eq!(result.content, "Mocked OCR text");
    assert_eq!(result.mime_type, "text/plain");
}

#[tokio::test]
async fn test_ocr_backend_process_image_owned_default_impl_is_object_safe() {
    let backend: Arc<dyn OcrBackend> = Arc::new(MockOcrBackend {
        languages: vec!["eng".to_string()],
    });

    let result = backend
        .process_image_owned(Arc::new(b"fake image data".to_vec()), &OcrConfig::default())
        .await
        .unwrap();

    assert_eq!(result.content, "Mocked OCR text");
    assert_eq!(result.mime_type, "text/plain");
}

#[test]
fn test_ocr_backend_supports_language() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string(), "deu".to_string()],
    };

    assert!(backend.supports_language("eng"));
    assert!(backend.supports_language("deu"));
    assert!(!backend.supports_language("fra"));
}

#[test]
fn test_ocr_backend_type() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    assert_eq!(backend.backend_type(), OcrBackendType::Custom);
}

#[test]
fn test_ocr_backend_supported_languages() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string(), "deu".to_string(), "fra".to_string()],
    };

    let supported = backend.supported_languages();
    assert_eq!(supported.len(), 3);
    assert!(supported.contains(&"eng".to_string()));
    assert!(supported.contains(&"deu".to_string()));
    assert!(supported.contains(&"fra".to_string()));
}

#[test]
fn test_ocr_backend_type_variants() {
    assert_eq!(OcrBackendType::Tesseract, OcrBackendType::Tesseract);
    assert_ne!(OcrBackendType::Tesseract, OcrBackendType::PaddleOCR);
    assert_ne!(OcrBackendType::PaddleOCR, OcrBackendType::Custom);
}

#[test]
fn test_ocr_backend_type_debug() {
    let backend_type = OcrBackendType::Tesseract;
    let debug_str = format!("{:?}", backend_type);
    assert!(debug_str.contains("Tesseract"));
}

#[test]
fn test_ocr_backend_type_clone() {
    let backend_type = OcrBackendType::PaddleOCR;
    let cloned = backend_type;
    assert_eq!(backend_type, cloned);
}

#[test]
fn test_ocr_backend_default_table_detection() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };
    assert!(!backend.supports_table_detection());
}

/// Regression test for the sceptre confidence-gating failure: a backend that reports a
/// page-level confidence number without declaring `confidence_semantics` must default to
/// `Uncalibrated`, never to `Legibility`. Defaulting to `Legibility` would let the next
/// backend added to this codebase silently inherit Tesseract's gate threshold and repeat
/// the sceptre failure, which rejected all 16 pages of a document and emptied it.
#[test]
fn should_default_to_uncalibrated_for_a_backend_that_does_not_declare_semantics() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    assert_eq!(backend.confidence_semantics(), ConfidenceSemantics::Uncalibrated);
}

/// Gating code reaches a backend as `&dyn OcrBackend` out of the registry, never as a
/// concrete type, so the declared semantics must survive dynamic dispatch — including the
/// `scale_max` payload, which is what a caller divides by instead of a hardcoded 100.
#[test]
fn should_report_declared_semantics_through_a_trait_object() {
    struct CalibratedBackend;

    impl Plugin for CalibratedBackend {
        fn name(&self) -> &str {
            "calibrated"
        }

        fn version(&self) -> String {
            "1.0.0".to_string()
        }

        fn initialize(&self) -> Result<()> {
            Ok(())
        }

        fn shutdown(&self) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl OcrBackend for CalibratedBackend {
        async fn process_image(&self, _image_bytes: &[u8], _config: &OcrConfig) -> Result<ExtractedDocument> {
            unreachable!("this backend exists only to declare confidence semantics")
        }

        fn backend_type(&self) -> OcrBackendType {
            OcrBackendType::Custom
        }

        fn supports_language(&self, lang: &str) -> bool {
            lang == "eng"
        }

        fn supported_languages(&self) -> Vec<String> {
            vec!["eng".to_string()]
        }

        fn confidence_semantics(&self) -> ConfidenceSemantics {
            ConfidenceSemantics::Legibility { scale_max: 255.0 }
        }
    }

    let backend: &dyn OcrBackend = &CalibratedBackend;

    match backend.confidence_semantics() {
        ConfidenceSemantics::Legibility { scale_max } => assert_eq!(scale_max, 255.0),
        other => panic!("expected the declared Legibility semantics, got {other:?}"),
    }
}

/// Regression guard for the rotation-handling capability: a backend that does not declare
/// `page_orientation_handling` must default to `RequiresUpright`, never to `SelfCorrecting`.
/// Defaulting to `SelfCorrecting` would let a new backend that cannot self-correct silently
/// inherit Tesseract's guarantee and emit garbage the first time it is handed a rotated
/// raster, mirroring the sceptre confidence-gating failure above.
#[test]
fn should_default_to_requires_upright_for_a_backend_that_does_not_declare_orientation_handling() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    let dynamic: &dyn OcrBackend = &backend;
    assert_eq!(
        dynamic.page_orientation_handling(),
        PageOrientationHandling::RequiresUpright
    );
}

/// `process_image_file`'s default impl returns `Other("File-based OCR processing
/// requires the tokio-runtime feature")` without that feature, so this test can only
/// assert the real behaviour in a build that has it.
#[cfg(feature = "tokio-runtime")]
#[tokio::test]
async fn test_ocr_backend_process_image_file_default_impl() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(b"fake image data").unwrap();
    let path = temp_file.path();

    let config = OcrConfig {
        backend: "mock".to_string(),
        language: vec!["eng".to_string()],
        ..Default::default()
    };

    let result = backend.process_image_file(path, &config).await.unwrap();
    assert_eq!(result.content, "Mocked OCR text");
}

#[test]
fn test_ocr_backend_plugin_interface() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    assert_eq!(backend.name(), "mock-ocr");
    assert_eq!(backend.version(), "1.0.0");
    assert!(backend.initialize().is_ok());
    assert!(backend.shutdown().is_ok());
}

#[test]
fn test_ocr_backend_empty_languages() {
    let backend = MockOcrBackend { languages: vec![] };

    let supported = backend.supported_languages();
    assert_eq!(supported.len(), 0);
    assert!(!backend.supports_language("eng"));
}

#[tokio::test]
async fn test_ocr_backend_with_empty_image() {
    let backend = MockOcrBackend {
        languages: vec!["eng".to_string()],
    };

    let config = OcrConfig {
        backend: "mock".to_string(),
        language: vec!["eng".to_string()],
        ..Default::default()
    };

    let result = backend.process_image(b"", &config).await;
    assert!(result.is_ok());
}

struct OptionAwareBackend;

impl Plugin for OptionAwareBackend {
    fn name(&self) -> &str {
        "option-aware"
    }

    fn version(&self) -> String {
        "1.0.0".to_string()
    }

    fn initialize(&self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl OcrBackend for OptionAwareBackend {
    async fn process_image(&self, _image_bytes: &[u8], config: &OcrConfig) -> Result<ExtractedDocument> {
        let mode = config
            .backend_options
            .as_ref()
            .and_then(|v| v.get("mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("standard");

        Ok(ExtractedDocument {
            content: format!("mode={mode}"),
            mime_type: Cow::Borrowed("text/plain"),
            ..Default::default()
        })
    }

    fn supports_language(&self, _: &str) -> bool {
        true
    }

    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Custom
    }
}

#[tokio::test]
async fn test_backend_reads_backend_options() {
    let backend = OptionAwareBackend;

    let config_with_options = OcrConfig {
        backend_options: Some(serde_json::json!({"mode": "fast", "threshold": 0.8})),
        ..Default::default()
    };
    let result = backend.process_image(b"img", &config_with_options).await.unwrap();
    assert_eq!(result.content, "mode=fast");

    let config_without_options = OcrConfig::default();
    let result = backend.process_image(b"img", &config_without_options).await.unwrap();
    assert_eq!(result.content, "mode=standard");
}

#[tokio::test]
async fn test_backend_options_unknown_keys_silently_ignored() {
    let backend = OptionAwareBackend;

    let config = OcrConfig {
        backend_options: Some(serde_json::json!({
            "unknown_key": "value",
            "another_unknown": 42
        })),
        ..Default::default()
    };
    let result = backend.process_image(b"img", &config).await;
    assert!(result.is_ok(), "unknown backend_options keys must not cause errors");
}

struct NamedMockOcrBackend {
    name: &'static str,
    languages: Vec<String>,
}

impl Plugin for NamedMockOcrBackend {
    fn name(&self) -> &str {
        self.name
    }
    fn version(&self) -> String {
        "1.0.0".to_string()
    }
    fn initialize(&self) -> Result<()> {
        Ok(())
    }
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl OcrBackend for NamedMockOcrBackend {
    async fn process_image(&self, _image_bytes: &[u8], _config: &OcrConfig) -> Result<ExtractedDocument> {
        Ok(ExtractedDocument::default())
    }

    fn supports_language(&self, lang: &str) -> bool {
        self.languages.iter().any(|l| l == lang)
    }

    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Custom
    }

    fn supported_languages(&self) -> Vec<String> {
        self.languages.clone()
    }
}

/// A backend that deliberately does not override `supported_languages`, to exercise the
/// trait's defaulted empty-list behaviour (see `OcrBackendCapabilities::supported_languages`).
struct UndeclaredLanguagesBackend {
    name: &'static str,
}

impl Plugin for UndeclaredLanguagesBackend {
    fn name(&self) -> &str {
        self.name
    }
    fn version(&self) -> String {
        "1.0.0".to_string()
    }
    fn initialize(&self) -> Result<()> {
        Ok(())
    }
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl OcrBackend for UndeclaredLanguagesBackend {
    async fn process_image(&self, _image_bytes: &[u8], _config: &OcrConfig) -> Result<ExtractedDocument> {
        Ok(ExtractedDocument::default())
    }

    fn supports_language(&self, _lang: &str) -> bool {
        true
    }

    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Custom
    }
}

#[test]
fn capabilities_report_each_backend_declared_languages() {
    let alpha: Arc<dyn OcrBackend> = Arc::new(NamedMockOcrBackend {
        name: "alpha-ocr",
        languages: vec!["eng".to_string(), "deu".to_string()],
    });
    let beta: Arc<dyn OcrBackend> = Arc::new(NamedMockOcrBackend {
        name: "beta-ocr",
        languages: vec!["fra".to_string()],
    });

    let capabilities =
        capabilities_from_snapshot(vec![("alpha-ocr".to_string(), alpha), ("beta-ocr".to_string(), beta)]);

    assert_eq!(
        capabilities,
        vec![
            OcrBackendCapabilities {
                name: "alpha-ocr".to_string(),
                supported_languages: vec!["eng".to_string(), "deu".to_string()],
            },
            OcrBackendCapabilities {
                name: "beta-ocr".to_string(),
                supported_languages: vec!["fra".to_string()],
            },
        ]
    );
}

#[test]
fn capabilities_are_ordered_by_backend_name() {
    let zed: Arc<dyn OcrBackend> = Arc::new(NamedMockOcrBackend {
        name: "zed-ocr",
        languages: vec!["eng".to_string()],
    });
    let alpha: Arc<dyn OcrBackend> = Arc::new(NamedMockOcrBackend {
        name: "alpha-ocr",
        languages: vec!["deu".to_string()],
    });

    // Fed reversed (zed before alpha), to fail if the helper's own sort is ever dropped
    // and it started trusting caller/registry order instead.
    let capabilities = capabilities_from_snapshot(vec![("zed-ocr".to_string(), zed), ("alpha-ocr".to_string(), alpha)]);

    let names: Vec<&str> = capabilities.iter().map(|capability| capability.name.as_str()).collect();
    assert_eq!(names, vec!["alpha-ocr", "zed-ocr"]);
}

#[test]
fn a_backend_that_does_not_declare_languages_reports_an_empty_list() {
    let backend: Arc<dyn OcrBackend> = Arc::new(UndeclaredLanguagesBackend { name: "undeclared-ocr" });

    let capabilities = capabilities_from_snapshot(vec![("undeclared-ocr".to_string(), backend)]);

    assert_eq!(
        capabilities[0].supported_languages,
        Vec::<String>::new(),
        "an empty supported_languages list means the backend does not enumerate its \
         languages, not that it supports none"
    );
}

#[test]
fn global_registry_capabilities_are_sorted_with_non_empty_names() {
    // Read-only sanity check against the live global registry. Deliberately does not
    // compare against a separate `list_ocr_backends()` call: the two take the registry
    // read lock separately, and a concurrent test in another module (e.g.
    // `doctor::config_lint`) registering or clearing backends between the two reads would
    // make such a comparison flaky for reasons that have nothing to do with this function.
    let capabilities = list_ocr_backend_capabilities().expect("registry lock must be acquirable");

    let names: Vec<&str> = capabilities.iter().map(|capability| capability.name.as_str()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    assert_eq!(names, sorted_names, "capabilities must be sorted by backend name");

    for capability in &capabilities {
        assert!(
            !capability.name.is_empty(),
            "a registered backend must report a non-empty name"
        );
    }
}
