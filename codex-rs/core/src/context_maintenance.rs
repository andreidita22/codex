use std::sync::Arc;

use crate::codex::Session;
use crate::codex::TurnContext;
use crate::compact::COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES;
use crate::compact::insert_items_before_last_summary_or_compaction;
use crate::compact::is_summary_message;
use crate::compact::retain_recent_raw_conversation_messages;
use crate::continuation_bridge::generate_continuation_bridge_item;
use crate::governance::thread_memory::generate_thread_memory_item;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::items::ContextCompactionItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TurnStartedEvent;
use codex_protocol::protocol::WarningEvent;
use serde_json::json;
use tracing::warn;

const PRUNE_MANIFEST_TAG: &str = "prune_manifest";
const CONTINUATION_BRIDGE_TAG: &str = "continuation_bridge";
const THREAD_MEMORY_TAG: &str = "thread_memory";

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TurnBoundaryMaintenanceActionForTests {
    Refresh,
    Prune,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CurrentTurnBoundaryMaintenanceBehaviorForTests {
    pub(crate) generates_thread_memory: bool,
    pub(crate) generates_continuation_bridge: bool,
    pub(crate) emits_prune_manifest: bool,
}

#[cfg(test)]
pub(crate) fn current_turn_boundary_maintenance_behavior_for_tests(
    action: TurnBoundaryMaintenanceActionForTests,
    path_variant: crate::config::GovernancePathVariant,
) -> CurrentTurnBoundaryMaintenanceBehaviorForTests {
    match action {
        TurnBoundaryMaintenanceActionForTests::Refresh => {
            CurrentTurnBoundaryMaintenanceBehaviorForTests {
                generates_thread_memory: crate::compact::is_thread_memory_required(path_variant),
                generates_continuation_bridge: true,
                emits_prune_manifest: false,
            }
        }
        TurnBoundaryMaintenanceActionForTests::Prune => {
            CurrentTurnBoundaryMaintenanceBehaviorForTests {
                generates_thread_memory: false,
                generates_continuation_bridge: false,
                emits_prune_manifest: true,
            }
        }
    }
}

pub(crate) async fn run_refresh(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> CodexResult<()> {
    send_turn_started_event(sess.as_ref(), turn_context.as_ref()).await;
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(turn_context.as_ref(), &compaction_item)
        .await;

    let history_snapshot = sess.clone_history().await;
    let raw_history = history_snapshot.raw_items().to_vec();
    let prompt_history = history_snapshot
        .clone()
        .for_prompt(&turn_context.model_info.input_modalities);
    let governance_path_variant = turn_context.governance_path_variant();
    let thread_memory_item = if crate::compact::is_thread_memory_required(governance_path_variant) {
        match generate_thread_memory_item(
            sess.as_ref(),
            turn_context.as_ref(),
            prompt_history.clone(),
        )
        .await
        {
            Ok(Some(item)) => Some(item),
            Ok(None) => {
                return Err(CodexErr::Fatal(format!(
                    "required thread memory generation returned empty output during /refresh (path_variant={governance_path_variant:?})"
                )));
            }
            Err(err) => {
                return Err(CodexErr::Fatal(format!(
                    "failed generating required thread memory during /refresh (path_variant={governance_path_variant:?}): {err}"
                )));
            }
        }
    } else {
        None
    };

    let continuation_bridge_item = match generate_continuation_bridge_item(
        sess.as_ref(),
        turn_context.as_ref(),
        prompt_history,
    )
    .await
    {
        Ok(item) => item,
        Err(err) => {
            warn!("failed generating continuation bridge during /refresh: {err}");
            None
        }
    };

    let (mut new_history, mut removed_count) = prune_superseded_artifacts(raw_history);
    if thread_memory_item.is_some() {
        let (next_history, removed) = remove_artifact_kind(new_history, ArtifactKind::ThreadMemory);
        new_history = next_history;
        removed_count = removed_count.saturating_add(removed);
    }
    if continuation_bridge_item.is_some() {
        let (next_history, removed) =
            remove_artifact_kind(new_history, ArtifactKind::ContinuationBridge);
        new_history = next_history;
        removed_count = removed_count.saturating_add(removed);
    }

    let authoritative_items: Vec<_> = thread_memory_item
        .into_iter()
        .chain(continuation_bridge_item)
        .collect();
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
    send_turn_started_event(sess.as_ref(), turn_context.as_ref()).await;
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(turn_context.as_ref(), &compaction_item)
        .await;

    let history_snapshot = sess.clone_history().await;
    let raw_history = history_snapshot.raw_items().to_vec();

    let original_len = raw_history.len();
    let (mut pruned_history, mut removed_count) = prune_superseded_artifacts(raw_history);
    let (next_history, manifest_removed) =
        remove_artifact_kind(pruned_history, ArtifactKind::PruneManifest);
    pruned_history = next_history;
    removed_count = removed_count.saturating_add(manifest_removed);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactKind {
    ContinuationBridge,
    ThreadMemory,
    PruneManifest,
}

fn prune_superseded_artifacts(items: Vec<ResponseItem>) -> (Vec<ResponseItem>, usize) {
    let mut last_continuation_bridge = None;
    let mut last_thread_memory = None;
    let mut last_prune_manifest = None;
    for (idx, item) in items.iter().enumerate().rev() {
        match tagged_artifact_kind(item) {
            Some(ArtifactKind::ContinuationBridge) if last_continuation_bridge.is_none() => {
                last_continuation_bridge = Some(idx);
            }
            Some(ArtifactKind::ThreadMemory) if last_thread_memory.is_none() => {
                last_thread_memory = Some(idx);
            }
            Some(ArtifactKind::PruneManifest) if last_prune_manifest.is_none() => {
                last_prune_manifest = Some(idx);
            }
            _ => {}
        }
    }

    let mut removed = 0usize;
    let items = items
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let keep = match tagged_artifact_kind(&item) {
                Some(ArtifactKind::ContinuationBridge) => Some(idx) == last_continuation_bridge,
                Some(ArtifactKind::ThreadMemory) => Some(idx) == last_thread_memory,
                Some(ArtifactKind::PruneManifest) => Some(idx) == last_prune_manifest,
                None => true,
            };
            if keep {
                Some(item)
            } else {
                removed = removed.saturating_add(1);
                None
            }
        })
        .collect();
    (items, removed)
}

fn remove_artifact_kind(
    items: Vec<ResponseItem>,
    kind: ArtifactKind,
) -> (Vec<ResponseItem>, usize) {
    let mut removed = 0usize;
    let items = items
        .into_iter()
        .filter_map(|item| {
            if tagged_artifact_kind(&item) == Some(kind) {
                removed = removed.saturating_add(1);
                None
            } else {
                Some(item)
            }
        })
        .collect();
    (items, removed)
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

fn tagged_artifact_kind(item: &ResponseItem) -> Option<ArtifactKind> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if role != "developer" {
        return None;
    }
    let text = crate::compact::content_items_to_text(content)?;
    if has_tagged_block(&text, CONTINUATION_BRIDGE_TAG) {
        return Some(ArtifactKind::ContinuationBridge);
    }
    if has_tagged_block(&text, THREAD_MEMORY_TAG) {
        return Some(ArtifactKind::ThreadMemory);
    }
    if has_tagged_block(&text, PRUNE_MANIFEST_TAG) {
        return Some(ArtifactKind::PruneManifest);
    }
    None
}

fn has_tagged_block(text: &str, tag: &str) -> bool {
    has_tagged_block_prefix(text, "<", tag) && has_tagged_block_prefix(text, "</", tag)
}

fn has_tagged_block_prefix(text: &str, prefix: &str, tag: &str) -> bool {
    text.match_indices(prefix)
        .any(|(idx, _)| text[idx + prefix.len()..].starts_with(tag))
}

fn build_prune_manifest_item(
    original_len: usize,
    final_len: usize,
    removed_count: usize,
) -> CodexResult<ResponseItem> {
    let manifest = json!({
        "schema": "prune_manifest_v1",
        "removed_item_count": removed_count,
        "original_history_len": original_len,
        "final_history_len": final_len,
    });
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    Ok(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "<{PRUNE_MANIFEST_TAG} schema=\"prune_manifest_v1\">\n{manifest_json}\n</{PRUNE_MANIFEST_TAG}>"
            ),
        }],
        end_turn: None,
        phase: None,
    })
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
    fn tagged_block_detection_does_not_require_allocated_tag_strings() {
        assert_eq!(
            has_tagged_block(
                "<thread_memory schema=\"x\">payload</thread_memory>",
                THREAD_MEMORY_TAG,
            ),
            true
        );
        assert_eq!(
            has_tagged_block("<thread_memory>payload", THREAD_MEMORY_TAG),
            false
        );
        assert_eq!(
            has_tagged_block("payload</thread_memory>", THREAD_MEMORY_TAG),
            false
        );
    }
}
