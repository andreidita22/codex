mod subagent_context;

use crate::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::ContinuationBridgeVariant;
use crate::context_maintenance_config::ContextMaintenanceRequestContext;
use crate::context_maintenance_config::resolve_context_maintenance_request_context;
use codex_api::ResponseEvent;
use codex_context_maintenance_policy::BridgeVariant;
use codex_context_maintenance_policy::ContinuationBridgeModelInput;
use codex_context_maintenance_policy::build_continuation_bridge_prompt_input;
use codex_context_maintenance_policy::content_items_to_text;
use codex_context_maintenance_policy::continuation_bridge_default_prompt;
use codex_context_maintenance_policy::continuation_bridge_output_schema;
use codex_context_maintenance_policy::continuation_bridge_response_item;
use codex_context_maintenance_policy::parse_continuation_bridge_payload;
use codex_otel::SessionTelemetry;
use codex_protocol::error::Result;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use futures::StreamExt;

pub(crate) fn default_prompt(variant: ContinuationBridgeVariant) -> &'static str {
    continuation_bridge_default_prompt(bridge_variant(variant))
}

struct ContinuationBridgeRequestContext {
    variant: BridgeVariant,
    model_info: ModelInfo,
    session_telemetry: SessionTelemetry,
    reasoning_effort: Option<ReasoningEffortConfig>,
}

fn bridge_variant(variant: ContinuationBridgeVariant) -> BridgeVariant {
    match variant {
        ContinuationBridgeVariant::Baton => BridgeVariant::Baton,
        ContinuationBridgeVariant::RichReview => BridgeVariant::RichReview,
    }
}

async fn resolve_request_context(
    sess: &Session,
    turn_context: &TurnContext,
) -> ContinuationBridgeRequestContext {
    let ContextMaintenanceRequestContext {
        model_info,
        session_telemetry,
        reasoning_effort,
    } = resolve_context_maintenance_request_context(sess, turn_context).await;
    ContinuationBridgeRequestContext {
        variant: bridge_variant(turn_context.continuation_bridge_variant()),
        model_info,
        session_telemetry,
        reasoning_effort,
    }
}

pub(crate) async fn generate_continuation_bridge_item(
    sess: &Session,
    turn_context: &TurnContext,
    input: Vec<ResponseItem>,
) -> Result<Option<ResponseItem>> {
    if input.is_empty() {
        return Ok(None);
    }

    let request_context = resolve_request_context(sess, turn_context).await;
    let supplemental_items = match subagent_context::build_subagent_context_item(sess).await? {
        Some(item) => vec![item],
        None => Vec::new(),
    };
    let prompt_input = build_continuation_bridge_prompt_input(ContinuationBridgeModelInput {
        source_items: input,
        supplemental_items,
        user_prompt_text: turn_context.continuation_bridge_prompt().to_string(),
    });

    let prompt = Prompt {
        input: prompt_input,
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: sess.get_base_instructions().await.text,
        },
        personality: turn_context.personality,
        output_schema: Some(continuation_bridge_output_schema(request_context.variant)),
    };
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut client_session = sess.services.model_client.new_session();
    let mut stream = client_session
        .stream(
            &prompt,
            &request_context.model_info,
            &request_context.session_telemetry,
            request_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header.as_deref(),
        )
        .await?;

    let mut result = String::new();
    while let Some(message) = stream.next().await.transpose()? {
        match message {
            ResponseEvent::OutputTextDelta(delta) => result.push_str(&delta),
            ResponseEvent::OutputItemDone(item) => {
                if result.is_empty()
                    && let ResponseItem::Message { content, .. } = item
                    && let Some(text) = content_items_to_text(&content)
                {
                    result.push_str(&text);
                }
            }
            ResponseEvent::Completed { .. } => break,
            ResponseEvent::Created
            | ResponseEvent::OutputItemAdded(_)
            | ResponseEvent::ServerModel(_)
            | ResponseEvent::ServerReasoningIncluded(_)
            | ResponseEvent::ReasoningSummaryDelta { .. }
            | ResponseEvent::ReasoningContentDelta { .. }
            | ResponseEvent::ReasoningSummaryPartAdded { .. }
            | ResponseEvent::RateLimits(_)
            | ResponseEvent::ModelsEtag(_) => {}
        }
    }

    let result = result.trim();
    if result.is_empty() {
        return Ok(None);
    }

    let bridge = parse_continuation_bridge_payload(result, request_context.variant)?;
    Ok(Some(continuation_bridge_response_item(bridge)?))
}
