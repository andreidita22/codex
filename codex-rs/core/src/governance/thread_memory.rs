use crate::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::Result;
use codex_api::ResponseEvent;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use serde_json::Value;
use std::io;
use tracing::warn;

pub(crate) const SCHEMA: &str = "odeu_thread_memory_v1";
pub(crate) const PROMPT: &str = include_str!("../../templates/thread_memory/prompt.md");
const OUTPUT_SCHEMA: &str = include_str!("../../templates/thread_memory/schema.json");

const THREAD_MEMORY_TAG: &str = "thread_memory";
const MAX_SOURCE_ITEMS: usize = 160;

pub(crate) fn output_schema() -> Value {
    match serde_json::from_str(OUTPUT_SCHEMA) {
        Ok(value) => value,
        Err(err) => panic!("invalid thread memory schema artifact: {err}"),
    }
}

pub(crate) async fn generate_thread_memory_item(
    sess: &Session,
    turn_context: &TurnContext,
    input: Vec<ResponseItem>,
) -> Result<Option<ResponseItem>> {
    if input.is_empty() {
        return Ok(None);
    }

    let (previous_memory, source_items) = split_previous_memory_and_source_items(&input);
    if source_items.is_empty()
        && let Some(previous_memory) = previous_memory
    {
        return Ok(Some(thread_memory_response_item(previous_memory)?));
    }

    let (mut source_items, trimmed_source_count) = limit_source_items(source_items);
    if source_items.is_empty() && previous_memory.is_none() {
        return Ok(None);
    }

    source_items.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: build_update_message(
                previous_memory.as_ref(),
                source_items.len(),
                trimmed_source_count,
            ),
        }],
        end_turn: None,
        phase: None,
    });

    let prompt = Prompt {
        input: source_items,
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: sess.get_base_instructions().await.text,
        },
        personality: turn_context.personality,
        output_schema: Some(output_schema()),
    };
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut client_session = sess.services.model_client.new_session();
    let mut stream = client_session
        .stream(
            &prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
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
                    && let Some(text) = crate::compact::content_items_to_text(&content)
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
    let memory = parse_thread_memory_response(result)?;
    Ok(Some(thread_memory_response_item(memory)?))
}

fn split_previous_memory_and_source_items(
    input: &[ResponseItem],
) -> (Option<Value>, Vec<ResponseItem>) {
    for (index, item) in input.iter().enumerate().rev() {
        if let Some(payload) = extract_thread_memory_payload(item) {
            match serde_json::from_str::<Value>(&payload) {
                Ok(memory) => {
                    let source_items = input
                        .iter()
                        .skip(index + 1)
                        .filter(|item| should_include_delta_source_item(item))
                        .cloned()
                        .collect();
                    return (Some(memory), source_items);
                }
                Err(err) => {
                    warn!("failed parsing previous thread memory payload; ignoring: {err}");
                }
            }
        }
    }

    (None, input.to_vec())
}

fn should_include_delta_source_item(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Compaction { .. } | ResponseItem::GhostSnapshot { .. } => false,
        ResponseItem::Message { role, content, .. } if role == "developer" => {
            let Some(text) = crate::compact::content_items_to_text(content) else {
                return true;
            };

            extract_tagged_payload(&text, THREAD_MEMORY_TAG).is_none()
                && extract_tagged_payload(&text, "continuation_bridge").is_none()
        }
        ResponseItem::Message { role, content, .. } if role == "user" => {
            let Some(text) = crate::compact::content_items_to_text(content) else {
                return true;
            };
            !text
                .trim_start()
                .starts_with(crate::compact::SUMMARY_PREFIX.trim_end())
        }
        _ => true,
    }
}

fn limit_source_items(source_items: Vec<ResponseItem>) -> (Vec<ResponseItem>, usize) {
    let source_items_len = source_items.len();
    if source_items_len <= MAX_SOURCE_ITEMS {
        return (source_items, 0);
    }

    let start = source_items_len - MAX_SOURCE_ITEMS;
    (source_items.into_iter().skip(start).collect(), start)
}

fn build_update_message(
    previous_memory: Option<&Value>,
    source_items_count: usize,
    trimmed_source_count: usize,
) -> String {
    let mut message = format!(
        "{PROMPT}\n\n<thread_memory_update_context>\nsource_items_count={source_items_count}\ntrimmed_source_items={trimmed_source_count}\n</thread_memory_update_context>"
    );
    if let Some(previous_memory) = previous_memory
        && let Ok(previous_memory_json) = serde_json::to_string_pretty(previous_memory)
    {
        message.push_str("\n\n<previous_thread_memory_json>\n");
        message.push_str(&previous_memory_json);
        message.push_str("\n</previous_thread_memory_json>");
    }

    message
}

