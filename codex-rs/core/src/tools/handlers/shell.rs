use async_trait::async_trait;
use codex_protocol::models::ShellCommandToolCallParams;
use codex_protocol::models::ShellToolCallParams;
use std::sync::Arc;

use crate::apply_patch;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::codex::TurnContext;
use crate::exec::ExecParams;
use crate::exec_env::create_env;
use crate::exec_policy::create_approval_requirement_for_command;
use crate::function_tool::FunctionCallError;
use crate::is_safe_command::is_known_safe_command;
use crate::protocol::ExecCommandSource;
use crate::sandboxing::SandboxPermissions;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEmitter;
use crate::tools::events::ToolEventCtx;
use crate::tools::orchestrator::ToolOrchestrator;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::runtimes::apply_patch::ApplyPatchRequest;
use crate::tools::runtimes::apply_patch::ApplyPatchRuntime;
use crate::tools::runtimes::shell::ShellRequest;
use crate::tools::runtimes::shell::ShellRuntime;
use crate::tools::sandboxing::ToolCtx;

pub struct ShellHandler;

pub struct ShellCommandHandler;

impl ShellHandler {
    fn to_exec_params(params: ShellToolCallParams, turn_context: &TurnContext) -> ExecParams {
        ExecParams {
            command: params.command,
            cwd: turn_context.resolve_path(params.workdir.clone()),
            expiration: params.timeout_ms.into(),
            env: create_env(&turn_context.shell_environment_policy),
            with_escalated_permissions: params.with_escalated_permissions,
            justification: params.justification,
            arg0: None,
        }
    }
}

impl ShellCommandHandler {
    fn to_exec_params(
        params: ShellCommandToolCallParams,
        session: &crate::codex::Session,
        turn_context: &TurnContext,
    ) -> ExecParams {
        let shell = session.user_shell();
        let use_login_shell = true;
        let command = shell.derive_exec_args(&params.command, use_login_shell);

        ExecParams {
            command,
            cwd: turn_context.resolve_path(params.workdir.clone()),
            expiration: params.timeout_ms.into(),
            env: create_env(&turn_context.shell_environment_policy),
            with_escalated_permissions: params.with_escalated_permissions,
            justification: params.justification,
            arg0: None,
        }
    }
}

#[async_trait]
impl ToolHandler for ShellHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::LocalShell { .. }
        )
    }

    fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        match &invocation.payload {
            ToolPayload::Function { arguments } => {
                serde_json::from_str::<ShellToolCallParams>(arguments)
                    .map(|params| !is_known_safe_command(&params.command))
                    .unwrap_or(true)
            }
            ToolPayload::LocalShell { params } => !is_known_safe_command(&params.command),
            _ => true, // unknown payloads => assume mutating
        }
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
        } = invocation;

        match payload {
            ToolPayload::Function { arguments } => {
                let params: ShellToolCallParams =
                    serde_json::from_str(&arguments).map_err(|e| {
                        FunctionCallError::RespondToModel(format!(
                            "failed to parse function arguments: {e:?}"
                        ))
                    })?;
                let exec_params = Self::to_exec_params(params, turn.as_ref());
                Self::run_exec_like(
                    tool_name.as_str(),
                    exec_params,
                    session,
                    turn,
                    tracker,
                    call_id,
                    false,
                )
                .await
            }
            ToolPayload::LocalShell { params } => {
                let exec_params = Self::to_exec_params(params, turn.as_ref());
                Self::run_exec_like(
                    tool_name.as_str(),
                    exec_params,
                    session,
                    turn,
                    tracker,
                    call_id,
                    false,
                )
                .await
            }
            _ => Err(FunctionCallError::RespondToModel(format!(
                "unsupported payload for shell handler: {tool_name}"
            ))),
        }
    }
}

#[async_trait]
impl ToolHandler for ShellCommandHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
        } = invocation;

        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::RespondToModel(format!(
                "unsupported payload for shell_command handler: {tool_name}"
            )));
        };

        let params: ShellCommandToolCallParams = serde_json::from_str(&arguments).map_err(|e| {
            FunctionCallError::RespondToModel(format!("failed to parse function arguments: {e:?}"))
        })?;
        let exec_params = Self::to_exec_params(params, session.as_ref(), turn.as_ref());
        ShellHandler::run_exec_like(
            tool_name.as_str(),
            exec_params,
            session,
            turn,
            tracker,
            call_id,
            true,
        )
        .await
    }
}

