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
use crate::ThreadMemoryGovernance;

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaseRoutePlan {
    context_injection: ContextInjectionPolicy,
    requested_artifacts: Vec<ArtifactRequest>,
    drop_prior_artifact_kinds: Vec<ArtifactKind>,
    legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
}

pub fn plan_route(
    request: MaintenancePlanningRequest,
) -> Result<MaintenancePolicyPlan, MaintenancePolicyError> {
    let base_plan = select_base_route_plan(request.action, request.timing, request.engine)?;
    Ok(apply_governance_overlay(
        base_plan,
        request.thread_memory_governance,
    ))
}

fn select_base_route_plan(
    action: MaintenanceAction,
    timing: MaintenanceTiming,
    engine: PolicyEngine,
) -> Result<BaseRoutePlan, MaintenancePolicyError> {
    match (action, timing, engine) {
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ContinuationBridge,
                    ArtifactLifetime::TurnScoped,
                    ArtifactRequiredness::BestEffort,
                )],
                drop_prior_artifact_kinds: vec![ArtifactKind::ContinuationBridge],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::RemoteHybrid) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ContinuationBridge,
                    ArtifactLifetime::TurnScoped,
                    ArtifactRequiredness::BestEffort,
                )],
                drop_prior_artifact_kinds: vec![ArtifactKind::ContinuationBridge],
                legacy_compaction_marker_policy:
                    LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::RemoteVanilla) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![],
                drop_prior_artifact_kinds: vec![ArtifactKind::ContinuationBridge],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::TurnBoundary, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::None,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ThreadMemory,
                    ArtifactLifetime::DurableAcrossTurns,
                    ArtifactRequiredness::Required,
                )],
                drop_prior_artifact_kinds: vec![
                    ArtifactKind::ThreadMemory,
                    ArtifactKind::ContinuationBridge,
                ],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            })
        }
        (
            MaintenanceAction::Compact,
            MaintenanceTiming::TurnBoundary,
            PolicyEngine::RemoteHybrid,
        ) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![artifact(
                ArtifactKind::ThreadMemory,
                ArtifactLifetime::DurableAcrossTurns,
                ArtifactRequiredness::Required,
            )],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy:
                LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility,
        }),
        (
            MaintenanceAction::Compact,
            MaintenanceTiming::TurnBoundary,
            PolicyEngine::RemoteVanilla,
        ) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
        }),
        (MaintenanceAction::Refresh, MaintenanceTiming::TurnBoundary, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::None,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ThreadMemory,
                    ArtifactLifetime::DurableAcrossTurns,
                    ArtifactRequiredness::Required,
                )],
                drop_prior_artifact_kinds: vec![
                    ArtifactKind::ThreadMemory,
                    ArtifactKind::ContinuationBridge,
                ],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            })
        }
        (
            MaintenanceAction::Refresh,
            MaintenanceTiming::TurnBoundary,
            PolicyEngine::RemoteHybrid,
        ) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![artifact(
                ArtifactKind::ThreadMemory,
                ArtifactLifetime::DurableAcrossTurns,
                ArtifactRequiredness::Required,
            )],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::ThreadMemory,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
        }),
        (MaintenanceAction::Prune, MaintenanceTiming::TurnBoundary, _) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![artifact(
                ArtifactKind::PruneManifest,
                ArtifactLifetime::MarkerOnly,
                ArtifactRequiredness::Required,
            )],
            drop_prior_artifact_kinds: vec![
                ArtifactKind::PruneManifest,
                ArtifactKind::ContinuationBridge,
            ],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
        }),
        _ => Err(MaintenancePolicyError::UnsupportedRoute {
            action,
            timing,
            engine,
        }),
    }
}

fn apply_governance_overlay(
    mut base_plan: BaseRoutePlan,
    thread_memory_governance: ThreadMemoryGovernance,
) -> MaintenancePolicyPlan {
    let mut governance_effects = Vec::new();

    if thread_memory_governance == ThreadMemoryGovernance::Disabled
        && remove_artifact(
            &mut base_plan.requested_artifacts,
            ArtifactKind::ThreadMemory,
        )
    {
        governance_effects.push(GovernanceEffect::ThreadMemorySuppressed);
    }

    MaintenancePolicyPlan {
        context_injection: base_plan.context_injection,
        requested_artifacts: base_plan.requested_artifacts,
        drop_prior_artifact_kinds: base_plan.drop_prior_artifact_kinds,
        legacy_compaction_marker_policy: base_plan.legacy_compaction_marker_policy,
        governance_effects,
    }
}

fn artifact(
    kind: ArtifactKind,
    lifetime: ArtifactLifetime,
    requiredness: ArtifactRequiredness,
) -> ArtifactRequest {
    ArtifactRequest {
        kind,
        lifetime,
        requiredness,
    }
}

fn remove_artifact(artifacts: &mut Vec<ArtifactRequest>, kind: ArtifactKind) -> bool {
    let original_len = artifacts.len();
    artifacts.retain(|artifact| artifact.kind != kind);
    artifacts.len() != original_len
}
