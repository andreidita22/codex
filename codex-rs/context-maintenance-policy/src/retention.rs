use codex_protocol::models::ResponseItem;

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

    use super::retain_recent_raw_conversation_messages;

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
}
