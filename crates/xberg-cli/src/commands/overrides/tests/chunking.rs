#[cfg(any(feature = "core-cli", feature = "analysis"))]
use super::super::*;
#[cfg(any(feature = "core-cli", feature = "analysis"))]
use super::{config_from_json, default_overrides};
#[cfg(any(feature = "core-cli", feature = "analysis"))]
use xberg::ChunkingConfig;
#[cfg(any(feature = "core-cli", feature = "analysis"))]
use xberg::ExtractionConfig;

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunking_enabled_defaults() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        chunk: Some(true),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let chunking = config.chunking.unwrap();
    assert_eq!(chunking.max_characters, 1000);
    assert_eq!(chunking.overlap, 200);
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunking_custom_size() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        chunk: Some(true),
        chunk_size: Some(500),
        chunk_overlap: Some(50),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let chunking = config.chunking.unwrap();
    assert_eq!(chunking.max_characters, 500);
    assert_eq!(chunking.overlap, 50);
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunking_disabled() {
    let mut config = ExtractionConfig {
        chunking: Some(ChunkingConfig::default()),
        ..Default::default()
    };
    let overrides = ExtractionOverrides {
        chunk: Some(false),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    assert!(config.chunking.is_none());
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn should_keep_config_json_chunking_siblings_when_chunk_flags_are_also_given() {
    let mut config = config_from_json(r#"{"chunking":{"chunker_type":"markdown","table_chunking":"repeat_header"}}"#);
    let overrides = ExtractionOverrides {
        chunk: Some(true),
        chunk_size: Some(512),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let chunking = config
        .chunking
        .expect("--chunk true must leave a chunking config in place");
    assert_eq!(chunking.max_characters, 512);
    assert_eq!(
        chunking.chunker_type,
        xberg::ChunkerType::Markdown,
        "chunker_type has no CLI flag and must survive --chunk/--chunk-size"
    );
    assert_eq!(chunking.table_chunking, xberg::TableChunkingMode::RepeatHeader);
}

/// Guards the other half of the precedence rule for chunking. Passes before
/// and after the fix.
#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn should_let_chunk_size_flag_win_over_config_json_max_chars() {
    let mut config = config_from_json(r#"{"chunking":{"max_chars":4000}}"#);
    let overrides = ExtractionOverrides {
        chunk: Some(true),
        chunk_size: Some(512),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let chunking = config
        .chunking
        .expect("--chunk true must leave a chunking config in place");
    assert_eq!(chunking.max_characters, 512);
}

/// Regression test for the field-drop shape #654 describes, structurally identical to the
/// `--ocr-backend` defect fixed in `5921a7cc23`: `apply_chunking` only materialised
/// `config.chunking` on `--chunk true`, `--chunking-tokenizer`, or a config file that
/// already set it, so `--chunk-size 512` alone (no `--chunk true`) hit the `let Some(chunking)
/// = config.chunking.as_mut() else { return; }` early return and was silently dropped --
/// no warning, no error, just an unchanged config. Before the fix (i.e. reverting
/// `has_chunk_field_flag` back out of the materialisation condition), `config.chunking`
/// stays `None` here and this assertion fails on `.expect(..)`.
#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunk_size_flag_materialises_chunking_without_chunk_true_flag() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        chunk_size: Some(512),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let chunking = config
        .chunking
        .expect("--chunk-size must materialise a chunking config even without --chunk true");
    assert_eq!(chunking.max_characters, 512);
}

/// Same defect, `--chunk-overlap` alone.
#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunk_overlap_flag_materialises_chunking_without_chunk_true_flag() {
    let mut config = ExtractionConfig::default();
    let overrides = ExtractionOverrides {
        chunk_overlap: Some(50),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let chunking = config
        .chunking
        .expect("--chunk-overlap must materialise a chunking config even without --chunk true");
    assert_eq!(chunking.overlap, 50);
}

#[cfg(feature = "analysis")]
#[test]
fn should_keep_config_json_language_detection_siblings_when_detect_language_flag_is_given() {
    let mut config = config_from_json(r#"{"language_detection":{"min_confidence":0.5,"detect_multiple":true}}"#);
    let overrides = ExtractionOverrides {
        detect_language: Some(true),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let detection = config
        .language_detection
        .expect("--detect-language true must leave a language-detection config in place");
    assert!(detection.enabled);
    assert_eq!(
        detection.min_confidence, 0.5,
        "min_confidence has no CLI flag of its own and must survive --detect-language"
    );
    assert!(detection.detect_multiple);
}

#[cfg(feature = "analysis")]
#[test]
fn should_keep_config_json_token_reduction_siblings_when_token_reduction_flag_is_given() {
    let mut config = config_from_json(r#"{"token_reduction":{"preserve_important_words":false}}"#);
    let overrides = ExtractionOverrides {
        token_reduction: Some(ReductionLevelArg::Aggressive),
        ..default_overrides()
    };

    overrides.apply(&mut config);

    let reduction = config.token_reduction.expect("--token-reduction must set a config");
    assert_eq!(reduction.mode, "aggressive");
    assert!(
        !reduction.preserve_important_words,
        "preserve_important_words has no CLI flag and must survive --token-reduction"
    );
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_validate_chunk_size_zero() {
    let overrides = ExtractionOverrides {
        chunk_size: Some(0),
        ..default_overrides()
    };
    assert!(overrides.validate().is_err());
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_validate_chunk_size_too_large() {
    let overrides = ExtractionOverrides {
        chunk_size: Some(2_000_000),
        ..default_overrides()
    };
    assert!(overrides.validate().is_err());
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_validate_overlap_exceeds_size() {
    let overrides = ExtractionOverrides {
        chunk_size: Some(100),
        chunk_overlap: Some(200),
        ..default_overrides()
    };
    assert!(overrides.validate().is_err());
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunk_overlap_clamped_on_existing_config() {
    let mut config = ExtractionConfig {
        chunking: Some(ChunkingConfig {
            max_characters: 800,
            overlap: 100,
            ..Default::default()
        }),
        ..Default::default()
    };
    let overrides = ExtractionOverrides {
        chunk_overlap: Some(1500),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let chunking = config.chunking.unwrap();
    assert_eq!(chunking.overlap, 800 / 4);
    assert_eq!(chunking.max_characters, 800);
}

#[cfg(any(feature = "core-cli", feature = "analysis"))]
#[test]
fn test_chunk_overlap_valid_on_existing_config() {
    let mut config = ExtractionConfig {
        chunking: Some(ChunkingConfig {
            max_characters: 800,
            overlap: 100,
            ..Default::default()
        }),
        ..Default::default()
    };
    let overrides = ExtractionOverrides {
        chunk_overlap: Some(200),
        ..default_overrides()
    };
    overrides.apply(&mut config);
    let chunking = config.chunking.unwrap();
    assert_eq!(chunking.overlap, 200);
    assert_eq!(chunking.max_characters, 800);
}

#[cfg(all(
    any(feature = "core-cli", feature = "analysis"),
    not(feature = "chunking-tokenizers")
))]
#[test]
fn test_validate_chunking_tokenizer_requires_feature() {
    let overrides = ExtractionOverrides {
        chunking_tokenizer: Some("Xenova/gpt-4o".to_string()),
        ..default_overrides()
    };
    let err = overrides.validate().unwrap_err();
    assert!(
        err.to_string()
            .contains("--chunking-tokenizer requires the chunking-tokenizers feature")
    );
}
