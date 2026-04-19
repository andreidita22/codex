use std::sync::Arc;
use std::time::Instant;

use crate::Prompt;
use crate::client::ModelClientSession;
use crate::client_common::ResponseEvent;
#[cfg(test)]
use crate::codex::PreviousTurnSettings;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::codex::get_last_assistant_message_from_turn;
use crate::config::CompactionEngine;
use crate::context_maintenance_config::resolve_context_maintenance_request_context;
use crate::context_maintenance_runtime::CompactInvocationTiming;
use crate::context_maintenance_runtime::execute_requested_artifact;
use crate::context_maintenance_runtime::runtime_plan_for_compact;
use crate::continuation_bridge::generate_continuation_bridge_item;
use crate::governance::thread_memory::generate_thread_memory_item;
use crate::util::backoff;
use codex_analytics::CodexCompactionEvent;
use codex_analytics::CompactionImplementation;
use codex_analytics::CompactionPhase;
use codex_analytics::CompactionReason;
use codex_analytics::CompactionStatus;
use codex_analytics::CompactionStrategy;
use codex_analytics::CompactionTrigger;
use codex_analytics::now_unix_seconds;
use codex_context_maintenance_policy::ArtifactKind;
use codex_context_maintenance_policy::apply_retention_directive;
use codex_context_maintenance_policy::insert_initial_context_before_last_real_user_or_summary as insert_initial_context_before_last_real_user_or_summary_impl;
use codex_context_maintenance_policy::insert_items_before_last_summary_or_compaction as insert_items_before_last_summary_or_compaction_impl;
#[cfg(test)]
use codex_context_maintenance_policy::retain_recent_raw_conversation_messages as retain_recent_raw_conversation_messages_impl;
use codex_features::Feature;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::items::ContextCompactionItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::CompactedItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TurnStartedEvent;
use codex_protocol::protocol::WarningEvent;
use codex_protocol::user_input::UserInput;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::truncate_text;
use futures::prelude::*;
use tracing::error;

pub const SUMMARIZATION_PROMPT: &str = include_str!("../templates/compact/prompt.md");
pub const SUMMARY_PREFIX: &str = include_str!("../templates/compact/summary_prefix.md");
const COMPACT_USER_MESSAGE_MAX_TOKENS: usize = 20_000;
/// Controls whether compaction replacement history must include initial context.
///
/// Pre-turn/manual compaction variants use `DoNotInject`: they replace history with a summary and
/// clear `reference_context_item`, so the next regular turn will fully reinject initial context
/// after compaction.
///
/// Mid-turn compaction injects initial context above the last real user message or summary because
/// the model is trained to see the compaction summary as the last item in replacement history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitialContextInjection {
    BeforeLastRealUserOrSummary,
    DoNotInject,
}

pub(crate) fn should_use_remote_compact_task(turn_context: &TurnContext) -> bool {
    !matches!(
        turn_context.config.compaction_engine.unwrap_or_default(),
        CompactionEngine::LocalPure
    )
}

pub(crate) async fn run_inline_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    timing: CompactInvocationTiming,
    reason: CompactionReason,
    phase: CompactionPhase,
) -> CodexResult<()> {
    let prompt = turn_context.compact_prompt().to_string();
    let input = vec![UserInput::Text {
        text: prompt,
        // Compaction prompt is synthesized; no UI element ranges to preserve.
        text_elements: Vec::new(),
    }];

    run_compact_task_inner(
        sess,
        turn_context,
        input,
        timing,
        CompactionTrigger::Auto,
        reason,
        phase,
    )
    .await?;
    Ok(())
}

pub(crate) async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
) -> CodexResult<()> {
    let start_event = EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: turn_context.sub_id.clone(),
        started_at: turn_context.turn_timing_state.started_at_unix_secs().await,
        model_context_window: turn_context.model_context_window(),
        collaboration_mode_kind: turn_context.collaboration_mode.mode,
    });
    sess.send_event(&turn_context, start_event).await;
    run_compact_task_inner(
        sess.clone(),
        turn_context,
        input,
        CompactInvocationTiming::TurnBoundary,
        CompactionTrigger::Manual,
        CompactionReason::UserRequested,
        CompactionPhase::StandaloneTurn,
    )
    .await
}

async fn run_compact_task_inner(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    timing: CompactInvocationTiming,
    trigger: CompactionTrigger,
    reason: CompactionReason,
    phase: CompactionPhase,
) -> CodexResult<()> {
    let attempt = CompactionAnalyticsAttempt::begin(
        sess.as_ref(),
        turn_context.as_ref(),
        trigger,
        reason,
        CompactionImplementation::Responses,
        phase,
    )
    .await;
    let result =
        run_compact_task_inner_impl(Arc::clone(&sess), Arc::clone(&turn_context), input, timing)
            .await;
    attempt
        .track(
            sess.as_ref(),
            compaction_status_from_result(&result),
            result.as_ref().err().map(ToString::to_string),
        )
        .await;
    result
}

