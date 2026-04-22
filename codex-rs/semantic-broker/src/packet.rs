use crate::adjudicator::BrokerResolution;
use crate::adjudicator::BrokerWitness;
use crate::adjudicator::ClarifyDisposition;
use crate::candidates::BrokerInput;
use crate::registry::CanonicalOperator;
use crate::registry::OperatorId;
use crate::registry::SchemaId;
use serde::Deserialize;
use serde::Serialize;

pub const ACTIVE_CONTEXT_PACKET_TAG: &str = "active_context_packet";
pub const ACTIVE_CONTEXT_PACKET_SCHEMA: &str = "codex.semantic_broker.active_packet.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceDecision {
    Resolved,
    Ambiguous,
    Unresolved,
    PacketTruncated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSchemaRef {
    pub schema_id: SchemaId,
    pub operator_id: OperatorId,
    pub canonical_operator: CanonicalOperator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedOperatorRef {
    pub operator_id: OperatorId,
    pub canonical_operator: CanonicalOperator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerResolutionSummary {
    pub decision: ConfidenceDecision,
    pub selected_schema_id: Option<SchemaId>,
    pub selected_operator_id: Option<OperatorId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LexicalEvolutionEvent {
    NovelOperatorProposed { term: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketBudget {
    pub max_tool_bindings: usize,
    pub max_total_packet_chars: usize,
}

impl Default for PacketBudget {
    fn default() -> Self {
        Self {
            max_tool_bindings: 8,
            max_total_packet_chars: 2_048,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveContextPacket {
    pub packet_schema: &'static str,
    pub packet_id: PacketId,
    pub resolution: BrokerResolutionSummary,
    pub active_schema: Option<ResolvedSchemaRef>,
    pub active_operator: Option<ResolvedOperatorRef>,
    pub active_track: Option<String>,
    pub tool_bindings: Vec<String>,
    pub clarify: ClarifyDisposition,
    pub lexical_events: Vec<LexicalEvolutionEvent>,
    pub witness: BrokerWitness,
}

pub fn build_active_context_packet(
    input: &BrokerInput,
    resolution: &BrokerResolution,
) -> ActiveContextPacket {
    let active_operator = resolution
        .selected_schema
        .as_ref()
        .map(|schema| ResolvedOperatorRef {
            operator_id: schema.operator_id.clone(),
            canonical_operator: schema.canonical_operator,
        });
    let selected_schema_id = resolution
        .selected_schema
        .as_ref()
        .map(|schema| schema.schema_id.clone());
    let selected_operator_id = resolution
        .selected_schema
        .as_ref()
        .map(|schema| schema.operator_id.clone());
    let packet_id = PacketId(format!(
        "v0:{}",
        selected_schema_id
            .as_ref()
            .map(|schema_id| schema_id.0.as_str())
            .unwrap_or("unresolved")
    ));

    ActiveContextPacket {
        packet_schema: ACTIVE_CONTEXT_PACKET_SCHEMA,
        packet_id,
        resolution: BrokerResolutionSummary {
            decision: resolution.confidence.decision,
            selected_schema_id,
            selected_operator_id,
        },
        active_schema: resolution.selected_schema.clone(),
        active_operator,
        active_track: input.active_track.clone(),
        tool_bindings: input.visible_tool_names.clone(),
        clarify: resolution.clarify,
        lexical_events: Vec::new(),
        witness: resolution.witness.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adjudicator::BrokerResolution;
    use crate::adjudicator::BrokerWitness;
    use crate::adjudicator::ClarifyDisposition;
    use crate::adjudicator::ResolutionConfidence;
    use pretty_assertions::assert_eq;

    #[test]
    fn active_packet_contract_is_derived_from_resolution() {
        let input = BrokerInput {
            current_turn_text: Some("review this".to_string()),
            visible_tool_names: vec!["shell".to_string(), "apply_patch".to_string()],
            session_source: None,
            active_track: Some("default".to_string()),
        };
        let resolution = BrokerResolution {
            selected_schema: Some(ResolvedSchemaRef {
                schema_id: SchemaId("workflow.review.code".to_string()),
                operator_id: OperatorId("review".to_string()),
                canonical_operator: CanonicalOperator::Review,
            }),
            ambiguity_candidates: Vec::new(),
            confidence: ResolutionConfidence {
                selected_score: 2,
                runner_up_score: Some(1),
                separation: Some(1),
                threshold: 1,
                decision: ConfidenceDecision::Resolved,
            },
            clarify: ClarifyDisposition::None,
            witness: BrokerWitness {
                adjudicator: "deterministic_v0".to_string(),
                candidate_count: 1,
                matched_terms: vec!["review this".to_string()],
            },
        };

        let packet = build_active_context_packet(&input, &resolution);

        assert_eq!(
            packet,
            ActiveContextPacket {
                packet_schema: ACTIVE_CONTEXT_PACKET_SCHEMA,
                packet_id: PacketId("v0:workflow.review.code".to_string()),
                resolution: BrokerResolutionSummary {
                    decision: ConfidenceDecision::Resolved,
                    selected_schema_id: Some(SchemaId("workflow.review.code".to_string())),
                    selected_operator_id: Some(OperatorId("review".to_string())),
                },
                active_schema: Some(ResolvedSchemaRef {
                    schema_id: SchemaId("workflow.review.code".to_string()),
                    operator_id: OperatorId("review".to_string()),
                    canonical_operator: CanonicalOperator::Review,
                }),
                active_operator: Some(ResolvedOperatorRef {
                    operator_id: OperatorId("review".to_string()),
                    canonical_operator: CanonicalOperator::Review,
                }),
                active_track: Some("default".to_string()),
                tool_bindings: vec!["shell".to_string(), "apply_patch".to_string()],
                clarify: ClarifyDisposition::None,
                lexical_events: Vec::new(),
                witness: BrokerWitness {
                    adjudicator: "deterministic_v0".to_string(),
                    candidate_count: 1,
                    matched_terms: vec!["review this".to_string()],
                },
            }
        );
    }
}
