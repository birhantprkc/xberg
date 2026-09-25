use super::*;

#[test]
fn should_reject_credential_provider_for_wasm_validation() {
    let config = LlmConfig {
        credential_provider: Some(Box::new(CredentialProviderConfig::VertexAdc { scope: None })),
        ..Default::default()
    };

    let error = validate_wasm_credential_provider(config.credential_provider.as_deref())
        .expect_err("managed credential providers must not be silently ignored on wasm32");
    match error {
        crate::XbergError::Validation { message, .. } => {
            assert_eq!(message, WASM_CREDENTIAL_PROVIDER_ERROR)
        }
        other => panic!("expected validation error, got {other:?}"),
    }
}

/// Regression test for https://github.com/xberg-io/xberg/issues/716
///
/// `LlmConfig` must implement `Default` so callers can use the struct-update
/// syntax documented in the VLM OCR guide:
///
/// ```rust
/// use xberg::core::config::LlmConfig;
/// let cfg = LlmConfig {
///     model: "openai/gpt-4o-mini".to_string(),
///     ..Default::default()
/// };
/// ```
#[test]
fn test_llm_config_default_trait_is_satisfied() {
    let cfg = LlmConfig::default();
    assert!(cfg.model.is_empty(), "default model should be empty string");
    assert!(cfg.api_key.is_none());
    assert!(cfg.base_url.is_none());
    assert!(cfg.timeout_secs.is_none());
    assert!(cfg.max_retries.is_none());
    assert!(cfg.max_concurrency.is_none());
    assert!(cfg.temperature.is_none());
    assert!(cfg.max_tokens.is_none());
    assert!(cfg.top_p.is_none());
    assert!(cfg.stop.is_none());
    assert!(cfg.seed.is_none());
    assert!(cfg.presence_penalty.is_none());
    assert!(cfg.frequency_penalty.is_none());
    assert!(cfg.reasoning_effort.is_none());
    assert!(cfg.extra_body.is_none());
    assert!(cfg.load_env.is_none());
    assert!(cfg.headers.is_none());
    assert!(cfg.providers.is_none());
    assert!(cfg.cache.is_none());
    assert!(cfg.budget.is_none());
    assert!(cfg.rate_limit.is_none());
    assert!(cfg.cost_tracking.is_none());
    assert!(cfg.tracing.is_none());
    assert!(cfg.cooldown_secs.is_none());
    assert!(cfg.health_check_secs.is_none());
    assert!(cfg.bedrock.is_none());
    assert!(cfg.credential_provider.is_none());
}

/// Verify the struct-update pattern from the issue compiles and produces
/// only the explicitly set field.
#[test]
fn test_llm_config_struct_update_syntax() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o-mini".to_string(),
        ..Default::default()
    };
    assert_eq!(cfg.model, "openai/gpt-4o-mini");
    assert!(cfg.api_key.is_none());
    assert!(cfg.base_url.is_none());
    assert!(cfg.timeout_secs.is_none());
    assert!(cfg.max_retries.is_none());
    assert!(cfg.max_concurrency.is_none());
    assert!(cfg.temperature.is_none());
    assert!(cfg.max_tokens.is_none());
    assert!(cfg.top_p.is_none());
    assert!(cfg.stop.is_none());
    assert!(cfg.seed.is_none());
    assert!(cfg.presence_penalty.is_none());
    assert!(cfg.frequency_penalty.is_none());
    assert!(cfg.reasoning_effort.is_none());
    assert!(cfg.extra_body.is_none());
    assert!(cfg.load_env.is_none());
    assert!(cfg.headers.is_none());
    assert!(cfg.providers.is_none());
    assert!(cfg.cache.is_none());
    assert!(cfg.budget.is_none());
    assert!(cfg.rate_limit.is_none());
    assert!(cfg.cost_tracking.is_none());
    assert!(cfg.tracing.is_none());
    assert!(cfg.cooldown_secs.is_none());
    assert!(cfg.health_check_secs.is_none());
    assert!(cfg.bedrock.is_none());
    assert!(cfg.credential_provider.is_none());
}

