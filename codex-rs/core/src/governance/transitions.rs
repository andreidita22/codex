use crate::config::GovernancePathVariant;
use crate::governance::compiler::CompiledGovernancePackets;
use crate::governance::diagnostics::GovernanceDiagnostic;
use crate::governance::diagnostics::GovernanceDiagnosticCode;
use crate::governance::diagnostics::GovernanceDiagnosticSeverity;
use crate::governance::diagnostics::GovernanceViolationKind;
use crate::governance::packets::PacketIdentity;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpawnRolePolicy {
    Inherit,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "transition", rename_all = "snake_case")]
pub enum GovernanceTransition {
    Spawn { role_policy: SpawnRolePolicy },
    Compact,
    Resume,
    Swap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceTransitionDecision {
    Allow,
    AllowWithDiagnostics,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GovernanceTransitionLegalityReport {
    pub transition: GovernanceTransition,
    pub diagnostics: Vec<GovernanceDiagnostic>,
    pub decision: GovernanceTransitionDecision,
}

pub struct GovernanceTransitionCheckInput<'a> {
    pub path_variant: GovernancePathVariant,
    pub transition: GovernanceTransition,
    pub previous: &'a CompiledGovernancePackets,
    pub next: &'a CompiledGovernancePackets,
}

pub fn check_transition_legality(
    input: GovernanceTransitionCheckInput<'_>,
) -> GovernanceTransitionLegalityReport {
    if input.path_variant == GovernancePathVariant::Off {
        return GovernanceTransitionLegalityReport {
            transition: input.transition,
            diagnostics: Vec::new(),
            decision: GovernanceTransitionDecision::Allow,
        };
    }

    let transition = input.transition;
    let mut diagnostics = Vec::new();
    check_constitution_legality(input.previous, input.next, &transition, &mut diagnostics);

    match &transition {
        GovernanceTransition::Spawn { role_policy } => {
            check_spawn_legality(
                *role_policy,
                input.previous,
                input.next,
                &transition,
                &mut diagnostics,
            );
        }
        GovernanceTransition::Compact => {
            check_continuity_legality(
                input.previous,
                input.next,
                &transition,
                GovernanceDiagnosticSeverity::Warning,
                &mut diagnostics,
            );
        }
        GovernanceTransition::Resume | GovernanceTransition::Swap => {
            check_continuity_legality(
                input.previous,
                input.next,
                &transition,
                GovernanceDiagnosticSeverity::Error,
                &mut diagnostics,
            );
        }
    }

    let decision = match input.path_variant {
        GovernancePathVariant::Off => GovernanceTransitionDecision::Allow,
        GovernancePathVariant::StrictV1Shadow => {
            if diagnostics.is_empty() {
                GovernanceTransitionDecision::Allow
            } else {
                GovernanceTransitionDecision::AllowWithDiagnostics
            }
        }
        GovernancePathVariant::StrictV1Enforce => {
            if diagnostics.iter().any(GovernanceDiagnostic::is_error) {
                GovernanceTransitionDecision::Block
            } else if diagnostics.is_empty() {
                GovernanceTransitionDecision::Allow
            } else {
                GovernanceTransitionDecision::AllowWithDiagnostics
            }
        }
    };

    GovernanceTransitionLegalityReport {
        transition,
        diagnostics,
        decision,
    }
}

fn check_constitution_legality(
    previous: &CompiledGovernancePackets,
    next: &CompiledGovernancePackets,
    transition: &GovernanceTransition,
    diagnostics: &mut Vec<GovernanceDiagnostic>,
) {
    if next.constitution.is_none() {
        diagnostics.push(GovernanceDiagnostic::error(
            GovernanceDiagnosticCode::MissingConstitutionPacket,
            GovernanceViolationKind::PacketProjectionBug,
            format!(
                "{} transition must preserve a constitution packet",
                transition_name(transition)
            ),
        ));
        return;
    }

    if let (Some(previous), Some(next)) = (&previous.constitution, &next.constitution)
        && !identities_match(&previous.identity, &next.identity)
    {
        diagnostics.push(GovernanceDiagnostic::error(
            GovernanceDiagnosticCode::ConstitutionIdentityChanged,
            GovernanceViolationKind::IllegalAmendment,
            format!(
                "{} transition changed constitution identity from `{}` to `{}`",
                transition_name(transition),
                previous.identity.packet_id,
                next.identity.packet_id
            ),
        ));
    }
}

fn check_spawn_legality(
    role_policy: SpawnRolePolicy,
    previous: &CompiledGovernancePackets,
    next: &CompiledGovernancePackets,
    transition: &GovernanceTransition,
    diagnostics: &mut Vec<GovernanceDiagnostic>,
) {
    match role_policy {
        SpawnRolePolicy::Inherit => {
            push_missing_if_dropped(
                previous.role.as_ref().map(|packet| &packet.identity),
                next.role.as_ref().map(|packet| &packet.identity),
                ContinuityDiagnosticSpec {
                    code: GovernanceDiagnosticCode::MissingRolePacket,
                    kind: GovernanceViolationKind::RoleUnderInstantiation,
                    severity: GovernanceDiagnosticSeverity::Error,
                    label: "role packet",
                },
                transition,
                diagnostics,
            );
            push_identity_change_if_changed(
                previous.role.as_ref().map(|packet| &packet.identity),
                next.role.as_ref().map(|packet| &packet.identity),
                ContinuityDiagnosticSpec {
                    code: GovernanceDiagnosticCode::RoleIdentityChanged,
                    kind: GovernanceViolationKind::InheritanceBug,
                    severity: GovernanceDiagnosticSeverity::Error,
                    label: "role packet",
                },
                transition,
                diagnostics,
            );
        }
        SpawnRolePolicy::Replace => {
            if next.role.is_none() {
                diagnostics.push(GovernanceDiagnostic::error(
                    GovernanceDiagnosticCode::MissingRolePacket,
                    GovernanceViolationKind::RoleUnderInstantiation,
                    "spawn transition with replace policy must provide a role packet".to_string(),
                ));
            }
        }
    }

    if next.task_charter.is_none() {
        diagnostics.push(GovernanceDiagnostic::error(
            GovernanceDiagnosticCode::MissingTaskCharterPacket,
            GovernanceViolationKind::WorkerBug,
            "spawn transition must provide a task charter packet".to_string(),
        ));
    }

    if let (Some(previous), Some(next)) = (&previous.runtime_fact_store, &next.runtime_fact_store)
        && identities_match(&previous.identity, &next.identity)
    {
        diagnostics.push(GovernanceDiagnostic::error(
            GovernanceDiagnosticCode::SpawnRuntimeFactStoreReused,
            GovernanceViolationKind::RuntimeContaminationBug,
            format!(
                "spawn transition reused runtime fact store identity `{}` instead of starting fresh runtime state",
                next.identity.packet_id
            ),
        ));
    }

    push_missing_if_dropped(
        previous
            .adapter_overlay
            .as_ref()
            .map(|packet| &packet.identity),
        next.adapter_overlay.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingAdapterOverlayPacket,
            kind: GovernanceViolationKind::AdapterOverlayBug,
            severity: GovernanceDiagnosticSeverity::Warning,
            label: "adapter overlay packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous
            .adapter_overlay
            .as_ref()
            .map(|packet| &packet.identity),
        next.adapter_overlay.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::AdapterOverlayIdentityChanged,
            kind: GovernanceViolationKind::AdapterOverlayBug,
            severity: GovernanceDiagnosticSeverity::Warning,
            label: "adapter overlay packet",
        },
        transition,
        diagnostics,
    );
}

