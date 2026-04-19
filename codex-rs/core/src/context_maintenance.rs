use std::sync::Arc;

use crate::codex::Session;
use crate::codex::TurnContext;
use crate::compact::COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES;
use crate::compact::insert_items_before_last_summary_or_compaction;
use crate::compact::is_summary_message;
use crate::compact::retain_recent_raw_conversation_messages;
use crate::context_maintenance_runtime::execute_requested_artifact;
use crate::context_maintenance_runtime::runtime_plan_for_turn_boundary_maintenance;
use crate::governance::thread_memory::generate_thread_memory_item;
use codex_context_maintenance_policy::ArtifactKind;
use codex_context_maintenance_policy::MaintenanceAction;
use codex_context_maintenance_policy::apply_history_disposition;
use codex_context_maintenance_policy::build_prune_manifest_item;
use codex_context_maintenance_policy::tagged_artifact_kind;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::items::ContextCompactionItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TurnStartedEvent;
use codex_protocol::protocol::WarningEvent;
pub(crate) async fn run_refresh(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> CodexResult<()> {
    let runtime_plan = runtime_plan_for_turn_boundary_maintenance(
        turn_context.as_ref(),
        MaintenanceAction::Refresh,
    )?;
    send_turn_started_event(sess.as_ref(), turn_context.as_ref()).await;
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(turn_context.as_ref(), &compaction_item)
        .await;

    let history_snapshot = sess.clone_history().await;
    let raw_history = history_snapshot.raw_items().to_vec();
    let prompt_history = history_snapshot
        .clone()
        .for_prompt(&turn_context.model_info.input_modalities);
    let thread_memory_item = execute_requested_artifact(
        runtime_plan.artifact_requiredness(ArtifactKind::ThreadMemory),
        "thread memory",
        "during /refresh",
        || {
            generate_thread_memory_item(
                sess.as_ref(),
                turn_context.as_ref(),
                prompt_history.clone(),
            )
        },
    )
    .await?;

    let disposition = apply_history_disposition(
        runtime_plan
            .history_disposition_request(raw_history, /*prune_superseded_artifacts*/ true),
    );
    let mut new_history = disposition.items;
    let mut removed_count = disposition.removed_count;

    let authoritative_items: Vec<_> = thread_memory_item.into_iter().collect();
    if !authoritative_items.is_empty() {
        new_history =
            insert_items_before_last_summary_or_compaction(new_history, authoritative_items);
    }

    let has_thread_memory = new_history
        .iter()
        .any(|item| tagged_artifact_kind(item) == Some(ArtifactKind::ThreadMemory));
    if has_thread_memory {
        let old_len = new_history.len();
        new_history = retain_recent_raw_conversation_messages(
            new_history,
            COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES,
        );
        removed_count = removed_count.saturating_add(old_len.saturating_sub(new_history.len()));
    }

    let reference_context_item = sess.reference_context_item().await;
    sess.replace_history(new_history, reference_context_item)
        .await;
    sess.recompute_token_usage(turn_context.as_ref()).await;
    sess.send_event(
        turn_context.as_ref(),
        EventMsg::Warning(WarningEvent {
            message: format!("/refresh completed. Removed {removed_count} stale item(s)."),
        }),
    )
    .await;
    sess.emit_turn_item_completed(turn_context.as_ref(), compaction_item)
        .await;
    Ok(())
}

pub(crate) async fn run_prune(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> CodexResult<()> {
    let runtime_plan = runtime_plan_for_turn_boundary_maintenance(
        turn_context.as_ref(),
        MaintenanceAction::Prune,
    )?;
    send_turn_started_event(sess.as_ref(), turn_context.as_ref()).await;
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(turn_context.as_ref(), &compaction_item)
        .await;

    let history_snapshot = sess.clone_history().await;
    let raw_history = history_snapshot.raw_items().to_vec();

    let original_len = raw_history.len();
    let disposition = apply_history_disposition(
        runtime_plan
            .history_disposition_request(raw_history, /*prune_superseded_artifacts*/ true),
    );
    let mut pruned_history = disposition.items;
    let mut removed_count = disposition.removed_count;
    let (next_history, summary_removed) = retain_latest_summary_message(pruned_history);
    pruned_history = next_history;
    removed_count = removed_count.saturating_add(summary_removed);

    let has_thread_memory = pruned_history
        .iter()
        .any(|item| tagged_artifact_kind(item) == Some(ArtifactKind::ThreadMemory));
    if has_thread_memory {
        let old_len = pruned_history.len();
        pruned_history = retain_recent_raw_conversation_messages(
            pruned_history,
            COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES,
        );
        removed_count = removed_count.saturating_add(old_len.saturating_sub(pruned_history.len()));
    }

    let post_prune_len = pruned_history.len();
    let manifest_item = build_prune_manifest_item(original_len, post_prune_len, removed_count)?;
    pruned_history =
        insert_items_before_last_summary_or_compaction(pruned_history, vec![manifest_item]);

    let reference_context_item = sess.reference_context_item().await;
    sess.replace_history(pruned_history, reference_context_item)
        .await;
    sess.recompute_token_usage(turn_context.as_ref()).await;

    sess.send_event(
        turn_context.as_ref(),
        EventMsg::Warning(WarningEvent {
            message: format!("/prune completed. Removed {removed_count} item(s)."),
        }),
    )
    .await;
    sess.emit_turn_item_completed(turn_context.as_ref(), compaction_item)
        .await;
    Ok(())
}

fn retain_latest_summary_message(items: Vec<ResponseItem>) -> (Vec<ResponseItem>, usize) {
    let mut latest_summary_index = None;
    for (idx, item) in items.iter().enumerate().rev() {
        if is_compaction_summary_message(item) {
            latest_summary_index = Some(idx);
            break;
        }
    }

    let mut removed = 0usize;
    let items = items
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if is_compaction_summary_message(&item) && Some(idx) != latest_summary_index {
                removed = removed.saturating_add(1);
                None
            } else {
                Some(item)
            }
        })
        .collect();
    (items, removed)
}

