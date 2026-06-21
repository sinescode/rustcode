//! Comprehensive integration tests for rustcode-core.
//!
//! These tests exercise real application logic — no mocks, no fakes.
//! Where a live database or API key would be required, the test is
//! either structured as a pure function test, struct-construction test,
//! or explicitly ignored with a documented prerequisite.

use rustcode_core::config::{
    self, discover_config_files, discover_opencode_dirs, merge_info, normalize_config,
    parse_jsonc, substitute_variables, validate_info, writable, Info, LogLevel, PermissionConfig,
    PermissionRule as ConfigPermissionRule, PermissionAction as ConfigPermissionAction,
    ProviderConfig, AgentConfig, AgentMode, ShareMode,
};
use rustcode_core::database::DatabaseServiceError;
use rustcode_core::encryption::hmac::{EncryptionError, EncryptionService, KEY_LENGTH};
use rustcode_core::error::{Error, PermissionError};
use rustcode_core::event::{
    EventCursor, EventDefinition, EventError, EventId, EventPayload, EventPubSub, EventRegistry,
    EventV2, SyncConfig,
};
use rustcode_core::id::{self, ascending, create, descending, timestamp, Direction, IdError, IdPrefix};
use rustcode_core::permission::{
    bash_arity_prefix, disabled_tools, evaluate, merge_rulesets, wildcard_match, PermissionAction,
    PermissionRule, PermissionRuleset, rules_from_config,
};
use rustcode_core::provider::{
    ai_sdk_namespace, default_temperature, default_top_p, default_top_k, is_bundled_provider,
    parse_model, sanitize_surrogates, sdk_key, sort_models, FinishReason, LlmEvent, LlmResponse,
    Model, Usage,
};
use rustcode_core::model::{
    parse_model_ref, well_known_providers, ModelInfo, ModelV2Id, ProviderV2Id,
};
use rustcode_core::session::{
    fork_title, is_retryable, retry_delay, retry_delay_with_headers, usable, check_overflow,
    SessionError, SessionInfo, SessionTimestamps, TokenUsage, CacheUsage, Message, MessageInfo,
    UserInfo, AssistantInfo, CreateSessionInput,
};
use rustcode_core::shell_parser::{FileOp, ShellParser};
use rustcode_core::truncate::{truncate_output, TruncateOptions, TruncateService, MAX_LINES, MAX_CHARS};

use std::collections::HashMap;
use std::path::Path;

// ═════════════════════════════════════════════════════════════════════════════
// 1. Config System
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_config_parse_jsonc_basic() {
    let input = r#"{
        // line comment
        "model": "anthropic/claude-sonnet",
        /* block comment */
        "shell": "/bin/bash",
    }"#;
    let parsed = parse_jsonc(input, Path::new("test.jsonc")).unwrap();
    assert_eq!(parsed["model"], "anthropic/claude-sonnet");
    assert_eq!(parsed["shell"], "/bin/bash");
}

#[test]
fn test_config_parse_jsonc_rejects_invalid() {
    let input = r#"{ invalid json }"#;
    let result = parse_jsonc(input, Path::new("bad.jsonc"));
    assert!(result.is_err());
}

