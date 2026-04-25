#![allow(clippy::expect_used)]

use std::fs;
use std::sync::Arc;

use anyhow::Result;
use codex_config::types::Personality;
use codex_core::config::GovernancePathVariant;
use codex_features::Feature;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::context_snapshot::ContextSnapshotRenderMode;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use sha1::Digest;

const PRETURN_CONTEXT_DIFF_CWD: &str = "PRETURN_CONTEXT_DIFF_CWD";

fn context_snapshot_options() -> ContextSnapshotOptions {
    ContextSnapshotOptions::default()
        .render_mode(ContextSnapshotRenderMode::KindWithTextPrefix { max_chars: 96 })
}

fn format_labeled_requests_snapshot(
    scenario: &str,
    sections: &[(&str, &ResponsesRequest)],
) -> String {
    context_snapshot::format_labeled_requests_snapshot(
        scenario,
        sections,
        &context_snapshot_options(),
    )
}

fn user_instructions_wrapper_count(request: &ResponsesRequest) -> usize {
    request
        .message_input_texts("user")
        .iter()
        .filter(|text| text.starts_with("# AGENTS.md instructions for "))
        .count()
}

fn governance_prompt_layer_sections(request: &ResponsesRequest) -> Vec<String> {
    request
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with("<governance_prompt_layers"))
        .collect()
}

fn only_governance_prompt_layer_payload(request: &ResponsesRequest) -> Value {
    let payloads = governance_prompt_layer_payloads(request);
    assert_eq!(
        payloads.len(),
        1,
        "expected exactly one developer-side governance prompt-layer section"
    );
    payloads.into_iter().next().expect("one payload")
}

fn governance_prompt_layer_payloads(request: &ResponsesRequest) -> Vec<Value> {
    governance_prompt_layer_sections(request)
        .iter()
        .map(|section| governance_prompt_layer_payload(section))
        .collect()
}

fn governance_prompt_layer_payload_for_phase(request: &ResponsesRequest, phase: &str) -> Value {
    let payloads = governance_prompt_layer_payloads(request)
        .into_iter()
        .filter(|payload| payload["phase"] == json!(phase))
        .collect::<Vec<_>>();
    assert_eq!(
        payloads.len(),
        1,
        "expected exactly one developer-side governance prompt-layer section for phase {phase}"
    );
    payloads.into_iter().next().expect("one payload")
}

fn governance_prompt_layer_payload(section: &str) -> Value {
    let json_start = section
        .find('{')
        .expect("governance prompt-layer section should contain a JSON payload");
    let mut stream = serde_json::Deserializer::from_str(&section[json_start..]).into_iter();
    let payload = stream
        .next()
        .expect("governance prompt-layer section should contain a JSON payload")
        .expect("governance prompt-layer JSON should parse");
    let trailing = section[json_start + stream.byte_offset()..].trim_start();
    assert!(
        trailing.starts_with("</governance_prompt_layers>"),
        "governance prompt-layer section should have a closing tag"
    );
    payload
}

fn assert_text_absent_from_rendered_developer_sections(request: &ResponsesRequest, text: &str) {
    let offending_sections = request
        .message_input_texts("developer")
        .into_iter()
        .filter(|section| {
            !section
                .trim_start()
                .starts_with("<governance_prompt_layers")
        })
        .filter(|section| section.contains(text))
        .collect::<Vec<_>>();
    assert_eq!(
        offending_sections,
        Vec::<String>::new(),
        "text must not be promoted into rendered developer sections"
    );
}

fn prompt_layer_content_hash(content: &str) -> String {
    let mut sha1 = sha1::Sha1::new();
    sha1.update(content.as_bytes());
    let digest = sha1.finalize();
    format!("sha1:{digest:x}")
}

fn assert_complete_strict_prompt_layer_payload(payload: &Value, expected_phase: &str) {
    assert_eq!(payload["schema"], json!("strict_v1_prompt_layers@1"));
    assert_eq!(payload["path_variant"], json!("strict_v1_shadow"));
    assert_eq!(payload["phase"], json!(expected_phase));
    assert_eq!(
        payload["source_presence"],
        json!({
            "constitutional": true,
            "role": true,
            "task": true,
            "runtime": true,
        })
    );
    assert_eq!(payload["compile_error"], Value::Null);
    assert_eq!(
        payload["active_layers"]["constitutional"]["packet_id"],
        json!("constitution:session")
    );
    assert_eq!(
        payload["active_layers"]["role"]["packet_id"],
        json!("role:session")
    );
    assert_eq!(
        payload["active_layers"]["task"]["packet_id"],
        json!("task:turn")
    );
    assert_eq!(
        payload["active_layers"]["runtime"]["packet_id"],
        json!("runtime:turn")
    );
}

