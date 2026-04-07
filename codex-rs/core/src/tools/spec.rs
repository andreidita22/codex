use crate::shell::Shell;
use crate::shell::ShellType;
use crate::tools::handlers::agent_jobs::BatchJobHandler;
use crate::tools::handlers::multi_agents_common::DEFAULT_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MAX_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MIN_WAIT_TIMEOUT_MS;
use crate::tools::registry::ToolRegistryBuilder;
use codex_mcp::ToolInfo;
use codex_protocol::dynamic_tools::DynamicToolSpec;
use codex_tools::DiscoverableTool;
use codex_tools::ToolHandlerKind;
use codex_tools::ToolNamespace;
use codex_tools::ToolRegistryPlanDeferredTool;
use codex_tools::ToolRegistryPlanParams;
use codex_tools::ToolUserShellType;
use codex_tools::ToolsConfig;
use codex_tools::WaitAgentTimeoutOptions;
use codex_tools::build_tool_registry_plan;
use codex_tools::augment_tool_spec_for_code_mode;
use codex_tools::create_assign_task_tool;
use codex_tools::create_close_agent_tool_v1;
use codex_tools::create_close_agent_tool_v2;
use codex_tools::create_code_mode_tool;
use codex_tools::create_exec_command_tool;
use codex_tools::create_inspect_agent_progress_tool;
use codex_tools::create_js_repl_reset_tool;
use codex_tools::create_js_repl_tool;
use codex_tools::create_list_agents_tool;
use codex_tools::create_list_dir_tool;
use codex_tools::create_list_mcp_resource_templates_tool;
use codex_tools::create_list_mcp_resources_tool;
use codex_tools::create_read_mcp_resource_tool;
use codex_tools::create_report_agent_job_result_tool;
use codex_tools::create_request_permissions_tool;
use codex_tools::create_request_user_input_tool;
use codex_tools::create_resume_agent_tool;
use codex_tools::create_send_input_tool_v1;
use codex_tools::create_send_message_tool;
use codex_tools::create_shell_command_tool;
use codex_tools::create_shell_tool;
use codex_tools::create_spawn_agent_tool_v1;
use codex_tools::create_spawn_agent_tool_v2;
use codex_tools::create_spawn_agents_on_csv_tool;
use codex_tools::create_test_sync_tool;
use codex_tools::create_tool_search_tool;
use codex_tools::create_tool_suggest_tool;
use codex_tools::create_view_image_tool;
use codex_tools::create_wait_agent_tool_v1;
use codex_tools::create_wait_agent_tool_v2;
use codex_tools::create_wait_for_agent_progress_tool;
use codex_tools::create_wait_tool;
use codex_tools::create_write_stdin_tool;
use codex_tools::dynamic_tool_to_responses_api_tool;
use codex_tools::mcp_tool_to_responses_api_tool;
use codex_tools::tool_spec_to_code_mode_tool_definition;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) fn tool_user_shell_type(user_shell: &Shell) -> ToolUserShellType {
    match user_shell.shell_type {
        ShellType::Zsh => ToolUserShellType::Zsh,
        ShellType::Bash => ToolUserShellType::Bash,
        ShellType::PowerShell => ToolUserShellType::PowerShell,
        ShellType::Sh => ToolUserShellType::Sh,
        ShellType::Cmd => ToolUserShellType::Cmd,
    }
}

struct McpToolPlanInputs {
    mcp_tools: HashMap<String, rmcp::model::Tool>,
    tool_namespaces: HashMap<String, ToolNamespace>,
}

fn map_mcp_tools_for_plan(mcp_tools: &HashMap<String, ToolInfo>) -> McpToolPlanInputs {
    McpToolPlanInputs {
        mcp_tools: mcp_tools
            .iter()
            .map(|(name, tool)| (name.clone(), tool.tool.clone()))
            .collect(),
        tool_namespaces: mcp_tools
            .iter()
            .map(|(name, tool)| {
                (
                    name.clone(),
                    ToolNamespace {
                        name: tool.callable_namespace.clone(),
                        description: tool.server_instructions.clone(),
                    },
                )
            })
            .collect(),
    }
}