async fn run_compact_task_inner_impl(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    timing: CompactInvocationTiming,
) -> CodexResult<()> {
    let runtime_plan = runtime_plan_for_compact(turn_context.as_ref(), timing)?;
    let injection_placement = runtime_plan.context_injection_placement();
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(&turn_context, &compaction_item)
        .await;
    let initial_input_for_turn: ResponseInputItem = ResponseInputItem::from(input);

    let mut history = sess.clone_history().await;
    let history_for_artifacts = history
        .clone()
        .for_prompt(&turn_context.model_info.input_modalities);
    let thread_memory_item = match execute_requested_artifact(
        runtime_plan.artifact_requiredness(ArtifactKind::ThreadMemory),
        "thread memory",
        "before local compaction",
        || generate_thread_memory_item(&sess, turn_context.as_ref(), history_for_artifacts.clone()),
    )
    .await
    {
        Ok(item) => item,
        Err(err) => {
            sess.send_event(
                &turn_context,
                EventMsg::Error(err.to_error_event(/*message_prefix*/ None)),
            )
            .await;
            return Err(err);
        }
    };
    let continuation_bridge_item = match execute_requested_artifact(
        runtime_plan.artifact_requiredness(ArtifactKind::ContinuationBridge),
        "continuation bridge",
        "before local compaction",
        || generate_continuation_bridge_item(&sess, turn_context.as_ref(), history_for_artifacts),
    )
    .await
    {
        Ok(item) => item,
        Err(err) => {
            sess.send_event(
                &turn_context,
                EventMsg::Error(err.to_error_event(/*message_prefix*/ None)),
            )
            .await;
            return Err(err);
        }
    };
    let authoritative_items: Vec<_> = thread_memory_item
        .into_iter()
        .chain(continuation_bridge_item)
        .collect();
    let request_context =
        resolve_context_maintenance_request_context(sess.as_ref(), turn_context.as_ref()).await;
    history.record_items(
        &[initial_input_for_turn.into()],
        turn_context.truncation_policy,
    );

    let mut truncated_count = 0usize;

    let max_retries = turn_context.provider.stream_max_retries();
    let mut retries = 0;
    let mut client_session = sess.services.model_client.new_session();
    // Reuse one client session so turn-scoped state (sticky routing, websocket incremental
    // request tracking)
    // survives retries within this compact turn.

    loop {
        // Clone is required because of the loop
        let turn_input = history
            .clone()
            .for_prompt(&request_context.model_info.input_modalities);
        let turn_input_len = turn_input.len();
        let prompt = Prompt {
            input: turn_input,
            base_instructions: sess.get_base_instructions().await,
            personality: turn_context.personality,
            ..Default::default()
        };
        let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
        let attempt_result = drain_to_completed(
            &sess,
            turn_context.as_ref(),
            &mut client_session,
            turn_metadata_header.as_deref(),
            &prompt,
            &request_context,
        )
        .await;

        match attempt_result {
            Ok(()) => {
                if truncated_count > 0 {
                    sess.notify_background_event(
                        turn_context.as_ref(),
                        format!(
                            "Trimmed {truncated_count} older thread item(s) before compacting so the prompt fits the model context window."
                        ),
                    )
                    .await;
                }
                break;
            }
            Err(CodexErr::Interrupted) => {
                return Err(CodexErr::Interrupted);
            }
            Err(e @ CodexErr::ContextWindowExceeded) => {
                if turn_input_len > 1 {
                    // Trim from the beginning to preserve cache (prefix-based) and keep recent messages intact.
                    error!(
                        "Context window exceeded while compacting; removing oldest history item. Error: {e}"
                    );
                    history.remove_first_item();
                    truncated_count += 1;
                    retries = 0;
                    continue;
                }
                sess.set_total_tokens_full(turn_context.as_ref()).await;
                let event = EventMsg::Error(e.to_error_event(/*message_prefix*/ None));
                sess.send_event(&turn_context, event).await;
                return Err(e);
            }
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        turn_context.as_ref(),
                        format!("Reconnecting... {retries}/{max_retries}"),
                        e,
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = EventMsg::Error(e.to_error_event(/*message_prefix*/ None));
                    sess.send_event(&turn_context, event).await;
                    return Err(e);
                }
            }
        }
    }

    let history_snapshot = sess.clone_history().await;
    let history_items = history_snapshot.raw_items();
    let summary_suffix = get_last_assistant_message_from_turn(history_items).unwrap_or_default();
    let summary_text = format!("{SUMMARY_PREFIX}\n{summary_suffix}");
    let user_messages = collect_user_messages(history_items);

    let mut new_history = build_compacted_history(Vec::new(), &user_messages, &summary_text);

    if matches!(
        injection_placement,
        InitialContextInjection::BeforeLastRealUserOrSummary
    ) {
        let initial_context = sess.build_initial_context(turn_context.as_ref()).await;
        new_history =
            insert_initial_context_before_last_real_user_or_summary(new_history, initial_context);
    }
    if !authoritative_items.is_empty() {
        new_history =
            insert_items_before_last_summary_or_compaction(new_history, authoritative_items);
    }
    new_history = apply_retention_directive(
        new_history,
        runtime_plan.retention_directive(),
        is_raw_conversation_message,
    )
    .items;
    let ghost_snapshots: Vec<ResponseItem> = history_items
        .iter()
        .filter(|item| matches!(item, ResponseItem::GhostSnapshot { .. }))
        .cloned()
        .collect();
    new_history.extend(ghost_snapshots);
    let reference_context_item = match injection_placement {
        InitialContextInjection::DoNotInject => None,
        InitialContextInjection::BeforeLastRealUserOrSummary => {
            Some(turn_context.to_turn_context_item())
        }
    };
    let compacted_item = CompactedItem {
        message: summary_text.clone(),
        replacement_history: Some(new_history.clone()),
    };
    sess.replace_compacted_history(new_history, reference_context_item, compacted_item)
        .await;
    client_session.reset_websocket_session();
    sess.recompute_token_usage(&turn_context).await;

    sess.emit_turn_item_completed(&turn_context, compaction_item)
        .await;
    let warning = EventMsg::Warning(WarningEvent {
        message: "Heads up: Long threads and multiple compactions can cause the model to be less accurate. Start a new thread when possible to keep threads small and targeted.".to_string(),
    });
    sess.send_event(&turn_context, warning).await;
    Ok(())
}