fn format_environment_context_subagents_snapshot(subagents: &[&str]) -> String {
    let subagents_block = if subagents.is_empty() {
        String::new()
    } else {
        let lines = subagents
            .iter()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n  <subagents>\n{lines}\n  </subagents>")
    };
    let items = vec![json!({
        "type": "message",
        "role": "user",
        "content": [{
            "type": "input_text",
            "text": format!(
                "<environment_context>\n  <cwd>/tmp/example</cwd>\n  <shell>bash</shell>{subagents_block}\n</environment_context>"
            ),
        }],
    })];
    context_snapshot::format_response_items_snapshot(items.as_slice(), &context_snapshot_options())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_governance_prompt_layers_are_visible_in_initial_request() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "turn complete"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_codex()
        .with_model("gpt-5.3-codex")
        .with_config(|config| {
            config.governance_path_variant = Some(GovernancePathVariant::StrictV1Shadow);
            config.developer_instructions = Some("role layer marker".to_string());
            config.user_instructions = Some("task layer marker".to_string());
        });
    let test = builder.build(&server).await?;

    let final_output_json_schema = json!("final schema marker");
    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "initial strict governance turn".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: Some(final_output_json_schema.clone()),
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let request = responses.single_request();
    let developer_texts = request.message_input_texts("developer");
    assert!(
        developer_texts
            .first()
            .is_some_and(|text| text.starts_with("<governance_prompt_layers")),
        "initial governance prompt-layer section should lead the developer message"
    );
    assert!(
        request
            .message_input_texts("user")
            .iter()
            .all(|text| !text.contains("<governance_prompt_layers")),
        "governance prompt-layer section must stay developer-side"
    );
    let payload = only_governance_prompt_layer_payload(&request);
    assert_complete_strict_prompt_layer_payload(&payload, "initial_context");
    let final_output_json_schema =
        serde_json::to_string(&final_output_json_schema).expect("schema should serialize");
    assert_eq!(
        payload["active_layers"]["task"]["content_hash"],
        json!(prompt_layer_content_hash(&format!(
            "final_output_json_schema: {final_output_json_schema}\n\ntask layer marker"
        )))
    );
    assert_text_absent_from_rendered_developer_sections(&request, "task layer marker");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_governance_omits_prompt_layers_from_initial_request() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "turn complete"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_codex()
        .with_model("gpt-5.3-codex")
        .with_config(|config| {
            config.governance_path_variant = Some(GovernancePathVariant::Off);
            config.developer_instructions = Some("role layer marker".to_string());
            config.user_instructions = Some("task layer marker".to_string());
        });
    let test = builder.build(&server).await?;

    test.submit_turn_with_policy(
        "initial governance-off turn",
        SandboxPolicy::new_read_only_policy(),
    )
    .await?;

    let request = responses.single_request();
    assert_eq!(
        governance_prompt_layer_sections(&request),
        Vec::<String>::new(),
        "governance-off request should not include prompt-layer metadata"
    );
    assert_text_absent_from_rendered_developer_sections(&request, "task layer marker");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_governance_prompt_layers_are_visible_in_settings_update_request() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "turn one complete"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "turn two complete"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_codex()
        .with_model("gpt-5.3-codex")
        .with_config(|config| {
            config.governance_path_variant = Some(GovernancePathVariant::StrictV1Shadow);
            config.developer_instructions = Some("role layer marker".to_string());
            config.user_instructions = Some("task layer marker".to_string());
        });
    let test = builder.build(&server).await?;

    test.submit_turn_with_policy(
        "first strict governance turn",
        SandboxPolicy::new_read_only_policy(),
    )
    .await?;

    let settings_update_cwd = test.cwd_path().join("settings-update-cwd");
    fs::create_dir_all(&settings_update_cwd)?;
    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "second strict governance turn".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: settings_update_cwd,
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two requests");

    let initial_payload = only_governance_prompt_layer_payload(&requests[0]);
    assert_complete_strict_prompt_layer_payload(&initial_payload, "initial_context");

    assert!(
        requests[1].has_message_with_input_texts("developer", |texts| {
            texts
                .first()
                .is_some_and(|text| text.starts_with("<governance_prompt_layers"))
                && texts
                    .iter()
                    .any(|text| text.contains("\"phase\": \"settings_update\""))
        }),
        "settings update governance prompt-layer section should lead the developer update"
    );
    assert!(
        requests[1]
            .message_input_texts("user")
            .iter()
            .all(|text| !text.contains("<governance_prompt_layers")),
        "settings update governance prompt-layer section must stay developer-side"
    );
    let settings_payload =
        governance_prompt_layer_payload_for_phase(&requests[1], "settings_update");
    assert_complete_strict_prompt_layer_payload(&settings_payload, "settings_update");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_model_visible_layout_turn_overrides() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "turn one complete"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "turn two complete"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_codex()
        .with_model("gpt-5.3-codex")
        .with_config(|config| {
            config
                .features
                .enable(Feature::Personality)
                .expect("test config should allow feature update");
            config.personality = Some(Personality::Pragmatic);
        });
    let test = builder.build(&server).await?;
    let preturn_context_diff_cwd = test.cwd_path().join(PRETURN_CONTEXT_DIFF_CWD);
    fs::create_dir_all(&preturn_context_diff_cwd)?;

    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "first turn".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "second turn with context updates".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: preturn_context_diff_cwd,
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: Some(Personality::Friendly),
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two requests");
    insta::assert_snapshot!(
        "model_visible_layout_turn_overrides",
        format_labeled_requests_snapshot(
            "Second turn changes cwd, approval policy, and personality while keeping model constant.",
            &[
                ("First Request (Baseline)", &requests[0]),
                ("Second Request (Turn Overrides)", &requests[1]),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Diff `user_instructions` and emit updates when AGENTS.md content changes
// (for example after cwd changes), then update this test to assert refreshed AGENTS content.
async fn snapshot_model_visible_layout_cwd_change_does_not_refresh_agents() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "turn one complete"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "turn two complete"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_codex().with_model("gpt-5.3-codex");
    let test = builder.build(&server).await?;
    let cwd_one = test.cwd_path().join("agents_one");
    let cwd_two = test.cwd_path().join("agents_two");
    fs::create_dir_all(&cwd_one)?;
    fs::create_dir_all(&cwd_two)?;
    fs::write(
        cwd_one.join("AGENTS.md"),
        "# AGENTS one\n\n<INSTRUCTIONS>\nTurn one agents instructions.\n</INSTRUCTIONS>\n",
    )?;
    fs::write(
        cwd_two.join("AGENTS.md"),
        "# AGENTS two\n\n<INSTRUCTIONS>\nTurn two agents instructions.\n</INSTRUCTIONS>\n",
    )?;

    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "first turn in agents_one".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd_one.clone(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    test.codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "second turn in agents_two".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd_two,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two requests");
    assert_eq!(
        user_instructions_wrapper_count(&requests[0]),
        0,
        "expected first request to omit the serialized user-instructions wrapper when cwd-only project docs are introduced after session init"
    );
    assert_eq!(
        user_instructions_wrapper_count(&requests[1]),
        0,
        "expected second request to keep omitting the serialized user-instructions wrapper after cwd change with the current session-scoped project doc behavior"
    );
    insta::assert_snapshot!(
        "model_visible_layout_cwd_change_does_not_refresh_agents",
        format_labeled_requests_snapshot(
            "Second turn changes cwd to a directory with different AGENTS.md; current behavior does not emit refreshed AGENTS instructions.",
            &[
                ("First Request (agents_one)", &requests[0]),
                ("Second Request (agents_two cwd)", &requests[1]),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_model_visible_layout_resume_with_personality_change() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut initial_builder = test_codex().with_config(|config| {
        config.model = Some("gpt-5.2".to_string());
    });
    let initial = initial_builder.build(&server).await?;
    let codex = Arc::clone(&initial.codex);
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    let initial_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-initial"),
            ev_assistant_message("msg-1", "recorded before resume"),
            ev_completed("resp-initial"),
        ]),
    )
    .await;
    codex
        .submit(Op::UserInput {
            environments: None,
            items: vec![UserInput::Text {
                text: "seed resume history".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
        })
        .await?;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    let initial_request = initial_mock.single_request();

    let resumed_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-resume"),
            ev_assistant_message("msg-2", "first resumed turn"),
            ev_completed("resp-resume"),
        ]),
    )
    .await;

    let mut resume_builder = test_codex().with_config(|config| {
        config.model = Some("gpt-5.3-codex".to_string());
        config
            .features
            .enable(Feature::Personality)
            .expect("test config should allow feature update");
        config.personality = Some(Personality::Pragmatic);
    });
    let resumed = resume_builder.resume(&server, home, rollout_path).await?;
    let resume_override_cwd = resumed.cwd_path().join(PRETURN_CONTEXT_DIFF_CWD);
    fs::create_dir_all(&resume_override_cwd)?;
    resumed
        .codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "resume and change personality".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: resume_override_cwd,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            permission_profile: None,
            model: resumed.session_configured.model.clone(),
            effort: resumed.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: Some(Personality::Friendly),
        })
        .await?;
    wait_for_event(&resumed.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let resumed_request = resumed_mock.single_request();
    insta::assert_snapshot!(
        "model_visible_layout_resume_with_personality_change",
        format_labeled_requests_snapshot(
            "First post-resume turn where resumed config model differs from rollout and personality changes.",
            &[
                ("Last Request Before Resume", &initial_request),
                ("First Request After Resume", &resumed_request),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_model_visible_layout_resume_override_matches_rollout_model() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut initial_builder = test_codex().with_config(|config| {
        config.model = Some("gpt-5.2".to_string());
    });
    let initial = initial_builder.build(&server).await?;
    let codex = Arc::clone(&initial.codex);
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    let initial_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-initial"),
            ev_assistant_message("msg-1", "recorded before resume"),
            ev_completed("resp-initial"),
        ]),
    )
    .await;
    codex
        .submit(Op::UserInput {
            environments: None,
            items: vec![UserInput::Text {
                text: "seed resume history".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
        })
        .await?;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    let initial_request = initial_mock.single_request();

    let resumed_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-resume"),
            ev_assistant_message("msg-2", "first resumed turn"),
            ev_completed("resp-resume"),
        ]),
    )
    .await;

    let mut resume_builder = test_codex().with_config(|config| {
        config.model = Some("gpt-5.3-codex".to_string());
    });
    let resumed = resume_builder.resume(&server, home, rollout_path).await?;
    let resume_override_cwd = resumed.cwd_path().join(PRETURN_CONTEXT_DIFF_CWD);
    fs::create_dir_all(&resume_override_cwd)?;
    resumed
        .codex
        .submit(Op::OverrideTurnContext {
            cwd: Some(resume_override_cwd),
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            permission_profile: None,
            windows_sandbox_level: None,
            model: Some("gpt-5.2".to_string()),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    resumed
        .codex
        .submit(Op::UserInput {
            environments: None,
            items: vec![UserInput::Text {
                text: "first resumed turn after model override".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
        })
        .await?;
    wait_for_event(&resumed.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let resumed_request = resumed_mock.single_request();
    insta::assert_snapshot!(
        "model_visible_layout_resume_override_matches_rollout_model",
        format_labeled_requests_snapshot(
            "First post-resume turn where pre-turn override sets model to rollout model; no model-switch update should appear.",
            &[
                ("Last Request Before Resume", &initial_request),
                ("First Request After Resume + Override", &resumed_request),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_model_visible_layout_environment_context_includes_one_subagent() -> Result<()> {
    insta::assert_snapshot!(
        "model_visible_layout_environment_context_includes_one_subagent",
        format_environment_context_subagents_snapshot(&["- agent-1: Atlas"])
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_model_visible_layout_environment_context_includes_two_subagents() -> Result<()> {
    insta::assert_snapshot!(
        "model_visible_layout_environment_context_includes_two_subagents",
        format_environment_context_subagents_snapshot(&["- agent-1: Atlas", "- agent-2: Juniper"])
    );

    Ok(())
}