fn check_continuity_legality(
    previous: &CompiledGovernancePackets,
    next: &CompiledGovernancePackets,
    transition: &GovernanceTransition,
    task_residual_missing_severity: GovernanceDiagnosticSeverity,
    diagnostics: &mut Vec<GovernanceDiagnostic>,
) {
    push_missing_if_dropped(
        previous.role.as_ref().map(|packet| &packet.identity),
        next.role.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingRolePacket,
            kind: GovernanceViolationKind::RoleUnderInstantiation,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "role packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous.role.as_ref().map(|packet| &packet.identity),
        next.role.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::RoleIdentityChanged,
            kind: GovernanceViolationKind::InheritanceBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "role packet",
        },
        transition,
        diagnostics,
    );

    push_missing_if_dropped(
        previous
            .task_charter
            .as_ref()
            .map(|packet| &packet.identity),
        next.task_charter.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingTaskCharterPacket,
            kind: GovernanceViolationKind::WorkerBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "task charter packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous
            .task_charter
            .as_ref()
            .map(|packet| &packet.identity),
        next.task_charter.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::TaskCharterIdentityChanged,
            kind: GovernanceViolationKind::InheritanceBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "task charter packet",
        },
        transition,
        diagnostics,
    );

    push_missing_if_dropped(
        previous
            .task_residual
            .as_ref()
            .map(|packet| &packet.identity),
        next.task_residual.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingTaskResidualPacket,
            kind: GovernanceViolationKind::InheritanceBug,
            severity: task_residual_missing_severity,
            label: "task residual packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous
            .task_residual
            .as_ref()
            .map(|packet| &packet.identity),
        next.task_residual.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::TaskResidualIdentityChanged,
            kind: GovernanceViolationKind::InheritanceBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "task residual packet",
        },
        transition,
        diagnostics,
    );

    push_missing_if_dropped(
        previous
            .adapter_overlay
            .as_ref()
            .map(|packet| &packet.identity),
        next.adapter_overlay.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingAdapterOverlayPacket,
            kind: GovernanceViolationKind::AdapterOverlayBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "adapter overlay packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous
            .adapter_overlay
            .as_ref()
            .map(|packet| &packet.identity),
        next.adapter_overlay.as_ref().map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::AdapterOverlayIdentityChanged,
            kind: GovernanceViolationKind::AdapterOverlayBug,
            severity: GovernanceDiagnosticSeverity::Error,
            label: "adapter overlay packet",
        },
        transition,
        diagnostics,
    );
}

