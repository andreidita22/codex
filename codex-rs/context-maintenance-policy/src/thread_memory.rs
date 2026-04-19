use std::io;

use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::content_items_to_text;
use codex_protocol::models::remove_corresponding_response_item;
use serde_json::Value;
use tracing::warn;

use crate::artifact_codecs::extract_tagged_payload;
use crate::artifact_codecs::tagged_artifact_kind_from_text;

const THREAD_MEMORY_SCHEMA: &str = "odeu_thread_memory_v1";
const THREAD_MEMORY_PROMPT: &str = include_str!("../templates/thread_memory/prompt.md");
const OUTPUT_SCHEMA: &str = include_str!("../templates/thread_memory/schema.json");
const THREAD_MEMORY_TAG: &str = "thread_memory";
const MAX_SOURCE_ITEMS: usize = 160;

pub struct ThreadMemorySourceSelection {
    pub previous_memory: Option<Value>,
    pub source_items: Vec<ResponseItem>,
}

pub struct ThreadMemoryTrimResult {
    pub source_items: Vec<ResponseItem>,
    pub trimmed_source_count: usize,
}

pub fn thread_memory_output_schema() -> Value {
    match serde_json::from_str(OUTPUT_SCHEMA) {
        Ok(value) => value,
        Err(err) => panic!("invalid thread memory schema artifact: {err}"),
    }
}

pub fn split_previous_memory_and_source_items<F>(
    input: &[ResponseItem],
    is_compaction_summary: F,
) -> ThreadMemorySourceSelection
where
    F: Fn(&ResponseItem) -> bool,
{
    for (index, item) in input.iter().enumerate().rev() {
        if let Some(payload) = extract_thread_memory_payload_from_item(item) {
            match serde_json::from_str::<Value>(&payload) {
                Ok(memory) => {
                    let source_items = input
                        .iter()
                        .skip(index + 1)
                        .filter(|item| {
                            should_include_delta_source_item(item, &is_compaction_summary)
                        })
                        .cloned()
                        .collect();
                    return ThreadMemorySourceSelection {
                        previous_memory: Some(memory),
                        source_items,
                    };
                }
                Err(err) => {
                    warn!("failed parsing previous thread memory payload; ignoring: {err}");
                }
            }
        }
    }

    ThreadMemorySourceSelection {
        previous_memory: None,
        source_items: input
            .iter()
            .filter(|item| should_include_delta_source_item(item, &is_compaction_summary))
            .cloned()
            .collect(),
    }
}

pub fn limit_thread_memory_source_items(source_items: Vec<ResponseItem>) -> ThreadMemoryTrimResult {
    let source_items_len = source_items.len();
    if source_items_len <= MAX_SOURCE_ITEMS {
        return ThreadMemoryTrimResult {
            source_items,
            trimmed_source_count: 0,
        };
    }

    let start = source_items_len - MAX_SOURCE_ITEMS;
    let mut source_items = source_items;
    let trimmed_items: Vec<_> = source_items.drain(..start).collect();
    let pre_cleanup_len = source_items.len();
    for item in &trimmed_items {
        remove_corresponding_response_item(&mut source_items, item);
    }
    let trimmed_source_count =
        start.saturating_add(pre_cleanup_len.saturating_sub(source_items.len()));

    ThreadMemoryTrimResult {
        source_items,
        trimmed_source_count,
    }
}

