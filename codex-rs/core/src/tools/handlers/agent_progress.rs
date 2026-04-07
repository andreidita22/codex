use crate::agent::agent_resolver::resolve_agent_target;
use crate::agent::progress::AgentProgressPhase;
use crate::agent::progress::AgentProgressSnapshot;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::multi_agents_common::DEFAULT_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MAX_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MIN_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::collab_agent_error;
use crate::tools::handlers::multi_agents_common::function_arguments;
use crate::tools::handlers::multi_agents_common::tool_output_code_mode_result;
use crate::tools::handlers::multi_agents_common::tool_output_json_text;
use crate::tools::handlers::multi_agents_common::tool_output_response_item;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseInputItem;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::time::Duration;
use tokio::time::Instant;
use tokio::time::timeout_at;

const DEFAULT_STALLED_AFTER_MS: i64 = 120_000;

pub(crate) struct InspectAgentProgressHandler;
pub(crate) struct WaitForAgentProgressHandler;

#[async_trait]
impl ToolHandler for InspectAgentProgressHandler {
    type Output = InspectAgentProgressResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: InspectAgentProgressArgs = parse_arguments(&arguments)?;
        let target = parse_target(&args.target)?;
        let stalled_after = parse_positive_millis(
            args.stalled_after_ms,
            DEFAULT_STALLED_AFTER_MS,
            "stalled_after_ms",
        )?;
        let (canonical_target, snapshot) =
            inspect_progress_target(&session, &turn, target, stalled_after).await?;

        Ok(InspectAgentProgressResult {
            canonical_target,
            snapshot,
        })
    }
}

#[async_trait]
impl ToolHandler for WaitForAgentProgressHandler {
    type Output = WaitForAgentProgressResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: WaitForAgentProgressArgs = parse_arguments(&arguments)?;
        let target = parse_target(&args.target)?;
        let stalled_after = parse_positive_millis(
            args.stalled_after_ms,
            DEFAULT_STALLED_AFTER_MS,
            "stalled_after_ms",
        )?;
        let timeout_ms = parse_wait_timeout_ms(args.timeout_ms)?;
        let until_phases = args.until_phases.unwrap_or_default();

        let agent_id = resolve_agent_target(&session, &turn, target).await?;
        let canonical_target = canonical_agent_target(&session, agent_id);
        let mut progress_seq_rx = session
            .services
            .agent_control
            .subscribe_progress_seq(agent_id)
            .await
            .map_err(|err| collab_agent_error(agent_id, err))?;
        let baseline_seq = args.since_seq.unwrap_or(*progress_seq_rx.borrow());
        let snapshot = session
            .services
            .agent_control
            .inspect_agent_progress(agent_id, Duration::from_millis(stalled_after))
            .await;

        if snapshot.seq > baseline_seq {
            return Ok(WaitForAgentProgressResult::matched(
                canonical_target.clone(),
                snapshot,
                WaitForAgentProgressMatchReason::SeqAdvanced,
            ));
        }
        if until_phases.contains(&snapshot.phase) {
            return Ok(WaitForAgentProgressResult::matched(
                canonical_target.clone(),
                snapshot,
                WaitForAgentProgressMatchReason::AlreadySatisfied,
            ));
        }
        if *progress_seq_rx.borrow() > baseline_seq {
            let snapshot = session
                .services
                .agent_control
                .inspect_agent_progress(agent_id, Duration::from_millis(stalled_after))
                .await;
            return Ok(WaitForAgentProgressResult::matched(
                canonical_target.clone(),
                snapshot,
                WaitForAgentProgressMatchReason::SeqAdvanced,
            ));
        }
        let _ = progress_seq_rx.borrow_and_update();

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            match timeout_at(deadline, progress_seq_rx.changed()).await {
                Ok(Ok(())) => {
                    let snapshot = session
                        .services
                        .agent_control
                        .inspect_agent_progress(agent_id, Duration::from_millis(stalled_after))
                        .await;
                    if let Some(match_reason) =
                        classify_wait_match(&snapshot, baseline_seq, &until_phases)
                    {
                        return Ok(WaitForAgentProgressResult::matched(
                            canonical_target.clone(),
                            snapshot,
                            match_reason,
                        ));
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    let snapshot = session
                        .services
                        .agent_control
                        .inspect_agent_progress(agent_id, Duration::from_millis(stalled_after))
                        .await;
                    if let Some(match_reason) =
                        classify_wait_match(&snapshot, baseline_seq, &until_phases)
                    {
                        return Ok(WaitForAgentProgressResult::matched(
                            canonical_target.clone(),
                            snapshot,
                            match_reason,
                        ));
                    }
                    return Ok(WaitForAgentProgressResult::timed_out(
                        canonical_target,
                        snapshot,
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct InspectAgentProgressArgs {
    target: String,
    stalled_after_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct WaitForAgentProgressArgs {
    target: String,
    since_seq: Option<u64>,
    until_phases: Option<Vec<AgentProgressPhase>>,
    timeout_ms: Option<i64>,
    stalled_after_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CanonicalAgentTarget {
    thread_id: String,
    task_name: String,
    nickname: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InspectAgentProgressResult {
    canonical_target: CanonicalAgentTarget,
    #[serde(flatten)]
    snapshot: AgentProgressSnapshot,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WaitForAgentProgressMatchReason {
    AlreadySatisfied,
    SeqAdvanced,
    PhaseMatched,
    TimedOut,
}

#[derive(Debug, Serialize)]
pub(crate) struct WaitForAgentProgressResult {
    message: String,
    timed_out: bool,
    match_reason: WaitForAgentProgressMatchReason,
    canonical_target: CanonicalAgentTarget,
    #[serde(flatten)]
    snapshot: AgentProgressSnapshot,
}

impl WaitForAgentProgressResult {
    fn matched(
        canonical_target: CanonicalAgentTarget,
        snapshot: AgentProgressSnapshot,
        match_reason: WaitForAgentProgressMatchReason,
    ) -> Self {
        let message = match match_reason {
            WaitForAgentProgressMatchReason::AlreadySatisfied => {
                "Agent already satisfied wait condition."
            }
            WaitForAgentProgressMatchReason::SeqAdvanced
            | WaitForAgentProgressMatchReason::PhaseMatched => "Observed agent progress.",
            WaitForAgentProgressMatchReason::TimedOut => "Wait for agent progress timed out.",
        };
        Self {
            message: message.to_string(),
            timed_out: false,
            match_reason,
            canonical_target,
            snapshot,
        }
    }

    fn timed_out(canonical_target: CanonicalAgentTarget, snapshot: AgentProgressSnapshot) -> Self {
        Self {
            message: "Wait for agent progress timed out.".to_string(),
            timed_out: true,
            match_reason: WaitForAgentProgressMatchReason::TimedOut,
            canonical_target,
            snapshot,
        }
    }
}

impl ToolOutput for InspectAgentProgressResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "inspect_agent_progress")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "inspect_agent_progress")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "inspect_agent_progress")
    }
}

impl ToolOutput for WaitForAgentProgressResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "wait_for_agent_progress")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(
            call_id,
            payload,
            self,
            Some(true),
            "wait_for_agent_progress",
        )
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "wait_for_agent_progress")
    }
}

fn parse_target(target: &str) -> Result<&str, FunctionCallError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "target must not be empty".to_string(),
        ));
    }
    Ok(target)
}

