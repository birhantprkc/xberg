use super::*;

#[test]
fn test_postprocessor_config_default() {
    let config = PostProcessorConfig::default();
    assert!(config.enabled);
    assert!(config.enabled_processors.is_none());
    assert!(config.disabled_processors.is_none());
}

#[test]
fn test_postprocessor_config_build_lookup_sets() {
    let mut config = PostProcessorConfig {
        enabled: true,
        enabled_processors: Some(vec!["a".to_string(), "b".to_string()]),
        disabled_processors: Some(vec!["c".to_string()]),
        enabled_set: None,
        disabled_set: None,
    };

    config.build_lookup_sets();

    assert!(config.enabled_set.is_some());
    assert!(config.disabled_set.is_some());
    assert!(config.enabled_set.unwrap().contains("a"));
    assert!(config.disabled_set.unwrap().contains("c"));
}

#[test]
fn test_chunking_config_defaults() {
    let config = ChunkingConfig::default();
    assert_eq!(config.max_characters, 1000);
    assert_eq!(config.overlap, 200);
    assert!(config.trim);
    assert_eq!(config.chunker_type, ChunkerType::Text);
    assert!(matches!(config.sizing, ChunkSizing::Characters));
}

#[test]
fn chunk_sizing_rejects_unknown_fields() {
    assert!(serde_json::from_str::<ChunkSizing>(r#"{"type":"characters","model":"ignored"}"#).is_err());
}

#[test]
fn chunking_config_serialization_omits_removed_breadcrumb_settings() {
    let config = serde_json::to_value(ChunkingConfig::default()).expect("chunking config must serialize");

    assert!(config.get("prepend_heading_context").is_none());
    assert!(config.get("breadcrumb_target").is_none());
}

#[test]
fn chunking_config_rejects_removed_breadcrumb_settings() {
    for json in [
        r#"{"prepend_heading_context":true}"#,
        r#"{"breadcrumb_target":"metadata"}"#,
    ] {
        assert!(serde_json::from_str::<ChunkingConfig>(json).is_err());
    }
}

#[test]
fn test_embedding_config_default() {
    let config = EmbeddingConfig::default();
    assert!(config.normalize);
    assert_eq!(config.batch_size, 32);
    assert!(config.cache_dir.is_none());
}

/// Tests that `EmbeddingModelType::default()` returns the "gte-modernbert-base" preset.
///
/// Language bindings that use struct-level `#[serde(default)]` resolve absent
/// `model` fields via this impl. An empty-string name caused "Unknown embedding
/// preset: " panics in `get_preset()`; the default must be a valid preset.
#[test]
fn test_embedding_model_type_default_is_gte_modernbert() {
    match EmbeddingModelType::default() {
        EmbeddingModelType::Preset { name } => {
            assert_eq!(
                name, "gte-modernbert-base",
                "Default model should be the gte-modernbert-base preset"
            );
        }
        other => panic!("Expected Preset variant, got {:?}", other),
    }
}

/// Tests that EmbeddingModelType::Preset serializes with "type" field (internally-tagged).
/// This validates the API schema matches the documented format:
/// `{"type": "preset", "name": "fast"}` NOT `{"preset": {"name": "fast"}}`
#[test]
fn test_embedding_model_type_preset_serialization() {
    let model = EmbeddingModelType::Preset {
        name: "fast".to_string(),
    };
    let json = serde_json::to_string(&model).unwrap();

    assert!(json.contains(r#""type":"preset""#), "Should contain type:preset field");
    assert!(json.contains(r#""name":"fast""#), "Should contain name:fast field");

    assert!(
        !json.contains(r#"{"preset":"#),
        "Should NOT use adjacently-tagged format"
    );
}

/// Tests that EmbeddingModelType::Preset deserializes from the documented API format.
/// API documentation shows: `{"type": "preset", "name": "fast"}`
#[test]
fn test_embedding_model_type_preset_deserialization() {
    let json = r#"{"type": "preset", "name": "fast"}"#;
    let model: EmbeddingModelType = serde_json::from_str(json).unwrap();

    match model {
        EmbeddingModelType::Preset { name } => {
            assert_eq!(name, "fast");
        }
        _ => panic!("Expected Preset variant"),
    }
}

/// Tests that the wrong format (adjacently-tagged) is rejected.
/// This ensures the API doesn't accept the old/wrong documentation format.
#[test]
fn test_embedding_model_type_rejects_wrong_format() {
    let wrong_json = r#"{"preset": {"name": "fast"}}"#;
    let result: Result<EmbeddingModelType, _> = serde_json::from_str(wrong_json);

    assert!(result.is_err(), "Should reject adjacently-tagged format");
}

#[test]
fn embedding_model_type_rejects_unknown_fields() {
    let json = r#"{"type":"preset","name":"fast","extra_name":"slow"}"#;
    assert!(serde_json::from_str::<EmbeddingModelType>(json).is_err());
}

/// Tests round-trip serialization/deserialization of EmbeddingConfig.
#[test]
fn test_embedding_config_roundtrip() {
    let config = EmbeddingConfig {
        model: EmbeddingModelType::Preset {
            name: "balanced".to_string(),
        },
        normalize: true,
        batch_size: 64,
        show_download_progress: false,
        cache_dir: None,
        acceleration: None,
        max_embed_duration_secs: Some(60),
        max_sequence_length: None,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: EmbeddingConfig = serde_json::from_str(&json).unwrap();

    match deserialized.model {
        EmbeddingModelType::Preset { name } => {
            assert_eq!(name, "balanced");
        }
        _ => panic!("Expected Preset variant"),
    }
    assert!(deserialized.normalize);
    assert_eq!(deserialized.batch_size, 64);
}

/// Tests Custom model type serialization format.
#[test]
fn test_embedding_model_type_custom_serialization() {
    let model = EmbeddingModelType::Custom {
        model_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
        dimensions: 384,
    };
    let json = serde_json::to_string(&model).unwrap();

    assert!(json.contains(r#""type":"custom""#), "Should contain type:custom field");
    assert!(json.contains(r#""model_id":"#), "Should contain model_id field");
    assert!(json.contains(r#""dimensions":384"#), "Should contain dimensions field");
}

#[test]
#[cfg(feature = "embeddings")]
fn test_resolve_preset_balanced() {
    let config = ChunkingConfig {
        preset: Some("balanced".to_string()),
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert_eq!(resolved.max_characters, 1024);
    assert_eq!(resolved.overlap, 100);
    assert!(resolved.embedding.is_none());
}

#[test]
#[cfg(feature = "embeddings")]
fn test_resolve_preset_preserves_explicit_embedding() {
    let explicit_embedding = EmbeddingConfig {
        model: EmbeddingModelType::Custom {
            model_id: "custom/model".to_string(),
            dimensions: 512,
        },
        batch_size: 64,
        ..Default::default()
    };
    let config = ChunkingConfig {
        preset: Some("fast".to_string()),
        embedding: Some(explicit_embedding),
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert_eq!(resolved.max_characters, 512);
    assert_eq!(resolved.overlap, 50);
    match &resolved.embedding.unwrap().model {
        EmbeddingModelType::Custom { model_id, .. } => assert_eq!(model_id, "custom/model"),
        _ => panic!("Expected Custom model type to be preserved"),
    }
}

#[cfg(any(feature = "embeddings", feature = "chunking"))]
#[test]
fn test_resolve_preset_no_preset_returns_unchanged() {
    let config = ChunkingConfig {
        max_characters: 500,
        overlap: 50,
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert_eq!(resolved.max_characters, 500);
    assert_eq!(resolved.overlap, 50);
    assert!(resolved.embedding.is_none());
}

#[cfg(any(feature = "embeddings", feature = "chunking"))]
#[test]
fn test_resolve_preset_unknown_name_returns_unchanged() {
    let config = ChunkingConfig {
        max_characters: 500,
        preset: Some("nonexistent".to_string()),
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert_eq!(resolved.max_characters, 500);
}

#[test]
fn test_embedding_model_type_llm_roundtrip() {
    let model_type = EmbeddingModelType::Llm {
        llm: Box::new(crate::core::config::llm::LlmConfig {
            model: "openai/text-embedding-3-small".to_string(),
            ..Default::default()
        }),
    };
    let json = serde_json::to_string(&model_type).unwrap();
    assert!(json.contains("\"type\":\"llm\""));
    assert!(json.contains("openai/text-embedding-3-small"));

    let deserialized: EmbeddingModelType = serde_json::from_str(&json).unwrap();
    match deserialized {
        EmbeddingModelType::Llm { llm } => {
            assert_eq!(llm.model, "openai/text-embedding-3-small");
        }
        _ => panic!("Expected Llm variant"),
    }
}

#[test]
#[should_panic(expected = "topic_threshold must be in [0.0, 1.0]")]
fn test_with_topic_threshold_panics_above_one() {
    ChunkingConfig::default().with_topic_threshold(1.1);
}

#[test]
#[should_panic(expected = "topic_threshold must be in [0.0, 1.0]")]
fn test_with_topic_threshold_panics_below_zero() {
    ChunkingConfig::default().with_topic_threshold(-0.1);
}

#[test]
fn test_with_topic_threshold_accepts_boundary_values() {
    let config = ChunkingConfig::default().with_topic_threshold(0.0);
    assert_eq!(config.topic_threshold, Some(0.0));

    let config = ChunkingConfig::default().with_topic_threshold(1.0);
    assert_eq!(config.topic_threshold, Some(1.0));
}

/// Tests Custom model type deserialization.
#[test]
fn test_embedding_model_type_custom_deserialization() {
    let json = r#"{"type": "custom", "model_id": "test/model", "dimensions": 512}"#;
    let model: EmbeddingModelType = serde_json::from_str(json).unwrap();

    match model {
        EmbeddingModelType::Custom { model_id, dimensions } => {
            assert_eq!(model_id, "test/model");
            assert_eq!(dimensions, 512);
        }
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn test_embedding_model_type_plugin_roundtrip() {
    let model = EmbeddingModelType::Plugin {
        name: "lilbee-llamacpp".to_string(),
    };
    let json = serde_json::to_string(&model).unwrap();
    assert!(json.contains("\"type\":\"plugin\""));
    assert!(json.contains("lilbee-llamacpp"));

    let deserialized: EmbeddingModelType = serde_json::from_str(&json).unwrap();
    match deserialized {
        EmbeddingModelType::Plugin { name } => assert_eq!(name, "lilbee-llamacpp"),
        _ => panic!("Expected Plugin variant"),
    }
}

#[test]
fn test_embedding_model_type_plugin_deserialization() {
    let json = r#"{"type": "plugin", "name": "my-embedder"}"#;
    let model: EmbeddingModelType = serde_json::from_str(json).unwrap();
    match model {
        EmbeddingModelType::Plugin { name } => assert_eq!(name, "my-embedder"),
        _ => panic!("Expected Plugin variant"),
    }
}

/// Preset with no explicit embedding: embedding must remain None.
///
/// Before the fix, `resolve_preset()` would silently inject an
/// `EmbeddingConfig` whenever a preset was configured, causing every
/// chunk to have an unexpected `.embedding` field populated.
#[test]
#[cfg(feature = "embeddings")]
fn test_resolve_preset_does_not_inject_embedding_when_none() {
    let config = ChunkingConfig {
        preset: Some("multilingual".to_string()),
        embedding: None,
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert!(
        resolved.embedding.is_none(),
        "preset alone must not inject an EmbeddingConfig (#797)"
    );
}

/// Preset with an explicit embedding: the embedding must be preserved unchanged.
#[test]
#[cfg(feature = "embeddings")]
fn test_resolve_preset_preserves_explicit_embedding_config() {
    let explicit = EmbeddingConfig {
        model: EmbeddingModelType::Custom {
            model_id: "my-org/model".to_string(),
            dimensions: 768,
        },
        batch_size: 16,
        ..Default::default()
    };
    let config = ChunkingConfig {
        preset: Some("multilingual".to_string()),
        embedding: Some(explicit),
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    let emb = resolved
        .embedding
        .expect("explicit embedding must survive resolve_preset");
    assert_eq!(emb.batch_size, 16);
    match emb.model {
        EmbeddingModelType::Custom { model_id, dimensions } => {
            assert_eq!(model_id, "my-org/model");
            assert_eq!(dimensions, 768);
        }
        other => panic!("expected Custom model type, got {other:?}"),
    }
}

/// No preset, no embedding: embedding must stay None (regression guard).
#[cfg(any(feature = "embeddings", feature = "chunking"))]
#[test]
fn test_resolve_preset_no_preset_no_embedding_stays_none() {
    let config = ChunkingConfig {
        preset: None,
        embedding: None,
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert!(resolved.embedding.is_none(), "no-preset path must not touch embedding");
}

#[test]
fn table_chunking_mode_defaults_to_split_when_field_absent() {
    let c: ChunkingConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(c.table_chunking, TableChunkingMode::Split);
}

/// Regression guard for #268: a `ChunkingConfig` JSON payload written before
/// `sparse_embedding`/`late_interaction` existed (i.e. missing both keys) must still
/// deserialize, with both fields defaulting to `None`. `ChunkingConfig` is nested inside
/// `ExtractionConfig`, which is `#[serde(deny_unknown_fields)]` — this guards the other
/// direction of that contract: old payloads must not become invalid just because the
/// schema grew new optional fields.
#[test]
fn sparse_and_late_interaction_configs_default_to_none_when_absent_from_json() {
    let c: ChunkingConfig = serde_json::from_str("{}").unwrap();
    assert!(c.sparse_embedding.is_none());
    assert!(c.late_interaction.is_none());
}

/// `ChunkingConfig::default()` must leave both new vector configs unset (#268) — no
/// behaviour change for existing callers who never opt in.
#[test]
fn chunking_config_default_has_no_sparse_or_late_interaction_config() {
    let config = ChunkingConfig::default();
    assert!(config.sparse_embedding.is_none());
    assert!(config.late_interaction.is_none());
}

/// `resolve_preset()` must carry an explicitly-set `sparse_embedding`/`late_interaction`
/// config through unchanged (#268), the same way it already preserves `embedding`.
#[cfg(any(feature = "embeddings", feature = "chunking"))]
#[test]
fn resolve_preset_preserves_explicit_sparse_and_late_interaction_configs() {
    let config = ChunkingConfig {
        preset: Some("balanced".to_string()),
        sparse_embedding: Some(crate::core::config::SparseEmbeddingConfig {
            batch_size: 4,
            ..Default::default()
        }),
        late_interaction: Some(crate::core::config::LateInteractionConfig {
            batch_size: 8,
            ..Default::default()
        }),
        ..Default::default()
    };
    let resolved = config.resolve_preset();
    assert_eq!(
        resolved
            .sparse_embedding
            .expect("sparse_embedding must survive resolve_preset")
            .batch_size,
        4
    );
    assert_eq!(
        resolved
            .late_interaction
            .expect("late_interaction must survive resolve_preset")
            .batch_size,
        8
    );
}