pub fn build_thread_memory_update_message(
    previous_memory: Option<&Value>,
    source_items_count: usize,
    trimmed_source_count: usize,
) -> String {
    let mut message = format!(
        "{THREAD_MEMORY_PROMPT}\n\n<thread_memory_update_context>\nsource_items_count={source_items_count}\ntrimmed_source_items={trimmed_source_count}\n</thread_memory_update_context>"
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

pub fn parse_thread_memory_payload(result: &str) -> CodexResult<Value> {
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
        None => return Ok(serde_json::json!({ "schema": THREAD_MEMORY_SCHEMA })),
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

    normalize_thread_memory_schema(&mut value);
    Ok(value)
}

pub fn thread_memory_response_item(mut memory: Value) -> CodexResult<ResponseItem> {
    let schema = normalize_thread_memory_schema(&mut memory);
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

fn should_include_delta_source_item<F>(item: &ResponseItem, is_compaction_summary: &F) -> bool
where
    F: Fn(&ResponseItem) -> bool,
{
    match item {
        ResponseItem::Compaction { .. } | ResponseItem::GhostSnapshot { .. } => false,
        ResponseItem::Message { role, content, .. } if role == "developer" => {
            let Some(text) = content_items_to_text(content) else {
                return true;
            };

            tagged_artifact_kind_from_text(&text).is_none()
        }
        ResponseItem::Message { role, .. } if role == "user" => !is_compaction_summary(item),
        _ => true,
    }
}

fn normalize_thread_memory_schema(memory: &mut Value) -> String {
    let schema = memory
        .get("schema")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or(THREAD_MEMORY_SCHEMA)
        .to_string();
    memory["schema"] = Value::String(schema.clone());
    schema
}

fn extract_thread_memory_payload_from_item(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if role != "developer" {
        return None;
    }

    let text = content_items_to_text(content)?;
    let payload = extract_tagged_payload(&text, THREAD_MEMORY_TAG)?;
    Some(payload.to_string())
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
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::FunctionCallOutputPayload;
    use codex_protocol::models::ResponseItem;
    use codex_protocol::models::content_items_to_text;
    use pretty_assertions::assert_eq;

    use super::THREAD_MEMORY_SCHEMA;
    use super::THREAD_MEMORY_TAG;
    use super::build_thread_memory_update_message;
    use super::extract_tagged_payload;
    use super::limit_thread_memory_source_items;
    use super::parse_thread_memory_payload;
    use super::split_previous_memory_and_source_items;
    use super::thread_memory_output_schema;
    use super::thread_memory_response_item;

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
                    "<{THREAD_MEMORY_TAG} schema=\"{THREAD_MEMORY_SCHEMA}\">\n{memory_json}\n</{THREAD_MEMORY_TAG}>"
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

    fn prune_manifest_item() -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "<prune_manifest schema=\"prune_manifest_v1\">\n{}\n</prune_manifest>"
                    .to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn function_call(call_id: &str) -> ResponseItem {
        ResponseItem::FunctionCall {
            id: None,
            name: "shell".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: call_id.to_string(),
        }
    }

    fn function_call_output(call_id: &str) -> ResponseItem {
        ResponseItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output: FunctionCallOutputPayload::from_text("done".to_string()),
        }
    }

    fn compaction_summary_item() -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text:
                    "Compact this conversation by preserving the most important context.\nsummary"
                        .to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn is_compaction_summary(item: &ResponseItem) -> bool {
        let ResponseItem::Message { role, content, .. } = item else {
            return false;
        };
        if role != "user" {
            return false;
        }

        let Some(text) = content_items_to_text(content) else {
            return false;
        };
        text.trim_start()
            .starts_with("Compact this conversation by preserving the most important context.")
    }

    #[test]
    fn output_schema_uses_artifact() {
        let schema = thread_memory_output_schema();
        let artifact: serde_json::Value = serde_json::from_str(super::OUTPUT_SCHEMA)
            .unwrap_or_else(|err| panic!("schema artifact should parse: {err}"));

        assert_eq!(schema, artifact);
        assert_eq!(
            schema["properties"]["schema"]["enum"][0].as_str(),
            Some(THREAD_MEMORY_SCHEMA)
        );
    }

    #[test]
    fn extract_tagged_payload_returns_body() {
        let tagged = format!(
            "<{THREAD_MEMORY_TAG} schema=\"{THREAD_MEMORY_SCHEMA}\">\n{{\"schema\":\"{THREAD_MEMORY_SCHEMA}\"}}\n</{THREAD_MEMORY_TAG}>"
        );

        let payload = extract_tagged_payload(&tagged, THREAD_MEMORY_TAG);

        assert_eq!(payload, Some("{\"schema\":\"odeu_thread_memory_v1\"}"));
    }

    #[test]
    fn split_previous_memory_uses_latest_marker() {
        let first = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
            "thread": { "compaction_epoch": 0 }
        })
        .to_string();
        let second = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
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

        let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);

        assert_eq!(
            selection.previous_memory,
            Some(serde_json::json!({
                "schema": THREAD_MEMORY_SCHEMA,
                "thread": { "compaction_epoch": 1 }
            }))
        );
        assert_eq!(selection.source_items, vec![user_message("delta two")]);
    }

    #[test]
    fn split_previous_memory_ignores_prior_compaction_artifacts_after_marker() {
        let prior_memory = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
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

        let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);

        assert_eq!(
            selection.previous_memory,
            Some(serde_json::json!({
                "schema": THREAD_MEMORY_SCHEMA,
                "thread": { "compaction_epoch": 2 }
            }))
        );
        assert_eq!(selection.source_items, Vec::<ResponseItem>::new());
    }

    #[test]
    fn split_previous_memory_keeps_real_delta_after_compaction_artifacts() {
        let prior_memory = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
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

        let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);

        assert_eq!(
            selection.previous_memory,
            Some(serde_json::json!({
                "schema": THREAD_MEMORY_SCHEMA,
                "thread": { "compaction_epoch": 3 }
            }))
        );
        assert_eq!(
            selection.source_items,
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
    fn split_previous_memory_excludes_prune_manifest_from_delta_source() {
        let prior_memory = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
            "thread": { "compaction_epoch": 4 }
        })
        .to_string();
        let input = vec![
            thread_memory_item(&prior_memory),
            prune_manifest_item(),
            user_message("new user request"),
        ];

        let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);

        assert_eq!(
            selection.previous_memory,
            Some(serde_json::json!({
                "schema": THREAD_MEMORY_SCHEMA,
                "thread": { "compaction_epoch": 4 }
            }))
        );
        assert_eq!(
            selection.source_items,
            vec![user_message("new user request")]
        );
    }

    #[test]
    fn split_previous_memory_without_prior_memory_filters_policy_artifacts_and_summary() {
        let input = vec![
            continuation_bridge_item(),
            prune_manifest_item(),
            compaction_summary_item(),
            user_message("real user request"),
        ];

        let selection = split_previous_memory_and_source_items(&input, is_compaction_summary);

        assert_eq!(selection.previous_memory, None);
        assert_eq!(
            selection.source_items,
            vec![user_message("real user request")]
        );
    }

    #[test]
    fn limit_source_items_removes_orphaned_function_call_outputs_after_tail_trim() {
        let mut source_items = vec![user_message("older"), function_call("call_1")];
        source_items.push(function_call_output("call_1"));
        source_items.extend((0..159).map(|index| user_message(&format!("tail {index}"))));

        let result = limit_thread_memory_source_items(source_items);

        assert_eq!(result.trimmed_source_count, 3);
        assert_eq!(result.source_items.len(), 159);
        assert_eq!(
            result
                .source_items
                .iter()
                .any(|item| matches!(item, ResponseItem::FunctionCallOutput { .. })),
            false
        );
    }

    #[test]
    fn build_update_message_includes_previous_memory_when_present() {
        let message = build_thread_memory_update_message(
            Some(&serde_json::json!({ "schema": THREAD_MEMORY_SCHEMA })),
            3,
            1,
        );

        assert!(message.contains("<thread_memory_update_context>"));
        assert!(message.contains("<previous_thread_memory_json>"));
    }

    #[test]
    fn parse_thread_memory_response_ignores_trailing_text() {
        let payload = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
            "thread": {},
            "thread_odeu": {},
            "routing": {},
            "subjects": [],
            "continuity": {},
            "recent_delta": {},
            "turn_projection": {}
        });
        let raw = format!("{payload}\ntrailing text");

        let parsed = parse_thread_memory_payload(&raw).expect("thread memory should parse");

        assert_eq!(parsed["schema"].as_str(), Some(THREAD_MEMORY_SCHEMA));
    }

    #[test]
    fn thread_memory_response_item_defaults_schema_when_missing() {
        let item = thread_memory_response_item(serde_json::json!({
            "thread": {}
        }))
        .expect("thread memory item");

        let ResponseItem::Message {
            role,
            content,
            end_turn,
            phase,
            ..
        } = item
        else {
            panic!("expected developer message item");
        };
        assert_eq!(role, "developer");
        assert_eq!(end_turn, None);
        assert_eq!(phase, None);
        assert_eq!(content.len(), 1);
        let ContentItem::InputText { text } = &content[0] else {
            panic!("expected input_text content");
        };
        let prefix = format!("<{THREAD_MEMORY_TAG} schema=\"{THREAD_MEMORY_SCHEMA}\">\n");
        let suffix = format!("\n</{THREAD_MEMORY_TAG}>");
        assert!(text.starts_with(&prefix));
        assert!(text.ends_with(&suffix));
        let payload = text
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(&suffix))
            .expect("wrapped thread memory payload");
        let parsed: serde_json::Value = serde_json::from_str(payload).expect("valid json payload");
        let expected_payload = serde_json::json!({
            "schema": THREAD_MEMORY_SCHEMA,
            "thread": {}
        });

        assert_eq!(parsed, expected_payload);
    }
}