fn parse_thread_memory_response(result: &str) -> Result<Value> {
    let mut stream = serde_json::Deserializer::from_str(result).into_iter::<Value>();
    let mut value = match stream.next() {
        Some(Ok(value)) => value,
        Some(Err(err)) => {
            let response_chars = result.chars().count();
            let preview = preview_text(result, /*max_chars*/ 1_024);
            warn!(
                "failed parsing thread memory response: {err}; response_chars={response_chars}; response_preview={preview:?}"
            );
            return Err(err.into());
        }
        None => {
            return Ok(serde_json::json!({ "schema": SCHEMA }));
        }
    };

    let trailing = result[stream.byte_offset()..].trim();
    if !trailing.is_empty() {
        let trailing_chars = trailing.chars().count();
        let trailing_preview = preview_text(trailing, /*max_chars*/ 512);
        warn!(
            "ignoring trailing content after thread memory JSON: trailing_chars={trailing_chars}; trailing_preview={trailing_preview:?}"
        );
    }

    if !value.is_object() {
        let err = serde_json::Error::io(io::Error::new(
            io::ErrorKind::InvalidData,
            "thread memory response must be a JSON object",
        ));
        return Err(err.into());
    }

    normalize_schema(&mut value);
    Ok(value)
}

fn normalize_schema(memory: &mut Value) -> String {
    let schema = memory
        .get("schema")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or(SCHEMA)
        .to_string();
    memory["schema"] = Value::String(schema.clone());
    schema
}

fn thread_memory_response_item(mut memory: Value) -> Result<ResponseItem> {
    let schema = normalize_schema(&mut memory);
    let memory_json = serde_json::to_string_pretty(&memory)?;
    Ok(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "<{THREAD_MEMORY_TAG} schema=\"{schema}\">\n{memory_json}\n</{THREAD_MEMORY_TAG}>"
            ),
        }],
        end_turn: None,
        phase: None,
    })
}

fn extract_thread_memory_payload(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if role != "developer" {
        return None;
    }

    let text = crate::compact::content_items_to_text(content)?;
    let payload = extract_tagged_payload(&text, THREAD_MEMORY_TAG)?;
    Some(payload.to_string())
}

