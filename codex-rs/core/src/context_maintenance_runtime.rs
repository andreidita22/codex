use std::future::Future;

use crate::codex::TurnContext;
use crate::compact::InitialContextInjection;
use crate::config::CompactionEngine;
use crate::config::GovernancePathVariant;
use codex_context_maintenance_policy::ArtifactKind;
use codex_context_maintenance_policy::ArtifactRequest;
use codex_context_maintenance_policy::ArtifactRequiredness;
use codex_context_maintenance_policy::ContextInjectionPolicy;
use codex_context_maintenance_policy::GovernanceEffect;
use codex_context_maintenance_policy::HistoryDispositionPolicy;
use codex_context_maintenance_policy::HistoryDispositionRequest;
use codex_context_maintenance_policy::MaintenanceAction;
use codex_context_maintenance_policy::MaintenancePlanningRequest;
use codex_context_maintenance_policy::MaintenancePolicyError;
use codex_context_maintenance_policy::MaintenancePolicyPlan;
use codex_context_maintenance_policy::MaintenanceTiming;
use codex_context_maintenance_policy::PolicyEngine;
use codex_context_maintenance_policy::RetentionDirective;
use codex_context_maintenance_policy::ThreadMemoryGovernance;
use codex_context_maintenance_policy::plan_route;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use tracing::warn;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeMaintenancePlan {
    requested_artifacts: Vec<ArtifactRequest>,
    history_disposition: HistoryDispositionPolicy,
    context_injection: ContextInjectionPolicy,
    retention_directive: RetentionDirective,
    governance_effects: Vec<GovernanceEffect>,
}

impl RuntimeMaintenancePlan {
    pub(crate) fn artifact_request(&self, kind: ArtifactKind) -> Option<&ArtifactRequest> {
        self.requested_artifacts
            .iter()
            .find(|artifact| artifact.kind == kind)
    }

    #[cfg(test)]
    pub(crate) fn requests_artifact(&self, kind: ArtifactKind) -> bool {
        self.artifact_request(kind).is_some()
    }

    pub(crate) fn artifact_requiredness(&self, kind: ArtifactKind) -> Option<ArtifactRequiredness> {
        self.artifact_request(kind)
            .map(|artifact| artifact.requiredness)
    }

    pub(crate) fn history_disposition_request(
        &self,
        items: Vec<codex_protocol::models::ResponseItem>,
    ) -> HistoryDispositionRequest {
        HistoryDispositionRequest {
            items,
            policy: self.history_disposition.clone(),
        }
    }

    pub(crate) fn context_injection_placement(&self) -> InitialContextInjection {
        match self.context_injection {
            ContextInjectionPolicy::None => InitialContextInjection::DoNotInject,
            ContextInjectionPolicy::BeforeLastRealUserOrSummary => {
                InitialContextInjection::BeforeLastRealUserOrSummary
            }
        }
    }

    pub(crate) fn context_injection_policy(&self) -> ContextInjectionPolicy {
        self.context_injection
    }

    pub(crate) fn history_disposition_policy(&self) -> &HistoryDispositionPolicy {
        &self.history_disposition
    }

    pub(crate) fn retention_directive(&self) -> RetentionDirective {
        self.retention_directive
    }

    #[cfg(test)]
    pub(crate) fn governance_effects(&self) -> &[GovernanceEffect] {
        &self.governance_effects
    }
}

pub(crate) async fn execute_requested_artifact<T, F, Fut>(
    requiredness: Option<ArtifactRequiredness>,
    artifact_name: &str,
    operation: &str,
    generate: F,
) -> CodexResult<Option<T>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = CodexResult<Option<T>>>,
{
    match requiredness {
        Some(ArtifactRequiredness::Required) => match generate().await {
            Ok(Some(item)) => Ok(Some(item)),
            Ok(None) => Err(CodexErr::Fatal(format!(
                "required {artifact_name} generation returned empty output {operation}"
            ))),
            Err(err) => Err(CodexErr::Fatal(format!(
                "failed generating required {artifact_name} {operation}: {}",
                required_artifact_error_detail(err)
            ))),
        },
        Some(ArtifactRequiredness::BestEffort) => match generate().await {
            Ok(item) => Ok(item),
            Err(err) => {
                warn!("failed generating {artifact_name} {operation}: {err}");
                Ok(None)
            }
        },
        None => Ok(None),
    }
}