impl ShellHandler {
    async fn run_exec_like(
        tool_name: &str,
        exec_params: ExecParams,
        session: Arc<crate::codex::Session>,
        turn: Arc<TurnContext>,
        tracker: crate::tools::context::SharedTurnDiffTracker,
        call_id: String,
        freeform: bool,
    ) -> Result<ToolOutput, FunctionCallError> {
        // Approval policy guard for explicit escalation in non-OnRequest modes.
        if exec_params.with_escalated_permissions.unwrap_or(false)
            && !matches!(
                turn.approval_policy,
                codex_protocol::protocol::AskForApproval::OnRequest
            )
        {
            return Err(FunctionCallError::RespondToModel(format!(
                "approval policy is {policy:?}; reject command — you should not ask for escalated permissions if the approval policy is {policy:?}",
                policy = turn.approval_policy
            )));
        }

        // Intercept apply_patch if present.
        match codex_apply_patch::maybe_parse_apply_patch_verified(
            &exec_params.command,
            &exec_params.cwd,
        ) {
            codex_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
                match apply_patch::apply_patch(session.as_ref(), turn.as_ref(), &call_id, changes)
                    .await
                {
                    InternalApplyPatchInvocation::Output(item) => {
                        // Programmatic apply_patch path; return its result.
                        let content = item?;
                        return Ok(ToolOutput::Function {
                            content,
                            content_items: None,
                            success: Some(true),
                        });
                    }
                    InternalApplyPatchInvocation::DelegateToExec(apply) => {
                        let emitter = ToolEmitter::apply_patch(
                            convert_apply_patch_to_protocol(&apply.action),
                            !apply.user_explicitly_approved_this_action,
                        );
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        emitter.begin(event_ctx).await;

                        let req = ApplyPatchRequest {
                            patch: apply.action.patch.clone(),
                            cwd: apply.action.cwd.clone(),
                            timeout_ms: exec_params.expiration.timeout_ms(),
                            user_explicitly_approved: apply.user_explicitly_approved_this_action,
                            codex_exe: turn.codex_linux_sandbox_exe.clone(),
                        };
                        let mut orchestrator = ToolOrchestrator::new();
                        let mut runtime = ApplyPatchRuntime::new();
                        let tool_ctx = ToolCtx {
                            session_arc: Arc::clone(&session),
                            session: session.as_ref(),
                            turn: turn.as_ref(),
                            call_id: call_id.clone(),
                            tool_name: tool_name.to_string(),
                        };
                        let out = orchestrator
                            .run(&mut runtime, &req, &tool_ctx, &turn, turn.approval_policy)
                            .await;
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        let content = emitter.finish(event_ctx, out).await?;
                        return Ok(ToolOutput::Function {
                            content,
                            content_items: None,
                            success: Some(true),
                        });
                    }
                }
            }
            codex_apply_patch::MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "apply_patch verification failed: {parse_error}"
                )));
            }
            codex_apply_patch::MaybeApplyPatchVerified::ShellParseError(error) => {
                tracing::trace!("Failed to parse shell command, {error:?}");
                // Fall through to regular shell execution.
            }
            codex_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => {
                // Fall through to regular shell execution.
            }
        }

        let source = ExecCommandSource::Agent;
        let emitter = ToolEmitter::shell(
            exec_params.command.clone(),
            exec_params.cwd.clone(),
            source,
            freeform,
        );
        let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), &call_id, None);
        emitter.begin(event_ctx).await;

        let req = ShellRequest {
            command: exec_params.command.clone(),
            cwd: exec_params.cwd.clone(),
            timeout_ms: exec_params.expiration.timeout_ms(),
            env: exec_params.env.clone(),
            with_escalated_permissions: exec_params.with_escalated_permissions,
            justification: exec_params.justification.clone(),
            approval_requirement: create_approval_requirement_for_command(
                &turn.exec_policy,
                &exec_params.command,
                turn.approval_policy,
                &turn.sandbox_policy,
                SandboxPermissions::from(exec_params.with_escalated_permissions.unwrap_or(false)),
            ),
        };
        #[cfg(feature = "semantic_shell_pause")]
        if turn.tools_config.semantic_shell_pause {
            // Build ExecEnv for semantic shell manager
            let spec = build_command_spec(
                &req.command,
                &req.cwd,
                &req.env,
                ExecExpiration::from(req.timeout_ms),
                req.with_escalated_permissions,
                req.justification.clone(),
            )?;
            let manager = SandboxManager::new();
            let exec_env = manager
                .transform(
                    spec,
                    &turn.sandbox_policy,
                    attempt.policy,
                    turn.cwd.clone(),
                    &turn.codex_linux_sandbox_exe,
                )
                .map_err(FunctionCallError::from)?;

            // Pause policy defaults to the execution timeout if provided.
            let pause_on_idle_ms = req.timeout_ms;
            let manager = session.services.semantic_shell.clone();
            let result = manager
                .exec_with_semantic_pause(crate::extensions::semantic_shell::ShellExecRequest {
                    exec_env,
                    sandbox_policy: turn.sandbox_policy.clone(),
                    stdout_stream: Some(
                        crate::exec::StdoutStream {
                            sub_id: turn.sub_id.clone(),
                            call_id: call_id.clone(),
                            tx_event: session.get_tx_event(),
                        },
                    ),
                    pause_on_idle_ms,
                    pause_on_ready_pattern: None,
                    pause_on_prompt_pattern: None,
                    tail_lines: 200,
                })
                .await
                .map_err(|err| FunctionCallError::Fatal(err.to_string()))?;

            return match result {
                crate::extensions::semantic_shell::ShellTurnResult::Completed {
                    exit,
                    duration_ms,
                    stdout,
                    stderr,
                    aggregated,
                } => Ok(ToolOutput::Function {
                    content: serde_json::json!({
                        "exit": exit,
                        "duration_ms": duration_ms,
                        "stdout": stdout,
                        "stderr": stderr,
                        "aggregated_output": aggregated,
                    }),
                    content_items: None,
                    success: Some(true),
                }),
                crate::extensions::semantic_shell::ShellTurnResult::Paused {
                    run_id,
                    reason,
                    pid,
                    idle_ms,
                    last_stdout,
                    last_stderr,
                    started_at,
                } => Ok(ToolOutput::Function {
                    content: serde_json::json!({
                        "status": "paused",
                        "run_id": run_id,
                        "reason": format_pause_reason(&reason),
                        "pid": pid,
                        "idle_ms": idle_ms,
                        "last_stdout": last_stdout,
                        "last_stderr": last_stderr,
                        "started_at": started_at,
                    }),
                    content_items: None,
                    success: Some(true),
                }),
                crate::extensions::semantic_shell::ShellTurnResult::Failed { error } => {
                    Err(FunctionCallError::Fatal(error))
                }
            };
        }
        let mut orchestrator = ToolOrchestrator::new();
        let mut runtime = ShellRuntime::new();
        let tool_ctx = ToolCtx {
            session_arc: Arc::clone(&session),
            session: session.as_ref(),
            turn: turn.as_ref(),
            call_id: call_id.clone(),
            tool_name: tool_name.to_string(),
        };
        let out = orchestrator
            .run(&mut runtime, &req, &tool_ctx, &turn, turn.approval_policy)
            .await;
        let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), &call_id, None);
        let content = emitter.finish(event_ctx, out).await?;
        Ok(ToolOutput::Function {
            content,
            content_items: None,
            success: Some(true),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::is_safe_command::is_known_safe_command;
    use crate::shell::Shell;
    use crate::shell::ShellType;

    /// The logic for is_known_safe_command() has heuristics for known shells,
    /// so we must ensure the commands generated by [ShellCommandHandler] can be
    /// recognized as safe if the `command` is safe.
    #[test]
    fn commands_generated_by_shell_command_handler_can_be_matched_by_is_known_safe_command() {
        let bash_shell = Shell {
            shell_type: ShellType::Bash,
            shell_path: PathBuf::from("/bin/bash"),
        };
        assert_safe(&bash_shell, "ls -la");

        let zsh_shell = Shell {
            shell_type: ShellType::Zsh,
            shell_path: PathBuf::from("/bin/zsh"),
        };
        assert_safe(&zsh_shell, "ls -la");

        let powershell = Shell {
            shell_type: ShellType::PowerShell,
            shell_path: PathBuf::from("pwsh.exe"),
        };
        assert_safe(&powershell, "ls -Name");
    }

    fn assert_safe(shell: &Shell, command: &str) {
        assert!(is_known_safe_command(
            &shell.derive_exec_args(command, /* use_login_shell */ true)
        ));
        assert!(is_known_safe_command(
            &shell.derive_exec_args(command, /* use_login_shell */ false)
        ));
    }
}
