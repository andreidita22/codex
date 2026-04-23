use super::tests::make_session_and_context;
use super::turn::build_prompt;
use super::turn::built_tools;
use crate::context::ContextualUserFragment;
use crate::context::UserShellCommand;
use crate::semantic_broker_runtime::append_semantic_broker_prompt_overlay;
use codex_features::Feature;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_semantic_broker::ConfidenceDecision;
use codex_semantic_broker::is_active_context_packet_text;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

fn developer_overlay(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn contextual_user_shell_message(command: &str) -> ResponseItem {
    user_message(&format!(
        "{}{command}{}",
        <UserShellCommand as ContextualUserFragment>::START_MARKER,
        <UserShellCommand as ContextualUserFragment>::END_MARKER
    ))
}

fn image_only_user_message() -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "data:image/png;base64,AAAA".to_string(),
            detail: None,
        }],
        end_turn: None,
        phase: None,
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RenderedSemanticBrokerResolution {
    decision: ConfidenceDecision,
    selected_schema_id: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RenderedSemanticBrokerWitness {
    matched_terms: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RenderedSemanticBrokerPacket {
    resolution: RenderedSemanticBrokerResolution,
    witness: RenderedSemanticBrokerWitness,
}

fn parse_broker_packet(item: &ResponseItem) -> RenderedSemanticBrokerPacket {
    let ResponseItem::Message { content, .. } = item else {
        panic!("expected developer message");
    };
    let [ContentItem::InputText { text }] = content.as_slice() else {
        panic!("expected single input text item");
    };
    let prefix = "<active_context_packet schema=\"codex.semantic_broker.active_packet.v0\">";
    let suffix = "</active_context_packet>";
    let payload = text
        .strip_prefix(prefix)
        .and_then(|payload| payload.strip_suffix(suffix))
        .expect("expected tagged packet");

    serde_json::from_str(payload).expect("packet payload should deserialize")
}

#[tokio::test]
async fn build_prompt_allows_prompt_only_overlay_without_persisting_history() {
    let (session, turn_context) = make_session_and_context().await;
    let cancellation_token = CancellationToken::new();
    let history_before = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let router = built_tools(
        &session,
        &turn_context,
        &history_before,
        &HashSet::new(),
        Some(turn_context.turn_skills.outcome.as_ref()),
        &cancellation_token,
    )
    .await
    .expect("build tool router");
    let overlay =
        developer_overlay("<active_context_packet schema=\"v0\">test</active_context_packet>");

    let mut prompt_input = history_before.clone();
    prompt_input.push(overlay.clone());

    let prompt = build_prompt(
        prompt_input,
        router.as_ref(),
        &turn_context,
        session.get_base_instructions().await,
    );

    let history_after = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);

    assert_eq!(prompt.input.last(), Some(&overlay));
    assert_eq!(history_after, history_before);
    assert!(!history_after.contains(&overlay));
}

#[tokio::test]
async fn semantic_broker_feature_off_leaves_prompt_input_unchanged() {
    let (session, turn_context) = make_session_and_context().await;
    let cancellation_token = CancellationToken::new();
    let history_before = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let router = built_tools(
        &session,
        &turn_context,
        &history_before,
        &HashSet::new(),
        Some(turn_context.turn_skills.outcome.as_ref()),
        &cancellation_token,
    )
    .await
    .expect("build tool router");

    let prompt_input = append_semantic_broker_prompt_overlay(
        history_before.clone(),
        router.as_ref(),
        &turn_context,
    );

    assert_eq!(prompt_input, history_before);
}

#[tokio::test]
async fn semantic_broker_feature_on_appends_one_prompt_only_overlay() {
    let (session, mut turn_context) = make_session_and_context().await;
    turn_context
        .features
        .enable(Feature::SemanticBroker)
        .expect("enable semantic broker");
    let cancellation_token = CancellationToken::new();
    let history_before = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let router = built_tools(
        &session,
        &turn_context,
        &history_before,
        &HashSet::new(),
        Some(turn_context.turn_skills.outcome.as_ref()),
        &cancellation_token,
    )
    .await
    .expect("build tool router");

    let prompt_input = append_semantic_broker_prompt_overlay(
        history_before.clone(),
        router.as_ref(),
        &turn_context,
    );
    let prompt = build_prompt(
        prompt_input.clone(),
        router.as_ref(),
        &turn_context,
        session.get_base_instructions().await,
    );
    let history_after = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let overlay = prompt.input.last().expect("semantic broker overlay");
    let ResponseItem::Message { role, content, .. } = overlay else {
        panic!("expected developer message");
    };
    let [ContentItem::InputText { text }] = content.as_slice() else {
        panic!("expected single input text item");
    };

    assert_eq!(role, "developer");
    assert!(is_active_context_packet_text(text));
    assert_eq!(
        prompt_input
            .iter()
            .filter(|item| matches!(item, ResponseItem::Message { role, content, .. }
                if role == "developer"
                    && matches!(content.as_slice(), [ContentItem::InputText { text }] if is_active_context_packet_text(text))))
            .count(),
        1
    );
    assert_eq!(history_after, history_before);
    assert!(!history_after.contains(overlay));
}

#[tokio::test]
async fn semantic_broker_ignores_older_text_when_latest_user_message_is_contextual() {
    let (session, mut turn_context) = make_session_and_context().await;
    turn_context
        .features
        .enable(Feature::SemanticBroker)
        .expect("enable semantic broker");
    let cancellation_token = CancellationToken::new();
    let history_before = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let router = built_tools(
        &session,
        &turn_context,
        &history_before,
        &HashSet::new(),
        Some(turn_context.turn_skills.outcome.as_ref()),
        &cancellation_token,
    )
    .await
    .expect("build tool router");

    let mut prompt_input = history_before;
    prompt_input.push(user_message("review this change"));
    prompt_input.push(contextual_user_shell_message("git status"));

    let prompt_input =
        append_semantic_broker_prompt_overlay(prompt_input, router.as_ref(), &turn_context);
    let packet = parse_broker_packet(prompt_input.last().expect("semantic broker overlay"));

    assert_eq!(
        packet,
        RenderedSemanticBrokerPacket {
            resolution: RenderedSemanticBrokerResolution {
                decision: ConfidenceDecision::Unresolved,
                selected_schema_id: None,
            },
            witness: RenderedSemanticBrokerWitness {
                matched_terms: Vec::new(),
            },
        }
    );
}

#[tokio::test]
async fn semantic_broker_does_not_fall_back_past_latest_image_only_user_message() {
    let (session, mut turn_context) = make_session_and_context().await;
    turn_context
        .features
        .enable(Feature::SemanticBroker)
        .expect("enable semantic broker");
    let cancellation_token = CancellationToken::new();
    let history_before = session
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    let router = built_tools(
        &session,
        &turn_context,
        &history_before,
        &HashSet::new(),
        Some(turn_context.turn_skills.outcome.as_ref()),
        &cancellation_token,
    )
    .await
    .expect("build tool router");

    let mut prompt_input = history_before;
    prompt_input.push(user_message("review this change"));
    prompt_input.push(image_only_user_message());

    let prompt_input =
        append_semantic_broker_prompt_overlay(prompt_input, router.as_ref(), &turn_context);
    let packet = parse_broker_packet(prompt_input.last().expect("semantic broker overlay"));

    assert_eq!(
        packet,
        RenderedSemanticBrokerPacket {
            resolution: RenderedSemanticBrokerResolution {
                decision: ConfidenceDecision::Unresolved,
                selected_schema_id: None,
            },
            witness: RenderedSemanticBrokerWitness {
                matched_terms: Vec::new(),
            },
        }
    );
}
