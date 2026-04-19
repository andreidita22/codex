use codex_protocol::models::ResponseItem;

use crate::RetentionDirective;
use crate::RetentionGate;
use crate::artifact_codecs::tagged_artifact_kind;

#[derive(Clone, Debug, PartialEq)]
pub struct RetentionApplicationResult {
    pub items: Vec<ResponseItem>,
    pub removed_count: usize,
}

pub fn apply_retention_directive<F>(
    items: Vec<ResponseItem>,
    directive: RetentionDirective,
    is_raw_conversation_message: F,
) -> RetentionApplicationResult
where
    F: Fn(&ResponseItem) -> bool,
{
    match directive {
        RetentionDirective::None => RetentionApplicationResult {
            items,
            removed_count: 0,
        },
        RetentionDirective::KeepRecentRawConversation {
            max_messages,
            gate: RetentionGate::Always,
        } => {
            let original_len = items.len();
            let items = retain_recent_raw_conversation_messages(
                items,
                max_messages,
                is_raw_conversation_message,
            );
            RetentionApplicationResult {
                removed_count: original_len.saturating_sub(items.len()),
                items,
            }
        }
        RetentionDirective::KeepRecentRawConversation {
            max_messages,
            gate: RetentionGate::FinalHistoryContainsArtifact(kind),
        } => {
            if items.is_empty() {
                return RetentionApplicationResult {
                    items,
                    removed_count: 0,
                };
            }

            let original_len = items.len();
            let mut retained = vec![true; original_len];
            let mut remaining = max_messages;
            let mut gate_satisfied = false;
            for (index, item) in items.iter().enumerate().rev() {
                gate_satisfied |= tagged_artifact_kind(item) == Some(kind);

                if !is_raw_conversation_message(item) {
                    continue;
                }
                if remaining > 0 {
                    remaining -= 1;
                    continue;
                }
                retained[index] = false;
            }

            if !gate_satisfied {
                return RetentionApplicationResult {
                    items,
                    removed_count: 0,
                };
            }

            let items = items
                .into_iter()
                .enumerate()
                .filter_map(|(index, item)| retained[index].then_some(item))
                .collect::<Vec<_>>();
            RetentionApplicationResult {
                removed_count: original_len.saturating_sub(items.len()),
                items,
            }
        }
    }
}

pub fn retain_recent_raw_conversation_messages<F>(
    compacted_history: Vec<ResponseItem>,
    max_messages: usize,
    is_raw_conversation_message: F,
) -> Vec<ResponseItem>
where
    F: Fn(&ResponseItem) -> bool,
{
    if compacted_history.is_empty() {
        return compacted_history;
    }

    let mut retained = vec![true; compacted_history.len()];
    let mut remaining = max_messages;
    for (index, item) in compacted_history.iter().enumerate().rev() {
        if !is_raw_conversation_message(item) {
            continue;
        }
        if remaining > 0 {
            remaining -= 1;
            continue;
        }
        retained[index] = false;
    }

    compacted_history
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| retained[index].then_some(item))
        .collect()
}

#[cfg(test)]
mod tests {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    use super::RetentionApplicationResult;
    use super::apply_retention_directive;
    use super::retain_recent_raw_conversation_messages;
    use crate::ArtifactKind;
    use crate::RetentionDirective;
    use crate::RetentionGate;

    fn is_raw_conversation_message(item: &ResponseItem) -> bool {
        matches!(
            item,
            ResponseItem::Message { role, .. } if role == "user" || role == "assistant"
        )
    }

    #[test]
    fn keeps_latest_window_and_preserves_non_candidates() {
        let history = vec![
            message("user", "older user"),
            message("assistant", "older assistant"),
            message("user", "latest user"),
            message("assistant", "latest assistant"),
            ResponseItem::Compaction {
                encrypted_content: "encrypted".to_string(),
            },
        ];

        let retained =
            retain_recent_raw_conversation_messages(history, 2, is_raw_conversation_message);

        assert_eq!(
            retained,
            vec![
                message("user", "latest user"),
                message("assistant", "latest assistant"),
                ResponseItem::Compaction {
                    encrypted_content: "encrypted".to_string(),
                }
            ]
        );
    }

    #[test]
    fn retention_directive_skips_when_gate_is_not_satisfied() {
        let history = vec![
            message("user", "older user"),
            message("assistant", "latest assistant"),
        ];

        let retained = apply_retention_directive(
            history.clone(),
            RetentionDirective::KeepRecentRawConversation {
                max_messages: 1,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            is_raw_conversation_message,
        );

        assert_eq!(
            retained,
            RetentionApplicationResult {
                items: history,
                removed_count: 0,
            }
        );
    }

    #[test]
    fn retention_directive_applies_when_gate_is_satisfied() {
        let history = vec![
            message("user", "older user"),
            developer_message("<thread_memory>durable</thread_memory>"),
            message("assistant", "latest assistant"),
        ];

        let retained = apply_retention_directive(
            history,
            RetentionDirective::KeepRecentRawConversation {
                max_messages: 1,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            is_raw_conversation_message,
        );

        assert_eq!(
            retained,
            RetentionApplicationResult {
                items: vec![
                    developer_message("<thread_memory>durable</thread_memory>"),
                    message("assistant", "latest assistant"),
                ],
                removed_count: 1,
            }
        );
    }

    fn message(role: &str, text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn developer_message(text: &str) -> ResponseItem {
        message("developer", text)
    }
}
