## Advanced

If you already lean on Codex every day and just need a little more control, this page collects the knobs you are most likely to reach for: tweak defaults in [Config](./config.md), add extra tools through [Model Context Protocol support](./advanced.md#model-context-protocol), and script full runs with [`codex exec`](./exec.md). Jump to the section you need and keep building.

## Config quickstart {#config-quickstart}

Most day-to-day tuning lives in `config.toml`: set approval + sandbox presets, pin model defaults, and add MCP server launchers. The [Config guide](./config.md) walks through every option and provides copy-paste examples for common setups.

## Semantic shell pause (experimental) {#semantic-shell-pause}

When you run `shell` tool calls in long-lived workflows (local dev servers, `npm run dev`, etc.) the model used to get wedged waiting for CTRL+C. The **semantic shell pause** feature teaches Codex how to pause the agent’s turn while keeping the underlying process alive, return a `run_id`, and let the agent (or you) decide whether to resume, interrupt, or kill the job later.

### Enabling the feature

Add the flag to `~/.codex/config.toml` (or an individual profile) and restart Codex:

```toml
[features]
semantic_shell_pause = true
```

When enabled:

1. Every `shell` call runs through a semantic wrapper that mirrors the sandbox/env from the legacy path.
2. Stdout/stderr continue streaming live to the UI while a ring buffer captures the most recent lines.
3. If the command becomes idle (default: 60s) or matches a “ready/prompt” regex, the tool returns a **Paused** response with a `run_id`, reason, pid, idle duration, and the buffered stdout/stderr tail.
4. The OS process continues running under Codex supervision until it exits or you issue a control action.

### Managing paused runs inside a turn

The feature automatically exposes a `semantic_shell_control` tool. Use it exactly like any other function/tool call:

```jsonc
{
  "name": "semantic_shell_control",
  "arguments": {
    "action": "resume",
    "run_id": "r-8d0bdf21",
    "pause_on_idle_ms": 30000
  }
}
```

Supported actions:

| Action     | Required args | Optional args                   | Description                                                                 |
| ---------- | ------------- | --------------------------------| --------------------------------------------------------------------------- |
| `resume`   | `run_id`      | `pause_on_idle_ms`, `pause_on_ready_pattern`, `pause_on_prompt_pattern` | Re-arm pause rules and continue streaming output until the next pause/exit. |
| `interrupt`| `run_id`      | `graceful_ms` (default 5000)    | Send SIGINT, wait `graceful_ms`, then SIGKILL if the process is still alive.|
| `kill`     | `run_id`      | –                              | Immediate `SIGKILL`.                                                        |
| `status`   | `run_id`      | –                              | Returns pid, uptime, and ms since last output.                              |
| `list`     | –             | –                              | Lists every currently paused run (run_id, pid, uptime, idle).               |

### Tool responses

- `Completed`: command exited; `stdout/stderr/aggregated_output` contain the full buffered text (same shape as the legacy shell output).
- `Paused`: the agent turn ends with metadata, e.g. “idle for 60s… run_id=r-a1b2c3”. The wrapper keeps reading stdout/stderr, so the next `resume` call picks up without losing logs.
- `Failed`: something went wrong before we could pause (spawn error, sandbox failure, etc.).

This makes loops like “start dev server → automatically pause → run tests → resume/kick server” possible without manual intervention.

## Tracing / verbose logging {#tracing-verbose-logging}

Because Codex is written in Rust, it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI defaults to `RUST_LOG=codex_core=info,codex_tui=info,codex_rmcp_client=info` and log messages are written to `~/.codex/log/codex-tui.log`, so you can leave the following running in a separate terminal to monitor log messages as they are written:

```
tail -F ~/.codex/log/codex-tui.log
```

By comparison, the non-interactive mode (`codex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.

## Model Context Protocol (MCP) {#model-context-protocol}

The Codex CLI and IDE extension is a MCP client which means that it can be configured to connect to MCP servers. For more information, refer to the [`config docs`](./config.md#mcp-integration).

## Using Codex as an MCP Server {#mcp-server}

The Codex CLI can also be run as an MCP _server_ via `codex mcp-server`. For example, you can use `codex mcp-server` to make Codex available as a tool inside of a multi-agent framework like the OpenAI [Agents SDK](https://platform.openai.com/docs/guides/agents). Use `codex mcp` separately to add/list/get/remove MCP server launchers in your configuration.

### Codex MCP Server Quickstart {#mcp-server-quickstart}

You can launch a Codex MCP server with the [Model Context Protocol Inspector](https://modelcontextprotocol.io/legacy/tools/inspector):

```bash
npx @modelcontextprotocol/inspector codex mcp-server
```

Send a `tools/list` request and you will see that there are two tools available:

**`codex`** - Run a Codex session. Accepts configuration parameters matching the Codex Config struct. The `codex` tool takes the following properties:

| Property                | Type   | Description                                                                                                                                            |
| ----------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ | --- |
| **`prompt`** (required) | string | The initial user prompt to start the Codex conversation.                                                                                               |
| `approval-policy`       | string | Approval policy for shell commands generated by the model: `untrusted`, `on-failure`, `on-request`, `never`.                                           |
| `base-instructions`     | string | The set of instructions to use instead of the default ones.                                                                                            |
| `config`                | object | Individual [config settings](https://github.com/openai/codex/blob/main/docs/config.md#config) that will override what is in `$CODEX_HOME/config.toml`. |
| `cwd`                   | string | Working directory for the session. If relative, resolved against the server process's current directory.                                               |     |
| `model`                 | string | Optional override for the model name (e.g. `o3`, `o4-mini`).                                                                                           |
| `profile`               | string | Configuration profile from `config.toml` to specify default options.                                                                                   |
| `sandbox`               | string | Sandbox mode: `read-only`, `workspace-write`, or `danger-full-access`.                                                                                 |

**`codex-reply`** - Continue a Codex session by providing the conversation id and prompt. The `codex-reply` tool takes the following properties:

| Property                        | Type   | Description                                              |
| ------------------------------- | ------ | -------------------------------------------------------- |
| **`prompt`** (required)         | string | The next user prompt to continue the Codex conversation. |
| **`conversationId`** (required) | string | The id of the conversation to continue.                  |

### Trying it Out {#mcp-server-trying-it-out}

> [!TIP]
> Codex often takes a few minutes to run. To accommodate this, adjust the MCP inspector's Request and Total timeouts to 600000ms (10 minutes) under ⛭ Configuration.

Use the MCP inspector and `codex mcp-server` to build a simple tic-tac-toe game with the following settings:

**approval-policy:** never

**prompt:** Implement a simple tic-tac-toe game with HTML, Javascript, and CSS. Write the game in a single file called index.html.

**sandbox:** workspace-write

Click "Run Tool" and you should see a list of events emitted from the Codex MCP server as it builds the game.
