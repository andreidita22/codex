use crate::registry::AliasTarget;
use crate::registry::CanonicalOperator;
use crate::registry::OperatorId;
use crate::registry::RegistryVersion;
use crate::registry::SchemaId;
use crate::registry::SchemaRegistrySnapshot;
use codex_protocol::protocol::SessionSource;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerInput {
    pub current_turn_text: Option<String>,
    pub visible_tool_names: Vec<String>,
    pub session_source: Option<SessionSource>,
    pub active_track: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaCandidate {
    pub schema_id: SchemaId,
    pub operator_id: OperatorId,
    pub canonical_operator: CanonicalOperator,
    pub matched_phrases: Vec<String>,
    pub matched_aliases: Vec<String>,
    pub score: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcludedCandidateReason {
    NoCurrentTurnText,
    NoTriggerMatch,
    InvalidOperatorReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedSchemaCandidate {
    pub schema_id: SchemaId,
    pub reason: ExcludedCandidateReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateNarrowingWitness {
    pub current_turn_text_present: bool,
    pub alias_hits: Vec<String>,
    pub visible_tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSet {
    pub registry_version: RegistryVersion,
    pub candidates: Vec<SchemaCandidate>,
    pub excluded: Vec<ExcludedSchemaCandidate>,
    pub narrowing_witness: CandidateNarrowingWitness,
}

pub fn build_candidate_set(input: &BrokerInput, registry: &SchemaRegistrySnapshot) -> CandidateSet {
    let mut alias_hits = Vec::new();
    let mut candidates = Vec::new();
    let mut excluded = Vec::new();

    let Some(current_turn_text) = input
        .current_turn_text
        .as_ref()
        .map(|text| text.to_ascii_lowercase())
    else {
        for schema in &registry.schemas {
            excluded.push(ExcludedSchemaCandidate {
                schema_id: schema.schema_id.clone(),
                reason: ExcludedCandidateReason::NoCurrentTurnText,
            });
        }
        return CandidateSet {
            registry_version: registry.version.clone(),
            candidates,
            excluded,
            narrowing_witness: CandidateNarrowingWitness {
                current_turn_text_present: false,
                alias_hits,
                visible_tool_names: input.visible_tool_names.clone(),
            },
        };
    };

    for schema in &registry.schemas {
        let mut matched_phrases = schema
            .trigger_phrases
            .iter()
            .filter(|phrase| current_turn_text.contains(&phrase.to_ascii_lowercase()))
            .cloned()
            .collect::<Vec<_>>();
        matched_phrases.sort();
        matched_phrases.dedup();

        let mut matched_aliases = registry
            .aliases
            .iter()
            .filter(|alias| current_turn_text.contains(&alias.alias.to_ascii_lowercase()))
            .filter(|alias| match &alias.target {
                AliasTarget::Schema { schema_id } => schema_id == &schema.schema_id,
                AliasTarget::Operator { operator_id } => operator_id == &schema.operator_id,
            })
            .map(|alias| alias.alias.clone())
            .collect::<Vec<_>>();
        matched_aliases.sort();
        matched_aliases.dedup();

        alias_hits.extend(matched_aliases.iter().cloned());

        let score = (matched_phrases.len() + matched_aliases.len()) as u16;
        if score == 0 {
            excluded.push(ExcludedSchemaCandidate {
                schema_id: schema.schema_id.clone(),
                reason: ExcludedCandidateReason::NoTriggerMatch,
            });
            continue;
        }

        let Some(canonical_operator) = registry
            .operators
            .iter()
            .find(|operator| operator.operator_id == schema.operator_id)
            .map(|operator| operator.canonical_operator)
        else {
            excluded.push(ExcludedSchemaCandidate {
                schema_id: schema.schema_id.clone(),
                reason: ExcludedCandidateReason::InvalidOperatorReference,
            });
            continue;
        };

        candidates.push(SchemaCandidate {
            schema_id: schema.schema_id.clone(),
            operator_id: schema.operator_id.clone(),
            canonical_operator,
            matched_phrases,
            matched_aliases,
            score,
        });
    }

    candidates.sort_by(|left, right| left.schema_id.cmp(&right.schema_id));
    excluded.sort_by(|left, right| left.schema_id.cmp(&right.schema_id));
    alias_hits.sort();
    alias_hits.dedup();

    CandidateSet {
        registry_version: registry.version.clone(),
        candidates,
        excluded,
        narrowing_witness: CandidateNarrowingWitness {
            current_turn_text_present: true,
            alias_hits,
            visible_tool_names: input.visible_tool_names.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::builtin_registry;
    use pretty_assertions::assert_eq;

    #[test]
    fn candidate_order_is_stable_for_identical_input() {
        let registry = builtin_registry().expect("builtin registry");
        let input = BrokerInput {
            current_turn_text: Some("please review and audit this change".to_string()),
            visible_tool_names: vec!["shell".to_string()],
            session_source: None,
            active_track: None,
        };

        let first = build_candidate_set(&input, &registry);
        let second = build_candidate_set(&input, &registry);
        assert_eq!(first, second);
    }

    #[test]
    fn excludes_schema_with_invalid_operator_reference() {
        let registry = SchemaRegistrySnapshot {
            version: RegistryVersion("v0".to_string()),
            operators: Vec::new(),
            schemas: vec![crate::registry::RegisteredSchema {
                schema_id: SchemaId("workflow.invalid".to_string()),
                operator_id: OperatorId("missing_operator".to_string()),
                source_family: crate::registry::RegistrySourceFamily::Builtin,
                summary: "Invalid schema".to_string(),
                trigger_phrases: vec!["review this".to_string()],
            }],
            aliases: Vec::new(),
        };
        let input = BrokerInput {
            current_turn_text: Some("review this".to_string()),
            visible_tool_names: Vec::new(),
            session_source: None,
            active_track: None,
        };

        let candidates = build_candidate_set(&input, &registry);

        assert_eq!(candidates.candidates, Vec::new());
        assert_eq!(
            candidates.excluded,
            vec![ExcludedSchemaCandidate {
                schema_id: SchemaId("workflow.invalid".to_string()),
                reason: ExcludedCandidateReason::InvalidOperatorReference,
            }]
        );
    }
}