pub(crate) struct CompactionAnalyticsAttempt {
    enabled: bool,
    thread_id: String,
    turn_id: String,
    trigger: CompactionTrigger,
    reason: CompactionReason,
    implementation: CompactionImplementation,
    phase: CompactionPhase,
    active_context_tokens_before: i64,
    started_at: u64,
    start_instant: Instant,
}

impl CompactionAnalyticsAttempt {
    pub(crate) async fn begin(
        sess: &Session,
        turn_context: &TurnContext,
        trigger: CompactionTrigger,
        reason: CompactionReason,
        implementation: CompactionImplementation,
        phase: CompactionPhase,
    ) -> Self {
        let enabled = sess.enabled(Feature::GeneralAnalytics);
        let active_context_tokens_before = sess.get_total_token_usage().await;
        Self {
            enabled,
            thread_id: sess.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
            trigger,
            reason,
            implementation,
            phase,
            active_context_tokens_before,
            started_at: now_unix_seconds(),
            start_instant: Instant::now(),
        }
    }

    pub(crate) async fn track(
        self,
        sess: &Session,
        status: CompactionStatus,
        error: Option<String>,
    ) {
        if !self.enabled {
            return;
        }
        let active_context_tokens_after = sess.get_total_token_usage().await;
        sess.services
            .analytics_events_client
            .track_compaction(CodexCompactionEvent {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                trigger: self.trigger,
                reason: self.reason,
                implementation: self.implementation,
                phase: self.phase,
                strategy: CompactionStrategy::Memento,
                status,
                error,
                active_context_tokens_before: self.active_context_tokens_before,
                active_context_tokens_after,
                started_at: self.started_at,
                completed_at: now_unix_seconds(),
                duration_ms: Some(
                    u64::try_from(self.start_instant.elapsed().as_millis()).unwrap_or(u64::MAX),
                ),
            });
    }
}

pub(crate) fn compaction_status_from_result<T>(result: &CodexResult<T>) -> CompactionStatus {
    match result {
        Ok(_) => CompactionStatus::Completed,
        Err(CodexErr::Interrupted | CodexErr::TurnAborted) => CompactionStatus::Interrupted,
        Err(_) => CompactionStatus::Failed,
    }
}

