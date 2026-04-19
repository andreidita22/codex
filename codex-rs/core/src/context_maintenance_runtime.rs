use crate::codex::TurnContext;
use crate::compact::InitialContextInjection;
use crate::config::CompactionEngine;
use crate::config::GovernancePathVariant;
use codex_context_maintenance_policy::ArtifactKind;
use codex_context_maintenance_policy::ContextInjectionPolicy;
use codex_context_maintenance_policy::GovernanceEffect;
use codex_context_maintenance_policy::LegacyCompactionMarkerPolicy;
use codex_context_maintenance_policy::MaintenanceAction;
use codex_context_maintenance_policy::MaintenancePlanningRequest;
use codex_context_maintenance_policy::MaintenancePolicyError;
use codex_context_maintenance_policy::MaintenancePolicyPlan;
use codex_context_maintenance_policy::MaintenanceTiming;
use codex_context_maintenance_policy::PolicyEngine;
use codex_context_maintenance_policy::ThreadMemoryGovernance;
use codex_context_maintenance_policy::plan_route;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeMaintenancePlan {
    requested_artifacts: Vec<ArtifactKind>,
    drop_prior_artifact_kinds: Vec<ArtifactKind>,
    context_injection: ContextInjectionPolicy,
    legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
    governance_effects: Vec<GovernanceEffect>,
}

impl RuntimeMaintenancePlan {
    pub(crate) fn requests_artifact(&self, kind: ArtifactKind) -> bool {
        self.requested_artifacts.contains(&kind)
    }

    #[cfg(test)]
    pub(crate) fn drops_prior_artifact(&self, kind: ArtifactKind) -> bool {
        self.drop_prior_artifact_kinds.contains(&kind)
    }

    pub(crate) fn drop_prior_artifact_kinds(&self) -> &[ArtifactKind] {
        &self.drop_prior_artifact_kinds
    }

    pub(crate) fn initial_context_injection(&self) -> InitialContextInjection {
        match self.context_injection {
            ContextInjectionPolicy::None => InitialContextInjection::DoNotInject,
            ContextInjectionPolicy::BeforeLastRealUserOrSummary => {
                InitialContextInjection::BeforeLastUserMessage
            }
        }
    }

    pub(crate) fn context_injection_policy(&self) -> ContextInjectionPolicy {
        self.context_injection
    }

    pub(crate) fn preserves_legacy_compaction_marker(&self) -> bool {
        !matches!(
            self.legacy_compaction_marker_policy,
            LegacyCompactionMarkerPolicy::Strip
        )
    }

