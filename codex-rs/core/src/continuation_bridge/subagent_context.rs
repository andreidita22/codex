use crate::agent::AgentStatus;
use crate::codex::Session;
use codex_context_maintenance_policy::ContinuationBridgeSubagentSnapshot;
use codex_context_maintenance_policy::ContinuationBridgeSubagentStatus;

pub(crate) async fn collect_subagent_snapshots(
    sess: &Session,
) -> Vec<ContinuationBridgeSubagentSnapshot> {
    sess.services
        .agent_control
        .list_subagents(sess.conversation_id)
        .await
        .into_iter()
        .map(|subagent| {
            let thread_id = subagent.thread_id.to_string();
            ContinuationBridgeSubagentSnapshot {
                agent_id: thread_id.clone(),
                thread_id,
                nickname: subagent.nickname.unwrap_or_default(),
                role: subagent.role.unwrap_or_default(),
                status: bridge_status(&subagent.status),
            }
        })
        .collect()
}

fn bridge_status(status: &AgentStatus) -> ContinuationBridgeSubagentStatus {
    match status {
        AgentStatus::PendingInit => ContinuationBridgeSubagentStatus::Pending,
        AgentStatus::Running | AgentStatus::Interrupted => {
            ContinuationBridgeSubagentStatus::Running
        }
        AgentStatus::Completed(_) => ContinuationBridgeSubagentStatus::Completed,
        AgentStatus::Errored(_) | AgentStatus::Shutdown | AgentStatus::NotFound => {
            ContinuationBridgeSubagentStatus::Failed
        }
    }
}
