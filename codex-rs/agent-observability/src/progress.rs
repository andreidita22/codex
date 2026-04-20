use codex_protocol::ThreadId;
use codex_protocol::protocol::AgentReasoningEvent;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::EventMsg;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::watch;

const MAX_RECENT_UPDATES: usize = 5;
const MAX_PREVIEW_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProgressPhase {
    Pending,
    Reasoning,
    MessageDrafting,
    Command,
    ToolCall,
    WaitingApproval,
    WaitingUserInput,
    Completed,
    Errored,
    Interrupted,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentBlockReason {
    ExecApproval,
    PatchApproval,
    PermissionsRequest,
    UserInputRequest,
    ElicitationRequest,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentActiveWorkKind {
    Reasoning,
    Message,
    Command,
    Tool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgentActiveWork {
    pub kind: AgentActiveWorkKind,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgentProgressSnapshot {
    pub lifecycle_status: AgentStatus,
    pub phase: AgentProgressPhase,
    pub blocked_on: Option<AgentBlockReason>,
    pub active_work: Option<AgentActiveWork>,
    pub recent_updates: Vec<String>,
    pub latest_visible_message: Option<String>,
    pub final_message: Option<String>,
    pub error_message: Option<String>,
    pub ever_entered_turn: bool,
    pub ever_reported_progress: bool,
    pub last_progress_age_ms: Option<u64>,
    pub seq: u64,
    pub stalled: bool,
}

impl AgentProgressSnapshot {
    pub fn not_found() -> Self {
        Self {
            lifecycle_status: AgentStatus::NotFound,
            phase: AgentProgressPhase::Pending,
            blocked_on: None,
            active_work: None,
            recent_updates: Vec::new(),
            latest_visible_message: None,
            final_message: None,
            error_message: None,
            ever_entered_turn: false,
            ever_reported_progress: false,
            last_progress_age_ms: None,
            seq: 0,
            stalled: false,
        }
    }
}

#[derive(Clone, Debug)]
struct LiveProgressSnapshot {
    spawned_at: Instant,
    turn_started_at: Option<Instant>,
    first_progress_at: Option<Instant>,
    last_progress_at: Option<Instant>,
    phase: AgentProgressPhase,
    blocked_on: Option<AgentBlockReason>,
    active_work: Option<AgentActiveWork>,
    recent_updates: VecDeque<String>,
    latest_visible_message: Option<String>,
    final_message: Option<String>,
    error_message: Option<String>,
    seq: u64,
}

impl LiveProgressSnapshot {
    fn seeded(now: Instant) -> Self {
        Self {
            spawned_at: now,
            turn_started_at: None,
            first_progress_at: None,
            last_progress_at: None,
            phase: AgentProgressPhase::Pending,
            blocked_on: None,
            active_work: None,
            recent_updates: VecDeque::new(),
            latest_visible_message: None,
            final_message: None,
            error_message: None,
            seq: 0,
        }
    }
}

struct TrackedProgressState {
    snapshot: LiveProgressSnapshot,
    seq_tx: watch::Sender<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProgressEventKind {
    Heartbeat,
    Milestone,
    Terminal,
}

impl ProgressEventKind {
    fn marks_progress_start(self) -> bool {
        !matches!(self, Self::Terminal)
    }

    fn bumps_seq(self) -> bool {
        !matches!(self, Self::Heartbeat)
    }
}

impl TrackedProgressState {
    fn seeded(now: Instant) -> Self {
        let (seq_tx, _) = watch::channel(0);
        Self {
            snapshot: LiveProgressSnapshot::seeded(now),
            seq_tx,
        }
    }
}

/// Reduces emitted protocol events into per-thread progress snapshots.
#[derive(Default)]
pub struct ProgressRegistry {
    by_thread: Mutex<HashMap<ThreadId, TrackedProgressState>>,
}

impl ProgressRegistry {
    pub fn seed(&self, thread_id: ThreadId) {
        let mut snapshots = self
            .by_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let _ = snapshots
            .entry(thread_id)
            .or_insert_with(|| TrackedProgressState::seeded(now));
    }

    pub fn subscribe_seq(&self, thread_id: ThreadId) -> watch::Receiver<u64> {
        let mut snapshots = self
            .by_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        snapshots
            .entry(thread_id)
            .or_insert_with(|| TrackedProgressState::seeded(now))
            .seq_tx
            .subscribe()
    }

    pub fn record_event(&self, thread_id: ThreadId, event: &EventMsg) {
        let mut snapshots = self
            .by_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let state = snapshots
            .entry(thread_id)
            .or_insert_with(|| TrackedProgressState::seeded(now));

        match event {
            EventMsg::TurnStarted(_) => {
                state.snapshot.phase = AgentProgressPhase::Pending;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                push_update(&mut state.snapshot, "turn started".to_string());
                note_turn_started(&mut state.snapshot, now);
            }
            EventMsg::AgentReasoning(event) => {
                apply_reasoning_event(state, now, event);
            }
            EventMsg::AgentReasoningDelta(event) => {
                let promotes_initial_progress = state.snapshot.first_progress_at.is_none();
                state.snapshot.phase = AgentProgressPhase::Reasoning;
                state.snapshot.blocked_on = None;
                append_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Reasoning,
                    &event.delta,
                );
                if promotes_initial_progress {
                    push_update(
                        &mut state.snapshot,
                        format!("reasoning: {}", truncate_preview(&event.delta)),
                    );
                }
                note_progress_event(
                    state,
                    now,
                    if promotes_initial_progress {
                        ProgressEventKind::Milestone
                    } else {
                        ProgressEventKind::Heartbeat
                    },
                );
            }
            EventMsg::AgentMessage(event) => {
                state.snapshot.phase = AgentProgressPhase::MessageDrafting;
                state.snapshot.blocked_on = None;
                state.snapshot.latest_visible_message = Some(truncate_preview(&event.message));
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Message,
                    truncate_preview(&event.message),
                );
                push_update(
                    &mut state.snapshot,
                    format!("drafting response: {}", truncate_preview(&event.message)),
                );
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::AgentMessageDelta(event) => {
                let promotes_initial_progress = state.snapshot.first_progress_at.is_none();
                state.snapshot.phase = AgentProgressPhase::MessageDrafting;
                state.snapshot.blocked_on = None;
                append_visible_message(&mut state.snapshot, &event.delta);
                append_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Message,
                    &event.delta,
                );
                if promotes_initial_progress {
                    push_update(
                        &mut state.snapshot,
                        format!("drafting response: {}", truncate_preview(&event.delta)),
                    );
                }
                note_progress_event(
                    state,
                    now,
                    if promotes_initial_progress {
                        ProgressEventKind::Milestone
                    } else {
                        ProgressEventKind::Heartbeat
                    },
                );
            }
            EventMsg::AgentMessageContentDelta(event) => {
                let promotes_initial_progress = state.snapshot.first_progress_at.is_none();
                state.snapshot.phase = AgentProgressPhase::MessageDrafting;
                state.snapshot.blocked_on = None;
                append_visible_message(&mut state.snapshot, &event.delta);
                append_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Message,
                    &event.delta,
                );
                if promotes_initial_progress {
                    push_update(
                        &mut state.snapshot,
                        format!("drafting response: {}", truncate_preview(&event.delta)),
                    );
                }
                note_progress_event(
                    state,
                    now,
                    if promotes_initial_progress {
                        ProgressEventKind::Milestone
                    } else {
                        ProgressEventKind::Heartbeat
                    },
                );
            }
            EventMsg::ExecCommandBegin(event) => {
                state.snapshot.phase = AgentProgressPhase::Command;
                state.snapshot.blocked_on = None;
                let label = truncate_preview(&event.command.join(" "));
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Command,
                    label.clone(),
                );
                push_update(&mut state.snapshot, format!("running command: {label}"));
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::ExecCommandOutputDelta(_) | EventMsg::TerminalInteraction(_) => {
                state.snapshot.phase = AgentProgressPhase::Command;
                note_progress_event(state, now, ProgressEventKind::Heartbeat);
            }
            EventMsg::ExecCommandEnd(event) => {
                push_finished_update(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Command,
                    truncate_preview(&event.command.join(" ")),
                );
                state.snapshot.phase = AgentProgressPhase::Pending;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::McpToolCallBegin(event) => {
                state.snapshot.phase = AgentProgressPhase::ToolCall;
                state.snapshot.blocked_on = None;
                let label = truncate_preview(&format!(
                    "{}::{}",
                    event.invocation.server, event.invocation.tool
                ));
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    label.clone(),
                );
                push_update(&mut state.snapshot, format!("running tool: {label}"));
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::DynamicToolCallRequest(event) => {
                state.snapshot.phase = AgentProgressPhase::ToolCall;
                state.snapshot.blocked_on = None;
                let label = truncate_preview(&event.tool);
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    label.clone(),
                );
                push_update(&mut state.snapshot, format!("running tool: {label}"));
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::WebSearchBegin(_) => {
                state.snapshot.phase = AgentProgressPhase::ToolCall;
                state.snapshot.blocked_on = None;
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    "web_search".to_string(),
                );
                push_update(&mut state.snapshot, "running tool: web_search".to_string());
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::ImageGenerationBegin(_) => {
                state.snapshot.phase = AgentProgressPhase::ToolCall;
                state.snapshot.blocked_on = None;
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    "image_generation".to_string(),
                );
                push_update(
                    &mut state.snapshot,
                    "running tool: image_generation".to_string(),
                );
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::PatchApplyBegin(event) => {
                state.snapshot.phase = AgentProgressPhase::ToolCall;
                state.snapshot.blocked_on = None;
                let label = truncate_preview(&format!(
                    "apply_patch ({} file{})",
                    event.changes.len(),
                    if event.changes.len() == 1 { "" } else { "s" }
                ));
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    label.clone(),
                );
                push_update(&mut state.snapshot, format!("running tool: {label}"));
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::McpToolCallEnd(_)
            | EventMsg::DynamicToolCallResponse(_)
            | EventMsg::WebSearchEnd(_)
            | EventMsg::ImageGenerationEnd(_)
            | EventMsg::PatchApplyEnd(_) => {
                push_finished_update(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Tool,
                    "tool".to_string(),
                );
                state.snapshot.phase = AgentProgressPhase::Pending;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::ExecApprovalRequest(event) => {
                state.snapshot.phase = AgentProgressPhase::WaitingApproval;
                state.snapshot.blocked_on = Some(AgentBlockReason::ExecApproval);
                set_active_work(
                    &mut state.snapshot,
                    AgentActiveWorkKind::Command,
                    truncate_preview(&event.command.join(" ")),
                );
                push_update(&mut state.snapshot, "waiting for exec approval".to_string());
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::ApplyPatchApprovalRequest(_) => {
                state.snapshot.phase = AgentProgressPhase::WaitingApproval;
                state.snapshot.blocked_on = Some(AgentBlockReason::PatchApproval);
                state.snapshot.active_work = None;
                push_update(
                    &mut state.snapshot,
                    "waiting for patch approval".to_string(),
                );
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::RequestPermissions(event) => {
                state.snapshot.phase = AgentProgressPhase::WaitingApproval;
                state.snapshot.blocked_on = Some(AgentBlockReason::PermissionsRequest);
                state.snapshot.active_work = None;
                push_update(
                    &mut state.snapshot,
                    event.reason.as_ref().map_or_else(
                        || "waiting for permissions request".to_string(),
                        |reason| format!("waiting for permissions: {}", truncate_preview(reason)),
                    ),
                );
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::RequestUserInput(_) => {
                state.snapshot.phase = AgentProgressPhase::WaitingUserInput;
                state.snapshot.blocked_on = Some(AgentBlockReason::UserInputRequest);
                state.snapshot.active_work = None;
                push_update(&mut state.snapshot, "waiting for user input".to_string());
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::ElicitationRequest(event) => {
                state.snapshot.phase = AgentProgressPhase::WaitingUserInput;
                state.snapshot.blocked_on = Some(AgentBlockReason::ElicitationRequest);
                state.snapshot.active_work = None;
                push_update(
                    &mut state.snapshot,
                    format!(
                        "waiting for elicitation: {}",
                        truncate_preview(event.request.message())
                    ),
                );
                note_progress_event(state, now, ProgressEventKind::Milestone);
            }
            EventMsg::Warning(event) => {
                push_update(
                    &mut state.snapshot,
                    format!("warning: {}", truncate_preview(&event.message)),
                );
            }
            EventMsg::StreamError(event) => {
                push_update(
                    &mut state.snapshot,
                    format!("stream error: {}", truncate_preview(&event.message)),
                );
            }
            EventMsg::TurnComplete(event) => {
                state.snapshot.phase = AgentProgressPhase::Completed;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                state.snapshot.final_message = event
                    .last_agent_message
                    .as_ref()
                    .map(|message| truncate_preview(message));
                note_progress_event(state, now, ProgressEventKind::Terminal);
            }
            EventMsg::Error(event) => {
                state.snapshot.phase = AgentProgressPhase::Errored;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                state.snapshot.error_message = Some(truncate_preview(&event.message));
                push_update(
                    &mut state.snapshot,
                    format!("error: {}", truncate_preview(&event.message)),
                );
                note_progress_event(state, now, ProgressEventKind::Terminal);
            }
            EventMsg::TurnAborted(event) => {
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                state.snapshot.phase = match event.reason {
                    codex_protocol::protocol::TurnAbortReason::Interrupted => {
                        AgentProgressPhase::Interrupted
                    }
                    _ => AgentProgressPhase::Errored,
                };
                note_progress_event(state, now, ProgressEventKind::Terminal);
            }
            EventMsg::ShutdownComplete => {
                state.snapshot.phase = AgentProgressPhase::Shutdown;
                state.snapshot.blocked_on = None;
                state.snapshot.active_work = None;
                note_progress_event(state, now, ProgressEventKind::Terminal);
            }
            _ => {}
        }
    }

    pub fn inspect(
        &self,
        thread_id: ThreadId,
        lifecycle_status: AgentStatus,
        stalled_after: Duration,
    ) -> AgentProgressSnapshot {
        let snapshot = self
            .by_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&thread_id)
            .map(|state| state.snapshot.clone())
            .unwrap_or_else(|| LiveProgressSnapshot::seeded(Instant::now()));
        let last_progress_age_ms = snapshot
            .last_progress_at
            .map(|instant| duration_millis_u64(instant.elapsed()));
        let stalled = !is_final_for_progress(&lifecycle_status)
            && snapshot.blocked_on.is_none()
            && duration_millis_u64(
                snapshot
                    .last_progress_at
                    .or(snapshot.turn_started_at)
                    .unwrap_or(snapshot.spawned_at)
                    .elapsed(),
            ) > duration_millis_u64(stalled_after);
        let mut resolved = AgentProgressSnapshot {
            lifecycle_status: lifecycle_status.clone(),
            phase: phase_for_status(&lifecycle_status).unwrap_or(snapshot.phase),
            blocked_on: snapshot.blocked_on,
            active_work: snapshot.active_work,
            recent_updates: snapshot.recent_updates.into_iter().collect(),
            latest_visible_message: snapshot.latest_visible_message,
            final_message: snapshot.final_message,
            error_message: snapshot.error_message,
            ever_entered_turn: snapshot.turn_started_at.is_some(),
            ever_reported_progress: snapshot.first_progress_at.is_some(),
            last_progress_age_ms,
            seq: snapshot.seq,
            stalled,
        };
        match &resolved.lifecycle_status {
            AgentStatus::Completed(message) => {
                resolved.blocked_on = None;
                resolved.active_work = None;
                if resolved.final_message.is_none() {
                    resolved.final_message = message.as_ref().map(|text| truncate_preview(text));
                }
            }
            AgentStatus::Errored(message) => {
                resolved.blocked_on = None;
                resolved.active_work = None;
                if resolved.error_message.is_none() {
                    resolved.error_message = Some(truncate_preview(message));
                }
            }
            AgentStatus::Shutdown | AgentStatus::Interrupted => {
                resolved.blocked_on = None;
                resolved.active_work = None;
            }
            AgentStatus::PendingInit | AgentStatus::Running | AgentStatus::NotFound => {}
        }
        resolved
    }

    pub fn remove(&self, thread_id: &ThreadId) {
        self.by_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(thread_id);
    }
}

fn apply_reasoning_event(
    state: &mut TrackedProgressState,
    now: Instant,
    event: &AgentReasoningEvent,
) {
    state.snapshot.phase = AgentProgressPhase::Reasoning;
    state.snapshot.blocked_on = None;
    set_active_work(
        &mut state.snapshot,
        AgentActiveWorkKind::Reasoning,
        truncate_preview(&event.text),
    );
    push_update(
        &mut state.snapshot,
        format!("reasoning: {}", truncate_preview(&event.text)),
    );
    note_progress_event(state, now, ProgressEventKind::Milestone);
}

fn phase_for_status(status: &AgentStatus) -> Option<AgentProgressPhase> {
    match status {
        AgentStatus::PendingInit => Some(AgentProgressPhase::Pending),
        AgentStatus::Interrupted => Some(AgentProgressPhase::Interrupted),
        AgentStatus::Completed(_) => Some(AgentProgressPhase::Completed),
        AgentStatus::Errored(_) => Some(AgentProgressPhase::Errored),
        AgentStatus::Shutdown => Some(AgentProgressPhase::Shutdown),
        AgentStatus::Running | AgentStatus::NotFound => None,
    }
}

fn is_final_for_progress(status: &AgentStatus) -> bool {
    !matches!(status, AgentStatus::PendingInit | AgentStatus::Running)
}

fn push_update(snapshot: &mut LiveProgressSnapshot, update: String) {
    if snapshot.recent_updates.back() == Some(&update) {
        return;
    }
    if snapshot.recent_updates.len() == MAX_RECENT_UPDATES {
        let _ = snapshot.recent_updates.pop_front();
    }
    snapshot.recent_updates.push_back(update);
}

fn set_active_work(snapshot: &mut LiveProgressSnapshot, kind: AgentActiveWorkKind, label: String) {
    snapshot.active_work = Some(AgentActiveWork { kind, label });
}

fn append_active_work(snapshot: &mut LiveProgressSnapshot, kind: AgentActiveWorkKind, delta: &str) {
    let appended = snapshot
        .active_work
        .as_ref()
        .and_then(|work| (work.kind == kind).then(|| work.label.clone()))
        .map(|existing| append_preview(&existing, delta))
        .unwrap_or_else(|| truncate_preview(delta));
    set_active_work(snapshot, kind, appended);
}

fn append_visible_message(snapshot: &mut LiveProgressSnapshot, delta: &str) {
    snapshot.latest_visible_message = Some(
        snapshot
            .latest_visible_message
            .as_ref()
            .map(|existing| append_preview(existing, delta))
            .unwrap_or_else(|| truncate_preview(delta)),
    );
}

fn push_finished_update(
    snapshot: &mut LiveProgressSnapshot,
    kind: AgentActiveWorkKind,
    fallback_label: String,
) {
    let label = snapshot
        .active_work
        .as_ref()
        .and_then(|work| (work.kind == kind).then(|| work.label.clone()))
        .unwrap_or(fallback_label);
    let update = if label.is_empty() {
        match kind {
            AgentActiveWorkKind::Command => "command finished".to_string(),
            AgentActiveWorkKind::Tool => "tool finished".to_string(),
            AgentActiveWorkKind::Message => "message drafting finished".to_string(),
            AgentActiveWorkKind::Reasoning => "reasoning finished".to_string(),
        }
    } else {
        match kind {
            AgentActiveWorkKind::Command => format!("command finished: {label}"),
            AgentActiveWorkKind::Tool => format!("tool finished: {label}"),
            AgentActiveWorkKind::Message => format!("message drafting finished: {label}"),
            AgentActiveWorkKind::Reasoning => format!("reasoning finished: {label}"),
        }
    };
    push_update(snapshot, update);
}

fn append_preview(existing: &str, delta: &str) -> String {
    truncate_preview(&format!("{existing}{delta}"))
}

fn truncate_preview(text: &str) -> String {
    let preview = text.trim();
    if preview.is_empty() {
        return String::new();
    }
    let truncated: String = preview.chars().take(MAX_PREVIEW_CHARS).collect();
    if preview.chars().count() > MAX_PREVIEW_CHARS {
        format!("{truncated}...")
    } else {
        preview.to_string()
    }
}

fn note_turn_started(snapshot: &mut LiveProgressSnapshot, now: Instant) {
    let _ = snapshot.turn_started_at.get_or_insert(now);
}

fn note_progress_event(state: &mut TrackedProgressState, now: Instant, kind: ProgressEventKind) {
    if kind.marks_progress_start() {
        let _ = state.snapshot.first_progress_at.get_or_insert(now);
    }
    state.snapshot.last_progress_at = Some(now);
    if kind.bumps_seq() {
        state.snapshot.seq = state.snapshot.seq.saturating_add(1);
        let _ = state.seq_tx.send_replace(state.snapshot.seq);
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::protocol::ExecCommandBeginEvent;
    use codex_protocol::protocol::ExecCommandEndEvent;
    use codex_protocol::protocol::ExecCommandOutputDeltaEvent;
    use codex_protocol::protocol::ExecCommandStatus;
    use codex_protocol::protocol::ExecOutputStream;
    use codex_protocol::protocol::StreamErrorEvent;
    use codex_protocol::protocol::TurnAbortReason;
    use codex_protocol::protocol::TurnAbortedEvent;
    use codex_protocol::protocol::TurnCompleteEvent;
    use codex_protocol::protocol::TurnStartedEvent;
    use codex_protocol::protocol::WarningEvent;
    use codex_protocol::request_permissions::RequestPermissionProfile;
    use codex_protocol::request_permissions::RequestPermissionsEvent;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use tokio::time::timeout;

    fn thread_id() -> ThreadId {
        ThreadId::from_string("019638d2-f60c-707b-a76d-5ba289fa0d5f").expect("valid thread id")
    }

    #[test]
    fn inspect_reports_spawn_seeded_silent_stall() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        std::thread::sleep(Duration::from_millis(5));

        let snapshot = registry.inspect(
            thread_id,
            AgentStatus::PendingInit,
            Duration::from_millis(1),
        );

        assert_eq!(snapshot.phase, AgentProgressPhase::Pending);
        assert_eq!(snapshot.seq, 0);
        assert_eq!(snapshot.ever_entered_turn, false);
        assert_eq!(snapshot.ever_reported_progress, false);
        assert_eq!(snapshot.last_progress_age_ms, None);
        assert_eq!(snapshot.stalled, true);
    }

    #[test]
    fn inspect_distinguishes_entered_turn_from_meaningful_progress() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        registry.record_event(
            thread_id,
            &EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                started_at: None,
                model_context_window: Some(256_000),
                collaboration_mode_kind: Default::default(),
            }),
        );

        let pending = registry.inspect(thread_id, AgentStatus::Running, Duration::from_secs(1));
        assert_eq!(pending.ever_entered_turn, true);
        assert_eq!(pending.ever_reported_progress, false);
        assert_eq!(pending.seq, 0);

        registry.record_event(
            thread_id,
            &EventMsg::AgentReasoning(AgentReasoningEvent {
                text: "scanning repo".to_string(),
            }),
        );
        let progressed = registry.inspect(thread_id, AgentStatus::Running, Duration::from_secs(1));
        assert_eq!(progressed.ever_entered_turn, true);
        assert_eq!(progressed.ever_reported_progress, true);
        assert_eq!(progressed.seq, 1);
    }

    #[test]
    fn inspect_promotes_initial_message_delta_to_first_milestone() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        registry.record_event(
            thread_id,
            &EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                started_at: None,
                model_context_window: Some(256_000),
                collaboration_mode_kind: Default::default(),
            }),
        );
        registry.record_event(
            thread_id,
            &EventMsg::AgentMessageDelta(codex_protocol::protocol::AgentMessageDeltaEvent {
                delta: "drafting the audit summary".to_string(),
            }),
        );

        let snapshot = registry.inspect(thread_id, AgentStatus::Running, Duration::from_secs(1));

        assert_eq!(snapshot.phase, AgentProgressPhase::MessageDrafting);
        assert_eq!(snapshot.ever_entered_turn, true);
        assert_eq!(snapshot.ever_reported_progress, true);
        assert_eq!(snapshot.seq, 1);
        assert_eq!(
            snapshot.recent_updates,
            vec![
                "turn started".to_string(),
                "drafting response: drafting the audit summary".to_string(),
            ]
        );
        assert_eq!(
            snapshot.active_work,
            Some(AgentActiveWork {
                kind: AgentActiveWorkKind::Message,
                label: "drafting the audit summary".to_string(),
            })
        );
    }

    #[test]
    fn inspect_reports_permission_blockers() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        registry.record_event(
            thread_id,
            &EventMsg::RequestPermissions(RequestPermissionsEvent {
                call_id: "call-1".to_string(),
                turn_id: "turn-1".to_string(),
                reason: Some("needs workspace write".to_string()),
                permissions: RequestPermissionProfile::default(),
            }),
        );

        let snapshot = registry.inspect(thread_id, AgentStatus::Running, Duration::from_millis(50));

        assert_eq!(snapshot.phase, AgentProgressPhase::WaitingApproval);
        assert_eq!(
            snapshot.blocked_on,
            Some(AgentBlockReason::PermissionsRequest)
        );
        assert_eq!(snapshot.ever_entered_turn, false);
        assert_eq!(snapshot.ever_reported_progress, true);
        assert_eq!(snapshot.seq, 1);
        assert_eq!(snapshot.stalled, false);
        assert_eq!(
            snapshot.recent_updates,
            vec!["waiting for permissions: needs workspace write".to_string()]
        );
    }

    #[test]
    fn inspect_preserves_completion_and_abort_state() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        registry.record_event(
            thread_id,
            &EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                started_at: None,
                model_context_window: Some(256_000),
                collaboration_mode_kind: Default::default(),
            }),
        );
        registry.record_event(
            thread_id,
            &EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: "turn-1".to_string(),
                last_agent_message: Some("finished the audit".to_string()),
                completed_at: None,
                duration_ms: None,
            }),
        );

        let snapshot = registry.inspect(
            thread_id,
            AgentStatus::Completed(Some("finished the audit".to_string())),
            Duration::from_secs(1),
        );

        assert_eq!(snapshot.phase, AgentProgressPhase::Completed);
        assert_eq!(
            snapshot.final_message,
            Some("finished the audit".to_string())
        );
        assert_eq!(snapshot.active_work, None);

        registry.record_event(
            thread_id,
            &EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: Some("turn-2".to_string()),
                reason: TurnAbortReason::Interrupted,
                completed_at: None,
                duration_ms: None,
            }),
        );
        let interrupted =
            registry.inspect(thread_id, AgentStatus::Interrupted, Duration::from_secs(1));
        assert_eq!(interrupted.phase, AgentProgressPhase::Interrupted);
        assert_eq!(interrupted.active_work, None);
        assert_eq!(interrupted.stalled, false);
    }

    #[tokio::test]
    async fn subscribe_seq_ignores_non_material_updates() {
        let registry = ProgressRegistry::default();
        let thread_id = thread_id();
        registry.seed(thread_id);
        let mut seq_rx = registry.subscribe_seq(thread_id);

        registry.record_event(
            thread_id,
            &EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                started_at: None,
                model_context_window: Some(256_000),
                collaboration_mode_kind: Default::default(),
            }),
        );
        assert!(
            timeout(Duration::from_millis(20), seq_rx.changed())
                .await
                .is_err()
        );

        registry.record_event(
            thread_id,
            &EventMsg::Warning(WarningEvent {
                message: "still initializing".to_string(),
            }),
        );
        assert!(
            timeout(Duration::from_millis(20), seq_rx.changed())
                .await
                .is_err()
        );

        registry.record_event(
            thread_id,
            &EventMsg::StreamError(StreamErrorEvent {
                message: "temporary disconnect".to_string(),
                codex_error_info: None,
                additional_details: None,
            }),
        );
        assert!(
            timeout(Duration::from_millis(20), seq_rx.changed())
                .await
                .is_err()
        );

        registry.record_event(
            thread_id,
            &EventMsg::AgentReasoning(AgentReasoningEvent {
                text: "scanning repo".to_string(),
            }),
        );
        seq_rx
            .changed()
            .await
            .expect("material progress should wake watchers");
        assert_eq!(*seq_rx.borrow(), 1);

        registry.record_event(
            thread_id,
            &EventMsg::AgentReasoningDelta(codex_protocol::protocol::AgentReasoningDeltaEvent {
                delta: " and cataloging modules".to_string(),
            }),
        );
        assert!(
            timeout(Duration::from_millis(20), seq_rx.changed())
                .await
                .is_err()
        );

        registry.record_event(
            thread_id,
            &EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "call-1".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                command: vec!["rg".to_string(), "--files".to_string()],
                cwd: AbsolutePathBuf::from_absolute_path("/tmp").expect("/tmp is absolute"),
                parsed_cmd: vec![],
                source: Default::default(),
                interaction_input: None,
            }),
        );
        seq_rx
            .changed()
            .await
            .expect("command begin should count as a material milestone");
        assert_eq!(*seq_rx.borrow(), 2);

        registry.record_event(
            thread_id,
            &EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
                call_id: "call-1".to_string(),
                stream: ExecOutputStream::Stdout,
                chunk: b"core/src/lib.rs\n".to_vec(),
            }),
        );
        assert!(
            timeout(Duration::from_millis(20), seq_rx.changed())
                .await
                .is_err()
        );

        let snapshot = registry.inspect(thread_id, AgentStatus::Running, Duration::from_secs(1));
        assert_eq!(snapshot.phase, AgentProgressPhase::Command);
        assert_eq!(snapshot.seq, 2);
        assert_eq!(
            snapshot.active_work,
            Some(AgentActiveWork {
                kind: AgentActiveWorkKind::Command,
                label: "rg --files".to_string(),
            })
        );

        registry.record_event(
            thread_id,
            &EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: "call-1".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                command: vec!["rg".to_string(), "--files".to_string()],
                cwd: AbsolutePathBuf::from_absolute_path("/tmp").expect("/tmp is absolute"),
                parsed_cmd: vec![],
                source: Default::default(),
                interaction_input: None,
                stdout: "core/src/lib.rs\n".to_string(),
                stderr: String::new(),
                aggregated_output: "core/src/lib.rs\n".to_string(),
                exit_code: 0,
                duration: Duration::from_millis(25),
                formatted_output: "core/src/lib.rs\n".to_string(),
                status: ExecCommandStatus::Completed,
            }),
        );
        seq_rx
            .changed()
            .await
            .expect("command end should count as a material milestone");
        assert_eq!(*seq_rx.borrow(), 3);

        let ended = registry.inspect(thread_id, AgentStatus::Running, Duration::from_secs(1));
        assert_eq!(ended.phase, AgentProgressPhase::Pending);
        assert_eq!(ended.active_work, None);
        assert_eq!(ended.seq, 3);
        assert_eq!(
            ended.recent_updates,
            vec![
                "warning: still initializing".to_string(),
                "stream error: temporary disconnect".to_string(),
                "reasoning: scanning repo".to_string(),
                "running command: rg --files".to_string(),
                "command finished: rg --files".to_string(),
            ]
        );
    }
}