/// `load_env` and `headers` must round-trip through TOML so they are settable
/// from a config file and every language binding.
#[test]
fn test_llm_config_load_env_and_headers_round_trip() {
    let toml_src = r#"
model = "openai/gpt-4o"
load_env = true

[headers]
"X-Gateway-Key" = "abc123"
"X-Tenant" = "acme"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    assert_eq!(cfg.model, "openai/gpt-4o");
    assert_eq!(cfg.load_env, Some(true));
    let headers = cfg.headers.as_ref().expect("headers present");
    assert_eq!(headers.get("X-Gateway-Key").map(String::as_str), Some("abc123"));
    assert_eq!(headers.get("X-Tenant").map(String::as_str), Some("acme"));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

#[test]
fn test_llm_config_max_concurrency_round_trip() {
    let cfg: LlmConfig = toml::from_str(
        r#"
model = "openai/gpt-4o"
max_concurrency = 3
"#,
    )
    .expect("deserialize LlmConfig from TOML");
    assert_eq!(cfg.max_concurrency, Some(3));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// Empty passthrough fields stay absent from serialized output so bindings
/// never emit `null`/empty knobs the user did not set.
#[test]
fn test_llm_config_omits_empty_passthrough_fields() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    assert!(!json.contains("top_p"), "top_p should be omitted when None: {json}");
    assert!(!json.contains("\"stop\""), "stop should be omitted when None: {json}");
    assert!(!json.contains("\"seed\""), "seed should be omitted when None: {json}");
    assert!(
        !json.contains("presence_penalty"),
        "presence_penalty should be omitted when None: {json}"
    );
    assert!(
        !json.contains("frequency_penalty"),
        "frequency_penalty should be omitted when None: {json}"
    );
    assert!(
        !json.contains("reasoning_effort"),
        "reasoning_effort should be omitted when None: {json}"
    );
    assert!(
        !json.contains("extra_body"),
        "extra_body should be omitted when None: {json}"
    );
    assert!(
        !json.contains("load_env"),
        "load_env should be omitted when None: {json}"
    );
    assert!(!json.contains("headers"), "headers should be omitted when None: {json}");
    assert!(
        !json.contains("providers"),
        "providers should be omitted when None: {json}"
    );
    assert!(!json.contains("cache"), "cache should be omitted when None: {json}");
    assert!(!json.contains("budget"), "budget should be omitted when None: {json}");
    assert!(
        !json.contains("rate_limit"),
        "rate_limit should be omitted when None: {json}"
    );
    assert!(
        !json.contains("cost_tracking"),
        "cost_tracking should be omitted when None: {json}"
    );
    assert!(!json.contains("tracing"), "tracing should be omitted when None: {json}");
    assert!(
        !json.contains("cooldown_secs"),
        "cooldown_secs should be omitted when None: {json}"
    );
    assert!(
        !json.contains("health_check_secs"),
        "health_check_secs should be omitted when None: {json}"
    );
    assert!(!json.contains("bedrock"), "bedrock should be omitted when None: {json}");
    assert!(
        !json.contains("credential_provider"),
        "credential_provider should be omitted when None: {json}"
    );
}

/// Regression test for https://github.com/xberg-io/xberg/issues/1381
///
/// `providers`, `cache`, `budget`, `rate_limit`, `cost_tracking`, `tracing`,
/// `cooldown_secs`, and `health_check_secs` must survive a TOML load and a
/// JSON round-trip so they are settable from a config file and from every
/// language binding, matching liter-llm's canonical `client::LlmConfig`.
#[test]
fn test_llm_config_full_passthrough_fields_round_trip_through_toml_and_json() {
    let toml_src = r#"
model = "openai/gpt-4o"
cost_tracking = true
tracing = false
cooldown_secs = 30
health_check_secs = 60

[[providers]]
name = "my-provider"
base_url = "https://my-llm.example.com/v1"
auth_header = "X-Api-Key"
model_prefixes = ["my-provider/"]

[cache]
max_entries = 512
ttl_seconds = 600
backend = "memory"

[budget]
global_limit = 100.0
enforcement = "hard"

[budget.model_limits]
"openai/gpt-4o" = 25.0

[rate_limit]
rpm = 60
tpm = 100000
window_seconds = 60
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");

    assert_eq!(cfg.cost_tracking, Some(true));
    assert_eq!(cfg.tracing, Some(false));
    assert_eq!(cfg.cooldown_secs, Some(30));
    assert_eq!(cfg.health_check_secs, Some(60));

    let providers = cfg.providers.as_ref().expect("providers present");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].name, "my-provider");
    assert_eq!(providers[0].base_url, "https://my-llm.example.com/v1");
    assert_eq!(providers[0].auth_header.as_deref(), Some("X-Api-Key"));
    assert_eq!(providers[0].model_prefixes, vec!["my-provider/".to_string()]);

    let cache = cfg.cache.as_ref().expect("cache present");
    assert_eq!(cache.max_entries, Some(512));
    assert_eq!(cache.ttl_seconds, Some(600));
    assert_eq!(cache.backend.as_deref(), Some("memory"));
    assert_eq!(cache.backend_config, None);

    let budget = cfg.budget.as_ref().expect("budget present");
    assert_eq!(budget.global_limit, Some(100.0));
    assert_eq!(budget.enforcement.as_deref(), Some("hard"));
    assert_eq!(
        budget.model_limits.as_ref().and_then(|m| m.get("openai/gpt-4o")),
        Some(&25.0)
    );

    let rate_limit = cfg.rate_limit.as_ref().expect("rate_limit present");
    assert_eq!(rate_limit.rpm, Some(60));
    assert_eq!(rate_limit.tpm, Some(100_000));
    assert_eq!(rate_limit.window_seconds, Some(60));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// Regression test for https://github.com/xberg-io/xberg/issues/1381
