use super::tests::make_session_and_context;
use super::turn::build_prompt;
use super::turn::built_tools;
use crate::semantic_broker_runtime::append_semantic_broker_prompt_overlay;
use codex_features::Feature;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_semantic_broker::is_active_context_packet_text;
use pretty_assertions::assert_eq;
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