pub(crate) fn build_specs_with_discoverable_tools(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, ToolInfo>>,
    deferred_mcp_tools: Option<HashMap<String, ToolInfo>>,
    discoverable_tools: Option<Vec<DiscoverableTool>>,
    dynamic_tools: &[DynamicToolSpec],
) -> ToolRegistryBuilder {
    use crate::tools::handlers::ApplyPatchHandler;
    use crate::tools::handlers::CodeModeExecuteHandler;
    use crate::tools::handlers::CodeModeWaitHandler;
    use crate::tools::handlers::DynamicToolHandler;
    use crate::tools::handlers::InspectAgentProgressHandler;
    use crate::tools::handlers::JsReplHandler;
    use crate::tools::handlers::JsReplResetHandler;
    use crate::tools::handlers::ListDirHandler;
    use crate::tools::handlers::McpHandler;
    use crate::tools::handlers::McpResourceHandler;
    use crate::tools::handlers::PlanHandler;
    use crate::tools::handlers::RequestPermissionsHandler;
    use crate::tools::handlers::RequestUserInputHandler;
    use crate::tools::handlers::ShellCommandHandler;
    use crate::tools::handlers::ShellHandler;
    use crate::tools::handlers::TestSyncHandler;
    use crate::tools::handlers::ToolSearchHandler;
    use crate::tools::handlers::ToolSuggestHandler;
    use crate::tools::handlers::UnifiedExecHandler;
    use crate::tools::handlers::ViewImageHandler;
    use crate::tools::handlers::WaitForAgentProgressHandler;
    use crate::tools::handlers::multi_agents::CloseAgentHandler;
    use crate::tools::handlers::multi_agents::ResumeAgentHandler;
    use crate::tools::handlers::multi_agents::SendInputHandler;
    use crate::tools::handlers::multi_agents::SpawnAgentHandler;
    use crate::tools::handlers::multi_agents::WaitAgentHandler;
    use crate::tools::handlers::multi_agents_v2::CloseAgentHandler as CloseAgentHandlerV2;
    use crate::tools::handlers::multi_agents_v2::FollowupTaskHandler as FollowupTaskHandlerV2;
    use crate::tools::handlers::multi_agents_v2::ListAgentsHandler as ListAgentsHandlerV2;
    use crate::tools::handlers::multi_agents_v2::SendMessageHandler as SendMessageHandlerV2;
    use crate::tools::handlers::multi_agents_v2::SpawnAgentHandler as SpawnAgentHandlerV2;
    use crate::tools::handlers::multi_agents_v2::WaitAgentHandler as WaitAgentHandlerV2;

    let mut builder = ToolRegistryBuilder::new();
    let mcp_tool_plan_inputs = mcp_tools.as_ref().map(map_mcp_tools_for_plan);
    let deferred_mcp_tool_sources = deferred_mcp_tools.as_ref().map(|tools| {
        tools
            .values()
            .map(|tool| ToolRegistryPlanDeferredTool {
                tool_name: tool.callable_name.as_str(),
                tool_namespace: tool.callable_namespace.as_str(),
                server_name: tool.server_name.as_str(),
                connector_name: tool.connector_name.as_deref(),
                connector_description: tool.connector_description.as_deref(),
            })
            .collect::<Vec<_>>()
    });
    let default_agent_type_description =
        crate::agent::role::spawn_tool_spec::build(&std::collections::BTreeMap::new());
    let plan = build_tool_registry_plan(
        config,
        ToolRegistryPlanParams {
            mcp_tools: mcp_tool_plan_inputs
                .as_ref()
                .map(|inputs| &inputs.mcp_tools),
            deferred_mcp_tools: deferred_mcp_tool_sources.as_deref(),
            tool_namespaces: mcp_tool_plan_inputs
                .as_ref()
                .map(|inputs| &inputs.tool_namespaces),
            discoverable_tools: discoverable_tools.as_deref(),
            dynamic_tools,
            default_agent_type_description: &default_agent_type_description,
            wait_agent_timeouts: WaitAgentTimeoutOptions {
                default_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
                min_timeout_ms: MIN_WAIT_TIMEOUT_MS,
                max_timeout_ms: MAX_WAIT_TIMEOUT_MS,
            },
        },
    );
    let shell_handler = Arc::new(ShellHandler);
    let unified_exec_handler = Arc::new(UnifiedExecHandler);
    let plan_handler = Arc::new(PlanHandler);
    let apply_patch_handler = Arc::new(ApplyPatchHandler);
    let dynamic_tool_handler = Arc::new(DynamicToolHandler);
    let view_image_handler = Arc::new(ViewImageHandler);
    let mcp_handler = Arc::new(McpHandler);
    let mcp_resource_handler = Arc::new(McpResourceHandler);
    let shell_command_handler = Arc::new(ShellCommandHandler::from(config.shell_command_backend));
    let request_permissions_handler = Arc::new(RequestPermissionsHandler);
    let request_user_input_handler = Arc::new(RequestUserInputHandler {
        default_mode_request_user_input: config.default_mode_request_user_input,
    });
    let mut tool_search_handler = None;
    let tool_suggest_handler = Arc::new(ToolSuggestHandler);
    let code_mode_handler = Arc::new(CodeModeExecuteHandler);
    let code_mode_wait_handler = Arc::new(CodeModeWaitHandler);
    let js_repl_handler = Arc::new(JsReplHandler);
    let js_repl_reset_handler = Arc::new(JsReplResetHandler);
    let exec_permission_approvals_enabled = config.exec_permission_approvals_enabled;

    if config.code_mode_enabled {
        let nested_config = config.for_code_mode_nested_tools();
        let (nested_specs, _) = build_specs_with_discoverable_tools(
            &nested_config,
            mcp_tools.clone(),
            app_tools.clone(),
            /*discoverable_tools*/ None,
            dynamic_tools,
        )
        .build();
        let mut enabled_tools = nested_specs
            .into_iter()
            .filter_map(|spec| tool_spec_to_code_mode_tool_definition(&spec.spec))
            .map(|tool| (tool.name, tool.description))
            .collect::<Vec<_>>();
        enabled_tools.sort_by(|left, right| left.0.cmp(&right.0));
        enabled_tools.dedup_by(|left, right| left.0 == right.0);
        push_tool_spec(
            &mut builder,
            create_code_mode_tool(&enabled_tools, config.code_mode_only_enabled),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler(PUBLIC_TOOL_NAME, code_mode_handler);
        push_tool_spec(
            &mut builder,
            create_wait_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler(WAIT_TOOL_NAME, code_mode_wait_handler);
    }

    match &config.shell_type {
        ConfigShellToolType::Default => {
            push_tool_spec(
                &mut builder,
                create_shell_tool(ShellToolOptions {
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
        ConfigShellToolType::Local => {
            push_tool_spec(
                &mut builder,
                ToolSpec::LocalShell {},
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
        ConfigShellToolType::UnifiedExec => {
            push_tool_spec(
                &mut builder,
                create_exec_command_tool(CommandToolOptions {
                    allow_login_shell: config.allow_login_shell,
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_write_stdin_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            builder.register_handler("exec_command", unified_exec_handler.clone());
            builder.register_handler("write_stdin", unified_exec_handler);
        }
        ConfigShellToolType::Disabled => {}
        ConfigShellToolType::ShellCommand => {
            push_tool_spec(
                &mut builder,
                create_shell_command_tool(CommandToolOptions {
                    allow_login_shell: config.allow_login_shell,
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
    }

    if config.shell_type != ConfigShellToolType::Disabled {
        builder.register_handler("shell", shell_handler.clone());
        builder.register_handler("container.exec", shell_handler.clone());
        builder.register_handler("local_shell", shell_handler);
        builder.register_handler("shell_command", shell_command_handler);
    }

    if mcp_tools.is_some() {
        push_tool_spec(
            &mut builder,
            create_list_mcp_resources_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        push_tool_spec(
            &mut builder,
            create_list_mcp_resource_templates_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        push_tool_spec(
            &mut builder,
            create_read_mcp_resource_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        builder.register_handler("list_mcp_resources", mcp_resource_handler.clone());
        builder.register_handler("list_mcp_resource_templates", mcp_resource_handler.clone());
        builder.register_handler("read_mcp_resource", mcp_resource_handler);
    }

    push_tool_spec(
        &mut builder,
        PLAN_TOOL.clone(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    builder.register_handler("update_plan", plan_handler);

    if config.js_repl_enabled {
        push_tool_spec(
            &mut builder,
            create_js_repl_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        push_tool_spec(
            &mut builder,
            create_js_repl_reset_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler("js_repl", js_repl_handler);
        builder.register_handler("js_repl_reset", js_repl_reset_handler);
    }

    if config.request_user_input {
        push_tool_spec(
            &mut builder,
            create_request_user_input_tool(request_user_input_tool_description(
                config.default_mode_request_user_input,
            )),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler("request_user_input", request_user_input_handler);
    }

    if config.request_permissions_tool_enabled {
        push_tool_spec(
            &mut builder,
            create_request_permissions_tool(request_permissions_tool_description()),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler("request_permissions", request_permissions_handler);
    }

    if config.search_tool
        && let Some(app_tools) = app_tools
    {
        let search_tool_handler = Arc::new(ToolSearchHandler::new(app_tools.clone()));
        push_tool_spec(
            &mut builder,
            create_tool_search_tool(
                &tool_search_app_infos(&app_tools),
                TOOL_SEARCH_DEFAULT_LIMIT,
            ),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        builder.register_handler(TOOL_SEARCH_TOOL_NAME, search_tool_handler);

        for tool in app_tools.values() {
            let alias_name =
                tool_handler_key(tool.tool_name.as_str(), Some(tool.tool_namespace.as_str()));
            builder.register_handler(alias_name, mcp_handler.clone());
        }
    }

    if config.tool_suggest
        && let Some(discoverable_tools) = discoverable_tools
            .as_ref()
            .filter(|tools| !tools.is_empty())
    {
        builder.push_spec_with_parallel_support(
            create_tool_suggest_tool(&tool_suggest_entries(discoverable_tools)),
            /*supports_parallel_tool_calls*/ true,
        );
        builder.register_handler(TOOL_SUGGEST_TOOL_NAME, tool_suggest_handler);
    }

    if let Some(apply_patch_tool_type) = &config.apply_patch_tool_type {
        match apply_patch_tool_type {
            ApplyPatchToolType::Freeform => {
                push_tool_spec(
                    &mut builder,
                    create_apply_patch_freeform_tool(),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
            }
            ApplyPatchToolType::Function => {
                push_tool_spec(
                    &mut builder,
                    create_apply_patch_json_tool(),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
            }
        }
        builder.register_handler("apply_patch", apply_patch_handler);
    }

    if config
        .experimental_supported_tools
        .iter()
        .any(|tool| tool == "list_dir")
    {
        let list_dir_handler = Arc::new(ListDirHandler);
        push_tool_spec(
            &mut builder,
            create_list_dir_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        builder.register_handler("list_dir", list_dir_handler);
    }

    if config
        .experimental_supported_tools
        .contains(&"test_sync_tool".to_string())
    {
        let test_sync_handler = Arc::new(TestSyncHandler);
        push_tool_spec(
            &mut builder,
            create_test_sync_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        builder.register_handler("test_sync_tool", test_sync_handler);
    }

    let external_web_access = match config.web_search_mode {
        Some(WebSearchMode::Cached) => Some(false),
        Some(WebSearchMode::Live) => Some(true),
        Some(WebSearchMode::Disabled) | None => None,
    };

    if let Some(external_web_access) = external_web_access {
        let search_content_types = match config.web_search_tool_type {
            WebSearchToolType::Text => None,
            WebSearchToolType::TextAndImage => Some(
                WEB_SEARCH_CONTENT_TYPES
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            ),
        };

        push_tool_spec(
            &mut builder,
            ToolSpec::WebSearch {
                external_web_access: Some(external_web_access),
                filters: config
                    .web_search_config
                    .as_ref()
                    .and_then(|cfg| cfg.filters.clone().map(Into::into)),
                user_location: config
                    .web_search_config
                    .as_ref()
                    .and_then(|cfg| cfg.user_location.clone().map(Into::into)),
                search_context_size: config
                    .web_search_config
                    .as_ref()
                    .and_then(|cfg| cfg.search_context_size),
                search_content_types,
            },
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
    }

    if config.image_gen_tool {
        push_tool_spec(
            &mut builder,
            ToolSpec::ImageGeneration {
                output_format: "png".to_string(),
            },
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
    }

    push_tool_spec(
        &mut builder,
        create_view_image_tool(ViewImageToolOptions {
            can_request_original_image_detail: config.can_request_original_image_detail,
        }),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    builder.register_handler("view_image", view_image_handler);

    if config.collab_tools {
        push_tool_spec(
            &mut builder,
            create_inspect_agent_progress_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler(
            "inspect_agent_progress",
            Arc::new(InspectAgentProgressHandler),
        );
        push_tool_spec(
            &mut builder,
            create_wait_for_agent_progress_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        builder.register_handler(
            "wait_for_agent_progress",
            Arc::new(WaitForAgentProgressHandler),
        );
        if config.multi_agent_v2 {
            if config.spawn_agent_tool {
                push_tool_spec(
                    &mut builder,
                    create_spawn_agent_tool_v2(SpawnAgentToolOptions {
                        available_models: &config.available_models,
                        agent_type_description: crate::agent::role::spawn_tool_spec::build(
                            &config.agent_roles,
                        ),
                    }),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
            }
            push_tool_spec(
                &mut builder,
                create_send_message_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_assign_task_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_wait_agent_tool_v2(WaitAgentTimeoutOptions {
                    default_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
                    min_timeout_ms: MIN_WAIT_TIMEOUT_MS,
                    max_timeout_ms: MAX_WAIT_TIMEOUT_MS,
                }),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_close_agent_tool_v2(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_list_agents_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            if config.spawn_agent_tool {
                builder.register_handler("spawn_agent", Arc::new(SpawnAgentHandlerV2));
            }
            builder.register_handler("send_message", Arc::new(SendMessageHandlerV2));
            builder.register_handler("assign_task", Arc::new(AssignTaskHandlerV2));
            builder.register_handler("wait_agent", Arc::new(WaitAgentHandlerV2));
            builder.register_handler("close_agent", Arc::new(CloseAgentHandlerV2));
            builder.register_handler("list_agents", Arc::new(ListAgentsHandlerV2));
        } else {
            if config.spawn_agent_tool {
                push_tool_spec(
                    &mut builder,
                    create_spawn_agent_tool_v1(SpawnAgentToolOptions {
                        available_models: &config.available_models,
                        agent_type_description: crate::agent::role::spawn_tool_spec::build(
                            &config.agent_roles,
                        ),
                    }),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
            }
            push_tool_spec(
                &mut builder,
                create_send_input_tool_v1(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_resume_agent_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            builder.register_handler("resume_agent", Arc::new(ResumeAgentHandler));
            push_tool_spec(
                &mut builder,
                create_wait_agent_tool_v1(WaitAgentTimeoutOptions {
                    default_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
                    min_timeout_ms: MIN_WAIT_TIMEOUT_MS,
                    max_timeout_ms: MAX_WAIT_TIMEOUT_MS,
                }),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            push_tool_spec(
                &mut builder,
                create_close_agent_tool_v1(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            if config.spawn_agent_tool {
                builder.register_handler("spawn_agent", Arc::new(SpawnAgentHandler));
            }
            builder.register_handler("send_input", Arc::new(SendInputHandler));
            builder.register_handler("wait_agent", Arc::new(WaitAgentHandler));
            builder.register_handler("close_agent", Arc::new(CloseAgentHandler));
        }
    }

    for handler in plan.handlers {
        match handler.kind {
            ToolHandlerKind::AgentJobs => {
                builder.register_handler(handler.name, Arc::new(BatchJobHandler));
            }
            ToolHandlerKind::ApplyPatch => {
                builder.register_handler(handler.name, apply_patch_handler.clone());
            }
            ToolHandlerKind::CloseAgentV1 => {
                builder.register_handler(handler.name, Arc::new(CloseAgentHandler));
            }
            ToolHandlerKind::CloseAgentV2 => {
                builder.register_handler(handler.name, Arc::new(CloseAgentHandlerV2));
            }
            ToolHandlerKind::CodeModeExecute => {
                builder.register_handler(handler.name, code_mode_handler.clone());
            }
            ToolHandlerKind::CodeModeWait => {
                builder.register_handler(handler.name, code_mode_wait_handler.clone());
            }
            ToolHandlerKind::DynamicTool => {
                builder.register_handler(handler.name, dynamic_tool_handler.clone());
            }
            ToolHandlerKind::FollowupTaskV2 => {
                builder.register_handler(handler.name, Arc::new(FollowupTaskHandlerV2));
            }
            ToolHandlerKind::JsRepl => {
                builder.register_handler(handler.name, js_repl_handler.clone());
            }
            ToolHandlerKind::JsReplReset => {
                builder.register_handler(handler.name, js_repl_reset_handler.clone());
            }
            ToolHandlerKind::ListAgentsV2 => {
                builder.register_handler(handler.name, Arc::new(ListAgentsHandlerV2));
            }
            ToolHandlerKind::ListDir => {
                builder.register_handler(handler.name, Arc::new(ListDirHandler));
            }
            ToolHandlerKind::Mcp => {
                builder.register_handler(handler.name, mcp_handler.clone());
            }
            ToolHandlerKind::McpResource => {
                builder.register_handler(handler.name, mcp_resource_handler.clone());
            }
            ToolHandlerKind::Plan => {
                builder.register_handler(handler.name, plan_handler.clone());
            }
            ToolHandlerKind::RequestPermissions => {
                builder.register_handler(handler.name, request_permissions_handler.clone());
            }
            ToolHandlerKind::RequestUserInput => {
                builder.register_handler(handler.name, request_user_input_handler.clone());
            }
            ToolHandlerKind::ResumeAgentV1 => {
                builder.register_handler(handler.name, Arc::new(ResumeAgentHandler));
            }
            ToolHandlerKind::SendInputV1 => {
                builder.register_handler(handler.name, Arc::new(SendInputHandler));
            }
            ToolHandlerKind::SendMessageV2 => {
                builder.register_handler(handler.name, Arc::new(SendMessageHandlerV2));
            }
            ToolHandlerKind::Shell => {
                builder.register_handler(handler.name, shell_handler.clone());
            }
            ToolHandlerKind::ShellCommand => {
                builder.register_handler(handler.name, shell_command_handler.clone());
            }
            ToolHandlerKind::SpawnAgentV1 => {
                builder.register_handler(handler.name, Arc::new(SpawnAgentHandler));
            }
            ToolHandlerKind::SpawnAgentV2 => {
                builder.register_handler(handler.name, Arc::new(SpawnAgentHandlerV2));
            }
            ToolHandlerKind::TestSync => {
                builder.register_handler(handler.name, Arc::new(TestSyncHandler));
            }
            ToolHandlerKind::ToolSearch => {
                if tool_search_handler.is_none() {
                    tool_search_handler = deferred_mcp_tools
                        .as_ref()
                        .map(|tools| Arc::new(ToolSearchHandler::new(tools.clone())));
                }
                if let Some(tool_search_handler) = tool_search_handler.as_ref() {
                    builder.register_handler(handler.name, tool_search_handler.clone());
                }
            }
            ToolHandlerKind::ToolSuggest => {
                builder.register_handler(handler.name, tool_suggest_handler.clone());
            }
            ToolHandlerKind::UnifiedExec => {
                builder.register_handler(handler.name, unified_exec_handler.clone());
            }
            ToolHandlerKind::ViewImage => {
                builder.register_handler(handler.name, view_image_handler.clone());
            }
            ToolHandlerKind::WaitAgentV1 => {
                builder.register_handler(handler.name, Arc::new(WaitAgentHandler));
            }
            ToolHandlerKind::WaitAgentV2 => {
                builder.register_handler(handler.name, Arc::new(WaitAgentHandlerV2));
            }
        }
    }
    builder
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