pub fn content_items_to_text(content: &[ContentItem]) -> Option<String> {
    let mut pieces = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    pieces.push(text.as_str());
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

pub(crate) fn collect_user_messages(items: &[ResponseItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match crate::event_mapping::parse_turn_item(item) {
            Some(TurnItem::UserMessage(user)) => {
                if is_summary_message(&user.message()) {
                    None
                } else {
                    Some(user.message())
                }
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn is_raw_conversation_message(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, .. } if role == "assistant" => true,
        ResponseItem::Message { role, .. } if role == "user" => is_real_user_message(item),
        _ => false,
    }
}

pub(crate) fn is_real_user_message(item: &ResponseItem) -> bool {
    matches!(
        crate::event_mapping::parse_turn_item(item),
        Some(TurnItem::UserMessage(user)) if !is_summary_message(&user.message())
    )
}

pub(crate) fn is_user_or_summary_message(item: &ResponseItem) -> bool {
    matches!(
        crate::event_mapping::parse_turn_item(item),
        Some(TurnItem::UserMessage(_))
    )
}

#[cfg(test)]
pub(crate) fn retain_recent_raw_conversation_messages(
    compacted_history: Vec<ResponseItem>,
    max_messages: usize,
) -> Vec<ResponseItem> {
    retain_recent_raw_conversation_messages_impl(
        compacted_history,
        max_messages,
        is_raw_conversation_message,
    )
}

pub(crate) fn is_summary_message(message: &str) -> bool {
    message.starts_with(format!("{SUMMARY_PREFIX}\n").as_str())
}

/// Inserts canonical initial context into compacted replacement history at the
/// model-expected boundary.
///
/// Placement rules:
/// - Prefer immediately before the last real user message.
/// - If no real user messages remain, insert before the compaction summary so
///   the summary stays last.
/// - If there are no user messages, insert before the last compaction item so
///   that item remains last (remote compaction may return only compaction items).
/// - If there are no user messages or compaction items, append the context.
pub(crate) fn insert_initial_context_before_last_real_user_or_summary(
    compacted_history: Vec<ResponseItem>,
    initial_context: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    insert_initial_context_before_last_real_user_or_summary_impl(
        compacted_history,
        initial_context,
        is_real_user_message,
        is_user_or_summary_message,
    )
}

/// Inserts authoritative post-compaction artifacts before the last summary or compaction marker.
///
/// Placement rules:
/// - Prefer immediately before the last compaction item, if present.
/// - Otherwise, insert before the last user-message-like item (including summaries).
/// - If neither exists, append the items.
pub(crate) fn insert_items_before_last_summary_or_compaction(
    compacted_history: Vec<ResponseItem>,
    items: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    insert_items_before_last_summary_or_compaction_impl(
        compacted_history,
        items,
        is_user_or_summary_message,
    )
}

pub(crate) fn build_compacted_history(
    initial_context: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
) -> Vec<ResponseItem> {
    build_compacted_history_with_limit(
        initial_context,
        user_messages,
        summary_text,
        COMPACT_USER_MESSAGE_MAX_TOKENS,
    )
}

fn build_compacted_history_with_limit(
    mut history: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
    max_tokens: usize,
) -> Vec<ResponseItem> {
    let mut selected_messages: Vec<String> = Vec::new();
    if max_tokens > 0 {
        let mut remaining = max_tokens;
        for message in user_messages.iter().rev() {
            if remaining == 0 {
                break;
            }
            let tokens = approx_token_count(message);
            if tokens <= remaining {
                selected_messages.push(message.clone());
                remaining = remaining.saturating_sub(tokens);
            } else {
                let truncated = truncate_text(message, TruncationPolicy::Tokens(remaining));
                selected_messages.push(truncated);
                break;
            }
        }
        selected_messages.reverse();
    }

    for message in &selected_messages {
        history.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: message.clone(),
            }],
            end_turn: None,
            phase: None,
        });
    }

    let summary_text = if summary_text.is_empty() {
        "(no summary available)".to_string()
    } else {
        summary_text.to_string()
    };

    history.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: summary_text }],
        end_turn: None,
        phase: None,
    });

    history
}

async fn drain_to_completed(
    sess: &Session,
    turn_context: &TurnContext,
    client_session: &mut ModelClientSession,
    turn_metadata_header: Option<&str>,
    prompt: &Prompt,
    request_context: &crate::context_maintenance_config::ContextMaintenanceRequestContext,
) -> CodexResult<()> {
    let mut stream = client_session
        .stream(
            prompt,
            &request_context.model_info,
            &request_context.session_telemetry,
            request_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header,
        )
        .await?;
    loop {
        let maybe_event = stream.next().await;
        let Some(event) = maybe_event else {
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };
        match event {
            Ok(ResponseEvent::OutputItemDone(item)) => {
                sess.record_into_history(std::slice::from_ref(&item), turn_context)
                    .await;
            }
            Ok(ResponseEvent::ServerReasoningIncluded(included)) => {
                sess.set_server_reasoning_included(included).await;
            }
            Ok(ResponseEvent::RateLimits(snapshot)) => {
                sess.update_rate_limits(turn_context, snapshot).await;
            }
            Ok(ResponseEvent::Completed { token_usage, .. }) => {
                sess.update_token_usage_info(turn_context, token_usage.as_ref())
                    .await;
                return Ok(());
            }
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
