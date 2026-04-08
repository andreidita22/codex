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
                ContinuitySeverityPolicy::compact(),
                &mut diagnostics,
            );
        }
        GovernanceTransition::Resume | GovernanceTransition::Swap => {
            check_continuity_legality(
                input.previous,
                input.next,
                &transition,
                ContinuitySeverityPolicy::strict(),
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
        && !strict_identities_match(&previous.identity, &next.identity)
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
                strict_identities_match,
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
        && previous.identity.packet_id == next.identity.packet_id
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
        strict_identities_match,
    );
}

#[derive(Debug, Clone, Copy)]
struct ContinuitySeverityPolicy {
    task_residual: GovernanceDiagnosticSeverity,
    runtime_fact_store: GovernanceDiagnosticSeverity,
    adapter_overlay: GovernanceDiagnosticSeverity,
}

impl ContinuitySeverityPolicy {
    fn compact() -> Self {
        Self {
            task_residual: GovernanceDiagnosticSeverity::Warning,
            runtime_fact_store: GovernanceDiagnosticSeverity::Warning,
            adapter_overlay: GovernanceDiagnosticSeverity::Warning,
        }
    }

    fn strict() -> Self {
        Self {
            task_residual: GovernanceDiagnosticSeverity::Error,
            runtime_fact_store: GovernanceDiagnosticSeverity::Error,
            adapter_overlay: GovernanceDiagnosticSeverity::Error,
        }
    }
}

fn check_continuity_legality(
    previous: &CompiledGovernancePackets,
    next: &CompiledGovernancePackets,
    transition: &GovernanceTransition,
    severity_policy: ContinuitySeverityPolicy,
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
        strict_identities_match,
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
        strict_identities_match,
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
            severity: severity_policy.task_residual,
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
            severity: severity_policy.task_residual,
            label: "task residual packet",
        },
        transition,
        diagnostics,
        continuity_identities_match,
    );

    push_missing_if_dropped(
        previous
            .runtime_fact_store
            .as_ref()
            .map(|packet| &packet.identity),
        next.runtime_fact_store
            .as_ref()
            .map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::MissingRuntimeFactStorePacket,
            kind: GovernanceViolationKind::RuntimeContaminationBug,
            severity: severity_policy.runtime_fact_store,
            label: "runtime fact store packet",
        },
        transition,
        diagnostics,
    );
    push_identity_change_if_changed(
        previous
            .runtime_fact_store
            .as_ref()
            .map(|packet| &packet.identity),
        next.runtime_fact_store
            .as_ref()
            .map(|packet| &packet.identity),
        ContinuityDiagnosticSpec {
            code: GovernanceDiagnosticCode::RuntimeFactStoreIdentityChanged,
            kind: GovernanceViolationKind::RuntimeContaminationBug,
            severity: severity_policy.runtime_fact_store,
            label: "runtime fact store packet",
        },
        transition,
        diagnostics,
        continuity_identities_match,
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
            severity: severity_policy.adapter_overlay,
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
            severity: severity_policy.adapter_overlay,
            label: "adapter overlay packet",
        },
        transition,
        diagnostics,
        strict_identities_match,
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
    matcher: fn(&PacketIdentity, &PacketIdentity) -> bool,
) {
    if let (Some(previous), Some(next)) = (previous, next)
        && !matcher(previous, next)
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

fn strict_identities_match(previous: &PacketIdentity, next: &PacketIdentity) -> bool {
    previous.packet_id == next.packet_id
        && previous.content_hash == next.content_hash
        && previous.schema == next.schema
}

fn continuity_identities_match(previous: &PacketIdentity, next: &PacketIdentity) -> bool {
    previous.packet_id == next.packet_id && previous.schema == next.schema
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
#[path = "transitions_tests.rs"]
mod tests;