///
/// `reasoning_effort` and `extra_body` must survive a TOML load and a JSON
/// round-trip so they are settable from a config file and from every language
/// binding, matching liter-llm's `ChatCompletionRequest` request-time fields.
#[test]
fn test_llm_config_reasoning_effort_and_extra_body_round_trip_through_toml_and_json() {
    let toml_src = r#"
model = "openai/gpt-4o"
reasoning_effort = "high"

[extra_body]
safety_settings = { harassment = "block_none" }
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");

    assert_eq!(cfg.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(
        cfg.extra_body,
        Some(serde_json::json!({"safety_settings": {"harassment": "block_none"}}))
    );

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// `top_p`, `stop`, `seed`, `presence_penalty`, and `frequency_penalty` must survive a
/// TOML load and a JSON round-trip so they are settable from a config file and from
/// every language binding, matching liter-llm's `ChatCompletionRequest` request-time
/// fields.
#[test]
fn test_llm_config_sampling_fields_round_trip_through_toml_and_json() {
    let toml_src = r#"
model = "openai/gpt-4o"
top_p = 0.9
stop = ["\n\n", "[END]"]
seed = 42
presence_penalty = 0.5
frequency_penalty = -0.5
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");

    assert_eq!(cfg.top_p, Some(0.9));
    assert_eq!(cfg.stop, Some(vec!["\n\n".to_string(), "[END]".to_string()]));
    assert_eq!(cfg.seed, Some(42));
    assert_eq!(cfg.presence_penalty, Some(0.5));
    assert_eq!(cfg.frequency_penalty, Some(-0.5));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// Empty passthrough fields stay absent from serialized output, exactly like the
/// other optional request-time parameters.
#[test]
fn test_llm_config_omits_empty_sampling_fields() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    assert_eq!(json, r#"{"model":"openai/gpt-4o"}"#);
}

/// `Debug` prints the five new sampling fields verbatim — none of them are credentials.
#[test]
fn test_llm_config_debug_prints_sampling_fields_verbatim() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        top_p: Some(0.9),
        stop: Some(vec!["\n\n".to_string()]),
        seed: Some(42),
        presence_penalty: Some(0.5),
        frequency_penalty: Some(-0.5),
        ..Default::default()
    };
    let rendered = format!("{cfg:?}");
    assert!(rendered.contains("top_p: Some(0.9)"), "{rendered}");
    assert!(rendered.contains(r#"stop: Some(["\n\n"])"#), "{rendered}");
    assert!(rendered.contains("seed: Some(42)"), "{rendered}");
    assert!(rendered.contains("presence_penalty: Some(0.5)"), "{rendered}");
    assert!(rendered.contains("frequency_penalty: Some(-0.5)"), "{rendered}");
}

/// `top_p` at the exact boundaries `0.0` and `1.0` must be accepted.
#[test]
fn test_llm_config_validate_accepts_top_p_at_boundaries() {
    for value in [0.0, 1.0, 0.5] {
        let cfg = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            top_p: Some(value),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok(), "top_p {value} should be accepted");
    }
}

/// `top_p` outside `[0.0, 1.0]` must be a named `XbergError::Validation` naming the
/// offending value and the accepted range.
#[test]
fn test_llm_config_validate_rejects_top_p_out_of_range() {
    for value in [-0.1, 1.1] {
        let cfg = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            top_p: Some(value),
            ..Default::default()
        };
        match cfg.validate() {
            Err(crate::XbergError::Validation { message, .. }) => {
                assert!(message.contains("top_p"), "{message}");
                assert!(message.contains(&value.to_string()), "{message}");
            }
            other => panic!("expected a Validation error for top_p {value}, got {other:?}"),
        }
    }
}

/// `presence_penalty` and `frequency_penalty` at the exact boundaries `-2.0` and `2.0`
/// must be accepted.
#[test]
fn test_llm_config_validate_accepts_penalties_at_boundaries() {
    for value in [-2.0, 2.0, 0.0] {
        let cfg = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            presence_penalty: Some(value),
            frequency_penalty: Some(value),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok(), "penalty {value} should be accepted");
    }
}

