use super::super::*;
use xberg::ExtractionConfig;

/// Lock around the `XBERG_LLM_API_KEY` env var to keep the resolution
/// tests deterministic in the multi-threaded test runner. Tests that touch
/// the environment must hold this guard for their full duration.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[allow(unsafe_code)]
fn with_env_var<R>(key: &str, value: Option<&str>, f: impl FnOnce() -> R) -> R {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var(key).ok();
    unsafe {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    let result = f();
    unsafe {
        match previous {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    result
}

#[test]
fn cli_flag_takes_precedence_over_env_var() {
    with_env_var("XBERG_LLM_API_KEY", Some("env-value"), || {
        let resolved = resolve_llm_api_key(Some("cli-value"));
        assert_eq!(resolved.as_deref(), Some("cli-value"));
    });
}

#[test]
fn env_var_used_when_cli_flag_absent() {
    with_env_var("XBERG_LLM_API_KEY", Some("env-value"), || {
        let resolved = resolve_llm_api_key(None);
        assert_eq!(resolved.as_deref(), Some("env-value"));
    });
}

#[test]
fn returns_none_when_neither_source_is_set() {
    with_env_var("XBERG_LLM_API_KEY", None, || {
        let resolved = resolve_llm_api_key(None);
        assert!(resolved.is_none());
    });
}

#[test]
fn empty_cli_flag_does_not_count() {
    with_env_var("XBERG_LLM_API_KEY", Some("env-value"), || {
        let resolved = resolve_llm_api_key(Some("   "));
        assert_eq!(resolved.as_deref(), Some("env-value"));
    });
}

#[test]
fn apply_llm_api_key_fills_translation_slot() {
    use xberg::core::config::{LlmConfig, TranslationConfig};
    let mut config = ExtractionConfig {
        translation: Some(TranslationConfig {
            target_lang: "de".to_string(),
            source_lang: None,
            preserve_markup: false,
            llm: LlmConfig {
                model: "openai/gpt-4o-mini".to_string(),
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    apply_llm_api_key(&mut config, "resolved");
    let t = config.translation.unwrap();
    assert_eq!(t.llm.api_key.as_deref(), Some("resolved"));
}

#[test]
fn apply_llm_api_key_preserves_existing_key() {
    use xberg::core::config::{LlmConfig, TranslationConfig};
    let mut config = ExtractionConfig {
        translation: Some(TranslationConfig {
            target_lang: "de".to_string(),
            source_lang: None,
            preserve_markup: false,
            llm: LlmConfig {
                model: "openai/gpt-4o-mini".to_string(),
                api_key: Some("explicit".to_string()),
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    apply_llm_api_key(&mut config, "resolved");
    let t = config.translation.unwrap();
    assert_eq!(
        t.llm.api_key.as_deref(),
        Some("explicit"),
        "explicit config keys take precedence over the resolved value"
    );
}

#[test]
fn apply_llm_api_key_fills_page_classification_slot() {
    use xberg::core::config::{LlmConfig, PageClassificationConfig};
    let mut config = ExtractionConfig {
        page_classification: Some(PageClassificationConfig {
            prompt_template: None,
            labels: vec!["a".to_string()],
            multi_label: false,
            llm: LlmConfig {
                model: "openai/gpt-4o-mini".to_string(),
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    apply_llm_api_key(&mut config, "resolved");
    let pc = config.page_classification.unwrap();
    assert_eq!(pc.llm.api_key.as_deref(), Some("resolved"));
}