fn is_compaction_summary_message(item: &ResponseItem) -> bool {
    let ResponseItem::Message { role, content, .. } = item else {
        return false;
    };
    if role != "user" {
        return false;
    }
    let Some(text) = crate::compact::content_items_to_text(content) else {
        return false;
    };
    is_summary_message(&text)
}

async fn send_turn_started_event(sess: &Session, turn_context: &TurnContext) {
    let start_event = EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: turn_context.sub_id.clone(),
        started_at: turn_context.turn_timing_state.started_at_unix_secs().await,
        model_context_window: turn_context.model_context_window(),
        collaboration_mode_kind: turn_context.collaboration_mode.mode,
    });
    sess.send_event(turn_context, start_event).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_context_maintenance_policy::prune_superseded_artifacts;
    use codex_context_maintenance_policy::remove_artifact_kind;
    use codex_protocol::models::ContentItem;
    use pretty_assertions::assert_eq;

    fn developer_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    #[test]
    fn prune_removes_all_but_latest_prune_manifest() {
        let items = vec![
            developer_message("<prune_manifest>old</prune_manifest>"),
            developer_message("<prune_manifest>new</prune_manifest>"),
        ];

        let (items, removed) = prune_superseded_artifacts(items);
        assert_eq!(removed, 1);
        assert_eq!(
            items,
            vec![developer_message("<prune_manifest>new</prune_manifest>")]
        );
    }

    #[test]
    fn prune_flow_preserves_ordering_between_artifacts_and_latest_summary() {
        let raw_history = vec![
            developer_message("<thread_memory>old memory</thread_memory>"),
            developer_message("<thread_memory>new memory</thread_memory>"),
            developer_message("<prune_manifest>old prune</prune_manifest>"),
            user_summary_message("older summary"),
            user_summary_message("latest summary"),
        ];

        let (pruned_history, removed) = prune_superseded_artifacts(raw_history);
        assert_eq!(removed, 1);

        let (pruned_history, removed_manifests) =
            remove_artifact_kind(pruned_history, ArtifactKind::PruneManifest);
        assert_eq!(removed_manifests, 1);

        let (pruned_history, removed_summaries) = retain_latest_summary_message(pruned_history);
        assert_eq!(removed_summaries, 1);

        let manifest = build_prune_manifest_item(
            /*original_len*/ 5, /*final_len*/ 2, /*removed_count*/ 2,
        )
        .expect("prune manifest should serialize");
        let pruned_history =
            insert_items_before_last_summary_or_compaction(pruned_history, vec![manifest]);

        assert_eq!(
            pruned_history,
            vec![
                developer_message("<thread_memory>new memory</thread_memory>"),
                developer_message(
                    "<prune_manifest schema=\"prune_manifest_v1\">\n{\n  \"final_history_len\": 2,\n  \"original_history_len\": 5,\n  \"removed_item_count\": 2,\n  \"schema\": \"prune_manifest_v1\"\n}\n</prune_manifest>"
                ),
                user_summary_message("latest summary"),
            ]
        );
    }

    fn user_summary_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![codex_protocol::models::ContentItem::InputText {
                text: format!("{}\n{text}", crate::compact::SUMMARY_PREFIX.trim_end()),
            }],
            end_turn: None,
            phase: None,
        }
    }
}
