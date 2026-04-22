use crate::candidates::BrokerInput;
use crate::candidates::CandidateSet;
use crate::packet::ConfidenceDecision;
use crate::packet::ResolvedSchemaRef;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClarifyDisposition {
    None,
    Recommended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionConfidence {
    pub selected_score: u16,
    pub runner_up_score: Option<u16>,
    pub separation: Option<i16>,
    pub threshold: u16,
    pub decision: ConfidenceDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerWitness {
    pub adjudicator: String,
    pub candidate_count: usize,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerResolution {
    pub selected_schema: Option<ResolvedSchemaRef>,
    pub ambiguity_candidates: Vec<ResolvedSchemaRef>,
    pub confidence: ResolutionConfidence,
    pub clarify: ClarifyDisposition,
    pub witness: BrokerWitness,
}

/// Selects a broker resolution from a pre-built candidate set.
///
/// Implementations should treat `CandidateSet` as the full admissible choice
/// space and must not invent schema or operator IDs that do not already appear
/// in the candidate set.
pub trait SchemaAdjudicator {
    fn adjudicate(&self, input: &BrokerInput, candidates: &CandidateSet) -> BrokerResolution;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeterministicAdjudicator {
    min_score: u16,
}

impl Default for DeterministicAdjudicator {
    fn default() -> Self {
        Self { min_score: 1 }
    }
}

impl SchemaAdjudicator for DeterministicAdjudicator {
    fn adjudicate(&self, _input: &BrokerInput, candidates: &CandidateSet) -> BrokerResolution {
        let mut ranked = candidates.candidates.clone();
        ranked.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.schema_id.cmp(&right.schema_id))
        });

        let Some(top) = ranked.first() else {
            return BrokerResolution {
                selected_schema: None,
                ambiguity_candidates: Vec::new(),
                confidence: ResolutionConfidence {
                    selected_score: 0,
                    runner_up_score: None,
                    separation: None,
                    threshold: self.min_score,
                    decision: ConfidenceDecision::Unresolved,
                },
                clarify: ClarifyDisposition::None,
                witness: BrokerWitness {
                    adjudicator: "deterministic_v0".to_string(),
                    candidate_count: 0,
                    matched_terms: Vec::new(),
                },
            };
        };

        let runner_up_score = ranked.get(1).map(|candidate| candidate.score);
        let separation = runner_up_score.map(|score| top.score as i16 - score as i16);
        let top_refs = ranked
            .iter()
            .take_while(|candidate| candidate.score == top.score)
            .map(|candidate| ResolvedSchemaRef {
                schema_id: candidate.schema_id.clone(),
                operator_id: candidate.operator_id.clone(),
                canonical_operator: candidate.canonical_operator,
            })
            .collect::<Vec<_>>();

        if top.score < self.min_score {
            return BrokerResolution {
                selected_schema: None,
                ambiguity_candidates: Vec::new(),
                confidence: ResolutionConfidence {
                    selected_score: top.score,
                    runner_up_score,
                    separation,
                    threshold: self.min_score,
                    decision: ConfidenceDecision::Unresolved,
                },
                clarify: ClarifyDisposition::Recommended,
                witness: BrokerWitness {
                    adjudicator: "deterministic_v0".to_string(),
                    candidate_count: ranked.len(),
                    matched_terms: top.matched_phrases.clone(),
                },
            };
        }

        if top_refs.len() > 1 {
            return BrokerResolution {
                selected_schema: None,
                ambiguity_candidates: top_refs,
                confidence: ResolutionConfidence {
                    selected_score: top.score,
                    runner_up_score,
                    separation,
                    threshold: self.min_score,
                    decision: ConfidenceDecision::Ambiguous,
                },
                clarify: ClarifyDisposition::Recommended,
                witness: BrokerWitness {
                    adjudicator: "deterministic_v0".to_string(),
                    candidate_count: ranked.len(),
                    matched_terms: top.matched_phrases.clone(),
                },
            };
        }

        BrokerResolution {
            selected_schema: top_refs.into_iter().next(),
            ambiguity_candidates: Vec::new(),
            confidence: ResolutionConfidence {
                selected_score: top.score,
                runner_up_score,
                separation,
                threshold: self.min_score,
                decision: ConfidenceDecision::Resolved,
            },
            clarify: ClarifyDisposition::None,
            witness: BrokerWitness {
                adjudicator: "deterministic_v0".to_string(),
                candidate_count: ranked.len(),
                matched_terms: top.matched_phrases.clone(),
            },
        }
    }
}

pub fn adjudicate_candidates(
    input: &BrokerInput,
    candidates: &CandidateSet,
    adjudicator: &dyn SchemaAdjudicator,
) -> BrokerResolution {
    adjudicator.adjudicate(input, candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::build_candidate_set;
    use crate::registry::builtin_registry;
    use pretty_assertions::assert_eq;

    #[test]
    fn ambiguous_ties_are_not_resolved_by_order_accident() {
        let registry = builtin_registry().expect("builtin registry");
        let input = BrokerInput {
            current_turn_text: Some("please do a review and audit of this change".to_string()),
            visible_tool_names: Vec::new(),
            session_source: None,
            active_track: None,
        };
        let candidates = build_candidate_set(&input, &registry);
        let resolution = DeterministicAdjudicator::default().adjudicate(&input, &candidates);

        assert_eq!(resolution.selected_schema, None);
        assert_eq!(
            resolution.confidence.decision,
            ConfidenceDecision::Ambiguous
        );
        assert_eq!(resolution.clarify, ClarifyDisposition::Recommended);
        assert_eq!(resolution.ambiguity_candidates.len(), 2);
    }

    #[test]
    fn deterministic_resolution_is_stable() {
        let registry = builtin_registry().expect("builtin registry");
        let input = BrokerInput {
            current_turn_text: Some("implement this change".to_string()),
            visible_tool_names: vec!["apply_patch".to_string()],
            session_source: None,
            active_track: None,
        };
        let candidates = build_candidate_set(&input, &registry);
        let adjudicator = DeterministicAdjudicator::default();

        let first = adjudicator.adjudicate(&input, &candidates);
        let second = adjudicator.adjudicate(&input, &candidates);
        assert_eq!(first, second);
        assert_eq!(first.confidence.decision, ConfidenceDecision::Resolved);
    }
}
