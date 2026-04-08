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
    let mut next = base_packets();
    if let Some(runtime_fact_store) = &mut next.runtime_fact_store {
        runtime_fact_store.identity.content_hash = "hash:runtime:changed".to_string();
        runtime_fact_store.facts.push(RuntimeFact {
            key: "note".to_string(),
            value: "mutated".to_string(),
            source: "runtime".to_string(),
        });
    }

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
fn compact_downgrades_adapter_overlay_drop_to_warning() {
    let previous = base_packets();
    let mut next = base_packets();
    next.adapter_overlay = None;

    let report = check_transition_legality(GovernanceTransitionCheckInput {
        path_variant: GovernancePathVariant::StrictV1Enforce,
        transition: GovernanceTransition::Compact,
        previous: &previous,
        next: &next,
    });

    assert_eq!(
        report.decision,
        GovernanceTransitionDecision::AllowWithDiagnostics
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == GovernanceDiagnosticCode::MissingAdapterOverlayPacket
            && diagnostic.severity == GovernanceDiagnosticSeverity::Warning
    }));
}

#[test]
fn compact_flags_missing_runtime_fact_store_as_warning() {
    let previous = base_packets();
    let mut next = base_packets();
    next.runtime_fact_store = None;

    let report = check_transition_legality(GovernanceTransitionCheckInput {
        path_variant: GovernancePathVariant::StrictV1Enforce,
        transition: GovernanceTransition::Compact,
        previous: &previous,
        next: &next,
    });

    assert_eq!(
        report.decision,
        GovernanceTransitionDecision::AllowWithDiagnostics
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == GovernanceDiagnosticCode::MissingRuntimeFactStorePacket
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
fn resume_accepts_task_residual_content_hash_changes() {
    let previous = base_packets();
    let mut next = base_packets();
    if let Some(task_residual) = &mut next.task_residual {
        task_residual.identity.content_hash = "hash:residual:updated".to_string();
        task_residual.accepted_decisions.push("d2".to_string());
    }

    let report = check_transition_legality(GovernanceTransitionCheckInput {
        path_variant: GovernancePathVariant::StrictV1Enforce,
        transition: GovernanceTransition::Resume,
        previous: &previous,
        next: &next,
    });

    assert_eq!(report.decision, GovernanceTransitionDecision::Allow);
    assert_eq!(report.diagnostics, Vec::new());
}

#[test]
fn resume_blocks_when_runtime_fact_store_identity_changes() {
    let previous = base_packets();
    let mut next = base_packets();
    next.runtime_fact_store = Some(RuntimeFactStore {
        identity: packet_identity("runtime:changed"),
        facts: vec![RuntimeFact {
            key: "phase".to_string(),
            value: "implementation".to_string(),
            source: "runtime".to_string(),
        }],
    });

    let report = check_transition_legality(GovernanceTransitionCheckInput {
        path_variant: GovernancePathVariant::StrictV1Enforce,
        transition: GovernanceTransition::Resume,
        previous: &previous,
        next: &next,
    });

    assert_eq!(report.decision, GovernanceTransitionDecision::Block);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == GovernanceDiagnosticCode::RuntimeFactStoreIdentityChanged
            && diagnostic.severity == GovernanceDiagnosticSeverity::Error
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
