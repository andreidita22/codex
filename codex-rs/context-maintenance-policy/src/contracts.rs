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
pub enum ArtifactRequiredness {
    Required,
    BestEffort,
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
    pub requiredness: ArtifactRequiredness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCompactionMarkerPolicy {
    Preserve,
    PreserveForUpstreamCompatibility,
    Strip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryDispositionPolicy {
    KeepAll,
    KeepLatestCompactionSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteCompactedHistoryKeepPolicy {
    KeepAll,
    DropDeveloperAndNonTurnUserMessages,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryDispositionPolicy {
    pub prune_superseded_artifacts: bool,
    pub summary_disposition: SummaryDispositionPolicy,
    pub remote_keep_policy: RemoteCompactedHistoryKeepPolicy,
    pub drop_prior_artifact_kinds: Vec<ArtifactKind>,
    pub legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryDispositionRequest {
    pub items: Vec<codex_protocol::models::ResponseItem>,
    pub policy: HistoryDispositionPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryDispositionResult {
    pub items: Vec<codex_protocol::models::ResponseItem>,
    pub removed_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceEffect {
    ThreadMemorySuppressed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionGate {
    Always,
    FinalHistoryContainsArtifact(ArtifactKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionDirective {
    None,
    KeepRecentRawConversation {
        max_messages: usize,
        gate: RetentionGate,
    },
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
    pub history_disposition: HistoryDispositionPolicy,
    pub retention_directive: RetentionDirective,
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
