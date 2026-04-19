use crate::ArtifactKind;
use crate::ArtifactLifetime;
use crate::ArtifactRequest;
use crate::ArtifactRequiredness;
use crate::ContextInjectionPolicy;
use crate::GovernanceEffect;
use crate::HistoryDispositionPolicy;
use crate::LegacyCompactionMarkerPolicy;
use crate::MaintenanceAction;
use crate::MaintenancePlanningRequest;
use crate::MaintenancePolicyError;
use crate::MaintenancePolicyPlan;
use crate::MaintenanceTiming;
use crate::PolicyEngine;
use crate::RemoteCompactedHistoryKeepPolicy;
use crate::RetentionDirective;
use crate::RetentionGate;
use crate::SummaryDispositionPolicy;
use crate::ThreadMemoryGovernance;

const RECENT_RAW_CONVERSATION_WINDOW_MESSAGES: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaseRoutePlan {
    context_injection: ContextInjectionPolicy,
    requested_artifacts: Vec<ArtifactRequest>,
    history_disposition: HistoryDispositionPolicy,
    retention_directive: RetentionDirective,
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
                history_disposition: history_disposition(
                    /*prune_superseded_artifacts*/ false,
                    SummaryDispositionPolicy::KeepAll,
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                    vec![ArtifactKind::ContinuationBridge],
                    LegacyCompactionMarkerPolicy::Strip,
                ),
                retention_directive: RetentionDirective::None,
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
                history_disposition: history_disposition(
                    /*prune_superseded_artifacts*/ false,
                    SummaryDispositionPolicy::KeepAll,
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                    vec![ArtifactKind::ContinuationBridge],
                    LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility,
                ),
                retention_directive: RetentionDirective::None,
            })
        }
        (MaintenanceAction::Compact, MaintenanceTiming::IntraTurn, PolicyEngine::RemoteVanilla) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
                requested_artifacts: vec![],
                history_disposition: history_disposition(
                    /*prune_superseded_artifacts*/ false,
                    SummaryDispositionPolicy::KeepAll,
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                    vec![ArtifactKind::ContinuationBridge],
                    LegacyCompactionMarkerPolicy::Preserve,
                ),
                retention_directive: RetentionDirective::None,
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
                history_disposition: history_disposition(
                    /*prune_superseded_artifacts*/ false,
                    SummaryDispositionPolicy::KeepAll,
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                    vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge],
                    LegacyCompactionMarkerPolicy::Strip,
                ),
                retention_directive: recent_raw_retention_when_thread_memory_present(),
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
            history_disposition: history_disposition(
                /*prune_superseded_artifacts*/ false,
                SummaryDispositionPolicy::KeepAll,
                RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge],
                LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility,
            ),
            retention_directive: recent_raw_retention_when_thread_memory_present(),
        }),
        (
            MaintenanceAction::Compact,
            MaintenanceTiming::TurnBoundary,
            PolicyEngine::RemoteVanilla,
        ) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![],
            history_disposition: history_disposition(
                /*prune_superseded_artifacts*/ false,
                SummaryDispositionPolicy::KeepAll,
                RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge],
                LegacyCompactionMarkerPolicy::Preserve,
            ),
            retention_directive: RetentionDirective::None,
        }),
        (MaintenanceAction::Refresh, MaintenanceTiming::TurnBoundary, PolicyEngine::LocalPure) => {
            Ok(BaseRoutePlan {
                context_injection: ContextInjectionPolicy::None,
                requested_artifacts: vec![artifact(
                    ArtifactKind::ThreadMemory,
                    ArtifactLifetime::DurableAcrossTurns,
                    ArtifactRequiredness::Required,
                )],
                history_disposition: history_disposition(
                    /*prune_superseded_artifacts*/ true,
                    SummaryDispositionPolicy::KeepAll,
                    RemoteCompactedHistoryKeepPolicy::KeepAll,
                    vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge],
                    LegacyCompactionMarkerPolicy::Strip,
                ),
                retention_directive: recent_raw_retention_when_thread_memory_present(),
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
            history_disposition: history_disposition(
                /*prune_superseded_artifacts*/ true,
                SummaryDispositionPolicy::KeepAll,
                RemoteCompactedHistoryKeepPolicy::KeepAll,
                vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge],
                LegacyCompactionMarkerPolicy::Strip,
            ),
            retention_directive: recent_raw_retention_when_thread_memory_present(),
        }),
        (MaintenanceAction::Prune, MaintenanceTiming::TurnBoundary, _) => Ok(BaseRoutePlan {
            context_injection: ContextInjectionPolicy::None,
            requested_artifacts: vec![artifact(
                ArtifactKind::PruneManifest,
                ArtifactLifetime::MarkerOnly,
                ArtifactRequiredness::Required,
            )],
            history_disposition: history_disposition(
                /*prune_superseded_artifacts*/ true,
                SummaryDispositionPolicy::KeepLatestCompactionSummary,
                RemoteCompactedHistoryKeepPolicy::KeepAll,
                vec![
                    ArtifactKind::PruneManifest,
                    ArtifactKind::ContinuationBridge,
                ],
                LegacyCompactionMarkerPolicy::Strip,
            ),
            retention_directive: recent_raw_retention_when_thread_memory_present(),
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
        history_disposition: base_plan.history_disposition,
        retention_directive: base_plan.retention_directive,
        governance_effects,
    }
}

fn history_disposition(
    prune_superseded_artifacts: bool,
    summary_disposition: SummaryDispositionPolicy,
    remote_keep_policy: RemoteCompactedHistoryKeepPolicy,
    drop_prior_artifact_kinds: Vec<ArtifactKind>,
    legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
) -> HistoryDispositionPolicy {
    HistoryDispositionPolicy {
        prune_superseded_artifacts,
        summary_disposition,
        remote_keep_policy,
        drop_prior_artifact_kinds,
        legacy_compaction_marker_policy,
    }
}

fn recent_raw_retention_when_thread_memory_present() -> RetentionDirective {
    RetentionDirective::KeepRecentRawConversation {
        max_messages: RECENT_RAW_CONVERSATION_WINDOW_MESSAGES,
        gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
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