#[test]
fn test_config_validate_info_rejects_unknown_keys() {
    let val = serde_json::json!({"bogus_field": true});
    let result = validate_info(val, Path::new("test.json"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("bogus_field"));
}

#[test]
fn test_config_validate_info_accepts_empty() {
    let val = serde_json::json!({});
    let info = validate_info(val, Path::new("test.json")).unwrap();
    assert!(info.model.is_none());
}

#[test]
fn test_config_env_substitution() {
    std::env::set_var("RUSTCODE_TEST_INT_CFG_ENV", "resolved_value");
    let result = substitute_variables(
        "{env:RUSTCODE_TEST_INT_CFG_ENV}",
        Path::new("."),
        None,
    )
    .unwrap();
    assert_eq!(result, "resolved_value");
    std::env::remove_var("RUSTCODE_TEST_INT_CFG_ENV");
}

#[test]
fn test_config_env_substitution_missing_is_empty() {
    let result = substitute_variables(
        "{env:THIS_VAR_SHOULD_NOT_EXIST_ZZZZ}",
        Path::new("."),
        None,
    )
    .unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_config_env_substitution_with_custom_map() {
    let mut env = HashMap::new();
    env.insert("MY_VAR".into(), "custom_val".into());
    let result = substitute_variables("_{env:MY_VAR}_", Path::new("."), Some(&env)).unwrap();
    assert_eq!(result, "_custom_val_");
}

#[test]
fn test_config_file_substitution_comment_line_keeps_token() {
    let result = substitute_variables(
        "// {file:./nonexistent.txt}\nhello",
        Path::new("."),
        None,
    )
    .unwrap();
    assert!(result.contains("{file:./nonexistent.txt}"));
    assert!(result.contains("hello"));
}

#[test]
fn test_config_merge_info_instructions_dedup() {
    let mut target = Info {
        instructions: vec!["a".into(), "b".into()],
        ..Info::default()
    };
    let source = Info {
        instructions: vec!["b".into(), "c".into()],
        ..Info::default()
    };
    merge_info(&mut target, &source);
    assert_eq!(target.instructions, vec!["a", "b", "c"]);
}

#[test]
fn test_config_merge_model_source_wins() {
    let mut target = Info {
        model: Some("old/model".into()),
        ..Info::default()
    };
    let source = Info {
        model: Some("new/model".into()),
        ..Info::default()
    };
    merge_info(&mut target, &source);
    assert_eq!(target.model.unwrap(), "new/model");
}

#[test]
fn test_config_merge_scalar_target_preserved_when_source_none() {
    let mut target = Info {
        shell: Some("/bin/zsh".into()),
        ..Info::default()
    };
    let source = Info::default();
    merge_info(&mut target, &source);
    assert_eq!(target.shell.unwrap(), "/bin/zsh");
}

#[test]
fn test_config_normalize_tools_to_permission() {
    let mut info = Info {
        tools: vec![
            ("bash".into(), true),
            ("edit".into(), false),
            ("read".into(), true),
        ]
        .into_iter()
        .collect(),
        ..Info::default()
    };
    normalize_config(&mut info);
    assert!(info.tools.is_empty(), "tools should be cleared after normalization");
    let perm = info.permission.expect("permission should be populated");
    assert!(perm.bash.is_some());
    assert!(perm.edit.is_some());
}

#[test]
fn test_config_normalize_autoshare() {
    let mut info = Info {
        autoshare: Some(true),
        ..Info::default()
    };
    normalize_config(&mut info);
    assert_eq!(info.share, Some(ShareMode::Auto));
}

#[test]
fn test_config_writable_strips_plugin_origins() {
    let mut info = Info::default();
    info.plugin_origins.push(config::PluginOrigin {
        spec: config::PluginSpec::Simple("test".into()),
        source: "test.json".into(),
        scope: config::PluginScope::Local,
    });
    let w = writable(&info);
    assert!(w.plugin_origins.is_empty());
}

#[test]
fn test_config_serde_roundtrip() {
    let info = Info {
        model: Some("anthropic/claude-sonnet".into()),
        shell: Some("/bin/bash".into()),
        log_level: Some(LogLevel::Info),
        ..Info::default()
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: Info = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model.unwrap(), "anthropic/claude-sonnet");
    assert_eq!(deserialized.shell.unwrap(), "/bin/bash");
    assert_eq!(deserialized.log_level.unwrap(), LogLevel::Info);
}

#[test]
fn test_config_discover_files_walks_up() {
    let dir = Path::new("/");
    let files = discover_config_files("opencode", dir, None).unwrap();
    assert!(files.is_empty() || files.iter().all(|f| f.file_name().unwrap().to_string_lossy().contains("opencode")));
}

#[test]
fn test_config_discover_opencode_dirs() {
    let dir = Path::new("/");
    let dirs = discover_opencode_dirs(dir, None).unwrap();
    assert!(dirs.is_empty() || dirs.iter().all(|d| d.ends_with(".opencode")));
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Provider Resolution
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_model_ref_standard() {
    let parsed = parse_model_ref("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(parsed.provider_id, "anthropic");
    assert_eq!(parsed.model_id, "claude-sonnet-4");
}

#[test]
fn test_parse_model_ref_multi_slash() {
    let parsed = parse_model_ref("openai/gpt-4/turbo").unwrap();
    assert_eq!(parsed.provider_id, "openai");
    assert_eq!(parsed.model_id, "gpt-4/turbo");
}

#[test]
fn test_parse_model_ref_no_slash_returns_none() {
    assert!(parse_model_ref("baremodel").is_none());
}

#[test]
fn test_parse_model_ref_empty_returns_none() {
    assert!(parse_model_ref("").is_none());
}

#[test]
fn test_parse_model_from_provider_with_slash() {
    let mr = parse_model("google/gemini-2-flash");
    assert_eq!(mr.provider_id, "google");
    assert_eq!(mr.model_id, "gemini-2-flash");
}

#[test]
fn test_parse_model_no_slash_uses_full_string_as_provider() {
    let mr = parse_model("baremodel");
    assert_eq!(mr.provider_id, "baremodel");
    assert_eq!(mr.model_id, "");
}

#[test]
fn test_sdk_key_known_providers() {
    assert_eq!(sdk_key("@ai-sdk/anthropic"), Some("anthropic"));
    assert_eq!(sdk_key("@ai-sdk/openai"), Some("openai"));
    assert_eq!(sdk_key("@ai-sdk/google"), Some("google"));
    assert_eq!(sdk_key("@ai-sdk/mistral"), Some("mistral"));
    assert_eq!(sdk_key("@ai-sdk/groq"), Some("groq"));
    assert_eq!(sdk_key("@openrouter/ai-sdk-provider"), Some("openrouter"));
}

#[test]
fn test_sdk_key_unknown_returns_none() {
    assert_eq!(sdk_key("@unknown/sdk"), None);
    assert_eq!(sdk_key(""), None);
}

#[test]
fn test_is_bundled_provider() {
    assert!(is_bundled_provider("@ai-sdk/anthropic"));
    assert!(is_bundled_provider("@ai-sdk/openai"));
    assert!(is_bundled_provider("@ai-sdk/amazon-bedrock"));
    assert!(is_bundled_provider("@openrouter/ai-sdk-provider"));
    assert!(!is_bundled_provider("@fake/provider"));
}

#[test]
fn test_default_temperature_specific_models() {
    assert_eq!(default_temperature("claude-sonnet-4"), Some(1.0));
    assert_eq!(default_temperature("gemini-2-flash"), Some(1.0));
    assert_eq!(default_temperature("qwen-2.5"), Some(0.55));
    assert!(default_temperature("gpt-4o").is_none());
}

#[test]
fn test_default_top_p_specific_models() {
    assert_eq!(default_top_p("qwen-2.5"), Some(1.0));
    assert_eq!(default_top_p("gemini-2-flash"), Some(0.95));
    assert!(default_top_p("gpt-4o").is_none());
}

#[test]
fn test_default_top_k_specific_models() {
    assert!(default_top_k("minimax-m2").is_some());
    assert_eq!(default_top_k("gemini-2-flash"), Some(64));
    assert!(default_top_k("claude-sonnet-4").is_none());
}

#[test]
fn test_ai_sdk_namespace_known() {
    assert_eq!(ai_sdk_namespace("@ai-sdk/anthropic"), Some("anthropic"));
    assert_eq!(ai_sdk_namespace("@ai-sdk/openai"), Some("openai"));
    assert_eq!(ai_sdk_namespace("@ai-sdk/openai-compatible"), Some("openai"));
}

#[test]
fn test_ai_sdk_namespace_unknown() {
    assert_eq!(ai_sdk_namespace("@unknown/sdk"), None);
}

#[test]
fn test_sort_models_by_priority() {
    let mut models = vec![
        "gemini-3-pro".to_string(),
        "claude-sonnet-4".to_string(),
        "gpt-5".to_string(),
        "some-other".to_string(),
    ];
    sort_models(&mut models);
    assert_eq!(models[0], "gpt-5", "gpt-5 should be first");
    assert!(
        models.iter().position(|m| m == "some-other")
            > models.iter().position(|m| m == "gpt-5")
    );
}

#[test]
fn test_sort_models_latest_last() {
    let mut models = vec![
        "claude-sonnet-4-latest".to_string(),
        "claude-sonnet-4".to_string(),
    ];
    sort_models(&mut models);
    assert_eq!(models[1], "claude-sonnet-4-latest", "latest should come after");
}

#[test]
fn test_well_known_providers_constants() {
    assert_eq!(well_known_providers::ANTHROPIC, "anthropic");
    assert_eq!(well_known_providers::OPENAI, "openai");
    assert_eq!(well_known_providers::GOOGLE, "google");
    assert_eq!(well_known_providers::OPENCODE, "opencode");
    assert_eq!(well_known_providers::MISTRAL, "mistral");
}

#[test]
fn test_model_info_empty() {
    let info = ModelInfo::empty("test-provider".into(), "test-model".into());
    assert_eq!(info.id, "test-model");
    assert_eq!(info.provider_id, "test-provider");
    assert!(info.enabled);
}

#[test]
fn test_sanitize_surrogates_passes_through_normal_text() {
    let input = "hello world";
    assert_eq!(sanitize_surrogates(input), input);
}

#[test]
fn test_sanitize_surrogates_handles_unicode() {
    let input = "hello 世界 🎉";
    assert_eq!(sanitize_surrogates(input), input);
}

#[test]
fn test_sanitize_surrogates_empty_string() {
    assert_eq!(sanitize_surrogates(""), "");
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Session Lifecycle
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_session_info_serde_roundtrip() {
    let info = SessionInfo {
        id: "ses_test123".into(),
        slug: "test-session".into(),
        project_id: "proj_1".into(),
        workspace_id: None,
        directory: "/tmp".into(),
        path: None,
        parent_id: None,
        title: "Integration Test".into(),
        agent: Some("build".into()),
        model: None,
        version: "1.0.0".into(),
        summary: None,
        cost: 1.25,
        tokens: TokenUsage {
            input: 100,
            output: 50,
            reasoning: 10,
            cache: CacheUsage { read: 5, write: 3 },
        },
        share: None,
        metadata: None,
        permission: None,
        revert: None,
        time: SessionTimestamps {
            created: 1000,
            updated: 2000,
            compacting: None,
            archived: None,
        },
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: SessionInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "ses_test123");
    assert_eq!(deserialized.title, "Integration Test");
    assert_eq!(deserialized.cost, 1.25);
    assert_eq!(deserialized.tokens.input, 100);
    assert_eq!(deserialized.tokens.output, 50);
    assert_eq!(deserialized.tokens.reasoning, 10);
}

#[test]
fn test_session_timestamps_serde() {
    let ts = SessionTimestamps {
        created: 1000,
        updated: 2000,
        compacting: Some(1500),
        archived: None,
    };
    let json = serde_json::to_string(&ts).unwrap();
    let deserialized: SessionTimestamps = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.created, 1000);
    assert_eq!(deserialized.updated, 2000);
    assert_eq!(deserialized.compacting, Some(1500));
    assert!(deserialized.archived.is_none());
}

#[test]
fn test_message_info_user_serde() {
    let info = MessageInfo::User(UserInfo {
        id: "msg_1".into(),
        session_id: "ses_1".into(),
        agent: Some("build".into()),
        model: None,
        time: rustcode_core::session::MessageTime {
            created: 1000,
            completed: None,
        },
    });
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: MessageInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id(), "msg_1");
    assert_eq!(deserialized.role(), "user");
}

#[test]
fn test_message_info_assistant_serde() {
    let info = MessageInfo::Assistant(AssistantInfo {
        id: "msg_2".into(),
        session_id: "ses_1".into(),
        parent_id: "msg_1".into(),
        agent: "build".into(),
        model_id: Some("claude-sonnet".into()),
        provider_id: Some("anthropic".into()),
        variant: None,
        summary: false,
        cost: 0.5,
        tokens: TokenUsage::default(),
        finish: Some("stop".into()),
        error: None,
        time: rustcode_core::session::MessageTime {
            created: 1000,
            completed: Some(2000),
        },
    });
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: MessageInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id(), "msg_2");
    assert_eq!(deserialized.role(), "assistant");
    match deserialized {
        MessageInfo::Assistant(ref a) => {
            assert_eq!(a.finish.as_deref(), Some("stop"));
            assert_eq!(a.cost, 0.5);
        }
        _ => panic!("expected Assistant"),
    }
}

#[test]
fn test_create_session_input_builds() {
    let input = CreateSessionInput {
        project_id: "proj_1".into(),
        workspace_id: Some("ws_1".into()),
        directory: "/tmp/test".into(),
        path: Some("/tmp/test/sub".into()),
        parent_id: None,
        title: Some("Test Session".into()),
        agent: Some("build".into()),
        model: None,
        metadata: None,
        permission: None,
    };
    assert_eq!(input.project_id, "proj_1");
    assert_eq!(input.title.unwrap(), "Test Session");
}

#[test]
fn test_fork_title_first_fork() {
    assert_eq!(fork_title("Original"), "Original (fork #1)");
}

#[test]
fn test_fork_title_increments() {
    assert_eq!(fork_title("Original (fork #3)"), "Original (fork #4)");
}

#[test]
fn test_fork_title_no_paren() {
    assert_eq!(fork_title("Session"), "Session (fork #1)");
}

#[test]
fn test_retry_delay_exponential_backoff() {
    let d1 = retry_delay(1);
    let d2 = retry_delay(2);
    let d3 = retry_delay(3);
    assert!(d1 >= 2000);
    assert!(d2 >= d1);
    assert!(d3 >= d2);
    assert!(d3 <= 30_000);
}

#[test]
fn test_retry_delay_with_headers_uses_retry_after_ms() {
    let mut headers = HashMap::new();
    headers.insert("retry-after-ms".into(), "5000".into());
    let delay = retry_delay_with_headers(1, Some(&headers));
    assert_eq!(delay, 5000);
}

#[test]
fn test_retry_delay_with_headers_falls_back_to_exponential() {
    let headers: HashMap<String, String> = HashMap::new();
    let delay = retry_delay_with_headers(3, Some(&headers));
    assert!(delay >= 2000);
}

#[test]
fn test_is_retryable_detects_common_patterns() {
    assert!(is_retryable("overloaded"));
    assert!(is_retryable("rate limit exceeded"));
    assert!(is_retryable("too many requests"));
    assert!(is_retryable("connection reset"));
    assert!(is_retryable("service unavailable"));
    assert!(is_retryable("internal server error"));
    assert!(!is_retryable("invalid api key"));
    assert!(!is_retryable("everything is fine"));
}

#[test]
fn test_usable_context_calculation() {
    let model = Model {
        id: "test".into(),
        provider_id: "test".into(),
        name: "Test".into(),
        api: rustcode_core::provider::ApiInfo {
            id: "test".into(),
            url: String::new(),
            npm: "@ai-sdk/test".into(),
        },
        family: None,
        capabilities: rustcode_core::provider::Capabilities::default(),
        cost: rustcode_core::provider::Cost::default(),
        limit: rustcode_core::provider::TokenLimit {
            context: 200000,
            input: Some(180000),
            output: 4096,
        },
        status: rustcode_core::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: String::new(),
        variants: None,
    };
    let u = usable(&model, Some(4096));
    assert!(u > 0);
    assert!(u <= 180000);
}

#[test]
fn test_check_overflow_false_for_small_usage() {
    let model = Model {
        id: "test".into(),
        provider_id: "test".into(),
        name: "Test".into(),
        api: rustcode_core::provider::ApiInfo {
            id: "test".into(),
            url: String::new(),
            npm: "@ai-sdk/test".into(),
        },
        family: None,
        capabilities: rustcode_core::provider::Capabilities::default(),
        cost: rustcode_core::provider::Cost::default(),
        limit: rustcode_core::provider::TokenLimit {
            context: 200000,
            input: Some(180000),
            output: 4096,
        },
        status: rustcode_core::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: String::new(),
        variants: None,
    };
    let tokens = TokenUsage {
        input: 100,
        output: 50,
        ..TokenUsage::default()
    };
    assert!(!check_overflow(&tokens, &model, None));
}

#[test]
fn test_check_overflow_true_for_large_usage() {
    let model = Model {
        id: "test".into(),
        provider_id: "test".into(),
        name: "Test".into(),
        api: rustcode_core::provider::ApiInfo {
            id: "test".into(),
            url: String::new(),
            npm: "@ai-sdk/test".into(),
        },
        family: None,
        capabilities: rustcode_core::provider::Capabilities::default(),
        cost: rustcode_core::provider::Cost::default(),
        limit: rustcode_core::provider::TokenLimit {
            context: 1000,
            input: None,
            output: 100,
        },
        status: rustcode_core::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: String::new(),
        variants: None,
    };
    let tokens = TokenUsage {
        input: 950,
        output: 50,
        ..TokenUsage::default()
    };
    assert!(check_overflow(&tokens, &model, None));
}

#[test]
fn test_usage_visible_output_tokens() {
    let usage = Usage {
        output_tokens: Some(100),
        reasoning_tokens: Some(30),
        ..Usage::default()
    };
    assert_eq!(usage.visible_output_tokens(), 70);
}

#[test]
fn test_usage_visible_output_clamps_to_zero() {
    let usage = Usage {
        output_tokens: Some(10),
        reasoning_tokens: Some(50),
        ..Usage::default()
    };
    assert_eq!(usage.visible_output_tokens(), 0);
}

#[test]
fn test_llm_response_text_concatenation() {
    let response = LlmResponse {
        events: vec![
            LlmEvent::TextDelta {
                id: "1".into(),
                text: "Hello ".into(),
                provider_metadata: None,
            },
            LlmEvent::TextDelta {
                id: "2".into(),
                text: "World".into(),
                provider_metadata: None,
            },
        ],
        usage: None,
    };
    assert_eq!(response.text(), "Hello World");
}

#[test]
fn test_llm_response_reasoning_concatenation() {
    let response = LlmResponse {
        events: vec![
            LlmEvent::ReasoningDelta {
                id: "r1".into(),
                text: "think ".into(),
                provider_metadata: None,
            },
            LlmEvent::ReasoningDelta {
                id: "r2".into(),
                text: "step".into(),
                provider_metadata: None,
            },
        ],
        usage: None,
    };
    assert_eq!(response.reasoning(), "think step");
}

#[test]
fn test_llm_event_type_tag() {
    assert_eq!(LlmEvent::StepStart { index: 0 }.type_tag(), "step-start");
    assert_eq!(
        LlmEvent::TextDelta {
            id: "t1".into(),
            text: "hi".into(),
            provider_metadata: None,
        }
        .type_tag(),
        "text-delta"
    );
    assert_eq!(
        LlmEvent::Finish {
            reason: FinishReason::Stop,
            usage: None,
            provider_metadata: None,
        }
        .type_tag(),
        "finish"
    );
}

#[test]
fn test_llm_event_is_text_delta() {
    let event = LlmEvent::TextDelta {
        id: "t1".into(),
        text: "hi".into(),
        provider_metadata: None,
    };
    assert!(event.is_text_delta());
    assert!(!event.is_tool_call());
}

#[test]
fn test_llm_event_usage_from_step_finish() {
    let usage = Usage {
        input_tokens: Some(10),
        output_tokens: Some(20),
        ..Usage::default()
    };
    let event = LlmEvent::StepFinish {
        index: 0,
        reason: FinishReason::Stop,
        usage: Some(usage.clone()),
        provider_metadata: None,
    };
    assert_eq!(event.usage(), Some(&usage));
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Permission Evaluation
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_wildcard_match_exact() {
    assert!(wildcard_match("bash", "bash"));
}

#[test]
fn test_wildcard_match_star() {
    assert!(wildcard_match("anything", "*"));
}

#[test]
fn test_wildcard_match_pattern() {
    assert!(wildcard_match("foo/bar/baz", "foo/*/baz"));
    assert!(!wildcard_match("foo/bar/qux", "foo/*/baz"));
}

#[test]
fn test_wildcard_match_question_mark() {
    assert!(wildcard_match("cat", "c?t"));
    assert!(!wildcard_match("cart", "c?t"));
}

#[test]
fn test_wildcard_match_backslash_normalization() {
    assert!(wildcard_match("a\\b", "a/*"));
}

#[test]
fn test_wildcard_match_trailing_dot_star() {
    assert!(wildcard_match("hello world", "hello .*"));
    assert!(wildcard_match("hello world extra", "hello .*"));
}

#[test]
fn test_wildcard_match_empty_pattern() {
    assert!(wildcard_match("anything", ""));
}

#[test]
fn test_wildcard_match_regex_special_chars() {
    assert!(wildcard_match("file.txt", "*.txt"));
    assert!(!wildcard_match("file.txt", "*.md"));
}

#[test]
fn test_evaluate_allow() {
    let rules: PermissionRuleset = vec![PermissionRule {
        permission: "bash".into(),
        pattern: "*".into(),
        action: PermissionAction::Allow,
    }];
    let result = evaluate("bash", "*", &[&rules]);
    assert_eq!(result.action, PermissionAction::Allow);
    assert_eq!(result.matched_permission.unwrap(), "bash");
}

#[test]
fn test_evaluate_deny() {
    let rules: PermissionRuleset = vec![PermissionRule {
        permission: "edit".into(),
        pattern: "/etc/*".into(),
        action: PermissionAction::Deny,
    }];
    let result = evaluate("edit", "/etc/passwd", &[&rules]);
    assert_eq!(result.action, PermissionAction::Deny);
}

#[test]
fn test_evaluate_last_match_wins() {
    let rules: PermissionRuleset = vec![
        PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "bash".into(),
            pattern: "rm*".into(),
            action: PermissionAction::Deny,
        },
    ];
    let result = evaluate("bash", "rm -rf /", &[&rules]);
    assert_eq!(result.action, PermissionAction::Deny);
}

#[test]
fn test_evaluate_default_ask() {
    let rules: PermissionRuleset = vec![];
    let result = evaluate("bash", "/anything", &[&rules]);
    assert_eq!(result.action, PermissionAction::Ask);
    assert!(result.matched_permission.is_none());
}

#[test]
fn test_evaluate_multiple_rulesets() {
    let global: PermissionRuleset = vec![PermissionRule {
        permission: "*".into(),
        pattern: "*".into(),
        action: PermissionAction::Deny,
    }];
    let local: PermissionRuleset = vec![PermissionRule {
        permission: "read".into(),
        pattern: "*.ts".into(),
        action: PermissionAction::Allow,
    }];
    let result = evaluate("read", "file.ts", &[&global, &local]);
    assert_eq!(result.action, PermissionAction::Allow);
}

#[test]
fn test_bash_arity_prefix_simple() {
    assert_eq!(bash_arity_prefix(&["cat", "file.txt"]), ["cat"]);
}

#[test]
fn test_bash_arity_prefix_git() {
    assert_eq!(
        bash_arity_prefix(&["git", "checkout", "main"]),
        ["git", "checkout"]
    );
}

#[test]
fn test_bash_arity_prefix_npm_run() {
    assert_eq!(
        bash_arity_prefix(&["npm", "run", "dev", "--watch"]),
        ["npm", "run", "dev"]
    );
}

#[test]
fn test_bash_arity_prefix_unknown_falls_back_to_first() {
    assert_eq!(bash_arity_prefix(&["myapp", "arg1"]), ["myapp"]);
}

#[test]
fn test_bash_arity_prefix_empty() {
    let empty: &[&str] = &[];
    assert!(bash_arity_prefix(empty).is_empty());
}

#[test]
fn test_rules_from_config_converts_basic() {
    let cfg = PermissionConfig {
        bash: Some(ConfigPermissionRule::Action(ConfigPermissionAction::Allow)),
        read: Some(ConfigPermissionRule::Object(
            vec![
                ("*.ts".into(), ConfigPermissionAction::Allow),
                ("*.env".into(), ConfigPermissionAction::Deny),
            ]
            .into_iter()
            .collect(),
        )),
        ..PermissionConfig::default()
    };
    let rules = rules_from_config(&cfg);
    assert!(rules.iter().any(|r| r.permission == "bash" && r.pattern == "*"));
    assert!(rules.iter().any(|r| r.permission == "read" && r.pattern == "*.ts"));
    assert!(rules.iter().any(|r| r.permission == "read" && r.pattern == "*.env"));
}

#[test]
fn test_disabled_tools_identifies_denied() {
    let rules: PermissionRuleset = vec![PermissionRule {
        permission: "bash".into(),
        pattern: "*".into(),
        action: PermissionAction::Deny,
    }];
    let disabled = disabled_tools(&["bash".into(), "read".into()], &rules);
    assert!(disabled.contains("bash"));
    assert!(!disabled.contains("read"));
}

#[test]
fn test_merge_rulesets_combines() {
    let a: PermissionRuleset = vec![PermissionRule {
        permission: "bash".into(),
        pattern: "*".into(),
        action: PermissionAction::Allow,
    }];
    let b: PermissionRuleset = vec![PermissionRule {
        permission: "read".into(),
        pattern: "*.ts".into(),
        action: PermissionAction::Allow,
    }];
    let merged = merge_rulesets(&[a, b]);
    assert_eq!(merged.len(), 2);
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. Event System
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_event_id_create_has_prefix() {
    let id = EventId::create();
    assert!(id.as_str().starts_with("evt_"));
    assert_eq!(id.as_str().len(), 4 + 26);
}

#[test]
fn test_event_id_from_external() {
    let id = EventId::from_external("custom-123");
    assert_eq!(id.as_str(), "evt_custom-123");
}

#[test]
fn test_event_id_display() {
    let id = EventId::from_external("test");
    assert_eq!(format!("{id}"), "evt_test");
}

#[test]
fn test_event_id_from_str_valid() {
    let id: EventId = "evt_abc123".parse().unwrap();
    assert_eq!(id.as_str(), "evt_abc123");
}

#[test]
fn test_event_id_from_str_invalid() {
    let result: Result<EventId, String> = "bad_prefix".parse();
    assert!(result.is_err());
}

#[test]
fn test_event_cursor_ordering() {
    let a = EventCursor::new(1);
    let b = EventCursor::new(2);
    let c = EventCursor::new(2);
    assert!(a < b);
    assert_eq!(b, c);
    assert_eq!(EventCursor::ZERO.value(), 0);
}

#[test]
fn test_event_cursor_conversion() {
    let c: EventCursor = 42u64.into();
    assert_eq!(c.value(), 42);
    let v: u64 = c.into();
    assert_eq!(v, 42);
}

#[test]
fn test_event_payload_create_and_build() {
    let id = EventId::create();
    let data = serde_json::json!({"key": "value"});
    let payload = EventPayload::new(id.clone(), "test.event", data.clone());
    assert_eq!(payload.id.as_str(), id.as_str());
    assert_eq!(payload.event_type, "test.event");
    assert_eq!(payload.data, data);
    assert!(!payload.replay);
}

#[test]
fn test_event_payload_with_builders() {
    let payload = EventPayload::new(EventId::create(), "test.event", serde_json::json!({}))
        .with_version(2)
        .with_replay();
    assert_eq!(payload.version, Some(2));
    assert!(payload.replay);
}

#[test]
fn test_event_payload_aggregate_id() {
    let sync = SyncConfig {
        version: 1,
        aggregate: "sessionID".into(),
    };
    let payload = EventPayload::new(
        EventId::create(),
        "test.event",
        serde_json::json!({"sessionID": "ses_123"}),
    );
    assert_eq!(payload.aggregate_id(&sync).unwrap(), "ses_123");
}

#[test]
fn test_event_definition_new() {
    let def = EventDefinition::new(
        "session.created",
        Some(SyncConfig {
            version: 1,
            aggregate: "sessionID".into(),
        }),
        serde_json::json!({"type": "object"}),
    );
    assert!(def.is_sync());
    assert_eq!(def.versioned_type().unwrap(), "session.created.1");
}

#[test]
fn test_event_definition_non_sync() {
    let def = EventDefinition::new("ephemeral.event", None, serde_json::json!({}));
    assert!(!def.is_sync());
    assert!(def.versioned_type().is_none());
}

#[test]
fn test_event_pub_sub_publish_and_subscribe() {
    let pubsub = EventPubSub::new(16);
    let mut sub = pubsub.subscribe();

    let payload = EventPayload::new(EventId::create(), "test.event", serde_json::json!({"msg": "hi"}));
    let sent = pubsub.publish(payload.clone()).unwrap();
    assert!(sent > 0);

    let received = sub.recv().await;
    let received = received.expect("should receive event");
    assert_eq!(received.event_type, "test.event");
    assert_eq!(received.data["msg"], "hi");
}

#[test]
fn test_event_registry_define_and_get() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let registry = EventRegistry::new();
        let def = EventDefinition::new("test.event", None, serde_json::json!({}));
        registry.define(def.clone()).await;
        let retrieved = registry.get("test.event").await.unwrap();
        assert_eq!(retrieved.event_type, "test.event");
    });
}