/// `presence_penalty` outside `[-2.0, 2.0]` must be a named `XbergError::Validation`.
#[test]
fn test_llm_config_validate_rejects_presence_penalty_out_of_range() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        presence_penalty: Some(2.1),
        ..Default::default()
    };
    match cfg.validate() {
        Err(crate::XbergError::Validation { message, .. }) => {
            assert!(message.contains("presence_penalty"), "{message}");
            assert!(message.contains("2.1"), "{message}");
        }
        other => panic!("expected a Validation error, got {other:?}"),
    }
}

/// `frequency_penalty` outside `[-2.0, 2.0]` must be a named `XbergError::Validation`.
#[test]
fn test_llm_config_validate_rejects_frequency_penalty_out_of_range() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        frequency_penalty: Some(-2.1),
        ..Default::default()
    };
    match cfg.validate() {
        Err(crate::XbergError::Validation { message, .. }) => {
            assert!(message.contains("frequency_penalty"), "{message}");
            assert!(message.contains("-2.1"), "{message}");
        }
        other => panic!("expected a Validation error, got {other:?}"),
    }
}

/// A config with none of the five new sampling fields set must validate successfully —
/// silence in config is always valid.
#[test]
fn test_llm_config_validate_accepts_all_unset_sampling_fields() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        ..Default::default()
    };
    assert!(cfg.validate().is_ok());
}

/// `seed` has no documented range (liter-llm forwards it to the provider as-is), so any
/// `i64` — including negative values — must be accepted by `validate`.
#[test]
fn test_llm_config_validate_accepts_any_seed_value() {
    for value in [i64::MIN, -1, 0, 1, i64::MAX] {
        let cfg = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            seed: Some(value),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok(), "seed {value} should be accepted");
    }
}