fn extract_tagged_payload<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open_tag_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let open_index = text.find(&open_tag_prefix)?;
    let open_tag_end = text[open_index..].find('>')? + open_index + 1;
    let close_index = text[open_tag_end..].find(&close_tag)? + open_tag_end;
    let payload = text[open_tag_end..close_index].trim();
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::SCHEMA;
    use super::THREAD_MEMORY_TAG;
    use super::extract_tagged_payload;
    use super::output_schema;
    use super::parse_thread_memory_response;
    use super::should_include_delta_source_item;
    use super::split_previous_memory_and_source_items;
    use super::thread_memory_response_item;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

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

    fn thread_memory_item(memory_json: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<{THREAD_MEMORY_TAG} schema=\"{SCHEMA}\">\n{memory_json}\n</{THREAD_MEMORY_TAG}>"
                ),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn continuation_bridge_item() -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "<continuation_bridge schema=\"continuation_bridge_baton_v1\">\n{}\n</continuation_bridge>".to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn compaction_summary_item() -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: format!("{}\nsummary", crate::compact::SUMMARY_PREFIX.trim_end()),
            }],
            end_turn: None,
            phase: None,
        }
    }

    #[test]
    fn output_schema_uses_artifact() {
        let schema = output_schema();
        let artifact: serde_json::Value = serde_json::from_str(super::OUTPUT_SCHEMA)
            .unwrap_or_else(|err| panic!("schema artifact should parse: {err}"));

        assert_eq!(schema, artifact);
        assert_eq!(
            schema["properties"]["schema"]["enum"][0].as_str(),
            Some(SCHEMA)
        );
    }

    #[test]
    fn extract_tagged_payload_returns_body() {
        let tagged = format!(
            "<{THREAD_MEMORY_TAG} schema=\"{SCHEMA}\">\n{{\"schema\":\"{SCHEMA}\"}}\n</{THREAD_MEMORY_TAG}>"
        );

        let payload = extract_tagged_payload(&tagged, THREAD_MEMORY_TAG);

        assert_eq!(payload, Some("{\"schema\":\"odeu_thread_memory_v1\"}"));
    }

    #[test]
    fn split_previous_memory_uses_latest_marker() {
        let first = serde_json::json!({
            "schema": SCHEMA,
            "thread": { "compaction_epoch": 0 }
        })
        .to_string();
        let second = serde_json::json!({
            "schema": SCHEMA,
            "thread": { "compaction_epoch": 1 }
        })
        .to_string();
        let input = vec![
            user_message("old message"),
            thread_memory_item(&first),
            user_message("delta one"),
            thread_memory_item(&second),
            user_message("delta two"),
        ];

        let (previous, source_items) = split_previous_memory_and_source_items(&input);

        assert_eq!(
            previous,
            Some(serde_json::json!({
                "schema": SCHEMA,
                "thread": { "compaction_epoch": 1 }
            }))
        );
        assert_eq!(source_items, vec![user_message("delta two")]);
    }

    #[test]
    fn split_previous_memory_ignores_prior_compaction_artifacts_after_marker() {
        let prior_memory = serde_json::json!({
            "schema": SCHEMA,
            "thread": { "compaction_epoch": 2 }
        })
        .to_string();
        let input = vec![
            thread_memory_item(&prior_memory),
            continuation_bridge_item(),
            compaction_summary_item(),
            ResponseItem::Compaction {
                encrypted_content: "encrypted".to_string(),
            },
        ];

        let (previous, source_items) = split_previous_memory_and_source_items(&input);

        assert_eq!(
            previous,
            Some(serde_json::json!({
                "schema": SCHEMA,
                "thread": { "compaction_epoch": 2 }
            }))
        );
        assert_eq!(source_items, Vec::<ResponseItem>::new());
    }

    #[test]
    fn split_previous_memory_keeps_real_delta_after_compaction_artifacts() {
        let prior_memory = serde_json::json!({
            "schema": SCHEMA,
            "thread": { "compaction_epoch": 3 }
        })
        .to_string();
        let input = vec![
            thread_memory_item(&prior_memory),
            continuation_bridge_item(),
            compaction_summary_item(),
            ResponseItem::Compaction {
                encrypted_content: "encrypted".to_string(),
            },
            user_message("new user request"),
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "new assistant reply".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];

        let (previous, source_items) = split_previous_memory_and_source_items(&input);

        assert_eq!(
            previous,
            Some(serde_json::json!({
                "schema": SCHEMA,
                "thread": { "compaction_epoch": 3 }
            }))
        );
        assert_eq!(
            source_items,
            vec![
                user_message("new user request"),
                ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "new assistant reply".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ]
        );
    }

    #[test]
    fn delta_source_filter_drops_compaction_summary_and_bridge_artifacts() {
        assert!(!should_include_delta_source_item(
            &continuation_bridge_item()
        ));
        assert!(!should_include_delta_source_item(&compaction_summary_item()));
        assert!(!should_include_delta_source_item(
            &ResponseItem::Compaction {
                encrypted_content: "encrypted".to_string(),
            }
        ));
        assert!(should_include_delta_source_item(&user_message(
            "actual delta"
        )));
    }

    #[test]
    fn parse_thread_memory_response_ignores_trailing_text() {
        let payload = serde_json::json!({
            "schema": SCHEMA,
            "thread": {},
            "thread_odeu": {},
            "routing": {},
            "subjects": [],
            "continuity": {},
            "recent_delta": {},
            "turn_projection": {}
        });
        let raw = format!("{payload}\ntrailing text");

        let parsed = parse_thread_memory_response(&raw).expect("thread memory should parse");

        assert_eq!(parsed["schema"].as_str(), Some(SCHEMA));
    }

    #[test]
    fn thread_memory_response_item_defaults_schema_when_missing() {
        let item = thread_memory_response_item(serde_json::json!({
            "thread": {}
        }))
        .expect("thread memory item");
        let expected = ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<{THREAD_MEMORY_TAG} schema=\"{SCHEMA}\">\n{}\n</{THREAD_MEMORY_TAG}>",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema": SCHEMA,
                        "thread": {}
                    }))
                    .expect("json")
                ),
            }],
            end_turn: None,
            phase: None,
        };

        assert_eq!(item, expected);
    }
}