struct ContinuityDiagnosticSpec<'a> {
    code: GovernanceDiagnosticCode,
    kind: GovernanceViolationKind,
    severity: GovernanceDiagnosticSeverity,
    label: &'a str,
}

fn push_missing_if_dropped(
    previous: Option<&PacketIdentity>,
    next: Option<&PacketIdentity>,
    spec: ContinuityDiagnosticSpec<'_>,
    transition: &GovernanceTransition,
    diagnostics: &mut Vec<GovernanceDiagnostic>,
) {
    if previous.is_some() && next.is_none() {
        diagnostics.push(build_diagnostic(
            spec.code,
            spec.kind,
            spec.severity,
            format!(
                "{} transition dropped {}",
                transition_name(transition),
                spec.label
            ),
        ));
    }
}

fn push_identity_change_if_changed(
    previous: Option<&PacketIdentity>,
    next: Option<&PacketIdentity>,
    spec: ContinuityDiagnosticSpec<'_>,
    transition: &GovernanceTransition,
    diagnostics: &mut Vec<GovernanceDiagnostic>,
) {
    if let (Some(previous), Some(next)) = (previous, next)
        && !identities_match(previous, next)
    {
        diagnostics.push(build_diagnostic(
            spec.code,
            spec.kind,
            spec.severity,
            format!(
                "{} transition changed {} identity from `{}` to `{}`",
                transition_name(transition),
                spec.label,
                previous.packet_id,
                next.packet_id
            ),
        ));
    }
}

fn build_diagnostic(
    code: GovernanceDiagnosticCode,
    kind: GovernanceViolationKind,
    severity: GovernanceDiagnosticSeverity,
    message: String,
) -> GovernanceDiagnostic {
    match severity {
        GovernanceDiagnosticSeverity::Error => GovernanceDiagnostic::error(code, kind, message),
        GovernanceDiagnosticSeverity::Warning => GovernanceDiagnostic::warning(code, kind, message),
        GovernanceDiagnosticSeverity::Info => GovernanceDiagnostic {
            code,
            kind,
            severity,
            message,
        },
    }
}

fn identities_match(previous: &PacketIdentity, next: &PacketIdentity) -> bool {
    previous.packet_id == next.packet_id
        && previous.content_hash == next.content_hash
        && previous.schema == next.schema
}

fn transition_name(transition: &GovernanceTransition) -> &'static str {
    match transition {
        GovernanceTransition::Spawn { .. } => "spawn",
        GovernanceTransition::Compact => "compact",
        GovernanceTransition::Resume => "resume",
        GovernanceTransition::Swap => "swap",
    }
}

#[cfg(test)]
mod tests {
    use super::GovernanceTransition;
    use super::GovernanceTransitionCheckInput;
    use super::GovernanceTransitionDecision;
    use super::SpawnRolePolicy;
    use super::check_transition_legality;
    use crate::config::GovernancePathVariant;
    use crate::governance::compiler::CompiledGovernancePackets;
    use crate::governance::diagnostics::GovernanceDiagnosticCode;
    use crate::governance::diagnostics::GovernanceDiagnosticSeverity;
    use crate::governance::packets::AdapterOverlayPacket;
    use crate::governance::packets::ConstitutionPacket;
    use crate::governance::packets::GovernanceClause;
    use crate::governance::packets::NormativeForce;
    use crate::governance::packets::PacketIdentity;
    use crate::governance::packets::RolePacket;
    use crate::governance::packets::RuntimeFact;
    use crate::governance::packets::RuntimeFactStore;
    use crate::governance::packets::TaskCharterPacket;
    use crate::governance::packets::TaskResidualPacket;
    use pretty_assertions::assert_eq;

    fn packet_identity(id: &str) -> PacketIdentity {
        PacketIdentity {
            packet_id: id.to_string(),
            content_hash: format!("hash:{id}"),
            schema: "governance@1".to_string(),
        }
    }

    fn clause() -> GovernanceClause {
        GovernanceClause {
            force: NormativeForce::Hard,
            subject: "safety".to_string(),
            instruction: "preserve invariants".to_string(),
        }
    }

