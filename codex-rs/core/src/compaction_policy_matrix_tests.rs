use crate::config::CompactionEngine;
use crate::config::GovernancePathVariant;
use crate::context_maintenance_runtime::CompactInvocationTiming;
use crate::context_maintenance_runtime::TurnBoundaryMaintenanceActionForTests;
use crate::context_maintenance_runtime::live_compact_route_behavior_for_tests;
use crate::context_maintenance_runtime::live_turn_boundary_maintenance_behavior_for_tests;
use crate::context_maintenance_runtime::try_live_turn_boundary_maintenance_behavior_for_tests;
use codex_context_maintenance_policy::ArtifactKind;
use codex_context_maintenance_policy::ArtifactRequiredness;
use codex_context_maintenance_policy::GovernanceEffect;
use codex_context_maintenance_policy::LegacyCompactionMarkerPolicy;
use codex_context_maintenance_policy::RetentionDirective;
use codex_context_maintenance_policy::RetentionGate;
use pretty_assertions::assert_eq;

#[test]
fn live_local_pure_intra_turn_compact_is_bridge_only() {
    let behavior = live_compact_route_behavior_for_tests(
        CompactionEngine::LocalPure,
        GovernancePathVariant::StrictV1Shadow,
        CompactInvocationTiming::IntraTurn,
    );

    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ThreadMemory),
        false
    );
    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ContinuationBridge),
        true
    );
    assert_eq!(
        behavior.artifact_requiredness(ArtifactKind::ContinuationBridge),
        Some(ArtifactRequiredness::BestEffort)
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .drop_prior_artifact_kinds,
        vec![ArtifactKind::ContinuationBridge]
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .legacy_compaction_marker_policy,
        LegacyCompactionMarkerPolicy::Strip
    );
    assert_eq!(behavior.retention_directive(), RetentionDirective::None);
}

#[test]
fn live_remote_hybrid_turn_boundary_compact_is_thread_memory_only() {
    let behavior = live_compact_route_behavior_for_tests(
        CompactionEngine::RemoteHybrid,
        GovernancePathVariant::StrictV1Shadow,
        CompactInvocationTiming::TurnBoundary,
    );

    assert_eq!(behavior.requests_artifact(ArtifactKind::ThreadMemory), true);
    assert_eq!(
        behavior.artifact_requiredness(ArtifactKind::ThreadMemory),
        Some(ArtifactRequiredness::Required)
    );
    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ContinuationBridge),
        false
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .drop_prior_artifact_kinds,
        vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge,]
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .legacy_compaction_marker_policy,
        LegacyCompactionMarkerPolicy::PreserveForUpstreamCompatibility
    );
    assert_eq!(
        behavior.retention_directive(),
        RetentionDirective::KeepRecentRawConversation {
            max_messages: 5,
            gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
        }
    );
}

#[test]
fn live_turn_boundary_compact_suppresses_thread_memory_when_governance_is_off() {
    let behavior = live_compact_route_behavior_for_tests(
        CompactionEngine::LocalPure,
        GovernancePathVariant::Off,
        CompactInvocationTiming::TurnBoundary,
    );

    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ThreadMemory),
        false
    );
    assert_eq!(
        behavior.governance_effects(),
        &[GovernanceEffect::ThreadMemorySuppressed]
    );
    assert_eq!(
        behavior.retention_directive(),
        RetentionDirective::KeepRecentRawConversation {
            max_messages: 5,
            gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
        }
    );
}

#[test]
fn live_refresh_is_thread_memory_only_for_supported_engines() {
    for engine in [CompactionEngine::LocalPure, CompactionEngine::RemoteHybrid] {
        let behavior = live_turn_boundary_maintenance_behavior_for_tests(
            engine,
            GovernancePathVariant::StrictV1Shadow,
            TurnBoundaryMaintenanceActionForTests::Refresh,
        );

        assert_eq!(behavior.requests_artifact(ArtifactKind::ThreadMemory), true);
        assert_eq!(
            behavior.artifact_requiredness(ArtifactKind::ThreadMemory),
            Some(ArtifactRequiredness::Required)
        );
        assert_eq!(
            behavior.requests_artifact(ArtifactKind::ContinuationBridge),
            false
        );
        assert_eq!(
            behavior.requests_artifact(ArtifactKind::PruneManifest),
            false
        );
        assert_eq!(
            behavior
                .history_disposition_policy()
                .drop_prior_artifact_kinds,
            vec![ArtifactKind::ThreadMemory, ArtifactKind::ContinuationBridge,]
        );
        assert_eq!(
            behavior
                .history_disposition_policy()
                .legacy_compaction_marker_policy,
            LegacyCompactionMarkerPolicy::Strip
        );
        assert_eq!(
            behavior.retention_directive(),
            RetentionDirective::KeepRecentRawConversation {
                max_messages: 5,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            }
        );
    }
}

#[test]
fn live_remote_vanilla_refresh_fails_closed() {
    let err = try_live_turn_boundary_maintenance_behavior_for_tests(
        CompactionEngine::RemoteVanilla,
        GovernancePathVariant::StrictV1Shadow,
        TurnBoundaryMaintenanceActionForTests::Refresh,
    )
    .expect_err("remote vanilla refresh should be unsupported");

    assert_eq!(
        err.to_string(),
        "Fatal error: Unsupported context-maintenance route: unsupported context-maintenance route: action=Refresh timing=TurnBoundary engine=RemoteVanilla"
    );
}

#[test]
fn live_prune_is_manifest_only_and_drops_turn_scoped_bridge() {
    let behavior = live_turn_boundary_maintenance_behavior_for_tests(
        CompactionEngine::RemoteHybrid,
        GovernancePathVariant::StrictV1Shadow,
        TurnBoundaryMaintenanceActionForTests::Prune,
    );

    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ThreadMemory),
        false
    );
    assert_eq!(
        behavior.requests_artifact(ArtifactKind::ContinuationBridge),
        false
    );
    assert_eq!(
        behavior.requests_artifact(ArtifactKind::PruneManifest),
        true
    );
    assert_eq!(
        behavior.artifact_requiredness(ArtifactKind::PruneManifest),
        Some(ArtifactRequiredness::Required)
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .drop_prior_artifact_kinds,
        vec![
            ArtifactKind::PruneManifest,
            ArtifactKind::ContinuationBridge,
        ]
    );
    assert_eq!(
        behavior
            .history_disposition_policy()
            .legacy_compaction_marker_policy,
        LegacyCompactionMarkerPolicy::Strip
    );
    assert_eq!(
        behavior.retention_directive(),
        RetentionDirective::KeepRecentRawConversation {
            max_messages: 5,
            gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
        }
    );
}
