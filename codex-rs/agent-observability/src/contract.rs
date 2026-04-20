pub const INSPECT_AGENT_PROGRESS_TOOL_NAME: &str = "inspect_agent_progress";
pub const WAIT_FOR_AGENT_PROGRESS_TOOL_NAME: &str = "wait_for_agent_progress";

pub const AGENT_PROGRESS_PHASE_NAMES: &[&str] = &[
    "pending",
    "reasoning",
    "message_drafting",
    "command",
    "tool_call",
    "waiting_approval",
    "waiting_user_input",
    "completed",
    "errored",
    "interrupted",
    "shutdown",
];

pub const AGENT_BLOCK_REASON_NAMES: &[&str] = &[
    "exec_approval",
    "patch_approval",
    "permissions_request",
    "user_input_request",
    "elicitation_request",
];

pub const AGENT_ACTIVE_WORK_KIND_NAMES: &[&str] = &["reasoning", "message", "command", "tool"];

pub const WAIT_FOR_AGENT_PROGRESS_MATCH_REASON_NAMES: &[&str] = &[
    "already_satisfied",
    "seq_advanced",
    "phase_matched",
    "timed_out",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentProgressPhase;
    use crate::WaitForAgentProgressMatchReason;
    use crate::progress::AgentActiveWorkKind;
    use crate::progress::AgentBlockReason;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn progress_contract_constants_match_current_runtime_contract() {
        assert_eq!(INSPECT_AGENT_PROGRESS_TOOL_NAME, "inspect_agent_progress");
        assert_eq!(WAIT_FOR_AGENT_PROGRESS_TOOL_NAME, "wait_for_agent_progress");
        assert_eq!(
            AGENT_PROGRESS_PHASE_NAMES,
            &[
                "pending",
                "reasoning",
                "message_drafting",
                "command",
                "tool_call",
                "waiting_approval",
                "waiting_user_input",
                "completed",
                "errored",
                "interrupted",
                "shutdown",
            ]
        );
        assert_eq!(
            AGENT_BLOCK_REASON_NAMES,
            &[
                "exec_approval",
                "patch_approval",
                "permissions_request",
                "user_input_request",
                "elicitation_request",
            ]
        );
        assert_eq!(
            AGENT_ACTIVE_WORK_KIND_NAMES,
            &["reasoning", "message", "command", "tool"]
        );
        assert_eq!(
            WAIT_FOR_AGENT_PROGRESS_MATCH_REASON_NAMES,
            &[
                "already_satisfied",
                "seq_advanced",
                "phase_matched",
                "timed_out",
            ]
        );
    }

    #[test]
    fn progress_contract_constants_match_serialized_enum_values() {
        assert_eq!(
            [
                AgentProgressPhase::Pending,
                AgentProgressPhase::Reasoning,
                AgentProgressPhase::MessageDrafting,
                AgentProgressPhase::Command,
                AgentProgressPhase::ToolCall,
                AgentProgressPhase::WaitingApproval,
                AgentProgressPhase::WaitingUserInput,
                AgentProgressPhase::Completed,
                AgentProgressPhase::Errored,
                AgentProgressPhase::Interrupted,
                AgentProgressPhase::Shutdown,
            ]
            .iter()
            .map(|phase| serde_json::to_value(phase).unwrap())
            .collect::<Vec<_>>(),
            AGENT_PROGRESS_PHASE_NAMES
                .iter()
                .map(|name| json!(name))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            [
                AgentBlockReason::ExecApproval,
                AgentBlockReason::PatchApproval,
                AgentBlockReason::PermissionsRequest,
                AgentBlockReason::UserInputRequest,
                AgentBlockReason::ElicitationRequest,
            ]
            .iter()
            .map(|reason| serde_json::to_value(reason).unwrap())
            .collect::<Vec<_>>(),
            AGENT_BLOCK_REASON_NAMES
                .iter()
                .map(|name| json!(name))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            [
                AgentActiveWorkKind::Reasoning,
                AgentActiveWorkKind::Message,
                AgentActiveWorkKind::Command,
                AgentActiveWorkKind::Tool,
            ]
            .iter()
            .map(|kind| serde_json::to_value(kind).unwrap())
            .collect::<Vec<_>>(),
            AGENT_ACTIVE_WORK_KIND_NAMES
                .iter()
                .map(|name| json!(name))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            [
                WaitForAgentProgressMatchReason::AlreadySatisfied,
                WaitForAgentProgressMatchReason::SeqAdvanced,
                WaitForAgentProgressMatchReason::PhaseMatched,
                WaitForAgentProgressMatchReason::TimedOut,
            ]
            .iter()
            .map(|reason| serde_json::to_value(reason).unwrap())
            .collect::<Vec<_>>(),
            WAIT_FOR_AGENT_PROGRESS_MATCH_REASON_NAMES
                .iter()
                .map(|name| json!(name))
                .collect::<Vec<_>>()
        );
    }
}