    fn base_packets() -> CompiledGovernancePackets {
        CompiledGovernancePackets {
            constitution: Some(ConstitutionPacket {
                identity: packet_identity("constitution:base"),
                clauses: vec![clause()],
            }),
            role: Some(RolePacket {
                identity: packet_identity("role:base"),
                role_name: "worker".to_string(),
                posture: vec![clause()],
            }),
            task_charter: Some(TaskCharterPacket {
                identity: packet_identity("task:base"),
                assignment_id: "assignment:base".to_string(),
                ordered_obligations: vec![clause()],
            }),
            task_residual: Some(TaskResidualPacket {
                identity: packet_identity("residual:base"),
                open_obligations: vec![clause()],
                blockers: Vec::new(),
                accepted_decisions: vec!["d1".to_string()],
            }),
            runtime_fact_store: Some(RuntimeFactStore {
                identity: packet_identity("runtime:base"),
                facts: vec![RuntimeFact {
                    key: "phase".to_string(),
                    value: "implementation".to_string(),
                    source: "runtime".to_string(),
                }],
            }),
            adapter_overlay: Some(AdapterOverlayPacket {
                identity: packet_identity("adapter:base"),
                adapter_id: "default".to_string(),
                clauses: vec![clause()],
            }),
            projection_witness: Default::default(),
        }
    }

    #[test]
    fn off_mode_skips_legality_checks() {
        let mut next = base_packets();
        next.constitution = None;

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::Off,
            transition: GovernanceTransition::Resume,
            previous: &base_packets(),
            next: &next,
        });

        assert_eq!(report.decision, GovernanceTransitionDecision::Allow);
        assert_eq!(report.diagnostics, Vec::new());
    }

    #[test]
    fn spawn_inherit_role_policy_requires_role_continuity() {
        let previous = base_packets();
        let mut next = base_packets();
        next.role = Some(RolePacket {
            identity: packet_identity("role:changed"),
            role_name: "worker".to_string(),
            posture: vec![clause()],
        });
        next.runtime_fact_store = Some(RuntimeFactStore {
            identity: packet_identity("runtime:fresh"),
            facts: Vec::new(),
        });

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::StrictV1Enforce,
            transition: GovernanceTransition::Spawn {
                role_policy: SpawnRolePolicy::Inherit,
            },
            previous: &previous,
            next: &next,
        });

        assert_eq!(report.decision, GovernanceTransitionDecision::Block);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == GovernanceDiagnosticCode::RoleIdentityChanged)
        );
    }

    #[test]
    fn spawn_detects_runtime_fact_store_reuse() {
        let previous = base_packets();
        let next = base_packets();

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::StrictV1Enforce,
            transition: GovernanceTransition::Spawn {
                role_policy: SpawnRolePolicy::Replace,
            },
            previous: &previous,
            next: &next,
        });

        assert_eq!(report.decision, GovernanceTransitionDecision::Block);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == GovernanceDiagnosticCode::SpawnRuntimeFactStoreReused
        }));
    }

    #[test]
    fn compact_flags_missing_task_residual_as_warning() {
        let previous = base_packets();
        let mut next = base_packets();
        next.task_residual = None;

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::StrictV1Shadow,
            transition: GovernanceTransition::Compact,
            previous: &previous,
            next: &next,
        });

        assert_eq!(
            report.decision,
            GovernanceTransitionDecision::AllowWithDiagnostics
        );
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == GovernanceDiagnosticCode::MissingTaskResidualPacket
                && diagnostic.severity == GovernanceDiagnosticSeverity::Warning
        }));
    }

    #[test]
    fn resume_blocks_when_constitution_identity_changes() {
        let previous = base_packets();
        let mut next = base_packets();
        next.constitution = Some(ConstitutionPacket {
            identity: packet_identity("constitution:changed"),
            clauses: vec![clause()],
        });

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::StrictV1Enforce,
            transition: GovernanceTransition::Resume,
            previous: &previous,
            next: &next,
        });

        assert_eq!(report.decision, GovernanceTransitionDecision::Block);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == GovernanceDiagnosticCode::ConstitutionIdentityChanged
        }));
    }

    #[test]
    fn swap_blocks_when_task_charter_identity_changes() {
        let previous = base_packets();
        let mut next = base_packets();
        next.task_charter = Some(TaskCharterPacket {
            identity: packet_identity("task:changed"),
            assignment_id: "assignment:base".to_string(),
            ordered_obligations: vec![clause()],
        });

        let report = check_transition_legality(GovernanceTransitionCheckInput {
            path_variant: GovernancePathVariant::StrictV1Enforce,
            transition: GovernanceTransition::Swap,
            previous: &previous,
            next: &next,
        });

        assert_eq!(report.decision, GovernanceTransitionDecision::Block);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == GovernanceDiagnosticCode::TaskCharterIdentityChanged
        }));
    }
}
