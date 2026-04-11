mod baton;
mod rich_review;
mod subagent_context;

use crate::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::ContinuationBridgeVariant;
use baton::BatonBridge;
use codex_api::ResponseEvent;
use codex_models_manager::manager::RefreshStrategy;
use codex_otel::SessionTelemetry;
use codex_protocol::error::Result;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use futures::StreamExt;
use rich_review::RichReviewBridge;
use serde_json::Value;
use tracing::warn;

const DEFAULT_CONTINUATION_BRIDGE_MODEL: &str = "gpt-5-codex-mini";
const DEFAULT_CONTINUATION_BRIDGE_REASONING_EFFORT: ReasoningEffortConfig =
    ReasoningEffortConfig::High;

pub(crate) fn default_prompt(variant: ContinuationBridgeVariant) -> &'static str {
    match variant {
        ContinuationBridgeVariant::Baton => baton::PROMPT,
        ContinuationBridgeVariant::RichReview => rich_review::PROMPT,
    }
}

fn schema_name(variant: ContinuationBridgeVariant) -> &'static str {
    match variant {
        ContinuationBridgeVariant::Baton => baton::SCHEMA,
        ContinuationBridgeVariant::RichReview => rich_review::SCHEMA,
    }
}

fn output_schema_artifact(variant: ContinuationBridgeVariant) -> &'static str {
    match variant {
        ContinuationBridgeVariant::Baton => baton::OUTPUT_SCHEMA,
        ContinuationBridgeVariant::RichReview => rich_review::OUTPUT_SCHEMA,
    }
}

