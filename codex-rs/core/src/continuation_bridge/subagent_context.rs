use crate::agent::AgentStatus;
use crate::codex::Session;
use codex_protocol::error::Result;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ContinuationBridgeSubagentSnapshot {
    agent_id: String,
    thread_id: String,
    nickname: String,
    role: String,
    status: String,
}

pub(crate) async fn build_subagent_context_item(sess: &Session) -> Result<Option<ResponseItem>> {
    let subagents = sess
        .services
        .agent_control
        .list_subagents(sess.conversation_id)
        .await;
    if subagents.is_empty() {
        return Ok(None);
    }

    let snapshots = subagents
        .into_iter()
        .map(|subagent| {
            let thread_id = subagent.thread_id.to_string();
            ContinuationBridgeSubagentSnapshot {
                agent_id: thread_id.clone(),
                thread_id,
                nickname: subagent.nickname.unwrap_or_default(),
                role: subagent.role.unwrap_or_default(),
                status: bridge_status(&subagent.status).to_string(),
            }
        })
        .collect::<Vec<_>>();
    let payload = serde_json::to_string_pretty(&snapshots)?;

    Ok(Some(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "<continuation_bridge_subagents>\n{payload}\n</continuation_bridge_subagents>"
            ),
        }],
        end_turn: None,
        phase: None,
    }))
}

fn bridge_status(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::PendingInit => "pending",
        AgentStatus::Running | AgentStatus::Interrupted => "running",
        AgentStatus::Completed(_) => "completed",
        AgentStatus::Errored(_) | AgentStatus::Shutdown | AgentStatus::NotFound => "failed",
    }
}