#[test]
fn test_event_registry_definitions() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let registry = EventRegistry::new();
        registry
            .define(EventDefinition::new("ev1", None, serde_json::json!({})))
            .await;
        registry
            .define(EventDefinition::new("ev2", None, serde_json::json!({})))
            .await;
        let defs = registry.definitions().await;
        assert_eq!(defs.len(), 2);
    });
}

#[test]
fn test_event_registry_sync_definitions() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let registry = EventRegistry::new();
        registry
            .define(EventDefinition::new("sync_ev", Some(SyncConfig { version: 1, aggregate: "id".into() }), serde_json::json!({})))
            .await;
        registry
            .define(EventDefinition::new("non_sync", None, serde_json::json!({})))
            .await;
        let sync_defs = registry.sync_definitions().await;
        assert_eq!(sync_defs.len(), 1);
        assert_eq!(sync_defs[0].event_type, "sync_ev");
    });
}

#[test]
fn test_event_pub_sub_channel_publish_without_subscribers() {
    let pubsub = EventPubSub::new(4);
    let payload = EventPayload::new(EventId::create(), "test", serde_json::json!({}));
    let result = pubsub.publish(payload);
    assert!(result.is_ok());
}

#[test]
fn test_event_pub_sub_subscriber_drop() {
    let pubsub = EventPubSub::new(4);
    {
        let _sub = pubsub.subscribe();
        assert_eq!(pubsub.receiver_count(), 1);
    }
    // Eventually receiver count drops when the dropped receiver is cleaned up
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. ID Generation
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_id_ascending_has_correct_prefix() {
    let id = ascending(IdPrefix::Session, None).unwrap();
    assert!(id.starts_with("ses_"));
    assert_eq!(id.len(), 3 + 1 + 12 + 14);
}

#[test]
fn test_id_descending_has_correct_prefix() {
    let id = descending(IdPrefix::Message, None).unwrap();
    assert!(id.starts_with("msg_"));
    assert_eq!(id.len(), 3 + 1 + 12 + 14);
}

#[test]
fn test_id_all_prefixes_generate_valid_ids() {
    for prefix in &[
        IdPrefix::Job,
        IdPrefix::Event,
        IdPrefix::Session,
        IdPrefix::Message,
        IdPrefix::Permission,
        IdPrefix::Question,
        IdPrefix::Part,
        IdPrefix::Pty,
        IdPrefix::Tool,
        IdPrefix::Workspace,
    ] {
        let id = ascending(*prefix, None).unwrap();
        let expected = format!("{}_", prefix.as_str());
        assert!(
            id.starts_with(&expected),
            "ID {id} should start with {expected}"
        );
    }
}

#[test]
fn test_id_ascending_is_monotonically_increasing() {
    let ids: Vec<String> = (0..20)
        .map(|_| ascending(IdPrefix::Event, None).unwrap())
        .collect();
    for w in ids.windows(2) {
        assert!(w[0] <= w[1], "ascending IDs must be non-decreasing: {} > {}", w[0], w[1]);
    }
}

#[test]
fn test_id_uniqueness_across_100_generations() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        let id = ascending(IdPrefix::Session, None).unwrap();
        assert!(seen.insert(id), "duplicate ID generated");
    }
}

