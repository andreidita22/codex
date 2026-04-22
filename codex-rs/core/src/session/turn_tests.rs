use super::tests::make_session_and_context;
use super::turn::build_prompt;
use super::turn::built_tools;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
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