/// `max_response_bytes: Some(0)` must be rejected by `validate` as a named
/// `XbergError::Validation`, before `llm::client::build_client_config` ever reaches
/// liter-llm.
#[test]
fn test_llm_config_validate_rejects_zero_max_response_bytes() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        max_response_bytes: Some(0),
        ..Default::default()
    };
    match cfg.validate() {
        Err(crate::XbergError::Validation { message, .. }) => {
            assert!(message.contains("max_response_bytes"), "{message}");
        }
        other => panic!("expected a Validation error for max_response_bytes 0, got {other:?}"),
    }
}

/// A nonzero `max_response_bytes`, and an unset one, must both be accepted by
/// `validate`.
#[test]
fn test_llm_config_validate_accepts_nonzero_or_unset_max_response_bytes() {
    for value in [None, Some(1), Some(usize::MAX)] {
        let cfg = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            max_response_bytes: value,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_ok(),
            "max_response_bytes {value:?} should be accepted"
        );
    }
}

/// `Debug` prints `reasoning_effort` and `extra_body` verbatim — neither is a
/// credential.
#[test]
fn test_llm_config_debug_prints_reasoning_effort_and_extra_body_verbatim() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        reasoning_effort: Some("high".to_string()),
        extra_body: Some(serde_json::json!({"foo": "bar"})),
        ..Default::default()
    };
    let rendered = format!("{cfg:?}");
    assert!(rendered.contains(r#"reasoning_effort: Some("high")"#), "{rendered}");
    assert!(rendered.contains(r#"extra_body: Some(Object"#), "{rendered}");
    assert!(rendered.contains("bar"), "{rendered}");
}

/// `Debug` prints the new passthrough fields verbatim — none of them are
/// credentials, unlike `api_key`, header values, and the Bedrock secrets.
#[test]
fn test_llm_config_debug_prints_new_passthrough_fields_verbatim() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        cost_tracking: Some(true),
        tracing: Some(true),
        cooldown_secs: Some(15),
        health_check_secs: Some(45),
        rate_limit: Some(Box::new(LlmRateLimitConfig {
            rpm: Some(30),
            tpm: None,
            window_seconds: None,
        })),
        ..Default::default()
    };
    let rendered = format!("{cfg:?}");
    assert!(rendered.contains("cost_tracking: Some(true)"), "{rendered}");
    assert!(rendered.contains("tracing: Some(true)"), "{rendered}");
    assert!(rendered.contains("cooldown_secs: Some(15)"), "{rendered}");
    assert!(rendered.contains("health_check_secs: Some(45)"), "{rendered}");
    assert!(rendered.contains("rpm: Some(30)"), "{rendered}");
}

/// Regression test for https://github.com/xberg-io/xberg/issues/1381
///
/// Bedrock region, cross-region prefix, and credentials must survive a TOML
/// load and a JSON round-trip so they are settable from a config file and
/// from every language binding.
#[test]
fn test_llm_config_bedrock_round_trips_through_toml_and_json() {
    let toml_src = r#"
model = "bedrock/anthropic.claude-3-sonnet-20240229-v1:0"

[bedrock]
region = "eu-central-1"
cross_region_prefix = "eu"
access_key_id = "AKIAEXAMPLE"
secret_access_key = "example-secret"
session_token = "example-token"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    assert_eq!(cfg.model, "bedrock/anthropic.claude-3-sonnet-20240229-v1:0");
    let bedrock = cfg.bedrock.as_ref().expect("bedrock present");
    assert_eq!(bedrock.region.as_deref(), Some("eu-central-1"));
    assert_eq!(bedrock.cross_region_prefix.as_deref(), Some("eu"));
    assert_eq!(bedrock.access_key_id.as_deref(), Some("AKIAEXAMPLE"));
    assert_eq!(bedrock.secret_access_key.as_deref(), Some("example-secret"));
    assert_eq!(bedrock.session_token.as_deref(), Some("example-token"));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// A `bedrock` table carrying only `region` must leave the credential fields
/// unset so the AWS default credential chain still applies.
#[test]
fn test_llm_config_bedrock_region_only_leaves_credentials_unset() {
    let cfg: LlmConfig = toml::from_str(
        r#"
model = "bedrock/anthropic.claude-3-sonnet-20240229-v1:0"

[bedrock]
region = "us-east-1"
"#,
    )
    .expect("deserialize LlmConfig from TOML");
    let bedrock = cfg.bedrock.as_ref().expect("bedrock present");
    assert_eq!(bedrock.region.as_deref(), Some("us-east-1"));
    assert_eq!(bedrock.cross_region_prefix, None);
    assert_eq!(bedrock.access_key_id, None);
    assert_eq!(bedrock.secret_access_key, None);
    assert_eq!(bedrock.session_token, None);
}

/// `Debug` must never print a credential — `LlmConfig` and `BedrockConfig` are
/// reachable from error contexts and diagnostic dumps.
#[test]
fn test_llm_config_debug_redacts_every_credential() {
    let mut headers = HashMap::new();
    headers.insert("X-Gateway-Key".to_string(), "header-secret".to_string());
    let cfg = LlmConfig {
        model: "bedrock/anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
        api_key: Some("sk-super-secret".to_string()),
        headers: Some(headers),
        bedrock: Some(Box::new(BedrockConfig {
            region: Some("eu-central-1".to_string()),
            cross_region_prefix: Some("eu".to_string()),
            access_key_id: Some("AKIAEXAMPLE".to_string()),
            secret_access_key: Some("aws-secret".to_string()),
            session_token: Some("aws-token".to_string()),
        })),
        ..Default::default()
    };

    let rendered = format!("{cfg:?}");
    for secret in [
        "sk-super-secret",
        "header-secret",
        "AKIAEXAMPLE",
        "aws-secret",
        "aws-token",
    ] {
        assert!(!rendered.contains(secret), "Debug leaked {secret}: {rendered}");
    }
    // Non-secret routing values stay visible so the output is still diagnosable.
    assert!(
        rendered.contains("eu-central-1"),
        "region should be printed: {rendered}"
    );
    assert!(rendered.contains("\"eu\""), "prefix should be printed: {rendered}");
    assert!(
        rendered.contains("X-Gateway-Key"),
        "header name should be printed: {rendered}"
    );
    assert_eq!(
        rendered.matches(REDACTED).count(),
        5,
        "expected 5 redactions: {rendered}"
    );
}

/// An unset credential must render as `None`, not as a redaction placeholder,
/// so an operator can tell "not configured" from "configured but hidden".
#[test]
fn test_bedrock_config_debug_distinguishes_unset_from_redacted() {
    let bedrock = BedrockConfig {
        region: Some("us-east-1".to_string()),
        ..Default::default()
    };
    let rendered = format!("{bedrock:?}");
    assert_eq!(rendered.matches(REDACTED).count(), 0, "nothing to redact: {rendered}");
    assert_eq!(rendered.matches("None").count(), 4, "4 unset fields: {rendered}");
}

#[test]
fn test_call_mode_serde_round_trip() {
    for (mode, wire) in [
        (CallMode::TextOnly, "\"text_only\""),
        (CallMode::VisionOnly, "\"vision_only\""),
        (CallMode::TextPlusVision, "\"text_plus_vision\""),
    ] {
        let json = serde_json::to_string(&mode).expect("serialize");
        assert_eq!(json, wire);
        let decoded: CallMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }
    assert_eq!(CallMode::default(), CallMode::TextOnly);
}

#[test]
fn test_merge_mode_serde_round_trip() {
    for (mode, wire) in [
        (MergeMode::ObjectMerge, "\"object_merge\""),
        (MergeMode::ArrayConcat, "\"array_concat\""),
        (MergeMode::ObjectFirst, "\"object_first\""),
    ] {
        let json = serde_json::to_string(&mode).expect("serialize");
        assert_eq!(json, wire);
        let decoded: MergeMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }
    assert_eq!(MergeMode::default(), MergeMode::ObjectMerge);
}

/// Regression test for https://github.com/xberg-io/xberg/issues/1381
///
/// An `AzureAd` credential provider must survive a TOML load and a JSON round-trip,
/// tagged with `type = "azure_ad"`.
#[test]
fn test_credential_provider_azure_ad_round_trips_through_toml_and_json() {
    let toml_src = r#"
model = "azure/gpt-4o"

[credential_provider]
type = "azure_ad"
tenant_id = "11111111-1111-1111-1111-111111111111"
client_id = "22222222-2222-2222-2222-222222222222"
client_secret = "example-client-secret"
scope = "https://cognitiveservices.azure.com/.default"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    match cfg.credential_provider.as_deref() {
        Some(CredentialProviderConfig::AzureAd {
            tenant_id,
            client_id,
            client_secret,
            scope,
        }) => {
            assert_eq!(tenant_id, "11111111-1111-1111-1111-111111111111");
            assert_eq!(client_id, "22222222-2222-2222-2222-222222222222");
            assert_eq!(client_secret, "example-client-secret");
            assert_eq!(scope.as_deref(), Some("https://cognitiveservices.azure.com/.default"));
        }
        other => panic!("expected AzureAd variant, got {other:?}"),
    }

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

#[test]
fn credential_provider_rejects_unknown_fields() {
    let json = r#"{
        "type":"azure_ad",
        "tenant_id":"tenant",
        "client_id":"client",
        "client_secret":"secret",
        "unexpected_secret":"secret"
    }"#;
    assert!(serde_json::from_str::<CredentialProviderConfig>(json).is_err());
}

/// A `VertexOauth2` credential provider must survive a TOML load and a JSON round-trip,
/// tagged with `type = "vertex_oauth2"`, and carries a key *file path* rather than key
/// material.
#[test]
fn test_credential_provider_vertex_oauth2_round_trips_through_toml_and_json() {
    let toml_src = r#"
model = "vertex_ai/gemini-1.5-pro"

[credential_provider]
type = "vertex_oauth2"
service_account_key_file = "/etc/xberg/vertex-service-account.json"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    match cfg.credential_provider.as_deref() {
        Some(CredentialProviderConfig::VertexOauth2 {
            service_account_key_file,
            scope,
        }) => {
            assert_eq!(service_account_key_file, "/etc/xberg/vertex-service-account.json");
            assert!(scope.is_none());
        }
        other => panic!("expected VertexOauth2 variant, got {other:?}"),
    }

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// A `VertexAdc` credential provider must survive a TOML load and a JSON round-trip,
/// tagged with `type = "vertex_adc"`, and carries no secret fields at all.
#[test]
fn test_credential_provider_vertex_adc_round_trips_through_toml_and_json() {
    let toml_src = r#"
model = "vertex_ai/gemini-1.5-pro"

[credential_provider]
type = "vertex_adc"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    assert!(matches!(
        cfg.credential_provider.as_deref(),
        Some(CredentialProviderConfig::VertexAdc { scope: None })
    ));

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// A `BedrockWebIdentity` credential provider must survive a TOML load and a JSON
/// round-trip, tagged with `type = "bedrock_web_identity"`.
#[test]
fn test_credential_provider_bedrock_web_identity_round_trips_through_toml_and_json() {
    let toml_src = r#"
model = "bedrock/anthropic.claude-3-sonnet-20240229-v1:0"

[credential_provider]
type = "bedrock_web_identity"
role_arn = "arn:aws:iam::123456789012:role/xberg-bedrock"
token_file = "/var/run/secrets/eks.amazonaws.com/serviceaccount/token"
session_name = "xberg-session"
region = "eu-central-1"
"#;
    let cfg: LlmConfig = toml::from_str(toml_src).expect("deserialize LlmConfig from TOML");
    match cfg.credential_provider.as_deref() {
        Some(CredentialProviderConfig::BedrockWebIdentity {
            role_arn,
            token_file,
            session_name,
            region,
        }) => {
            assert_eq!(role_arn, "arn:aws:iam::123456789012:role/xberg-bedrock");
            assert_eq!(token_file, "/var/run/secrets/eks.amazonaws.com/serviceaccount/token");
            assert_eq!(session_name.as_deref(), Some("xberg-session"));
            assert_eq!(region.as_deref(), Some("eu-central-1"));
        }
        other => panic!("expected BedrockWebIdentity variant, got {other:?}"),
    }

    let round_tripped: LlmConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).expect("serialize")).expect("deserialize");
    assert_eq!(round_tripped, cfg);
}

/// `Debug` must redact `AzureAd`'s `client_secret` while still printing the
/// non-secret routing fields, matching `LlmConfig`'s own redaction policy.
#[test]
fn test_credential_provider_debug_redacts_azure_client_secret() {
    let provider = CredentialProviderConfig::AzureAd {
        tenant_id: "tenant-123".to_string(),
        client_id: "client-456".to_string(),
        client_secret: "super-secret-value".to_string(),
        scope: Some("https://cognitiveservices.azure.com/.default".to_string()),
    };
    let rendered = format!("{provider:?}");
    assert!(!rendered.contains("super-secret-value"), "leaked secret: {rendered}");
    assert!(rendered.contains("tenant-123"), "{rendered}");
    assert!(rendered.contains("client-456"), "{rendered}");
    assert!(rendered.contains(REDACTED), "{rendered}");
}

/// `Debug` must print every field of the non-secret variants verbatim: `VertexOauth2`'s
/// key *file path*, `VertexAdc`'s scope, and `BedrockWebIdentity`'s role/token-path/
/// session/region are not credentials in themselves.
#[test]
fn test_credential_provider_debug_prints_non_secret_variants_verbatim() {
    let vertex_oauth2 = CredentialProviderConfig::VertexOauth2 {
        service_account_key_file: "/etc/xberg/vertex.json".to_string(),
        scope: None,
    };
    assert_eq!(
        format!("{vertex_oauth2:?}"),
        r#"VertexOauth2 { service_account_key_file: "/etc/xberg/vertex.json", scope: None }"#
    );

    let vertex_adc = CredentialProviderConfig::VertexAdc { scope: None };
    assert_eq!(format!("{vertex_adc:?}"), "VertexAdc { scope: None }");

    let bedrock = CredentialProviderConfig::BedrockWebIdentity {
        role_arn: "arn:aws:iam::123456789012:role/xberg-bedrock".to_string(),
        token_file: "/var/run/token".to_string(),
        session_name: None,
        region: None,
    };
    let rendered = format!("{bedrock:?}");
    assert!(rendered.starts_with("BedrockWebIdentity {"), "{rendered}");
    assert!(
        rendered.contains(r#"role_arn: "arn:aws:iam::123456789012:role/xberg-bedrock""#),
        "{rendered}"
    );
    assert!(rendered.contains(r#"token_file: "/var/run/token""#), "{rendered}");
    assert!(rendered.contains("session_name: None"), "{rendered}");
    assert!(rendered.contains("region: None"), "{rendered}");
}

/// The `credential_provider` field must be omitted from serialized output when unset,
/// same as every other optional passthrough field.
#[test]
fn test_credential_provider_omitted_from_json_when_none() {
    let cfg = LlmConfig {
        model: "openai/gpt-4o".to_string(),
        ..LlmConfig::default()
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    assert_eq!(json, r#"{"model":"openai/gpt-4o"}"#);
}