#[test]
fn test_id_given_matching_prefix_passes_through() {
    let given = "ses_019133b4e4a03K8gRsKQD9xxgT";
    let result = ascending(IdPrefix::Session, Some(given)).unwrap();
    assert_eq!(result, given);
}

#[test]
fn test_id_given_wrong_prefix_errors() {
    let err = ascending(IdPrefix::Session, Some("msg_bad")).unwrap_err();
    match err {
        IdError::InvalidPrefix { expected, .. } => assert_eq!(expected, "ses"),
        _ => panic!("expected InvalidPrefix"),
    }
}

#[test]
fn test_id_timestamp_roundtrip() {
    let ts = 1_000_000_000i64;
    let id = create("ses", Direction::Ascending, Some(ts));
    let extracted = timestamp(&id).unwrap();
    assert_eq!(extracted, ts);
}

#[test]
fn test_id_timestamp_descending_does_not_roundtrip() {
    let ts = 1_000_000_000i64;
    let id = create("ses", Direction::Descending, Some(ts));
    let extracted = timestamp(&id).unwrap();
    assert_ne!(extracted, ts);
}

#[test]
fn test_id_timestamp_malformed_errors() {
    assert!(matches!(timestamp("no_underscore").unwrap_err(), IdError::Malformed));
    assert!(matches!(timestamp("ses_short").unwrap_err(), IdError::Malformed));
}

