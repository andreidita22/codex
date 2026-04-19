use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde::Serialize;

const CONTINUATION_BRIDGE_SUBAGENTS_TAG: &str = "continuation_bridge_subagents";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationBridgeSubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContinuationBridgeSubagentSnapshot {
    pub agent_id: String,
    pub thread_id: String,
    pub nickname: String,
    pub role: String,
    pub status: ContinuationBridgeSubagentStatus,
}

pub fn continuation_bridge_subagent_context_item(
    snapshots: Vec<ContinuationBridgeSubagentSnapshot>,
) -> CodexResult<Option<ResponseItem>> {
    if snapshots.is_empty() {
        return Ok(None);
    }

    let payload = serde_json::to_string_pretty(&snapshots)?;
    Ok(Some(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "<{CONTINUATION_BRIDGE_SUBAGENTS_TAG}>\n{payload}\n</{CONTINUATION_BRIDGE_SUBAGENTS_TAG}>"
            ),
        }],
        end_turn: None,
        phase: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::ContinuationBridgeSubagentSnapshot;
    use super::ContinuationBridgeSubagentStatus;
    use super::continuation_bridge_subagent_context_item;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;

    #[test]
    fn continuation_bridge_subagent_context_item_returns_none_when_empty() {
        let item = continuation_bridge_subagent_context_item(Vec::new())
            .expect("empty snapshot list should not fail");

        assert_eq!(item, None);
    }

    #[test]
    fn continuation_bridge_subagent_context_item_matches_expected_output_shape() {
        let item = continuation_bridge_subagent_context_item(vec![
            ContinuationBridgeSubagentSnapshot {
                agent_id: "thread-123".to_string(),
                thread_id: "thread-123".to_string(),
                nickname: "worker".to_string(),
                role: "default".to_string(),
                status: ContinuationBridgeSubagentStatus::Running,
            },
            ContinuationBridgeSubagentSnapshot {
                agent_id: "thread-456".to_string(),
                thread_id: "thread-456".to_string(),
                nickname: String::new(),
                role: String::new(),
                status: ContinuationBridgeSubagentStatus::Failed,
            },
        ])
        .expect("snapshot serialization should succeed");

        assert_eq!(
            item,
            Some(ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "<continuation_bridge_subagents>\n[\n  {\n    \"agent_id\": \"thread-123\",\n    \"thread_id\": \"thread-123\",\n    \"nickname\": \"worker\",\n    \"role\": \"default\",\n    \"status\": \"running\"\n  },\n  {\n    \"agent_id\": \"thread-456\",\n    \"thread_id\": \"thread-456\",\n    \"nickname\": \"\",\n    \"role\": \"\",\n    \"status\": \"failed\"\n  }\n]\n</continuation_bridge_subagents>".to_string(),
                }],
                end_turn: None,
                phase: None,
            })
        );
    }
}
