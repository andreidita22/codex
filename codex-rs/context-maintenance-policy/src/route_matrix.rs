use codex_config::config_toml::GovernancePathVariant;

use crate::ArtifactKind;
use crate::ArtifactLifetime;
use crate::ArtifactRequest;
use crate::ContextInjectionPolicy;
use crate::GovernanceEffect;
use crate::LegacyCompactionMarkerPolicy;
use crate::MaintenanceAction;
use crate::MaintenancePlanningRequest;
use crate::MaintenancePolicyError;
use crate::MaintenancePolicyPlan;
use crate::MaintenanceTiming;
use crate::PolicyEngine;

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaseRoutePlan {
    context_injection: ContextInjectionPolicy,
    requested_artifacts: Vec<ArtifactRequest>,
    legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
}

pub fn plan_route(
    request: MaintenancePlanningRequest,
) -> Result<MaintenancePolicyPlan, MaintenancePolicyError> {
    let base_plan = select_base_route_plan(request.action, request.timing, request.engine)?;
    Ok(apply_governance_overlay(
        base_plan,
        request.governance_variant,
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
                )],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::RemoteHybrid) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ContinuationBridge,
                    ArtifactLifetime::TurnScoped,
                )],
                legacy_compaction_marker_policy:
                    LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::RemoteVanilla) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::TurnBoundary, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::None,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ThreadMemory,
                    ArtifactLifetime::DurableAcrossTurns,
                )],
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
            )],
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
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
        }),
        (MaintenanceAction::Refresh, MaintenanceTiming::TurnBoundary, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::None,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ThreadMemory,
                    ArtifactLifetime::DurableAcrossTurns,
                )],
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
            )],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
        }),
        (MaintenanceAction::Prune, MaintenanceTiming::TurnBoundary, _) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![artifact(
                ArtifactKind::PruneManifest,
                ArtifactLifetime::MarkerOnly,
            )],
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
    governance_variant: GovernancePathVariant,
) -> MaintenancePolicyPlan {
    let mut governance_effects = Vec::new();

    if governance_variant == GovernancePathVariant::Off
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
        legacy_compaction_marker_policy: base_plan.legacy_compaction_marker_policy,
        governance_effects,
    }
}

fn artifact(kind: ArtifactKind, lifetime: ArtifactLifetime) -> ArtifactRequest {
    ArtifactRequest { kind, lifetime }
}

fn remove_artifact(artifacts: &mut Vec<ArtifactRequest>, kind: ArtifactKind) -> bool {
    let original_len = artifacts.len();
    artifacts.retain(|artifact| artifact.kind != kind);
    artifacts.len() != original_len
}
