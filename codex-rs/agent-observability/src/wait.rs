use crate::AgentProgressPhase;
use crate::AgentProgressSnapshot;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitObservationMoment {
    Initial,
    AfterProgress,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaitForAgentProgressMatchReason {
    AlreadySatisfied,
    SeqAdvanced,
    PhaseMatched,
    TimedOut,
}

pub fn classify_wait_observation(
    snapshot: &AgentProgressSnapshot,
    baseline_seq: u64,
    until_phases: &[AgentProgressPhase],
    moment: WaitObservationMoment,
) -> Option<WaitForAgentProgressMatchReason> {
    if snapshot.seq() > baseline_seq {
        Some(WaitForAgentProgressMatchReason::SeqAdvanced)
    } else if until_phases.contains(&snapshot.phase()) {
        Some(match moment {
            WaitObservationMoment::Initial => WaitForAgentProgressMatchReason::AlreadySatisfied,
            WaitObservationMoment::AfterProgress => WaitForAgentProgressMatchReason::PhaseMatched,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentProgressPhase;
    use codex_protocol::protocol::AgentStatus;
    use pretty_assertions::assert_eq;

    fn snapshot(seq: u64, phase: AgentProgressPhase) -> AgentProgressSnapshot {
        AgentProgressSnapshot {
            lifecycle_status: AgentStatus::Running,
            phase,
            blocked_on: None,
            active_work: None,
            recent_updates: Vec::new(),
            latest_visible_message: None,
            final_message: None,
            error_message: None,
            ever_entered_turn: true,
            ever_reported_progress: true,
            last_progress_age_ms: Some(10),
            seq,
            stalled: false,
        }
    }

    #[test]
    fn classify_wait_observation_prefers_seq_advanced_over_phase_match() {
        let snapshot = snapshot(2, AgentProgressPhase::WaitingApproval);

        assert_eq!(
            classify_wait_observation(
                &snapshot,
                1,
                &[AgentProgressPhase::WaitingApproval],
                WaitObservationMoment::AfterProgress,
            ),
            Some(WaitForAgentProgressMatchReason::SeqAdvanced)
        );
    }

    #[test]
    fn classify_wait_observation_returns_already_satisfied_for_initial_phase_match() {
        let snapshot = snapshot(3, AgentProgressPhase::WaitingUserInput);

        assert_eq!(
            classify_wait_observation(
                &snapshot,
                3,
                &[AgentProgressPhase::WaitingUserInput],
                WaitObservationMoment::Initial,
            ),
            Some(WaitForAgentProgressMatchReason::AlreadySatisfied)
        );
    }

    #[test]
    fn classify_wait_observation_returns_phase_matched_after_progress_observation() {
        let snapshot = snapshot(3, AgentProgressPhase::WaitingUserInput);

        assert_eq!(
            classify_wait_observation(
                &snapshot,
                3,
                &[AgentProgressPhase::WaitingUserInput],
                WaitObservationMoment::AfterProgress,
            ),
            Some(WaitForAgentProgressMatchReason::PhaseMatched)
        );
    }
}
