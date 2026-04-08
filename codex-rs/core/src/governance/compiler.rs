use crate::config::GovernancePathVariant;
use crate::governance::packets::AdapterOverlayPacket;
use crate::governance::packets::ConstitutionPacket;
use crate::governance::packets::GovernancePacket;
use crate::governance::packets::PacketLayer;
use crate::governance::packets::RolePacket;
use crate::governance::packets::RuntimeFactStore;
use crate::governance::packets::TaskCharterPacket;
use crate::governance::packets::TaskResidualPacket;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PacketSourceKind {
    InlineConfig,
    FileArtifact,
    PromptChannel,
    RuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "classification", rename_all = "snake_case")]
pub enum SourceFragment {
    Packet {
        source_id: String,
        source_kind: PacketSourceKind,
        packet: GovernancePacket,
    },
    MixedLayer {
        source_id: String,
        source_kind: PacketSourceKind,
        claimed_layers: Vec<PacketLayer>,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GovernanceCompilationInput {
    pub path_variant: GovernancePathVariant,
    pub fragments: Vec<SourceFragment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcceptedPacketProjection {
    pub source_id: String,
    pub source_kind: PacketSourceKind,
    pub layer: PacketLayer,
    pub packet_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedFragmentProjection {
    pub source_id: String,
    pub source_kind: PacketSourceKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectionAmbiguity {
    pub source_id: String,
    pub source_kind: PacketSourceKind,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProjectionWitness {
    pub accepted: Vec<AcceptedPacketProjection>,
    pub rejected: Vec<RejectedFragmentProjection>,
    pub ambiguities: Vec<ProjectionAmbiguity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct CompiledGovernancePackets {
    pub constitution: Option<ConstitutionPacket>,
    pub role: Option<RolePacket>,
    pub task_charter: Option<TaskCharterPacket>,
    pub task_residual: Option<TaskResidualPacket>,
    pub runtime_fact_store: Option<RuntimeFactStore>,
    pub adapter_overlay: Option<AdapterOverlayPacket>,
    pub projection_witness: ProjectionWitness,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GovernanceCompileError {
    #[error("packet `{packet_id}` from source `{source_id}` has invalid identity: {reason}")]
    InvalidPacketIdentity {
        packet_id: String,
        source_id: String,
        reason: String,
    },
    #[error(
        "duplicate packet for layer `{layer:?}` from source `{source_id}` (already provided by `{existing_source_id}`)"
    )]
    DuplicateLayerPacket {
        layer: PacketLayer,
        source_id: String,
        existing_source_id: String,
    },
    #[error("governance path `{path_variant:?}` requires a constitution packet")]
    MissingConstitutionPacket { path_variant: GovernancePathVariant },
}

pub fn compile_packets(
    input: GovernanceCompilationInput,
) -> Result<CompiledGovernancePackets, GovernanceCompileError> {
    let mut compiled = CompiledGovernancePackets::default();

    for fragment in input.fragments {
        match fragment {
            SourceFragment::Packet {
                source_id,
                source_kind,
                packet,
            } => {
                let (packet_id, content_hash, schema) = {
                    let identity = packet.identity();
                    (
                        identity.packet_id.clone(),
                        identity.content_hash.clone(),
                        identity.schema.clone(),
                    )
                };
                if packet_id.trim().is_empty() {
                    return Err(GovernanceCompileError::InvalidPacketIdentity {
                        packet_id,
                        source_id,
                        reason: "packet_id must be non-empty".to_string(),
                    });
                }
                if content_hash.trim().is_empty() {
                    return Err(GovernanceCompileError::InvalidPacketIdentity {
                        packet_id,
                        source_id,
                        reason: "content_hash must be non-empty".to_string(),
                    });
                }
                if schema.trim().is_empty() {
                    return Err(GovernanceCompileError::InvalidPacketIdentity {
                        packet_id,
                        source_id,
                        reason: "schema must be non-empty".to_string(),
                    });
                }

                let layer = packet.layer();
                if let Some(existing_source_id) = compiled
                    .projection_witness
                    .accepted
                    .iter()
                    .find(|projection| projection.layer == layer)
                    .map(|projection| projection.source_id.clone())
                {
                    return Err(GovernanceCompileError::DuplicateLayerPacket {
                        layer,
                        source_id,
                        existing_source_id,
                    });
                }

                match packet {
                    GovernancePacket::Constitution(packet) => {
                        compiled.constitution = Some(packet);
                    }
                    GovernancePacket::Role(packet) => {
                        compiled.role = Some(packet);
                    }
                    GovernancePacket::TaskCharter(packet) => {
                        compiled.task_charter = Some(packet);
                    }
                    GovernancePacket::TaskResidual(packet) => {
                        compiled.task_residual = Some(packet);
                    }
                    GovernancePacket::RuntimeFactStore(packet) => {
                        compiled.runtime_fact_store = Some(packet);
                    }
                    GovernancePacket::AdapterOverlay(packet) => {
                        compiled.adapter_overlay = Some(packet);
                    }
                }

                compiled
                    .projection_witness
                    .accepted
                    .push(AcceptedPacketProjection {
                        source_id,
                        source_kind,
                        layer,
                        packet_id,
                        content_hash,
                    });
            }
            SourceFragment::MixedLayer {
                source_id,
                source_kind,
                claimed_layers,
                reason,
            } => {
                let claimed = claimed_layers
                    .iter()
                    .map(|layer| format!("{layer:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                compiled
                    .projection_witness
                    .rejected
                    .push(RejectedFragmentProjection {
                        source_id,
                        source_kind,
                        reason: format!("{reason}; claimed_layers=[{claimed}]"),
                    });
            }
        }
    }

    if input.path_variant != GovernancePathVariant::Off && compiled.constitution.is_none() {
        return Err(GovernanceCompileError::MissingConstitutionPacket {
            path_variant: input.path_variant,
        });
    }

    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::GovernanceCompilationInput;
    use super::GovernanceCompileError;
    use super::PacketSourceKind;
    use super::SourceFragment;
    use super::compile_packets;
    use crate::config::GovernancePathVariant;
    use crate::governance::packets::ConstitutionPacket;
    use crate::governance::packets::GovernanceClause;
    use crate::governance::packets::GovernancePacket;
    use crate::governance::packets::NormativeForce;
    use crate::governance::packets::PacketIdentity;
    use crate::governance::packets::PacketLayer;
    use pretty_assertions::assert_eq;

    fn constitution_packet(packet_id: &str) -> ConstitutionPacket {
        ConstitutionPacket {
            identity: PacketIdentity {
                packet_id: packet_id.to_string(),
                content_hash: "sha256:abc123".to_string(),
                schema: "constitution_packet@1".to_string(),
            },
            clauses: vec![GovernanceClause {
                force: NormativeForce::Hard,
                subject: "tool_safety".to_string(),
                instruction: "never bypass approval requirements".to_string(),
            }],
        }
    }

    #[test]
    fn off_variant_allows_empty_packet_set() {
        let compiled = compile_packets(GovernanceCompilationInput {
            path_variant: GovernancePathVariant::Off,
            fragments: Vec::new(),
        })
        .expect("off variant should allow empty compilation input");

        assert_eq!(compiled.constitution, None);
        assert_eq!(compiled.projection_witness.accepted, Vec::new());
        assert_eq!(compiled.projection_witness.rejected, Vec::new());
        assert_eq!(compiled.projection_witness.ambiguities, Vec::new());
    }

    #[test]
    fn strict_variant_requires_constitution_packet() {
        let err = compile_packets(GovernanceCompilationInput {
            path_variant: GovernancePathVariant::StrictV1Enforce,
            fragments: Vec::new(),
        })
        .expect_err("strict path should require constitution packet");

        assert_eq!(
            err,
            GovernanceCompileError::MissingConstitutionPacket {
                path_variant: GovernancePathVariant::StrictV1Enforce
            }
        );
    }

    #[test]
    fn compile_accepts_constitution_and_records_provenance() {
        let packet = constitution_packet("constitution@main");
        let compiled = compile_packets(GovernanceCompilationInput {
            path_variant: GovernancePathVariant::StrictV1Shadow,
            fragments: vec![SourceFragment::Packet {
                source_id: "config:constitution".to_string(),
                source_kind: PacketSourceKind::FileArtifact,
                packet: GovernancePacket::Constitution(packet.clone()),
            }],
        })
        .expect("compilation should succeed");

        assert_eq!(compiled.constitution, Some(packet));
        assert_eq!(compiled.projection_witness.accepted.len(), 1);
        assert_eq!(
            compiled.projection_witness.accepted[0].source_id,
            "config:constitution"
        );
        assert_eq!(
            compiled.projection_witness.accepted[0].layer,
            PacketLayer::Constitution
        );
        assert_eq!(compiled.projection_witness.rejected, Vec::new());
    }

    #[test]
    fn compile_rejects_duplicate_layers() {
        let err = compile_packets(GovernanceCompilationInput {
            path_variant: GovernancePathVariant::StrictV1Shadow,
            fragments: vec![
                SourceFragment::Packet {
                    source_id: "config:a".to_string(),
                    source_kind: PacketSourceKind::InlineConfig,
                    packet: GovernancePacket::Constitution(constitution_packet("a")),
                },
                SourceFragment::Packet {
                    source_id: "config:b".to_string(),
                    source_kind: PacketSourceKind::InlineConfig,
                    packet: GovernancePacket::Constitution(constitution_packet("b")),
                },
            ],
        })
        .expect_err("duplicate constitution packets should fail");

        assert_eq!(
            err,
            GovernanceCompileError::DuplicateLayerPacket {
                layer: PacketLayer::Constitution,
                source_id: "config:b".to_string(),
                existing_source_id: "config:a".to_string(),
            }
        );
    }

    #[test]
    fn compile_records_mixed_layer_rejections() {
        let compiled = compile_packets(GovernanceCompilationInput {
            path_variant: GovernancePathVariant::StrictV1Shadow,
            fragments: vec![
                SourceFragment::MixedLayer {
                    source_id: "prompt:bundle".to_string(),
                    source_kind: PacketSourceKind::PromptChannel,
                    claimed_layers: vec![PacketLayer::Role, PacketLayer::TaskCharter],
                    reason: "mixed-layer fragment".to_string(),
                },
                SourceFragment::Packet {
                    source_id: "config:constitution".to_string(),
                    source_kind: PacketSourceKind::FileArtifact,
                    packet: GovernancePacket::Constitution(constitution_packet("main")),
                },
            ],
        })
        .expect("compilation should succeed");

        assert_eq!(compiled.projection_witness.rejected.len(), 1);
        assert_eq!(
            compiled.projection_witness.rejected[0].source_id,
            "prompt:bundle"
        );
        assert!(
            compiled.projection_witness.rejected[0]
                .reason
                .contains("claimed_layers=[Role, TaskCharter]")
        );
    }
}
