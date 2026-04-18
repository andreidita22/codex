use crate::compact::CurrentCompactRouteBehaviorForTests;
use crate::compact::InitialContextInjection;
use crate::compact::MaintenanceTimingForTests;
use crate::compact::current_compact_route_behavior_for_tests;
use crate::config::GovernancePathVariant;
use crate::context_maintenance::CurrentTurnBoundaryMaintenanceBehaviorForTests;
use crate::context_maintenance::TurnBoundaryMaintenanceActionForTests;
use crate::context_maintenance::current_turn_boundary_maintenance_behavior_for_tests;
use pretty_assertions::assert_eq;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedAction {
    Compact,
    Refresh,
    Prune,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedTiming {
    TurnBoundary,
    IntraTurn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedEngine {
    RemoteVanilla,
    RemoteHybrid,
    LocalPure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetRouteValidity {
    Supported,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TargetRouteFixture {
    action: ExpectedAction,
    timing: ExpectedTiming,
    engine: ExpectedEngine,
    validity: TargetRouteValidity,
    generates_thread_memory: bool,
    generates_continuation_bridge: bool,
}

fn target_route_fixture(
    action: ExpectedAction,
    timing: ExpectedTiming,
    engine: ExpectedEngine,
) -> TargetRouteFixture {
    match (action, timing, engine) {
        (ExpectedAction::Compact, ExpectedTiming::IntraTurn, ExpectedEngine::LocalPure)
        | (ExpectedAction::Compact, ExpectedTiming::IntraTurn, ExpectedEngine::RemoteHybrid) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Supported,
                generates_thread_memory: false,
                generates_continuation_bridge: true,
            }
        }
        (ExpectedAction::Compact, ExpectedTiming::IntraTurn, ExpectedEngine::RemoteVanilla) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Supported,
                generates_thread_memory: false,
                generates_continuation_bridge: false,
            }
        }
        (ExpectedAction::Compact, ExpectedTiming::TurnBoundary, ExpectedEngine::LocalPure)
        | (ExpectedAction::Compact, ExpectedTiming::TurnBoundary, ExpectedEngine::RemoteHybrid) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Supported,
                generates_thread_memory: true,
                generates_continuation_bridge: false,
            }
        }
        (ExpectedAction::Compact, ExpectedTiming::TurnBoundary, ExpectedEngine::RemoteVanilla) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Supported,
                generates_thread_memory: false,
                generates_continuation_bridge: false,
            }
        }
        (ExpectedAction::Refresh, ExpectedTiming::TurnBoundary, ExpectedEngine::LocalPure)
        | (ExpectedAction::Refresh, ExpectedTiming::TurnBoundary, ExpectedEngine::RemoteHybrid) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Supported,
                generates_thread_memory: true,
                generates_continuation_bridge: false,
            }
        }
        (ExpectedAction::Refresh, ExpectedTiming::TurnBoundary, ExpectedEngine::RemoteVanilla) => {
            TargetRouteFixture {
                action,
                timing,
                engine,
                validity: TargetRouteValidity::Invalid,
                generates_thread_memory: false,
                generates_continuation_bridge: false,
            }
        }
        (ExpectedAction::Prune, ExpectedTiming::TurnBoundary, _) => TargetRouteFixture {
            action,
            timing,
            engine,
            validity: TargetRouteValidity::Supported,
            generates_thread_memory: false,
            generates_continuation_bridge: false,
        },
        (ExpectedAction::Refresh, ExpectedTiming::IntraTurn, _)
        | (ExpectedAction::Prune, ExpectedTiming::IntraTurn, _) => TargetRouteFixture {
            action,
            timing,
            engine,
            validity: TargetRouteValidity::Invalid,
            generates_thread_memory: false,
            generates_continuation_bridge: false,
        },
    }
}

#[test]
fn target_matrix_marks_refresh_and_prune_intra_turn_routes_invalid() {
    for action in [ExpectedAction::Refresh, ExpectedAction::Prune] {
        for engine in [
            ExpectedEngine::RemoteVanilla,
            ExpectedEngine::RemoteHybrid,
            ExpectedEngine::LocalPure,
        ] {
            let fixture = target_route_fixture(action, ExpectedTiming::IntraTurn, engine);
            assert_eq!(fixture.validity, TargetRouteValidity::Invalid);
        }
    }
}

#[test]
fn target_matrix_marks_remote_vanilla_refresh_invalid() {
    let fixture = target_route_fixture(
        ExpectedAction::Refresh,
        ExpectedTiming::TurnBoundary,
        ExpectedEngine::RemoteVanilla,
    );
    assert_eq!(fixture.validity, TargetRouteValidity::Invalid);
    assert_eq!(fixture.generates_thread_memory, false);
    assert_eq!(fixture.generates_continuation_bridge, false);
}

