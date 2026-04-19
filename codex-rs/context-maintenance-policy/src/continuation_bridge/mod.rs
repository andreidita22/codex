mod baton;
mod rich_review;
mod supplemental;

use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde_json::Value;
use tracing::warn;

pub use baton::BatonBridge;
pub use rich_review::RichReviewBridge;
pub use supplemental::ContinuationBridgeSubagentSnapshot;
pub use supplemental::ContinuationBridgeSubagentStatus;
pub use supplemental::continuation_bridge_subagent_context_item;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeVariant {
    Baton,
    RichReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationBridgePayload {
    Baton(Box<BatonBridge>),
    RichReview(Box<RichReviewBridge>),
}

pub struct ContinuationBridgeModelInput {
    pub source_items: Vec<ResponseItem>,
    pub supplemental_items: Vec<ResponseItem>,
    pub user_prompt_text: String,
}

pub fn default_prompt(variant: BridgeVariant) -> &'static str {
    match variant {
        BridgeVariant::Baton => baton::PROMPT,
        BridgeVariant::RichReview => rich_review::PROMPT,
    }
}

pub fn build_continuation_bridge_prompt_input(
    input: ContinuationBridgeModelInput,
) -> Vec<ResponseItem> {
    let ContinuationBridgeModelInput {
        mut source_items,
        mut supplemental_items,
        user_prompt_text,
    } = input;
    source_items.append(&mut supplemental_items);
    source_items.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: user_prompt_text,
        }],
        end_turn: None,
        phase: None,
    });
    source_items
}

impl ContinuationBridgePayload {
    fn default_for_variant(variant: BridgeVariant) -> Self {
        match variant {
            BridgeVariant::Baton => Self::Baton(Box::default()),
            BridgeVariant::RichReview => Self::RichReview(Box::default()),
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

    fn pretty_json(&self) -> CodexResult<String> {
        match self {
            Self::Baton(bridge) => Ok(serde_json::to_string_pretty(bridge)?),
            Self::RichReview(bridge) => Ok(serde_json::to_string_pretty(bridge)?),
        }
    }
}

fn schema_name(variant: BridgeVariant) -> &'static str {
    match variant {
        BridgeVariant::Baton => baton::SCHEMA,
        BridgeVariant::RichReview => rich_review::SCHEMA,
    }
}

fn output_schema_artifact(variant: BridgeVariant) -> &'static str {
    match variant {
        BridgeVariant::Baton => baton::OUTPUT_SCHEMA,
        BridgeVariant::RichReview => rich_review::OUTPUT_SCHEMA,
    }
}

fn variant_from_schema(schema: &str) -> Option<BridgeVariant> {
    match schema {
        baton::SCHEMA => Some(BridgeVariant::Baton),
        rich_review::SCHEMA | rich_review::LEGACY_SCHEMA => Some(BridgeVariant::RichReview),
        _ => None,
    }
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

pub fn parse_continuation_bridge_payload(
    result: &str,
    fallback_variant: BridgeVariant,
) -> CodexResult<ContinuationBridgePayload> {
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
        BridgeVariant::Baton => Ok(ContinuationBridgePayload::Baton(Box::new(
            serde_json::from_value(value)?,
        ))),
        BridgeVariant::RichReview => Ok(ContinuationBridgePayload::RichReview(Box::new(
            serde_json::from_value(value)?,
        ))),
    }
}

pub fn continuation_bridge_response_item(
    payload: ContinuationBridgePayload,
) -> CodexResult<ResponseItem> {
    let payload = payload.normalize();
    let bridge_json = payload.pretty_json()?;
    let schema = payload.schema();
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

pub fn continuation_bridge_output_schema(variant: BridgeVariant) -> Value {
    match serde_json::from_str(output_schema_artifact(variant)) {
        Ok(value) => value,
        Err(err) => panic!("invalid continuation bridge schema artifact: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    use super::BatonBridge;
    use super::BridgeVariant;
    use super::ContinuationBridgeModelInput;
    use super::ContinuationBridgePayload;
    use super::baton;
    use super::build_continuation_bridge_prompt_input;
    use super::continuation_bridge_output_schema;
    use super::continuation_bridge_response_item;
    use super::default_prompt;
    use super::parse_continuation_bridge_payload;
    use super::rich_review;

    #[test]
    fn continuation_bridge_defaults_to_baton_prompt() {
        assert_eq!(default_prompt(BridgeVariant::Baton), baton::PROMPT);
    }

    #[test]
    fn build_continuation_bridge_prompt_input_appends_supplements_then_prompt() {
        let input = ContinuationBridgeModelInput {
            source_items: vec![message("source", "developer")],
            supplemental_items: vec![message("supplement", "developer")],
            user_prompt_text: "bridge prompt".to_string(),
        };

        assert_eq!(
            build_continuation_bridge_prompt_input(input),
            vec![
                message("source", "developer"),
                message("supplement", "developer"),
                message("bridge prompt", "user"),
            ],
        );
    }

    #[test]
    fn output_schema_uses_baton_artifact() {
        let schema = continuation_bridge_output_schema(BridgeVariant::Baton);
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
        let schema = continuation_bridge_output_schema(BridgeVariant::RichReview);
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

        let parsed =
            parse_continuation_bridge_payload(&legacy_json.to_string(), BridgeVariant::RichReview)
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
        let payload = continuation_bridge_response_item(
            ContinuationBridgePayload::default_for_variant(BridgeVariant::Baton),
        )
        .expect("bridge response item");
        let expected = ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<continuation_bridge schema=\"{}\">\n{}\n</continuation_bridge>",
                    baton::SCHEMA,
                    serde_json::to_string_pretty(&BatonBridge::default()).expect("bridge json"),
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
            serde_json::to_string(&BatonBridge::default()).expect("bridge json"),
        );

        let parsed = parse_continuation_bridge_payload(&raw, BridgeVariant::Baton)
            .expect("bridge should parse");

        match parsed {
            ContinuationBridgePayload::Baton(bridge) => {
                assert_eq!(bridge.schema, baton::SCHEMA);
            }
            ContinuationBridgePayload::RichReview(_) => panic!("expected baton bridge"),
        }
    }

    fn message(text: &str, role: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }
}
