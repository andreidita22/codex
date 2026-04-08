use crate::config::GovernancePathVariant;
use crate::governance::compiler::CompiledGovernancePackets;
use crate::governance::compiler::GovernanceCompilationInput;
use crate::governance::compiler::GovernanceCompileError;
use crate::governance::compiler::PacketSourceKind;
use crate::governance::compiler::ProjectionWitness;
use crate::governance::compiler::SourceFragment;
use crate::governance::compiler::compile_packets;
use crate::governance::packets::ConstitutionPacket;
use crate::governance::packets::GovernanceClause;
use crate::governance::packets::GovernancePacket;
use crate::governance::packets::NormativeForce;
use crate::governance::packets::PacketIdentity;
use crate::governance::packets::RolePacket;
use crate::governance::packets::RuntimeFact;
use crate::governance::packets::RuntimeFactStore;
use crate::governance::packets::TaskCharterPacket;
use codex_protocol::protocol::SessionSource;
use serde::Serialize;
use sha1::Digest;
use sha1::Sha1;

pub(crate) const GOVERNANCE_PROMPT_LAYERS_TAG: &str = "governance_prompt_layers";

const PROMPT_LAYERING_SCHEMA: &str = "strict_v1_prompt_layers@1";
const PACKET_SCHEMA: &str = "strict_v1_packet@1";
const PROMPT_LAYERING_CONTRACT: &str =
    include_str!("../../templates/governance/prompt_layering.md");
const CONSTITUTION_PACKET_ID: &str = "constitution:session";
const ROLE_PACKET_ID: &str = "role:session";
const TASK_PACKET_ID: &str = "task:turn";
const RUNTIME_PACKET_ID: &str = "runtime:turn";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PromptLayeringPhase {
    InitialContext,
    SettingsUpdate,
}