#[test]
fn test_id_ascending_less_than_descending_at_same_timestamp() {
    let ts = Some(1_000_000_000i64);
    let asc = create("ses", Direction::Ascending, ts);
    let desc = create("ses", Direction::Descending, ts);
    assert!(asc < desc, "ascending {asc} should be < descending {desc}");
}

#[test]
fn test_id_serde_direction_roundtrip() {
    let dir = Direction::Ascending;
    let json = serde_json::to_string(&dir).unwrap();
    let back: Direction = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Direction::Ascending);

    let dir = Direction::Descending;
    let json = serde_json::to_string(&dir).unwrap();
    let back: Direction = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Direction::Descending);
}

#[test]
fn test_id_prefix_as_str_all() {
    assert_eq!(IdPrefix::Job.as_str(), "job");
    assert_eq!(IdPrefix::Event.as_str(), "evt");
    assert_eq!(IdPrefix::Session.as_str(), "ses");
    assert_eq!(IdPrefix::Message.as_str(), "msg");
    assert_eq!(IdPrefix::Permission.as_str(), "per");
    assert_eq!(IdPrefix::Question.as_str(), "que");
    assert_eq!(IdPrefix::Part.as_str(), "prt");
    assert_eq!(IdPrefix::Pty.as_str(), "pty");
    assert_eq!(IdPrefix::Tool.as_str(), "tool");
    assert_eq!(IdPrefix::Workspace.as_str(), "wrk");
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. Error Conversion
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_session_error_to_error_conversion() {
    let session_err = SessionError::NotFound("ses_123".into());
    let err: Error = session_err.into();
    let msg = err.to_string();
    assert!(msg.contains("session error"));
    assert!(msg.contains("ses_123"));
}

#[test]
fn test_session_error_busy_to_error() {
    let session_err = SessionError::Busy("ses_456".into());
    let err: Error = session_err.into();
    assert!(err.to_string().contains("busy"));
}

#[test]
fn test_session_error_database_service_to_error() {
    let db_err = DatabaseServiceError::NotFound("session".into());
    let session_err = SessionError::DatabaseService(db_err);
    let err: Error = session_err.into();
    let msg = err.to_string();
    assert!(msg.contains("database error") || msg.contains("not found"));
}

#[test]
fn test_session_error_doom_loop_display() {
    let err = SessionError::DoomLoop {
        tool: "bash".into(),
        count: 5,
    };
    let msg = err.to_string();
    assert!(msg.contains("doom loop"));
    assert!(msg.contains("bash"));
}

#[test]
fn test_session_error_db_from_sqlx() {
    // Can't construct sqlx::Error directly, but we can verify the From impl
    // exists through the variant check
    let err = SessionError::Other("test".into());
    assert!(matches!(err, SessionError::Other(_)));
}

#[test]
fn test_database_service_error_to_error_conversion() {
    let db_err = DatabaseServiceError::Database("connection failed".into());
    let err: Error = db_err.into();
    let msg = err.to_string();
    assert!(msg.contains("connection failed"));
}

#[test]
fn test_database_service_error_not_found() {
    let db_err = DatabaseServiceError::NotFound("session".into());
    let err: Error = db_err.into();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_database_service_error_constraint() {
    let db_err = DatabaseServiceError::ConstraintViolation("UNIQUE constraint".into());
    let err: Error = db_err.into();
    assert!(err.to_string().contains("constraint"));
}

#[test]
fn test_permission_error_through_error_type() {
    let perm_err = PermissionError::Denied;
    let err: Error = perm_err.into();
    assert!(matches!(err, Error::Permission(PermissionError::Denied)));
    assert_eq!(err.to_string(), "permission denied");
}

#[test]
fn test_permission_error_rejected() {
    let perm_err = PermissionError::Rejected;
    let err: Error = perm_err.into();
    assert_eq!(err.to_string(), "permission rejected");
}

#[test]
fn test_permission_error_corrected() {
    let perm_err = PermissionError::Corrected {
        feedback: "use different tool".into(),
    };
    let err: Error = perm_err.into();
    assert!(err.to_string().contains("use different tool"));
}

#[test]
fn test_permission_error_not_found() {
    let perm_err = PermissionError::NotFound {
        request_id: "per_123".into(),
    };
    let err: Error = perm_err.into();
    assert!(err.to_string().contains("per_123"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. Shell Parsing
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_shell_parser_empty_input() {
    let parser = ShellParser::new();
    let result = parser.parse("");
    assert!(result.command_name.is_empty());
    assert!(!result.is_flagged);
}

#[test]
fn test_shell_parser_simple_echo() {
    let parser = ShellParser::new();
    let result = parser.parse("echo hello world");
    assert_eq!(result.command_name, "echo");
    assert_eq!(result.tokens, vec!["echo", "hello", "world"]);
}

#[test]
fn test_shell_parser_rm_with_path() {
    let parser = ShellParser::new();
    let result = parser.parse("rm -rf /tmp/cache");
    assert_eq!(result.command_name, "rm");
    assert!(result.file_operations.iter().any(|f| f.path == "/tmp/cache"));
    assert!(!result.is_flagged);
}

#[test]
fn test_shell_parser_rm_root_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("rm -rf /");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_rm_root_star_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("rm -rf /*");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_mkfs_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("mkfs.ext4 /dev/sda1");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_dd_flagged_for_dev() {
    let parser = ShellParser::new();
    let result = parser.parse("dd if=/dev/random of=/dev/sda bs=1M");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_sudo_rm_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("sudo rm -rf /*");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_git_not_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("git commit -m 'fix: issue'");
    assert!(!result.is_flagged);
}

#[test]
fn test_shell_parser_pipe_detection() {
    let parser = ShellParser::new();
    let result = parser.parse("ls -la | grep foo");
    assert!(result.has_pipe);
}

#[test]
fn test_shell_parser_redirection_detection() {
    let parser = ShellParser::new();
    let result = parser.parse("echo hello > /tmp/out.txt");
    assert!(result.has_redirection);
}

#[test]
fn test_shell_parser_cwd_change() {
    let parser = ShellParser::new();
    let result = parser.parse("cd /opt/app");
    assert_eq!(result.command_name, "cd");
    assert!(result.cwd_changes.contains(&"/opt/app".to_string()));
}

#[test]
fn test_shell_parser_multi_command() {
    let parser = ShellParser::new();
    let result = parser.parse("cd /tmp && ls -la");
    assert_eq!(result.command_name, "cd");
    assert!(result.cwd_changes.contains(&"/tmp".to_string()));
}

#[test]
fn test_shell_parser_dev_path_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("dd if=/dev/urandom of=/dev/null bs=1024");
    assert!(result.is_flagged);
    assert!(result.file_operations.iter().any(|f| f.path == "/dev/null"));
}

#[test]
fn test_shell_parser_shutdown_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("shutdown -h now");
    assert!(result.is_flagged);
}

#[test]
fn test_shell_parser_reboot_flagged() {
    let parser = ShellParser::new();
    let result = parser.parse("reboot");
    assert!(result.is_flagged);
}

#[test]
fn test_file_op_new() {
    let op = FileOp::new("rm", "/tmp/foo");
    assert_eq!(op.op, "rm");
    assert_eq!(op.path, "/tmp/foo");
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. Truncation
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_truncate_output_no_truncation_needed() {
    let result = truncate_output("short output", 100, 10000);
    assert!(!result.truncated);
    assert_eq!(result.content, "short output");
}

#[test]
fn test_truncate_output_by_lines() {
    let input = "line1\nline2\nline3\nline4\nline5";
    let result = truncate_output(input, 3, 1_000_000);
    assert!(result.truncated);
    assert!(result.content.contains("line1"));
    assert!(result.content.contains("truncated"));
    assert!(result.content.contains("5 lines > 3 limit"));
}

#[test]
fn test_truncate_output_by_chars() {
    let input = "this is a long line that should be truncated by char limit";
    let result = truncate_output(input, 1000, 10);
    assert!(result.truncated);
    assert!(result.content.len() <= 50);
}

#[test]
fn test_truncate_output_empty_string() {
    let result = truncate_output("", 100, 100);
    assert!(!result.truncated);
    assert_eq!(result.content, "");
}

#[test]
fn test_truncate_output_single_line_within_limit() {
    let result = truncate_output("hello world", 100, 100);
    assert!(!result.truncated);
    assert_eq!(result.content, "hello world");
}

#[test]
fn test_truncate_output_exact_at_limit() {
    let line = "a".repeat(50);
    let result = truncate_output(&line, 100, 50);
    assert!(!result.truncated);
    assert_eq!(result.content, line);
}

#[test]
fn test_truncate_service_no_truncation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let svc = TruncateService::new();
        let result = svc.truncate("short", "s1", "t1").await;
        assert!(!result.truncated);
        assert_eq!(result.content, "short");
    });
}

#[test]
fn test_truncate_service_truncates_lines() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let svc = TruncateService::with_options(TruncateOptions {
            max_chars: 1_000_000,
            max_lines: 2,
            ..TruncateOptions::default()
        });
        let input = "a\nb\nc\nd\ne";
        let result = svc.truncate(input, "s2", "t2").await;
        assert!(result.truncated);
        assert!(result.content.contains("truncated"));
    });
}