    #[cfg(test)]
    pub(crate) fn governance_effects(&self) -> &[GovernanceEffect] {
        &self.governance_effects
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactTimingForTests {
    TurnBoundary,
    IntraTurn,
}

#[cfg(test)]
pub(crate) fn live_compact_route_behavior_for_tests(
    engine: CompactionEngine,
    governance_variant: GovernancePathVariant,
    timing: CompactTimingForTests,
) -> RuntimeMaintenancePlan {
    runtime_plan(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: match timing {
            CompactTimingForTests::TurnBoundary => MaintenanceTiming::TurnBoundary,
            CompactTimingForTests::IntraTurn => MaintenanceTiming::IntraTurn,
        },
        engine: policy_engine(engine),
        thread_memory_governance: policy_thread_memory_governance(governance_variant),
    })
    .expect("compact test route should plan")
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TurnBoundaryMaintenanceActionForTests {
    Refresh,
    Prune,
}

#[cfg(test)]
pub(crate) fn live_turn_boundary_maintenance_behavior_for_tests(
    engine: CompactionEngine,
    governance_variant: GovernancePathVariant,
    action: TurnBoundaryMaintenanceActionForTests,
) -> RuntimeMaintenancePlan {
    try_live_turn_boundary_maintenance_behavior_for_tests(engine, governance_variant, action)
        .expect("turn-boundary maintenance test route should plan")
}

#[cfg(test)]
pub(crate) fn try_live_turn_boundary_maintenance_behavior_for_tests(
    engine: CompactionEngine,
    governance_variant: GovernancePathVariant,
    action: TurnBoundaryMaintenanceActionForTests,
) -> CodexResult<RuntimeMaintenancePlan> {
    runtime_plan(MaintenancePlanningRequest {
        action: match action {
            TurnBoundaryMaintenanceActionForTests::Refresh => MaintenanceAction::Refresh,
            TurnBoundaryMaintenanceActionForTests::Prune => MaintenanceAction::Prune,
        },
        timing: MaintenanceTiming::TurnBoundary,
        engine: policy_engine(engine),
        thread_memory_governance: policy_thread_memory_governance(governance_variant),
    })
}

pub(crate) fn runtime_plan_for_compact(
    turn_context: &TurnContext,
    timing: MaintenanceTiming,
) -> CodexResult<RuntimeMaintenancePlan> {
    runtime_plan(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing,
        engine: policy_engine(compaction_engine(turn_context)),
        thread_memory_governance: policy_thread_memory_governance(
            turn_context.governance_path_variant(),
        ),
    })
}

pub(crate) fn runtime_plan_for_turn_boundary_maintenance(
    turn_context: &TurnContext,
    action: MaintenanceAction,
) -> CodexResult<RuntimeMaintenancePlan> {
    runtime_plan(MaintenancePlanningRequest {
        action,
        timing: MaintenanceTiming::TurnBoundary,
        engine: policy_engine(compaction_engine(turn_context)),
        thread_memory_governance: policy_thread_memory_governance(
            turn_context.governance_path_variant(),
        ),
    })
}

pub(crate) fn compact_timing_from_initial_context_injection(
    initial_context_injection: InitialContextInjection,
) -> MaintenanceTiming {
    match initial_context_injection {
        InitialContextInjection::DoNotInject => MaintenanceTiming::TurnBoundary,
        InitialContextInjection::BeforeLastUserMessage => MaintenanceTiming::IntraTurn,
    }
}

pub(crate) fn compaction_engine(turn_context: &TurnContext) -> CompactionEngine {
    turn_context.config.compaction_engine.unwrap_or_default()
}

fn runtime_plan(request: MaintenancePlanningRequest) -> CodexResult<RuntimeMaintenancePlan> {
    plan_route(request)
        .map(runtime_plan_from_policy_plan)
        .map_err(unsupported_route_error)
}

fn runtime_plan_from_policy_plan(plan: MaintenancePolicyPlan) -> RuntimeMaintenancePlan {
    RuntimeMaintenancePlan {
        requested_artifacts: plan
            .requested_artifacts
            .into_iter()
            .map(|artifact| artifact.kind)
            .collect(),
        drop_prior_artifact_kinds: plan.drop_prior_artifact_kinds,
        context_injection: plan.context_injection,
        legacy_compaction_marker_policy: plan.legacy_compaction_marker_policy,
        governance_effects: plan.governance_effects,
    }
}

fn unsupported_route_error(err: MaintenancePolicyError) -> CodexErr {
    CodexErr::Fatal(format!("Unsupported context-maintenance route: {err}"))
}

fn policy_engine(engine: CompactionEngine) -> PolicyEngine {
    match engine {
        CompactionEngine::RemoteVanilla => PolicyEngine::RemoteVanilla,
        CompactionEngine::RemoteHybrid => PolicyEngine::RemoteHybrid,
        CompactionEngine::LocalPure => PolicyEngine::LocalPure,
    }
}

fn policy_thread_memory_governance(variant: GovernancePathVariant) -> ThreadMemoryGovernance {
    match variant {
        GovernancePathVariant::Off => ThreadMemoryGovernance::Disabled,
        GovernancePathVariant::StrictV1Shadow => ThreadMemoryGovernance::Enabled,
        GovernancePathVariant::StrictV1Enforce => ThreadMemoryGovernance::Enabled,
    }
}
