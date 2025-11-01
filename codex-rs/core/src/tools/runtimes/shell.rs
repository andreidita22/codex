/*
Runtime: shell

Executes shell requests under the orchestrator: asks for approval when needed,
builds a CommandSpec, and runs it under the current SandboxAttempt.
*/
use crate::command_safety::is_dangerous_command::command_might_be_dangerous;
use crate::command_safety::is_safe_command::is_known_safe_command;
#[cfg(feature = "semantic_shell_pause")]
use crate::error::CodexErr;
use crate::exec::ExecToolCallOutput;
#[cfg(feature = "semantic_shell_pause")]
use crate::exec::StreamOutput;
use crate::protocol::SandboxPolicy;
use crate::sandboxing::execute_env;
#[cfg(feature = "semantic_shell_pause")]
use crate::semantic_shell::ShellExecRequest;
#[cfg(feature = "semantic_shell_pause")]
use crate::semantic_shell::ShellTurnResult;
#[cfg(feature = "semantic_shell_pause")]
use crate::semantic_shell::format_pause_reason;
#[cfg(feature = "semantic_shell_pause")]
use crate::semantic_shell::format_recent_lines;
use crate::tools::runtimes::build_command_spec;
use crate::tools::sandboxing::Approvable;
use crate::tools::sandboxing::ApprovalCtx;
use crate::tools::sandboxing::ProvidesSandboxRetryData;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::SandboxRetryData;
use crate::tools::sandboxing::Sandboxable;
use crate::tools::sandboxing::SandboxablePreference;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;
use crate::tools::sandboxing::with_cached_approval;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::ReviewDecision;
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ShellRequest {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: std::collections::HashMap<String, String>,
    pub with_escalated_permissions: Option<bool>,
    pub justification: Option<String>,
    #[cfg(feature = "semantic_shell_pause")]
    pub pause_policy: Option<ShellPausePolicy>,
}

#[cfg(feature = "semantic_shell_pause")]
#[derive(Clone, Debug)]
pub struct ShellPausePolicy {
    pub pause_on_idle_ms: u64,
    pub pause_on_ready_pattern: Option<String>,
    pub pause_on_prompt_pattern: Option<String>,
    pub tail_lines: usize,
}

#[cfg(feature = "semantic_shell_pause")]
impl Default for ShellPausePolicy {
    fn default() -> Self {
        Self {
            pause_on_idle_ms: 60_000,
            pause_on_ready_pattern: Some("Press Ctrl\\+C to stop\\.".to_string()),
            pause_on_prompt_pattern: None,
            tail_lines: 200,
        }
    }
}

impl ProvidesSandboxRetryData for ShellRequest {
    fn sandbox_retry_data(&self) -> Option<SandboxRetryData> {
        Some(SandboxRetryData {
            command: self.command.clone(),
            cwd: self.cwd.clone(),
        })
    }
}

#[derive(Default)]
pub struct ShellRuntime;

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ApprovalKey {
    command: Vec<String>,
    cwd: PathBuf,
    escalated: bool,
}

impl ShellRuntime {
    pub fn new() -> Self {
        Self
    }

    fn stdout_stream(ctx: &ToolCtx<'_>, req: &ShellRequest) -> Option<crate::exec::StdoutStream> {
        Some(crate::exec::StdoutStream {
            sub_id: ctx.turn.sub_id.clone(),
            call_id: ctx.call_id.clone(),
            tx_event: ctx.session.get_tx_event(),
            meta: Some(crate::exec::ShellMeta {
                meta_config: Some(crate::exec::MetaShellConfig {
                    silence_threshold: Duration::from_secs(30),
                    heartbeat_interval: Duration::from_secs(5),
                    command_label: summarize_command(&req.command),
                    cwd_display: req.cwd.display().to_string(),
                    reporter: Some(crate::exec::MetaObservationSink::new(Arc::clone(
                        &ctx.session_arc,
                    ))),
                    interrupt_on_silence: true,
                }),
            }),
        })
    }
}

impl Sandboxable for ShellRuntime {
    fn sandbox_preference(&self) -> SandboxablePreference {
        SandboxablePreference::Auto
    }
    fn escalate_on_failure(&self) -> bool {
        true
    }
}

impl Approvable<ShellRequest> for ShellRuntime {
    type ApprovalKey = ApprovalKey;

    fn approval_key(&self, req: &ShellRequest) -> Self::ApprovalKey {
        ApprovalKey {
            command: req.command.clone(),
            cwd: req.cwd.clone(),
            escalated: req.with_escalated_permissions.unwrap_or(false),
        }
    }

