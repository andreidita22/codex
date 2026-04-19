use codex_protocol::models::ResponseItem;

use crate::ArtifactKind;
use crate::ContextInjectionPolicy;
use crate::HistoryDispositionRequest;
use crate::LegacyCompactionMarkerPolicy;
use crate::apply_history_disposition;
use crate::retain_recent_raw_conversation_messages;

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteCompactedHistoryShapeRequest {
    pub compacted_history: Vec<ResponseItem>,
    pub initial_context: Vec<ResponseItem>,
    pub authoritative_items: Vec<ResponseItem>,
    pub drop_prior_artifact_kinds: Vec<ArtifactKind>,
    pub legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
    pub raw_message_retention_limit: usize,
    pub context_injection: ContextInjectionPolicy,
    pub retain_recent_raw_messages: bool,
}

pub fn insert_initial_context_before_last_real_user_or_summary<FRealUser, FUserOrSummary>(
    mut compacted_history: Vec<ResponseItem>,
    initial_context: Vec<ResponseItem>,
    is_real_user_message: FRealUser,
    is_user_or_summary_message: FUserOrSummary,
) -> Vec<ResponseItem>
where
    FRealUser: Fn(&ResponseItem) -> bool,
    FUserOrSummary: Fn(&ResponseItem) -> bool,
{
    let mut last_user_or_summary_index = None;
    let mut last_real_user_index = None;
    for (i, item) in compacted_history.iter().enumerate().rev() {
        if !is_user_or_summary_message(item) {
            continue;
        }
        last_user_or_summary_index.get_or_insert(i);
        if is_real_user_message(item) {
            last_real_user_index = Some(i);
            break;
        }
    }
    let last_compaction_index = compacted_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, item)| matches!(item, ResponseItem::Compaction { .. }).then_some(i));
    let insertion_index = last_real_user_index
        .or(last_user_or_summary_index)
        .or(last_compaction_index);

    if let Some(insertion_index) = insertion_index {
        compacted_history.splice(insertion_index..insertion_index, initial_context);
    } else {
        compacted_history.extend(initial_context);
    }

    compacted_history
}

pub fn insert_items_before_last_summary_or_compaction<FUserOrSummary>(
    mut compacted_history: Vec<ResponseItem>,
    items: Vec<ResponseItem>,
    is_user_or_summary_message: FUserOrSummary,
) -> Vec<ResponseItem>
where
    FUserOrSummary: Fn(&ResponseItem) -> bool,
{
    let last_user_or_summary_index = compacted_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, item)| is_user_or_summary_message(item).then_some(i));
    let last_compaction_index = compacted_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, item)| matches!(item, ResponseItem::Compaction { .. }).then_some(i));
    let insertion_index = last_compaction_index.or(last_user_or_summary_index);

    if let Some(insertion_index) = insertion_index {
        compacted_history.splice(insertion_index..insertion_index, items);
    } else {
        compacted_history.extend(items);
    }

    compacted_history
}

pub fn shape_remote_compacted_history<FKeepItem, FRealUser, FUserOrSummary, FRawConversation>(
    request: RemoteCompactedHistoryShapeRequest,
    should_keep_item: FKeepItem,
    is_real_user_message: FRealUser,
    is_user_or_summary_message: FUserOrSummary,
    is_raw_conversation_message: FRawConversation,
) -> Vec<ResponseItem>
where
    FKeepItem: Fn(&ResponseItem) -> bool,
    FRealUser: Fn(&ResponseItem) -> bool,
    FUserOrSummary: Fn(&ResponseItem) -> bool,
    FRawConversation: Fn(&ResponseItem) -> bool,
{
    let disposition = apply_history_disposition(HistoryDispositionRequest {
        items: request.compacted_history,
        prune_superseded_artifacts: false,
        drop_prior_artifact_kinds: request.drop_prior_artifact_kinds,
        legacy_compaction_marker_policy: request.legacy_compaction_marker_policy,
    });
    let mut compacted_history: Vec<_> = disposition
        .items
        .into_iter()
        .filter(should_keep_item)
        .collect();

    if matches!(
        request.context_injection,
        ContextInjectionPolicy::BeforeLastRealUserOrSummary
    ) {
        compacted_history = insert_initial_context_before_last_real_user_or_summary(
            compacted_history,
            request.initial_context,
            is_real_user_message,
            &is_user_or_summary_message,
        );
    }

    if !request.authoritative_items.is_empty() {
        compacted_history = insert_items_before_last_summary_or_compaction(
            compacted_history,
            request.authoritative_items,
            &is_user_or_summary_message,
        );
    }

    if request.retain_recent_raw_messages {
        compacted_history = retain_recent_raw_conversation_messages(
            compacted_history,
            request.raw_message_retention_limit,
            is_raw_conversation_message,
        );
    }

    compacted_history
}