fn parse_positive_millis(
    value: Option<i64>,
    default_ms: i64,
    field_name: &str,
) -> Result<u64, FunctionCallError> {
    let value = value.unwrap_or(default_ms);
    let value = u64::try_from(value).map_err(|_| {
        FunctionCallError::RespondToModel(format!("{field_name} must be greater than zero"))
    })?;
    if value == 0 {
        return Err(FunctionCallError::RespondToModel(format!(
            "{field_name} must be greater than zero"
        )));
    }
    Ok(value)
}

fn parse_wait_timeout_ms(timeout_ms: Option<i64>) -> Result<u64, FunctionCallError> {
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_WAIT_TIMEOUT_MS);
    let timeout_ms = match timeout_ms {
        ms if ms <= 0 => {
            return Err(FunctionCallError::RespondToModel(
                "timeout_ms must be greater than zero".to_string(),
            ));
        }
        ms => ms.clamp(MIN_WAIT_TIMEOUT_MS, MAX_WAIT_TIMEOUT_MS),
    };
    u64::try_from(timeout_ms).map_err(|_| {
        FunctionCallError::RespondToModel("timeout_ms must be greater than zero".to_string())
    })
}

async fn inspect_progress_target(
    session: &std::sync::Arc<crate::codex::Session>,
    turn: &std::sync::Arc<crate::codex::TurnContext>,
    target: &str,
    stalled_after_ms: u64,
) -> Result<(CanonicalAgentTarget, AgentProgressSnapshot), FunctionCallError> {
    let agent_id = resolve_agent_target(session, turn, target).await?;
    let canonical_target = canonical_agent_target(session, agent_id);
    let snapshot = session
        .services
        .agent_control
        .inspect_agent_progress(agent_id, Duration::from_millis(stalled_after_ms))
        .await;
    Ok((canonical_target, snapshot))
}

fn canonical_agent_target(
    session: &std::sync::Arc<crate::codex::Session>,
    agent_id: ThreadId,
) -> CanonicalAgentTarget {
    let metadata = session.services.agent_control.get_agent_metadata(agent_id);
    CanonicalAgentTarget {
        thread_id: agent_id.to_string(),
        task_name: metadata
            .as_ref()
            .and_then(|metadata| metadata.agent_path.as_ref())
            .map(ToString::to_string)
            .unwrap_or_else(|| agent_id.to_string()),
        nickname: metadata.and_then(|metadata| metadata.agent_nickname),
    }
}

fn classify_wait_match(
    snapshot: &AgentProgressSnapshot,
    baseline_seq: u64,
    until_phases: &[AgentProgressPhase],
) -> Option<WaitForAgentProgressMatchReason> {
    if snapshot.seq > baseline_seq {
        Some(WaitForAgentProgressMatchReason::SeqAdvanced)
    } else if until_phases.contains(&snapshot.phase) {
        Some(WaitForAgentProgressMatchReason::PhaseMatched)
    } else {
        None
    }
}
