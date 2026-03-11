use crate::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::Result;
use codex_api::ResponseEvent;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

pub(crate) const CONTINUATION_BRIDGE_PROMPT: &str =
    include_str!("../templates/continuation_bridge/prompt.md");
pub(crate) const CONTINUATION_BRIDGE_OUTPUT_SCHEMA: &str =
    include_str!("../templates/continuation_bridge/schema.json");
pub(crate) const CONTINUATION_BRIDGE_SCHEMA: &str = "continuation_bridge_v2";
#[cfg(test)]
const CONTINUATION_BRIDGE_LEGACY_SCHEMA: &str = "continuation_bridge_v1";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContinuationBridge {
    #[serde(default = "default_continuation_bridge_schema")]
    schema: String,
    #[serde(default)]
    task: ContinuationBridgeTask,
    #[serde(default)]
    repo_identity: ContinuationBridgeRepoIdentity,
    #[serde(default)]
    state: ContinuationBridgeListSection,
    #[serde(default)]
    blocking_state: ContinuationBridgeBlockingState,
    #[serde(default)]
    artifacts: ContinuationBridgeArtifacts,
    #[serde(default)]
    active_subagents: Vec<ContinuationBridgeSubagent>,
    #[serde(default)]
    key_claims_with_evidence: Vec<ContinuationBridgeClaim>,
    #[serde(default)]
    invariants: ContinuationBridgeInvariants,
    #[serde(default)]
    epistemics: ContinuationBridgeEpistemics,
    #[serde(default)]
    provenance: ContinuationBridgeProvenance,
    #[serde(default)]
    working_thesis: ContinuationBridgeWorkingThesis,
    #[serde(default)]
    recommended_output_shape: Vec<String>,
    #[serde(default)]
    next: ContinuationBridgeNext,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeTask {
    #[serde(default)]
    objective: String,
    #[serde(default)]
    current_phase: String,
    #[serde(default)]
    success_condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeRepoIdentity {
    #[serde(default)]
    repo_root: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    head_commit: String,
    #[serde(default)]
    worktree_dirty: bool,
    #[serde(default)]
    dirty_files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeListSection {
    #[serde(default)]
    completed: Vec<String>,
    #[serde(default)]
    in_progress: Vec<String>,
    #[serde(default)]
    not_started: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeBlockingState {
    #[serde(default)]
    blocking: Vec<ContinuationBridgeBlocker>,
    #[serde(default)]
    non_blocking: Vec<String>,
    #[serde(default)]
    optional_followups: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeBlocker {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    unblocks: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeArtifacts {
    #[serde(default)]
    files_touched: Vec<String>,
    #[serde(default)]
    authoritative_files: Vec<String>,
    #[serde(default)]
    partial_implementations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeSubagent {
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    thread_id: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    task: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    blocking: bool,
    #[serde(default)]
    last_result_summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeClaim {
    #[serde(default)]
    claim: String,
    #[serde(default)]
    confidence: String,
    #[serde(default)]
    ready_for_output: bool,
    #[serde(default)]
    evidence: Vec<ContinuationBridgeEvidence>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeEvidence {
    #[serde(default)]
    path: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    why_it_supports_claim: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeInvariants {
    #[serde(default)]
    must_preserve: Vec<String>,
    #[serde(default)]
    must_not_do: Vec<String>,
    #[serde(default)]
    assumptions_in_force: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeEpistemics {
    #[serde(default)]
    known_uncertainties: Vec<String>,
    #[serde(default)]
    questions_already_resolved: Vec<String>,
    #[serde(default)]
    questions_still_open: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeProvenance {
    #[serde(default)]
    why_current_code_looks_like_this: Vec<String>,
    #[serde(default)]
    rejected_paths: Vec<String>,
    #[serde(default)]
    pending_decisions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeWorkingThesis {
    #[serde(default)]
    current_best_answer: String,
    #[serde(default)]
    main_caveats: Vec<String>,
    #[serde(default)]
    likely_conclusion: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContinuationBridgeNext {
    #[serde(default)]
    immediate_next_action: String,
    #[serde(default)]
    fallback_if_blocked: String,
    #[serde(default)]
    validation_step: String,
}

fn default_continuation_bridge_schema() -> String {
    CONTINUATION_BRIDGE_SCHEMA.to_string()
}

impl ContinuationBridge {
    fn normalize(mut self) -> Self {
        let schema = self.schema.trim();
        self.schema = if schema.is_empty() {
            CONTINUATION_BRIDGE_SCHEMA.to_string()
        } else {
            schema.to_string()
        };
        self
    }

    fn into_response_item(self) -> Result<ResponseItem> {
        let bridge = self.normalize();
        let bridge_json = serde_json::to_string_pretty(&bridge)?;
        let schema = bridge.schema;
        Ok(ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<continuation_bridge schema=\"{schema}\">\n{bridge_json}\n</continuation_bridge>"
                ),
            }],
            end_turn: None,
            phase: None,
        })
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

fn parse_continuation_bridge_response(result: &str) -> Result<ContinuationBridge> {
    let mut stream = serde_json::Deserializer::from_str(result).into_iter::<ContinuationBridge>();
    let bridge = match stream.next() {
        Some(Ok(bridge)) => bridge,
        Some(Err(err)) => {
            let response_chars = result.chars().count();
            let preview = preview_text(result, 1_024);
            warn!(
                "failed parsing continuation bridge response: {err}; response_chars={response_chars}; response_preview={preview:?}"
            );
            return Err(err.into());
        }
        None => return Ok(ContinuationBridge::default()),
    };

    let trailing = result[stream.byte_offset()..].trim();
    if !trailing.is_empty() {
        let trailing_chars = trailing.chars().count();
        let trailing_preview = preview_text(trailing, 512);
        warn!(
            "ignoring trailing content after continuation bridge JSON: trailing_chars={trailing_chars}; trailing_preview={trailing_preview:?}"
        );
    }

    Ok(bridge)
}

pub(crate) fn output_schema() -> Value {
    match serde_json::from_str(CONTINUATION_BRIDGE_OUTPUT_SCHEMA) {
        Ok(value) => value,
        Err(err) => panic!("invalid continuation bridge schema artifact: {err}"),
    }
}

pub(crate) async fn generate_continuation_bridge_item(
    sess: &Session,
    turn_context: &TurnContext,
    mut input: Vec<ResponseItem>,
) -> Result<Option<ResponseItem>> {
    if input.is_empty() {
        return Ok(None);
    }

    input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: turn_context.continuation_bridge_prompt().to_string(),
        }],
        end_turn: None,
        phase: None,
    });

    let prompt = Prompt {
        input,
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

    let bridge = parse_continuation_bridge_response(result)?;
    Ok(Some(bridge.into_response_item()?))
}

#[cfg(test)]
mod tests {
    use super::CONTINUATION_BRIDGE_LEGACY_SCHEMA;
    use super::CONTINUATION_BRIDGE_OUTPUT_SCHEMA;
    use super::CONTINUATION_BRIDGE_SCHEMA;
    use super::ContinuationBridge;
    use super::output_schema;
    use super::parse_continuation_bridge_response;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    #[test]
    fn continuation_bridge_defaults_schema_when_missing() {
        let bridge = ContinuationBridge::default()
            .into_response_item()
            .expect("bridge response item");
        let expected_bridge = ContinuationBridge::default().normalize();
        let expected = ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<continuation_bridge schema=\"{CONTINUATION_BRIDGE_SCHEMA}\">\n{}\n</continuation_bridge>",
                    serde_json::to_string_pretty(&expected_bridge).expect("bridge json"),
                ),
            }],
            end_turn: None,
            phase: None,
        };

        assert_eq!(bridge, expected);
    }

    #[test]
    fn output_schema_uses_standalone_schema_artifact() {
        let schema = output_schema();
        let artifact: serde_json::Value = serde_json::from_str(CONTINUATION_BRIDGE_OUTPUT_SCHEMA)
            .unwrap_or_else(|err| panic!("schema artifact should parse: {err}"));

        assert_eq!(schema, artifact);
        assert_eq!(
            schema["properties"]["schema"]["enum"][0].as_str(),
            Some(CONTINUATION_BRIDGE_SCHEMA)
        );
    }

    #[test]
    fn output_schema_requires_all_evidence_fields() {
        let schema = output_schema();
        let required = schema["properties"]["key_claims_with_evidence"]["items"]["properties"]
            ["evidence"]["items"]["required"]
            .as_array()
            .expect("evidence.required should be an array")
            .iter()
            .map(|value| value.as_str().expect("required values should be strings"))
            .collect::<Vec<_>>();

        assert_eq!(
            required,
            vec!["path", "line", "kind", "why_it_supports_claim"]
        );
    }

    #[test]
    fn continuation_bridge_accepts_legacy_v1_payloads() {
        let legacy_json = serde_json::json!({
            "schema": CONTINUATION_BRIDGE_LEGACY_SCHEMA,
            "task": {
                "objective": "obj",
                "current_phase": "phase",
                "success_condition": "done"
            },
            "state": {
                "completed": ["a"],
                "in_progress": ["b"],
                "not_started": ["c"]
            },
            "artifacts": {
                "files_touched": ["/tmp/file"],
                "authoritative_files": [],
                "partial_implementations": []
            },
            "invariants": {
                "must_preserve": [],
                "must_not_do": [],
                "assumptions_in_force": []
            },
            "epistemics": {
                "known_uncertainties": [],
                "questions_already_resolved": [],
                "questions_still_open": []
            },
            "provenance": {
                "why_current_code_looks_like_this": [],
                "rejected_paths": [],
                "pending_decisions": []
            },
            "next": {
                "immediate_next_action": "next",
                "fallback_if_blocked": "fallback",
                "validation_step": "validate"
            }
        });

        let bridge: ContinuationBridge =
            serde_json::from_value(legacy_json).expect("legacy bridge should parse");

        assert_eq!(bridge.normalize().schema, CONTINUATION_BRIDGE_LEGACY_SCHEMA);
    }

    #[test]
    fn continuation_bridge_ignores_trailing_text_after_valid_json() {
        let expected_bridge = ContinuationBridge::default().normalize();
        let raw = format!(
            "{}\n\nTrailing commentary that should not break parsing.",
            serde_json::to_string(&expected_bridge).expect("bridge json"),
        );

        let parsed = parse_continuation_bridge_response(&raw).expect("bridge should parse");

        assert_eq!(parsed.normalize(), expected_bridge);
    }
}
