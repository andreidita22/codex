use codex_protocol::models::ResponseItem;

use crate::ContextInjectionPolicy;
use crate::HistoryDispositionPolicy;
use crate::HistoryDispositionRequest;
use crate::RemoteCompactedHistoryKeepPolicy;
use crate::RetentionDirective;
use crate::apply_retention_directive;
use crate::artifact_codecs::apply_history_disposition_with_summary_classifier;

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteCompactedHistoryShapeRequest {
    pub compacted_history: Vec<ResponseItem>,
    pub initial_context: Vec<ResponseItem>,
    pub authoritative_items: Vec<ResponseItem>,
    pub history_disposition: HistoryDispositionPolicy,
    pub retention_directive: RetentionDirective,
    pub context_injection: ContextInjectionPolicy,
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

pub fn shape_remote_compacted_history<
    FRemoteUserKeep,
    FRealUser,
    FUserOrSummary,
    FSummary,
    FRawConversation,
>(
    request: RemoteCompactedHistoryShapeRequest,
    should_keep_remote_user_message: FRemoteUserKeep,
    is_real_user_message: FRealUser,
    is_user_or_summary_message: FUserOrSummary,
    is_compaction_summary_message: FSummary,
    is_raw_conversation_message: FRawConversation,
) -> Vec<ResponseItem>
where
    FRemoteUserKeep: Fn(&ResponseItem) -> bool,
    FRealUser: Fn(&ResponseItem) -> bool,
    FUserOrSummary: Fn(&ResponseItem) -> bool,
    FSummary: Fn(&ResponseItem) -> bool,
    FRawConversation: Fn(&ResponseItem) -> bool,
{
    let disposition = apply_history_disposition_with_summary_classifier(
        HistoryDispositionRequest {
            items: request.compacted_history,
            policy: request.history_disposition.clone(),
        },
        is_compaction_summary_message,
    );
    let mut compacted_history: Vec<_> = disposition
        .items
        .into_iter()
        .filter(|item| {
            should_keep_remote_compacted_history_item(
                item,
                request.history_disposition.remote_keep_policy,
                &should_keep_remote_user_message,
            )
        })
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

    compacted_history = apply_retention_directive(
        compacted_history,
        request.retention_directive,
        is_raw_conversation_message,
    )
    .items;

    compacted_history
}

fn should_keep_remote_compacted_history_item<FUserKeep>(
    item: &ResponseItem,
    keep_policy: RemoteCompactedHistoryKeepPolicy,
    should_keep_remote_user_message: &FUserKeep,
) -> bool
where
    FUserKeep: Fn(&ResponseItem) -> bool,
{
    match keep_policy {
        RemoteCompactedHistoryKeepPolicy::KeepAll => true,
        RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages => match item {
            ResponseItem::Message { role, .. } if role == "developer" => false,
            ResponseItem::Message { role, .. } if role == "user" => {
                should_keep_remote_user_message(item)
            }
            ResponseItem::Message { role, .. } if role == "assistant" => true,
            ResponseItem::Message { .. } => false,
            ResponseItem::Compaction { .. } => true,
            ResponseItem::Reasoning { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::FunctionCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::FunctionCallOutput { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::CustomToolCallOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::GhostSnapshot { .. }
            | ResponseItem::Other => false,
        },
    }
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
    use crate::HistoryDispositionPolicy;
    use crate::LegacyCompactionMarkerPolicy;
    use crate::RemoteCompactedHistoryKeepPolicy;
    use crate::RetentionDirective;
    use crate::RetentionGate;
    use crate::SummaryDispositionPolicy;

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
            history_disposition: HistoryDispositionPolicy {
                prune_superseded_artifacts: false,
                summary_disposition: SummaryDispositionPolicy::KeepAll,
                remote_keep_policy:
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                drop_prior_artifact_kinds: vec![],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            },
            retention_directive: RetentionDirective::None,
            context_injection: ContextInjectionPolicy::BeforeLastRealUserOrSummary,
        };

        let refreshed = shape_remote_compacted_history(
            request,
            should_keep_remote_user_message,
            is_real_user_message,
            is_user_or_summary_message,
            is_summary_message,
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
            history_disposition: HistoryDispositionPolicy {
                prune_superseded_artifacts: false,
                summary_disposition: SummaryDispositionPolicy::KeepAll,
                remote_keep_policy: RemoteCompactedHistoryKeepPolicy::KeepAll,
                drop_prior_artifact_kinds: vec![ArtifactKind::ThreadMemory],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Strip,
            },
            retention_directive: RetentionDirective::None,
            context_injection: ContextInjectionPolicy::None,
        };

        let refreshed = shape_remote_compacted_history(
            request,
            |_| true,
            is_real_user_message,
            is_user_or_summary_message,
            is_summary_message,
            is_raw_conversation_message,
        );

        assert_eq!(refreshed, vec![message("user", "summary")]);
    }

    #[test]
    fn remote_history_shaping_applies_retention_directive_after_authoritative_insertion() {
        let request = RemoteCompactedHistoryShapeRequest {
            compacted_history: vec![
                message("user", "older user"),
                message("assistant", "latest assistant"),
                summary_message("summary text"),
            ],
            initial_context: vec![],
            authoritative_items: vec![developer_message(
                "<thread_memory>durable memory</thread_memory>",
            )],
            history_disposition: HistoryDispositionPolicy {
                prune_superseded_artifacts: false,
                summary_disposition: SummaryDispositionPolicy::KeepAll,
                remote_keep_policy:
                    RemoteCompactedHistoryKeepPolicy::DropDeveloperAndNonTurnUserMessages,
                drop_prior_artifact_kinds: vec![],
                legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy::Preserve,
            },
            retention_directive: RetentionDirective::KeepRecentRawConversation {
                max_messages: 1,
                gate: RetentionGate::FinalHistoryContainsArtifact(ArtifactKind::ThreadMemory),
            },
            context_injection: ContextInjectionPolicy::None,
        };

        let refreshed = shape_remote_compacted_history(
            request,
            should_keep_remote_user_message,
            is_real_user_message,
            is_user_or_summary_message,
            is_summary_message,
            is_raw_conversation_message,
        );

        assert_eq!(
            refreshed,
            vec![
                message("assistant", "latest assistant"),
                developer_message("<thread_memory>durable memory</thread_memory>"),
                summary_message("summary text"),
            ]
        );
    }

    fn should_keep_remote_user_message(item: &ResponseItem) -> bool {
        matches!(item, ResponseItem::Message { role, .. } if role == "user")
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
        matches!(item, ResponseItem::Message { role, .. } if role == "assistant")
            || is_real_user_message(item)
    }

    fn is_summary_message(item: &ResponseItem) -> bool {
        matches!(
            item,
            ResponseItem::Message { role, content, .. }
                if role == "user" && content_text(content).is_some_and(|text| is_summary_text(&text))
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