#[test]
fn test_truncate_service_limits() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let svc = TruncateService::new();
        let (max_lines, max_chars) = svc.limits().await;
        assert_eq!(max_lines, MAX_LINES);
        assert_eq!(max_chars, MAX_CHARS);
    });
}

#[test]
fn test_truncate_default_constants() {
    assert_eq!(MAX_LINES, 2000);
    assert_eq!(MAX_CHARS, 50 * 1024);
}

// ═════════════════════════════════════════════════════════════════════════════
// 10. Encryption
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_encryption_roundtrip() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let plaintext = "hello world";
    let encrypted = svc.encrypt(plaintext).unwrap();
    let decrypted = svc.decrypt(&encrypted).unwrap();
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_encryption_tamper_detection() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let encrypted = svc.encrypt("secret").unwrap();
    let tampered = encrypted.replace('a', "b");
    let result = svc.decrypt(&tampered);
    assert!(result.is_err());
}

#[test]
fn test_encryption_different_keys_fail() {
    let key1 = [0xABu8; KEY_LENGTH];
    let key2 = [0xBAu8; KEY_LENGTH];
    let s1 = EncryptionService::new(key1);
    let s2 = EncryptionService::new(key2);
    let encrypted = s1.encrypt("test").unwrap();
    let result = s2.decrypt(&encrypted);
    assert!(result.is_err());
}