#[test]
fn target_matrix_marks_local_pure_intra_turn_compact_as_bridge_only() {
    let fixture = target_route_fixture(
        ExpectedAction::Compact,
        ExpectedTiming::IntraTurn,
        ExpectedEngine::LocalPure,
    );

    assert_eq!(fixture.validity, TargetRouteValidity::Supported);
    assert_eq!(fixture.generates_thread_memory, false);
    assert_eq!(fixture.generates_continuation_bridge, true);
}

#[test]
fn current_compact_turn_boundary_route_matches_locked_behavior_for_strict_memory() {
    let behavior = current_compact_route_behavior_for_tests(
        GovernancePathVariant::StrictV1Shadow,
        MaintenanceTimingForTests::TurnBoundary,
    );
    let expected = CurrentCompactRouteBehaviorForTests {
        initial_context_injection: InitialContextInjection::DoNotInject,
        regenerates_thread_memory: true,
    };
    assert_eq!(behavior, expected);
}

#[test]
fn current_compact_intra_turn_route_matches_locked_behavior_for_strict_memory() {
    let behavior = current_compact_route_behavior_for_tests(
        GovernancePathVariant::StrictV1Shadow,
        MaintenanceTimingForTests::IntraTurn,
    );
    let expected = CurrentCompactRouteBehaviorForTests {
        initial_context_injection: InitialContextInjection::BeforeLastUserMessage,
        regenerates_thread_memory: false,
    };
    assert_eq!(behavior, expected);
}

#[test]
fn current_compact_turn_boundary_route_keeps_thread_memory_off_when_governance_is_off() {
    let behavior = current_compact_route_behavior_for_tests(
        GovernancePathVariant::Off,
        MaintenanceTimingForTests::TurnBoundary,
    );
    let expected = CurrentCompactRouteBehaviorForTests {
        initial_context_injection: InitialContextInjection::DoNotInject,
        regenerates_thread_memory: false,
    };
    assert_eq!(behavior, expected);
}

#[test]
fn current_refresh_behavior_is_a_known_delta_from_target_matrix() {
    let target = target_route_fixture(
        ExpectedAction::Refresh,
        ExpectedTiming::TurnBoundary,
        ExpectedEngine::LocalPure,
    );
    let current = current_turn_boundary_maintenance_behavior_for_tests(
        TurnBoundaryMaintenanceActionForTests::Refresh,
        GovernancePathVariant::StrictV1Shadow,
    );

    assert_eq!(target.validity, TargetRouteValidity::Supported);
    assert_eq!(target.generates_thread_memory, true);
    assert_eq!(target.generates_continuation_bridge, false);
    assert_eq!(
        current,
        CurrentTurnBoundaryMaintenanceBehaviorForTests {
            generates_thread_memory: true,
            generates_continuation_bridge: true,
            emits_prune_manifest: false,
        }
    );
}

#[test]
fn current_refresh_behavior_is_also_a_known_delta_for_remote_vanilla() {
    let target = target_route_fixture(
        ExpectedAction::Refresh,
        ExpectedTiming::TurnBoundary,
        ExpectedEngine::RemoteVanilla,
    );
    let current = current_turn_boundary_maintenance_behavior_for_tests(
        TurnBoundaryMaintenanceActionForTests::Refresh,
        GovernancePathVariant::StrictV1Shadow,
    );

    assert_eq!(target.validity, TargetRouteValidity::Invalid);
    assert_eq!(
        current,
        CurrentTurnBoundaryMaintenanceBehaviorForTests {
            generates_thread_memory: true,
            generates_continuation_bridge: true,
            emits_prune_manifest: false,
        }
    );
}

#[test]
fn current_prune_behavior_matches_locked_turn_boundary_behavior() {
    let target = target_route_fixture(
        ExpectedAction::Prune,
        ExpectedTiming::TurnBoundary,
        ExpectedEngine::LocalPure,
    );
    let current = current_turn_boundary_maintenance_behavior_for_tests(
        TurnBoundaryMaintenanceActionForTests::Prune,
        GovernancePathVariant::StrictV1Shadow,
    );

    assert_eq!(target.validity, TargetRouteValidity::Supported);
    assert_eq!(target.generates_thread_memory, false);
    assert_eq!(target.generates_continuation_bridge, false);
    assert_eq!(
        current,
        CurrentTurnBoundaryMaintenanceBehaviorForTests {
            generates_thread_memory: false,
            generates_continuation_bridge: false,
            emits_prune_manifest: true,
        }
    );
}