pub(crate) struct PromptLayeringInput<'a> {
    pub(crate) path_variant: GovernancePathVariant,
    pub(crate) phase: PromptLayeringPhase,
    pub(crate) model: &'a str,
    pub(crate) session_source: &'a SessionSource,
    pub(crate) base_instructions: &'a str,
    pub(crate) constitutional_sections: &'a [String],
    pub(crate) role_sections: &'a [String],
    pub(crate) task_sections: &'a [String],
    pub(crate) runtime_sections: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayeringPayload {
    schema: &'static str,
    path_variant: GovernancePathVariant,
    phase: PromptLayeringPhase,
    model: String,
    session_source: String,
    precedence: PromptLayeringPrecedence,
    source_presence: PromptLayerPresence,
    active_layers: PromptLayerStates,
    fallback_semantics: PromptLayerFallbackSemantics,
    projection_witness: ProjectionWitness,
    compile_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayeringPrecedence {
    read_order: [&'static str; 4],
    specialization_rule: &'static str,
    action_selection_rule: &'static str,
}

impl Default for PromptLayeringPrecedence {
    fn default() -> Self {
        Self {
            read_order: ["constitutional", "role", "task", "runtime"],
            specialization_rule: "lower layers narrow upper layers; they do not rewrite constitutional law",
            action_selection_rule: "execute determinate task obligations first, then apply role defaults, then use runtime facts only for feasibility checks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayerPresence {
    constitutional: bool,
    role: bool,
    task: bool,
    runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayerStates {
    constitutional: PromptLayerState,
    role: PromptLayerState,
    task: PromptLayerState,
    runtime: PromptLayerState,
}

impl PromptLayerStates {
    fn from_compiled(compiled: &CompiledGovernancePackets) -> Self {
        Self {
            constitutional: PromptLayerState::from_identity(
                compiled
                    .constitution
                    .as_ref()
                    .map(|packet| &packet.identity),
            ),
            role: PromptLayerState::from_identity(
                compiled.role.as_ref().map(|packet| &packet.identity),
            ),
            task: PromptLayerState::from_identity(
                compiled
                    .task_charter
                    .as_ref()
                    .map(|packet| &packet.identity),
            ),
            runtime: PromptLayerState::from_identity(
                compiled
                    .runtime_fact_store
                    .as_ref()
                    .map(|packet| &packet.identity),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayerState {
    present: bool,
    packet_id: Option<String>,
    content_hash: Option<String>,
}

impl PromptLayerState {
    fn from_identity(identity: Option<&PacketIdentity>) -> Self {
        Self {
            present: identity.is_some(),
            packet_id: identity.map(|identity| identity.packet_id.clone()),
            content_hash: identity.map(|identity| identity.content_hash.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptLayerFallbackSemantics {
    missing_role: &'static str,
    missing_task: &'static str,
    missing_runtime: &'static str,
    compiler_failure: &'static str,
}

pub(crate) fn build_prompt_layering_section(input: PromptLayeringInput<'_>) -> Option<String> {
    if input.path_variant == GovernancePathVariant::Off {
        return None;
    }

    let source_presence = PromptLayerPresence {
        constitutional: true,
        role: has_any_text(input.role_sections),
        task: has_any_text(input.task_sections),
        runtime: has_any_text(input.runtime_sections),
    };

    let compile_result = compile_prompt_layer_packets(&input);
    let (compiled, compile_error) = match compile_result {
        Ok(compiled) => (compiled, None),
        Err(err) => (CompiledGovernancePackets::default(), Some(err.to_string())),
    };

    let payload = PromptLayeringPayload {
        schema: PROMPT_LAYERING_SCHEMA,
        path_variant: input.path_variant,
        phase: input.phase,
        model: input.model.to_string(),
        session_source: format!("{:?}", input.session_source),
        precedence: PromptLayeringPrecedence::default(),
        source_presence,
        active_layers: PromptLayerStates::from_compiled(&compiled),
        fallback_semantics: fallback_semantics(input.path_variant),
        projection_witness: compiled.projection_witness,
        compile_error,
    };

    let payload_json = serde_json::to_string_pretty(&payload).ok()?;
    Some(format!(
        "<{GOVERNANCE_PROMPT_LAYERS_TAG} schema=\"{PROMPT_LAYERING_SCHEMA}\">\n{}\n\n{payload_json}\n</{GOVERNANCE_PROMPT_LAYERS_TAG}>",
        PROMPT_LAYERING_CONTRACT.trim()
    ))
}

fn compile_prompt_layer_packets(
    input: &PromptLayeringInput<'_>,
) -> Result<CompiledGovernancePackets, GovernanceCompileError> {
    let constitution_content =
        layer_content_with_base(input.base_instructions, input.constitutional_sections);
    let role_content = layer_content(input.role_sections);
    let task_content = layer_content(input.task_sections);
    let runtime_content = layer_content(input.runtime_sections);

    let mut fragments = vec![SourceFragment::Packet {
        source_id: "prompt-layering:constitutional".to_string(),
        source_kind: PacketSourceKind::PromptChannel,
        packet: GovernancePacket::Constitution(ConstitutionPacket {
            identity: packet_identity(CONSTITUTION_PACKET_ID, &constitution_content),
            clauses: vec![GovernanceClause {
                force: NormativeForce::Hard,
                subject: "constitutional".to_string(),
                instruction: summarize_for_clause(&constitution_content),
            }],
        }),
    }];

    if let Some(role_content) = role_content {
        fragments.push(SourceFragment::Packet {
            source_id: "prompt-layering:role".to_string(),
            source_kind: PacketSourceKind::PromptChannel,
            packet: GovernancePacket::Role(RolePacket {
                identity: packet_identity(ROLE_PACKET_ID, &role_content),
                role_name: "session_role".to_string(),
                posture: vec![GovernanceClause {
                    force: NormativeForce::Default,
                    subject: "role".to_string(),
                    instruction: summarize_for_clause(&role_content),
                }],
            }),
        });
    }

    if let Some(task_content) = task_content {
        fragments.push(SourceFragment::Packet {
            source_id: "prompt-layering:task".to_string(),
            source_kind: PacketSourceKind::PromptChannel,
            packet: GovernancePacket::TaskCharter(TaskCharterPacket {
                identity: packet_identity(TASK_PACKET_ID, &task_content),
                assignment_id: "current_turn".to_string(),
                ordered_obligations: vec![GovernanceClause {
                    force: NormativeForce::Hard,
                    subject: "task".to_string(),
                    instruction: summarize_for_clause(&task_content),
                }],
            }),
        });
    }

    if let Some(runtime_content) = runtime_content {
        fragments.push(SourceFragment::Packet {
            source_id: "prompt-layering:runtime".to_string(),
            source_kind: PacketSourceKind::RuntimeState,
            packet: GovernancePacket::RuntimeFactStore(RuntimeFactStore {
                identity: packet_identity(RUNTIME_PACKET_ID, &runtime_content),
                facts: vec![
                    RuntimeFact {
                        key: "runtime_summary".to_string(),
                        value: summarize_for_clause(&runtime_content),
                        source: "prompt_layering".to_string(),
                    },
                    RuntimeFact {
                        key: "session_source".to_string(),
                        value: format!("{:?}", input.session_source),
                        source: "turn_context".to_string(),
                    },
                    RuntimeFact {
                        key: "model".to_string(),
                        value: input.model.to_string(),
                        source: "turn_context".to_string(),
                    },
                ],
            }),
        });
    }

    compile_packets(GovernanceCompilationInput {
        path_variant: input.path_variant,
        fragments,
    })
}

fn layer_content_with_base(base: &str, sections: &[String]) -> String {
    let mut chunks = Vec::new();
    if !base.trim().is_empty() {
        chunks.push(base.trim().to_string());
    }
    if let Some(section_content) = layer_content(sections) {
        chunks.push(section_content);
    }
    if chunks.is_empty() {
        chunks.push("base instructions unavailable".to_string());
    }
    chunks.join("\n\n")
}

fn layer_content(sections: &[String]) -> Option<String> {
    let compact: Vec<String> = sections
        .iter()
        .map(|section| section.trim())
        .filter(|section| !section.is_empty())
        .map(ToString::to_string)
        .collect();
    if compact.is_empty() {
        None
    } else {
        Some(compact.join("\n\n"))
    }
}

fn has_any_text(sections: &[String]) -> bool {
    sections.iter().any(|section| !section.trim().is_empty())
}

fn packet_identity(packet_id: &str, content: &str) -> PacketIdentity {
    let mut sha1 = Sha1::new();
    sha1.update(content.as_bytes());
    let digest = sha1.finalize();
    PacketIdentity {
        packet_id: packet_id.to_string(),
        content_hash: format!("sha1:{digest:x}"),
        schema: PACKET_SCHEMA.to_string(),
    }
}

fn summarize_for_clause(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "no content".to_string();
    }

    let mut summary: String = normalized.chars().take(240).collect();
    if normalized.chars().count() > 240 {
        summary.push_str("...");
    }
    summary
}

fn fallback_semantics(path_variant: GovernancePathVariant) -> PromptLayerFallbackSemantics {
    let compiler_failure = match path_variant {
        GovernancePathVariant::Off => "governance path disabled; no layered fallback required",
        GovernancePathVariant::StrictV1Shadow => {
            "record diagnostics and continue with the remaining available layers"
        }
        GovernancePathVariant::StrictV1Enforce => {
            "treat compile failure as invalid narrowing; preserve constitutional constraints and request clarification before irreversible actions"
        }
    };
    PromptLayerFallbackSemantics {
        missing_role: "fall back to constitutional + task + runtime; do not invent role-only norms",
        missing_task: "fall back to constitutional + role + runtime and ask for explicit assignment before irreversible work",
        missing_runtime: "fall back to the last known stable runtime assumptions and avoid privileged operations",
        compiler_failure,
    }
}

#[cfg(test)]
mod tests {
    use super::GOVERNANCE_PROMPT_LAYERS_TAG;
    use super::PromptLayeringInput;
    use super::PromptLayeringPhase;
    use super::build_prompt_layering_section;
    use crate::config::GovernancePathVariant;
    use codex_protocol::protocol::SessionSource;

    #[test]
    fn build_prompt_layering_section_returns_none_when_governance_is_off() {
        let empty_sections = Vec::new();
        let input = PromptLayeringInput {
            path_variant: GovernancePathVariant::Off,
            phase: PromptLayeringPhase::InitialContext,
            model: "gpt-5.4",
            session_source: &SessionSource::Exec,
            base_instructions: "base",
            constitutional_sections: &empty_sections,
            role_sections: &empty_sections,
            task_sections: &empty_sections,
            runtime_sections: &empty_sections,
        };

        assert_eq!(build_prompt_layering_section(input), None);
    }

    #[test]
    fn build_prompt_layering_section_emits_tagged_json_payload_for_strict_mode() {
        let constitutional_sections = vec!["model switch guidance".to_string()];
        let role_sections = vec!["role posture".to_string()];
        let task_sections = vec!["task charter".to_string()];
        let runtime_sections = vec!["runtime capability".to_string()];

        let input = PromptLayeringInput {
            path_variant: GovernancePathVariant::StrictV1Shadow,
            phase: PromptLayeringPhase::InitialContext,
            model: "gpt-5.4",
            session_source: &SessionSource::Exec,
            base_instructions: "base instructions",
            constitutional_sections: &constitutional_sections,
            role_sections: &role_sections,
            task_sections: &task_sections,
            runtime_sections: &runtime_sections,
        };

        let section =
            build_prompt_layering_section(input).expect("strict mode should emit a section");
        assert!(section.contains(&format!("<{GOVERNANCE_PROMPT_LAYERS_TAG}")));
        assert!(section.contains("\"path_variant\": \"strict_v1_shadow\""));
        assert!(section.contains("\"phase\": \"initial_context\""));
        assert!(section.contains("\"constitutional\""));
        assert!(section.contains("\"role\""));
        assert!(section.contains("\"task\""));
        assert!(section.contains("\"runtime\""));
    }
}