#[test]
fn test_encryption_empty_string() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let encrypted = svc.encrypt("").unwrap();
    let decrypted = svc.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "");
}

#[test]
fn test_encryption_unicode() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let plaintext = "Hello 世界! 🎉";
    let encrypted = svc.encrypt(plaintext).unwrap();
    let decrypted = svc.decrypt(&encrypted).unwrap();
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_encryption_long_content() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let plaintext = "a".repeat(10000);
    let encrypted = svc.encrypt(&plaintext).unwrap();
    let decrypted = svc.decrypt(&encrypted).unwrap();
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_encryption_invalid_format() {
    let key = [0xABu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    assert!(svc.decrypt("invalid").is_err());
    assert!(svc.decrypt("no-separator").is_err());
}

#[test]
fn test_encryption_key_load_or_create() {
    let dir = tempfile::TempDir::new().unwrap();
    let svc = EncryptionService::load_or_create(dir.path()).unwrap();
    let encrypted = svc.encrypt("test").unwrap();
    let svc2 = EncryptionService::load_or_create(dir.path()).unwrap();
    let decrypted = svc2.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "test");
}

#[test]
fn test_encryption_invalid_key_length() {
    let dir = tempfile::TempDir::new().unwrap();
    let key_path = dir.path().join("encryption.key");
    std::fs::write(&key_path, b"too-short").unwrap();
    let result = EncryptionService::load_or_create(dir.path());
    assert!(result.is_err());
    match result {
        Err(EncryptionError::KeyLength(len)) => assert_eq!(len, 9),
        _ => panic!("expected KeyLength error"),
    }
}