    fn start_approval_async<'a>(
        &'a mut self,
        req: &'a ShellRequest,
        ctx: ApprovalCtx<'a>,
    ) -> BoxFuture<'a, ReviewDecision> {
        let key = self.approval_key(req);
        let command = req.command.clone();
        let cwd = req.cwd.clone();
        let reason = ctx
            .retry_reason
            .clone()
            .or_else(|| req.justification.clone());
        let risk = ctx.risk.clone();
        let session = ctx.session;
        let turn = ctx.turn;
        let call_id = ctx.call_id.to_string();
        Box::pin(async move {
            with_cached_approval(&session.services, key, move || async move {
                session
                    .request_command_approval(turn, call_id, command, cwd, reason, risk)
                    .await
            })
            .await
        })
    }

    fn wants_initial_approval(
        &self,
        req: &ShellRequest,
        policy: AskForApproval,
        sandbox_policy: &SandboxPolicy,
    ) -> bool {
        if is_known_safe_command(&req.command) {
            return false;
        }
        match policy {
            AskForApproval::Never | AskForApproval::OnFailure => false,
            AskForApproval::OnRequest => {
                // In DangerFullAccess, only prompt if the command looks dangerous.
                if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) {
                    return command_might_be_dangerous(&req.command);
                }

                // In restricted sandboxes (ReadOnly/WorkspaceWrite), do not prompt for
                // non‑escalated, non‑dangerous commands — let the sandbox enforce
                // restrictions (e.g., block network/write) without a user prompt.
                let wants_escalation = req.with_escalated_permissions.unwrap_or(false);
                if wants_escalation {
                    return true;
                }
                command_might_be_dangerous(&req.command)
            }
            AskForApproval::UnlessTrusted => !is_known_safe_command(&req.command),
        }
    }

    fn wants_escalated_first_attempt(&self, req: &ShellRequest) -> bool {
        req.with_escalated_permissions.unwrap_or(false)
    }
}

impl ToolRuntime<ShellRequest, ExecToolCallOutput> for ShellRuntime {
    async fn run(
        &mut self,
        req: &ShellRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx<'_>,
    ) -> Result<ExecToolCallOutput, ToolError> {
        let spec = build_command_spec(
            &req.command,
            &req.cwd,
            &req.env,
            req.timeout_ms,
            req.with_escalated_permissions,
            req.justification.clone(),
        )?;
        let env = attempt
            .env_for(&spec)
            .map_err(|err| ToolError::Codex(err.into()))?;
        let stdout_stream = Self::stdout_stream(ctx, req);
        #[cfg(feature = "semantic_shell_pause")]
        if ctx.turn.tools_config.semantic_shell_pause {
            if let Some(pause_policy) = req.pause_policy.clone() {
                let mut paused_stdout_stream = stdout_stream.clone();
                if let Some(stream) = paused_stdout_stream.as_mut() {
                    stream.meta = None;
                }
                let sem_req = ShellExecRequest {
                    exec_env: env.clone(),
                    sandbox_policy: ctx.turn.sandbox_policy.clone(),
                    stdout_stream: paused_stdout_stream,
                    pause_on_idle_ms: Some(pause_policy.pause_on_idle_ms),
                    pause_on_ready_pattern: pause_policy.pause_on_ready_pattern.clone(),
                    pause_on_prompt_pattern: pause_policy.pause_on_prompt_pattern.clone(),
                    tail_lines: pause_policy.tail_lines,
                };
                let manager = Arc::clone(&ctx.session.services.semantic_shell);
                let result = manager
                    .exec_with_semantic_pause(sem_req)
                    .await
                    .map_err(|err| {
                        ToolError::Codex(CodexErr::Fatal(format!("semantic shell failed: {err:#}")))
                    })?;
                return semantic_result_to_output(result);
            }
        }
        let out = execute_env(&env, attempt.policy, stdout_stream)
            .await
            .map_err(ToolError::Codex)?;
        Ok(out)
    }
}

fn summarize_command(command: &[String]) -> String {
    const MAX_LEN: usize = 80;
    if command.is_empty() {
        return "<empty>".to_string();
    }

    let joined = command.join(" ");
    if joined.chars().count() <= MAX_LEN {
        return joined;
    }

    let mut truncated: String = joined.chars().take(MAX_LEN - 1).collect();
    truncated.push('…');
    truncated
}

#[cfg(feature = "semantic_shell_pause")]
fn semantic_result_to_output(result: ShellTurnResult) -> Result<ExecToolCallOutput, ToolError> {
    match result {
        ShellTurnResult::Completed {
            exit,
            duration_ms,
            stdout,
            stderr,
            aggregated,
        } => Ok(ExecToolCallOutput {
            exit_code: exit,
            stdout: StreamOutput::new(stdout),
            stderr: StreamOutput::new(stderr),
            aggregated_output: StreamOutput::new(aggregated),
            duration: Duration::from_millis(duration_ms),
            timed_out: false,
        }),
        ShellTurnResult::Paused {
            run_id,
            reason,
            pid,
            idle_ms,
            last_stdout,
            last_stderr,
            started_at,
        } => {
            let reason_text = format_pause_reason(&reason);
            let stdout_snapshot = format_recent_lines(&last_stdout);
            let stderr_snapshot = format_recent_lines(&last_stderr);
            let summary = format!(
                "Semantic shell paused: {reason_text}. run_id={run_id}, pid={pid}, started_at={started_at}.\
                 \nUse `codex shell resume {run_id}` to continue or interrupt/kill as needed.\
                 \nRecent stdout:\n{stdout}\n\nRecent stderr:\n{stderr}",
                stdout = stdout_snapshot,
                stderr = stderr_snapshot,
            );
            Ok(ExecToolCallOutput {
                exit_code: 0,
                stdout: StreamOutput::new(stdout_snapshot),
                stderr: StreamOutput::new(stderr_snapshot),
                aggregated_output: StreamOutput::new(summary),
                duration: Duration::from_millis(idle_ms),
                timed_out: false,
            })
        }
        ShellTurnResult::Failed { error } => Err(ToolError::Codex(CodexErr::Fatal(error))),
    }
}