fn variant_from_schema(schema: &str) -> Option<ContinuationBridgeVariant> {
    match schema {
        baton::SCHEMA => Some(ContinuationBridgeVariant::Baton),
        rich_review::SCHEMA | rich_review::LEGACY_SCHEMA => {
            Some(ContinuationBridgeVariant::RichReview)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContinuationBridgePayload {
    Baton(Box<BatonBridge>),
    RichReview(Box<RichReviewBridge>),
}

impl ContinuationBridgePayload {
    fn default_for_variant(variant: ContinuationBridgeVariant) -> Self {
        match variant {
            ContinuationBridgeVariant::Baton => Self::Baton(Box::default()),
            ContinuationBridgeVariant::RichReview => Self::RichReview(Box::default()),
        }
    }

    fn normalize(self) -> Self {
        match self {
            Self::Baton(mut bridge) => {
                bridge.schema = normalize_schema(&bridge.schema, baton::SCHEMA);
                Self::Baton(bridge)
            }
            Self::RichReview(mut bridge) => {
                bridge.schema = normalize_schema(&bridge.schema, rich_review::SCHEMA);
                Self::RichReview(bridge)
            }
        }
    }

    fn schema(&self) -> &str {
        match self {
            Self::Baton(bridge) => bridge.schema.as_str(),
            Self::RichReview(bridge) => bridge.schema.as_str(),
        }
    }

    fn pretty_json(&self) -> Result<String> {
        match self {
            Self::Baton(bridge) => Ok(serde_json::to_string_pretty(bridge)?),
            Self::RichReview(bridge) => Ok(serde_json::to_string_pretty(bridge)?),
        }
    }

    fn into_response_item(self) -> Result<ResponseItem> {
        let bridge = self.normalize();
        let bridge_json = bridge.pretty_json()?;
        let schema = bridge.schema();
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

struct ContinuationBridgeRequestContext {
    variant: ContinuationBridgeVariant,
    model_info: ModelInfo,
    session_telemetry: SessionTelemetry,
    reasoning_effort: Option<ReasoningEffortConfig>,
}

fn normalize_schema(schema: &str, default_schema: &str) -> String {
    let schema = schema.trim();
    if schema.is_empty() {
        default_schema.to_string()
    } else {
        schema.to_string()
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

fn parse_continuation_bridge_response(
    result: &str,
    fallback_variant: ContinuationBridgeVariant,
) -> Result<ContinuationBridgePayload> {
    let mut stream = serde_json::Deserializer::from_str(result).into_iter::<Value>();
    let value = match stream.next() {
        Some(Ok(value)) => value,
        Some(Err(err)) => {
            let response_chars = result.chars().count();
            let preview = preview_text(result, /*max_chars*/ 1_024);
            warn!(
                "failed parsing continuation bridge response: {err}; response_chars={response_chars}; response_preview={preview:?}"
            );
            return Err(err.into());
        }
        None => {
            return Ok(ContinuationBridgePayload::default_for_variant(
                fallback_variant,
            ));
        }
    };

    let trailing = result[stream.byte_offset()..].trim();
    if !trailing.is_empty() {
        let trailing_chars = trailing.chars().count();
        let trailing_preview = preview_text(trailing, /*max_chars*/ 512);
        warn!(
            "ignoring trailing content after continuation bridge JSON: trailing_chars={trailing_chars}; trailing_preview={trailing_preview:?}"
        );
    }

    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|schema| !schema.is_empty());
    let variant = match schema.and_then(variant_from_schema) {
        Some(variant) => variant,
        None => {
            if let Some(schema) = schema {
                warn!(
                    "unknown continuation bridge schema `{schema}`; falling back to {}",
                    schema_name(fallback_variant)
                );
            }
            fallback_variant
        }
    };

    match variant {
        ContinuationBridgeVariant::Baton => Ok(ContinuationBridgePayload::Baton(Box::new(
            serde_json::from_value(value)?,
        ))),
        ContinuationBridgeVariant::RichReview => Ok(ContinuationBridgePayload::RichReview(
            Box::new(serde_json::from_value(value)?),
        )),
    }
}

pub(crate) fn output_schema(variant: ContinuationBridgeVariant) -> Value {
    match serde_json::from_str(output_schema_artifact(variant)) {
        Ok(value) => value,
        Err(err) => panic!("invalid continuation bridge schema artifact: {err}"),
    }
}

async fn resolve_request_context(
    sess: &Session,
    turn_context: &TurnContext,
) -> ContinuationBridgeRequestContext {
    let variant = turn_context.continuation_bridge_variant();
    let requested_model = turn_context
        .config
        .continuation_bridge_model
        .as_deref()
        .unwrap_or(DEFAULT_CONTINUATION_BRIDGE_MODEL);
    let requested_reasoning_effort = turn_context
        .config
        .continuation_bridge_reasoning_effort
        .unwrap_or(DEFAULT_CONTINUATION_BRIDGE_REASONING_EFFORT);

    let resident_context = || ContinuationBridgeRequestContext {
        variant,
        model_info: turn_context.model_info.clone(),
        session_telemetry: turn_context.session_telemetry.clone(),
        reasoning_effort: turn_context.reasoning_effort,
    };

    let available_models = sess
        .services
        .models_manager
        .list_models(RefreshStrategy::Offline)
        .await;
    if !available_models
        .iter()
        .any(|model| model.model == requested_model)
    {
        warn!(
            "continuation bridge model `{requested_model}` unavailable; falling back to resident model `{}`",
            turn_context.model_info.slug
        );
        return resident_context();
    }

    let bridge_turn = turn_context
        .with_model(requested_model.to_string(), &sess.services.models_manager)
        .await;
    if !bridge_turn
        .model_info
        .supported_reasoning_levels
        .iter()
        .any(|preset| preset.effort == requested_reasoning_effort)
    {
        warn!(
            "continuation bridge reasoning effort `{requested_reasoning_effort}` is unsupported for model `{requested_model}`; falling back to resident model `{}`",
            turn_context.model_info.slug
        );
        return resident_context();
    }

    ContinuationBridgeRequestContext {
        variant,
        model_info: bridge_turn.model_info,
        session_telemetry: bridge_turn.session_telemetry,
        reasoning_effort: Some(requested_reasoning_effort),
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

    let request_context = resolve_request_context(sess, turn_context).await;
    if let Some(subagent_context_item) = subagent_context::build_subagent_context_item(sess).await?
    {
        input.push(subagent_context_item);
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
        output_schema: Some(output_schema(request_context.variant)),
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

    let bridge = parse_continuation_bridge_response(result, request_context.variant)?;
    Ok(Some(bridge.into_response_item()?))
}

#[cfg(test)]
mod tests {
    use super::ContinuationBridgePayload;
    use super::baton;
    use super::default_prompt;
    use super::output_schema;
    use super::parse_continuation_bridge_response;
    use super::rich_review;
    use crate::config::ContinuationBridgeVariant;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    #[test]
    fn continuation_bridge_defaults_to_baton_prompt() {
        assert_eq!(
            default_prompt(ContinuationBridgeVariant::Baton),
            baton::PROMPT
        );
    }

    #[test]
    fn output_schema_uses_baton_artifact() {
        let schema = output_schema(ContinuationBridgeVariant::Baton);
        let artifact: serde_json::Value = serde_json::from_str(baton::OUTPUT_SCHEMA)
            .unwrap_or_else(|err| panic!("schema artifact should parse: {err}"));

        assert_eq!(schema, artifact);
        assert_eq!(
            schema["properties"]["schema"]["enum"][0].as_str(),
            Some(baton::SCHEMA)
        );
    }

    #[test]
    fn output_schema_uses_rich_review_artifact() {
        let schema = output_schema(ContinuationBridgeVariant::RichReview);
        let artifact: serde_json::Value = serde_json::from_str(rich_review::OUTPUT_SCHEMA)
            .unwrap_or_else(|err| panic!("schema artifact should parse: {err}"));

        assert_eq!(schema, artifact);
        assert_eq!(
            schema["properties"]["schema"]["enum"][0].as_str(),
            Some(rich_review::SCHEMA)
        );
    }

    #[test]
    fn rich_review_accepts_legacy_v1_payloads() {
        let legacy_json = serde_json::json!({
            "schema": rich_review::LEGACY_SCHEMA,
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

        let parsed = parse_continuation_bridge_response(
            &legacy_json.to_string(),
            ContinuationBridgeVariant::RichReview,
        )
        .expect("legacy bridge should parse");

        match parsed {
            ContinuationBridgePayload::RichReview(bridge) => {
                assert_eq!(bridge.schema, rich_review::LEGACY_SCHEMA);
            }
            ContinuationBridgePayload::Baton(_) => panic!("expected rich review bridge"),
        }
    }

    #[test]
    fn baton_response_item_defaults_schema_when_missing() {
        let payload =
            ContinuationBridgePayload::default_for_variant(ContinuationBridgeVariant::Baton)
                .into_response_item()
                .expect("bridge response item");
        let expected = ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<continuation_bridge schema=\"{}\">\n{}\n</continuation_bridge>",
                    baton::SCHEMA,
                    serde_json::to_string_pretty(&baton::BatonBridge::default())
                        .expect("bridge json"),
                ),
            }],
            end_turn: None,
            phase: None,
        };

        assert_eq!(payload, expected);
    }

    #[test]
    fn continuation_bridge_ignores_trailing_text_after_valid_json() {
        let raw = format!(
            "{}\n\nTrailing commentary that should not break parsing.",
            serde_json::to_string(&baton::BatonBridge::default()).expect("bridge json"),
        );

        let parsed = parse_continuation_bridge_response(&raw, ContinuationBridgeVariant::Baton)
            .expect("bridge should parse");

        match parsed {
            ContinuationBridgePayload::Baton(bridge) => {
                assert_eq!(bridge.schema, baton::SCHEMA);
            }
            ContinuationBridgePayload::RichReview(_) => panic!("expected baton bridge"),
        }
    }
}