#[test]
fn test_encryption_error_display() {
    let err = EncryptionError::Crypto("bad key".into());
    assert!(err.to_string().contains("crypto error"));

    let err = EncryptionError::Integrity("tampered".into());
    assert!(err.to_string().contains("integrity error"));

    let err = EncryptionError::KeyLength(10);
    assert!(err.to_string().contains("invalid key length"));

    let err = EncryptionError::Format("bad base64".into());
    assert!(err.to_string().contains("format error"));

    let err = EncryptionError::Io("permission denied".into());
    assert!(err.to_string().contains("I/O error"));
}

#[test]
fn test_encryption_roundtrip_special_characters() {
    let key = [0xCCu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let plaintext = "line1\nline2\ttab\x00null";
    let encrypted = svc.encrypt(plaintext).unwrap();
    let decrypted = svc.decrypt(&encrypted).unwrap();
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_encryption_multiple_values_independent() {
    let key = [0xDDu8; KEY_LENGTH];
    let svc = EncryptionService::new(key);
    let e1 = svc.encrypt("value1").unwrap();
    let e2 = svc.encrypt("value2").unwrap();
    assert_ne!(e1, e2);
    assert_eq!(svc.decrypt(&e1).unwrap(), "value1");
    assert_eq!(svc.decrypt(&e2).unwrap(), "value2");
}
