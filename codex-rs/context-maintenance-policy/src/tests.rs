use pretty_assertions::assert_eq;

use crate::ArtifactKind;
use crate::ArtifactLifetime;
use crate::ArtifactRequest;
use crate::ArtifactRequiredness;
use crate::ContextInjectionPolicy;
use crate::GovernanceEffect;
use crate::LegacyCompactionMarkerPolicy;
use crate::MaintenanceAction;
use crate::MaintenancePlanningRequest;
use crate::MaintenancePolicyError;
use crate::MaintenancePolicyPlan;
use crate::MaintenanceTiming;
use crate::PolicyEngine;
use crate::RetentionDirective;
use crate::RetentionGate;
use crate::ThreadMemoryGovernance;
use crate::plan_route;

#[test]
fn compact_intra_turn_local_pure_requests_turn_scoped_bridge() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: MaintenanceTiming::IntraTurn,
        engine: PolicyEngine::LocalPure,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    })
    .expect("local pure intra-turn compact should be supported");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
            requested_artifacts: vec![ArtifactRequest {
                kind: ArtifactKind::ContinuationBridge,
                lifetime: ArtifactLifetime::TurnScoped,
                requiredness: ArtifactRequiredness::BestEffort,
            }],
            drop_prior_artifact_kinds: vec![ArtifactKind::ContinuationBridge],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            retention_directive: RetentionDirective::None,
            governance_effects: vec![],
        }
    );
}

#[test]
fn compact_turn_boundary_local_pure_requests_durable_thread_memory() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::LocalPure,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    })
    .expect("local pure turn-boundary compact should be supported");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![ArtifactRequest {
                kind: ArtifactKind::ThreadMemory,
                lifetime: ArtifactLifetime::DurableAcrossTurns,
                requiredness: ArtifactRequiredness::Required,
            }],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            retention_directive: RetentionDirective::KeepRecentRawConversation {
                max_messages: 5,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            governance_effects: vec![],
        }
    );
}

#[test]
fn compact_turn_boundary_governance_off_suppresses_thread_memory() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::LocalPure,
        thread_memory_governance: ThreadMemoryGovernance::Disabled,
    })
    .expect("governance off should still support local pure compact");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            retention_directive: RetentionDirective::KeepRecentRawConversation {
                max_messages: 5,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            governance_effects: vec![GovernanceEffect::ThreadMemorySuppressed],
        }
    );
}

#[test]
fn compact_turn_boundary_remote_vanilla_preserves_legacy_marker_without_fork_artifacts() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::RemoteVanilla,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    })
    .expect("remote vanilla turn-boundary compact should be supported");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            retention_directive: RetentionDirective::None,
            governance_effects: vec![],
        }
    );
}

#[test]
fn refresh_turn_boundary_local_pure_requests_durable_thread_memory() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Refresh,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::LocalPure,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    })
    .expect("local pure refresh should be supported");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![ArtifactRequest {
                kind: ArtifactKind::ThreadMemory,
                lifetime: ArtifactLifetime::DurableAcrossTurns,
                requiredness: ArtifactRequiredness::Required,
            }],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            retention_directive: RetentionDirective::KeepRecentRawConversation {
                max_messages: 5,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            governance_effects: vec![],
        }
    );
}

#[test]
fn refresh_turn_boundary_remote_vanilla_is_unsupported() {
    let result = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Refresh,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::RemoteVanilla,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    });

    assert_eq!(
        result,
        Err(MaintenancePolicyError::UnsupportedRoute {
            action: MaintenanceAction::Refresh,
            timing: MaintenanceTiming::TurnBoundary,
            engine: PolicyEngine::RemoteVanilla,
        })
    );
}

#[test]
fn refresh_intra_turn_is_unsupported() {
    let result = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Refresh,
        timing: MaintenanceTiming::IntraTurn,
        engine: PolicyEngine::LocalPure,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    });

    assert_eq!(
        result,
        Err(MaintenancePolicyError::UnsupportedRoute {
            action: MaintenanceAction::Refresh,
            timing: MaintenanceTiming::IntraTurn,
            engine: PolicyEngine::LocalPure,
        })
    );
}

#[test]
fn prune_turn_boundary_requests_marker_only_prune_manifest() {
    let plan = plan_route(MaintenancePlanningRequest {
        action: MaintenanceAction::Prune,
        timing: MaintenanceTiming::TurnBoundary,
        engine: PolicyEngine::RemoteHybrid,
        thread_memory_governance: ThreadMemoryGovernance::Enabled,
    })
    .expect("turn-boundary prune should be supported");

    assert_eq!(
        plan,
        MaintenancePolicyPlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![ArtifactRequest {
                kind: ArtifactKind::PruneManifest,
                lifetime: ArtifactLifetime::MarkerOnly,
                requiredness: ArtifactRequiredness::Required,
            }],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::PruneManifest,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            retention_directive: RetentionDirective::KeepRecentRawConversation {
                max_messages: 5,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            governance_effects: vec![],
        }
    );
}