#[cfg(test)]
mod tests {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    use super::RemoteCompactedHistoryShapeRequest;
    use super::insert_initial_context_before_last_real_user_or_summary;
    use super::insert_items_before_last_summary_or_compaction;
    use super::shape_remote_compacted_history;
    use crate::ArtifactKind;
    use crate::ContextInjectionPolicy;
    use crate::LegacyCompactionMarkerPolicy;

    #[test]
    fn initial_context_insertion_keeps_summary_last_when_no_real_user_remains() {
        let compacted_history = vec![
            message("user", "older user"),
            summary_message("summary text"),
        ];
        let initial_context = vec![message("developer", "fresh permissions")];

        let refreshed = insert_initial_context_before_last_real_user_or_summary(
            compacted_history,
            initial_context,
            is_real_user_message,
            is_user_or_summary_message,
        );

        assert_eq!(
            refreshed,
            vec![
                message("developer", "fresh permissions"),
                message("user", "older user"),
                summary_message("summary text"),
            ]
        );
    }

    #[test]
    fn authoritative_item_insertion_keeps_compaction_last() {
        let history = vec![
            message("user", "latest user"),
            ResponseItem::Compaction {
                encrypted_content: "encrypted".to_string(),
            },
        ];

        let refreshed = insert_items_before_last_summary_or_compaction(
            history,
            vec![message("developer", "authoritative memory")],
            is_user_or_summary_message,
        );

        assert_eq!(
            refreshed,
            vec![
                message("user", "latest user"),
                message("developer", "authoritative memory"),
                ResponseItem::Compaction {
                    encrypted_content: "encrypted".to_string(),
                },
            ]
        );
    }

    #[test]
    fn remote_history_shaping_filters_injects_and_reinserts_authoritative_items() {
        let request = RemoteCompactedHistoryShapeRequest {
            compacted_history: vec![
                message("developer", "stale developer"),
                message("user", "summary"),
            ],
            initial_context: vec![message("developer", "fresh context")],
            authoritative_items: vec![message("developer", "thread memory")],
            drop_prior_artifact_kinds: vec![],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            raw_message_retention_limit: 10,
            context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
            retain_recent_raw_messages: false,
        };

        let refreshed = shape_remote_compacted_history(
            request,
            should_keep_item,
            is_real_user_message,
            is_user_or_summary_message,
            is_raw_conversation_message,
        );

        assert_eq!(
            refreshed,
            vec![
                message("developer", "fresh context"),
                message("developer", "thread memory"),
                message("user", "summary"),
            ]
        );
    }

    #[test]
    fn remote_history_shaping_applies_marker_policy_and_artifact_drops() {
        let request = RemoteCompactedHistoryShapeRequest {
            compacted_history: vec![
                message("user", "summary"),
                developer_message("<thread_memory>stale</thread_memory>"),
                ResponseItem::Compaction {
                    encrypted_content: "encrypted".to_string(),
                },
            ],
            initial_context: vec![],
            authoritative_items: vec![],
            drop_prior_artifact_kinds: vec![ArtifactKind::ThreadMemory],
            legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            raw_message_retention_limit: 10,
            context_injection: ContextInjectionPolicy::None,
            retain_recent_raw_messages: false,
        };

        let refreshed = shape_remote_compacted_history(
            request,
            |_| true,
            is_real_user_message,
            is_user_or_summary_message,
            is_raw_conversation_message,
        );

        assert_eq!(refreshed, vec![message("user", "summary")]);
    }

    fn should_keep_item(item: &ResponseItem) -> bool {
        !matches!(item, ResponseItem::Message { role, .. } if role == "developer")
    }

    fn is_real_user_message(item: &ResponseItem) -> bool {
        matches!(
            item,
            ResponseItem::Message { role, content, .. }
                if role == "user" && content_text(content).is_some_and(|text| !is_summary_text(&text))
        )
    }

    fn is_user_or_summary_message(item: &ResponseItem) -> bool {
        matches!(item, ResponseItem::Message { role, .. } if role == "user")
    }

    fn is_raw_conversation_message(item: &ResponseItem) -> bool {
        matches!(
            item,
            ResponseItem::Message { role, .. } if role == "user" || role == "assistant"
        )
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

    fn summary_message(text: &str) -> ResponseItem {
        message("user", &format!("Summary of conversation above:\n{text}"))
    }

    fn is_summary_text(text: &str) -> bool {
        text.starts_with("Summary of conversation above:\n")
    }

    fn content_text(content: &[ContentItem]) -> Option<String> {
        let mut pieces = Vec::new();
        for item in content {
            match item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    if !text.is_empty() {
                        pieces.push(text.clone());
                    }
                }
                ContentItem::InputImage { .. } => {}
            }
        }
        if pieces.is_empty() {
            None
        } else {
            Some(pieces.join("\n"))
        }
    }
}
