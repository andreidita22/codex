use crate::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::context_maintenance_config::resolve_context_maintenance_request_context;
use codex_api::ResponseEvent;
use codex_context_maintenance_policy::build_thread_memory_update_message;
use codex_context_maintenance_policy::limit_thread_memory_source_items;
use codex_context_maintenance_policy::parse_thread_memory_payload;
use codex_context_maintenance_policy::split_previous_memory_and_source_items;
use codex_context_maintenance_policy::thread_memory_output_schema;
use codex_context_maintenance_policy::thread_memory_response_item;
use codex_protocol::error::Result;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::content_items_to_text;
use futures::StreamExt;

pub(crate) async fn generate_thread_memory_item(
    sess: &Session,
    turn_context: &TurnContext,
    input: Vec<ResponseItem>,
) -> Result<Option<ResponseItem>> {
    if input.is_empty() {
        return Ok(None);
    }

    let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);
    let codex_context_maintenance_policy::ThreadMemorySourceSelection {
        previous_memory,
        source_items,
    } = selection;
    if source_items.is_empty()
        && let Some(previous_memory) = previous_memory
    {
        return Ok(Some(thread_memory_response_item(previous_memory)?));
    }

    let trim_result = limit_thread_memory_source_items(source_items);
    let mut source_items = trim_result.source_items;

    source_items.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: build_thread_memory_update_message(
                previous_memory.as_ref(),
                source_items.len(),
                trim_result.trimmed_source_count,
            ),
        }],
        end_turn: None,
        phase: None,
    });
    let request_context = resolve_context_maintenance_request_context(sess, turn_context).await;

    let prompt = Prompt {
        input: source_items,
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: sess.get_base_instructions().await.text,
        },
        personality: turn_context.personality,
        output_schema: Some(thread_memory_output_schema()),
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
    let memory = parse_thread_memory_payload(result)?;
    Ok(Some(thread_memory_response_item(memory)?))
}

fn is_compaction_summary(item: &ResponseItem) -> bool {
    let ResponseItem::Message { role, content, .. } = item else {
        return false;
    };
    if role != "user" {
        return false;
    }

    content
        .iter()
        .find_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                (!text.is_empty()).then_some(text.as_str())
            }
            ContentItem::InputImage { .. } => None,
        })
        .is_some_and(|text| {
            text.trim_start()
                .starts_with(crate::compact::SUMMARY_PREFIX.trim_end())
        })
}
