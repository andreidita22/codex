use anyhow::Error;
use async_trait::async_trait;
use serde::Deserialize;

use crate::extensions::semantic_shell::PausePolicy;
use crate::extensions::semantic_shell::RunListing;
use crate::extensions::semantic_shell::ShellTurnResult;
use crate::extensions::semantic_shell::format_pause_reason;
use crate::extensions::semantic_shell::format_recent_lines;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ControlAction {
    Resume,
    Interrupt,
    Kill,
    Status,
    List,
}

#[derive(Debug, Deserialize)]
struct ControlArgs {
    action: ControlAction,
    run_id: Option<String>,
    pause_on_idle_ms: Option<u64>,
    pause_on_ready_pattern: Option<String>,
    pause_on_prompt_pattern: Option<String>,
    graceful_ms: Option<u64>,
}

pub struct SemanticShellControlHandler;

#[async_trait]
impl ToolHandler for SemanticShellControlHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;

        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::Fatal(
                "semantic_shell_control requires function arguments".to_string(),
            ));
        };

        let args: ControlArgs = serde_json::from_str(&arguments).map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to parse semantic_shell_control arguments: {err:?}"
            ))
        })?;

        let manager = session.services.semantic_shell.clone();
        let response = match args.action {
            ControlAction::Resume => {
                let run_id = require_run_id(args.run_id.clone(), "resume")?;
                let policy = build_policy(&args);
                let result = manager
                    .resume(&run_id, policy)
                    .await
                    .map_err(to_function_error)?;
                describe_turn_result(&result)
            }
            ControlAction::Interrupt => {
                let run_id = require_run_id(args.run_id.clone(), "interrupt")?;
                let graceful_ms = args.graceful_ms.unwrap_or(5_000);
                manager
                    .interrupt(&run_id, graceful_ms)
                    .await
                    .map_err(to_function_error)?;
                format!(
                    "Sent interrupt to run {run_id}; will SIGKILL after {graceful_ms}ms if it does not exit."
                )
            }
            ControlAction::Kill => {
                let run_id = require_run_id(args.run_id.clone(), "kill")?;
                manager.kill(&run_id).await.map_err(to_function_error)?;
                format!("Force-killed run {run_id}.")
            }
            ControlAction::Status => {
                let run_id = require_run_id(args.run_id.clone(), "status")?;
                let status = manager.status(&run_id).await.map_err(to_function_error)?;
                format!(
                    "Run {run_id}: pid={} uptime={}ms last_output={}ms",
                    status.pid, status.uptime_ms, status.last_output_ms
                )
            }
            ControlAction::List => {
                let listings = manager.list().await.map_err(to_function_error)?;
                format_run_list(&listings)
            }
        };

        Ok(ToolOutput::Function {
            content: response,
            content_items: None,
            success: Some(true),
        })
    }
}

fn build_policy(args: &ControlArgs) -> Option<PausePolicy> {
    if args.pause_on_idle_ms.is_none()
        && args.pause_on_ready_pattern.is_none()
        && args.pause_on_prompt_pattern.is_none()
    {
        return None;
    }

    Some(PausePolicy {
        pause_on_idle_ms: args.pause_on_idle_ms,
        pause_on_ready_pattern: args.pause_on_ready_pattern.clone(),
        pause_on_prompt_pattern: args.pause_on_prompt_pattern.clone(),
    })
}

fn describe_turn_result(result: &ShellTurnResult) -> String {
    match result {
        ShellTurnResult::Completed {
            exit,
            duration_ms,
            aggregated,
            ..
        } => format!(
            "Run completed with exit code {exit} after {duration_ms}ms.\nOutput:\n{aggregated}"
        ),
        ShellTurnResult::Paused {
            run_id,
            reason,
            pid,
            last_stdout,
            last_stderr,
            started_at,
            ..
        } => {
            let reason_text = format_pause_reason(reason);
            format!(
                "Run {run_id} paused ({reason_text}). pid={pid} started_at={started_at}.\nRecent stdout:\n{stdout}\n\nRecent stderr:\n{stderr}",
                stdout = format_recent_lines(last_stdout),
                stderr = format_recent_lines(last_stderr)
            )
        }
        ShellTurnResult::Failed { error } => {
            format!("Run failed: {error}")
        }
    }
}

fn format_run_list(listings: &[RunListing]) -> String {
    if listings.is_empty() {
        return "No active semantic shell runs.".to_string();
    }
    let mut out = String::new();
    out.push_str("Active semantic shell runs:\n");
    for run in listings {
        let line = format!(
            "- run_id={} pid={} uptime={}ms idle={}ms started_at={}",
            run.run_id, run.pid, run.uptime_ms, run.last_output_ms, run.started_at
        );
        out.push_str(&line);
        out.push('\n');
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn require_run_id(run_id: Option<String>, action: &str) -> Result<String, FunctionCallError> {
    run_id.ok_or_else(|| {
        FunctionCallError::RespondToModel(format!(
            "semantic_shell_control action '{action}' requires run_id"
        ))
    })
}

fn to_function_error(err: Error) -> FunctionCallError {
    FunctionCallError::RespondToModel(format!("semantic shell error: {err:#}"))
}