fn required_artifact_error_detail(err: CodexErr) -> String {
    match err {
        CodexErr::Fatal(message) => message,
        other => other.to_string(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactInvocationTiming {
    TurnBoundary,
    IntraTurn,
}

impl CompactInvocationTiming {
    fn maintenance_timing(self) -> MaintenanceTiming {
        match self {
            Self::TurnBoundary => MaintenanceTiming::TurnBoundary,
            Self::IntraTurn => MaintenanceTiming::IntraTurn,
        }
    }
}

#[cfg(test)]
pub(crate) fn live_compact_route_behavior_for_tests(
    engine: CompactionEngine,
    governance_variant: GovernancePathVariant,
    timing: CompactInvocationTiming,
) -> RuntimeMaintenancePlan {
    runtime_plan(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: timing.maintenance_timing(),
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
    timing: CompactInvocationTiming,
) -> CodexResult<RuntimeMaintenancePlan> {
    runtime_plan(MaintenancePlanningRequest {
        action: MaintenanceAction::Compact,
        timing: timing.maintenance_timing(),
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
        requested_artifacts: plan.requested_artifacts,
        history_disposition: plan.history_disposition,
        context_injection: plan.context_injection,
        retention_directive: plan.retention_directive,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn execute_requested_artifact_skips_generation_when_not_requested() {
        let called = Arc::new(AtomicBool::new(false));
        let called_for_closure = Arc::clone(&called);

        let result = execute_requested_artifact(None, "thread memory", "during test", move || {
            called_for_closure.store(true, Ordering::SeqCst);
            async { Ok(Some("unused")) }
        })
        .await
        .expect("missing request should skip generation");

        assert_eq!(result, None);
        assert_eq!(called.load(Ordering::SeqCst), false);
    }

    #[tokio::test]
    async fn execute_requested_artifact_fails_when_required_output_is_missing() {
        let err = execute_requested_artifact(
            Some(ArtifactRequiredness::Required),
            "thread memory",
            "during test",
            || async { Ok(None::<()>) },
        )
        .await
        .expect_err("required artifact should fail when generation returns no item");

        assert_eq!(
            err.to_string(),
            "Fatal error: required thread memory generation returned empty output during test"
        );
    }

    #[tokio::test]
    async fn execute_requested_artifact_ignores_best_effort_failures() {
        let result = execute_requested_artifact(
            Some(ArtifactRequiredness::BestEffort),
            "continuation bridge",
            "during test",
            || async { Err(CodexErr::Fatal("boom".to_string())) as CodexResult<Option<()>> },
        )
        .await
        .expect("best-effort artifact failures should not fail the runtime path");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn execute_requested_artifact_propagates_required_failures() {
        let err = execute_requested_artifact(
            Some(ArtifactRequiredness::Required),
            "continuation bridge",
            "during test",
            || async { Err(CodexErr::Fatal("boom".to_string())) as CodexResult<Option<()>> },
        )
        .await
        .expect_err("required artifact failures should be fatal");

        assert_eq!(
            err.to_string(),
            "Fatal error: failed generating required continuation bridge during test: boom"
        );
    }

    #[test]
    fn compact_invocation_timing_maps_to_maintenance_timing() {
        assert_eq!(
            CompactInvocationTiming::TurnBoundary.maintenance_timing(),
            MaintenanceTiming::TurnBoundary
        );
        assert_eq!(
            CompactInvocationTiming::IntraTurn.maintenance_timing(),
            MaintenanceTiming::IntraTurn
        );
    }

    #[test]
    fn compact_timing_drives_context_injection_placement() {
        let turn_boundary = live_compact_route_behavior_for_tests(
            CompactionEngine::LocalPure,
            GovernancePathVariant::StrictV1Shadow,
            CompactInvocationTiming::TurnBoundary,
        );
        let intra_turn = live_compact_route_behavior_for_tests(
            CompactionEngine::LocalPure,
            GovernancePathVariant::StrictV1Shadow,
            CompactInvocationTiming::IntraTurn,
        );

        assert_eq!(
            turn_boundary.context_injection_placement(),
            InitialContextInjection::DoNotInject
        );
        assert_eq!(
            intra_turn.context_injection_placement(),
            InitialContextInjection::BeforeLastRealUserOrSummary
        );
    }
}
