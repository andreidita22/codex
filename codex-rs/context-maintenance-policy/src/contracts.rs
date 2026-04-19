use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceAction {
    Compact,
    Refresh,
    Prune,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceTiming {
    TurnBoundary,
    IntraTurn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyEngine {
    RemoteVanilla,
    RemoteHybrid,
    LocalPure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextInjectionPolicy {
    None,
    BeforeLastRealUserOrSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactLifetime {
    TurnScoped,
    DurableAcrossTurns,
    MarkerOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    ThreadMemory,
    ContinuationBridge,
    PruneManifest,
    CompactionMarker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactRequest {
    pub kind: ArtifactKind,
    pub lifetime: ArtifactLifetime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCompactionMarkerPolicy {
    Preserve,
    PreserveForUpstreamCompatibility,
    Strip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceEffect {
    ThreadMemorySuppressed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadMemoryGovernance {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenancePlanningRequest {
    pub action: MaintenanceAction,
    pub timing: MaintenanceTiming,
    pub engine: PolicyEngine,
    pub thread_memory_governance: ThreadMemoryGovernance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenancePolicyPlan {
    pub context_injection: ContextInjectionPolicy,
    pub requested_artifacts: Vec<ArtifactRequest>,
    pub drop_prior_artifact_kinds: Vec<ArtifactKind>,
    pub legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
    pub governance_effects: Vec<GovernanceEffect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MaintenancePolicyError {
    #[error(
        "unsupported context-maintenance route: action={action:?} timing={timing:?} engine={engine:?}"
    )]
    UnsupportedRoute {
        action: MaintenanceAction,
        timing: MaintenanceTiming,
        engine: PolicyEngine,
    },
}
