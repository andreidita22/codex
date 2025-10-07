use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use codex_backend_client::Client as BackendClient;
use codex_backend_openapi_models::models::TaskListItem;
use codex_cloud_tasks::util::extract_chatgpt_account_id;
use codex_cloud_tasks::util::normalize_base_url;
use codex_cloud_tasks::util::set_user_agent_suffix;
use codex_cloud_tasks_client::AttemptStatus;
use codex_cloud_tasks_client::CloudBackend;
use codex_cloud_tasks_client::CloudTaskError;
use codex_cloud_tasks_client::CreateTaskReq;
use codex_cloud_tasks_client::DiffSummary;
use codex_cloud_tasks_client::HttpClient;
use codex_cloud_tasks_client::MockClient;
use codex_cloud_tasks_client::TaskId;
use codex_cloud_tasks_client::TaskKind;
use codex_cloud_tasks_client::TaskStatus;
use codex_cloud_tasks_client::TaskSummary;
use codex_cloud_tasks_client::TaskText;
use codex_cloud_tasks_client::TurnAttempt;
use codex_common::CliConfigOverrides;
use codex_git_apply::ApplyGitRequest;
use codex_git_apply::apply_git_patch;
use codex_login::AuthManager;
use futures::stream::StreamExt;
use futures::stream::{self};
use humantime::parse_duration;
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio::time::sleep;
use which::which;

const UA_SUFFIX: &str = "codex_cloud_headless";

#[derive(Debug, Parser)]
#[command(
    about = "Headless tooling for Codex Cloud tasks",
    long_about = "Browse tasks, inspect variants, export patches, and apply Codex Cloud outputs without launching the TUI.",
    after_help = "Examples:\n  codex cloud list --json\n  codex cloud show TASK_ID --json\n  codex cloud diff TASK_ID --variant 2\n  codex cloud export TASK_ID --variant 1 --dir out/bo4\n  codex cloud apply TASK_ID --all --base main --branch-prefix bo4/var --worktrees --three-way"
)]
pub struct CloudCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub command: Option<CloudSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum CloudSubcommand {
    /// Launch the interactive Codex Cloud TUI.
    #[clap(alias = "tui")]
    Interactive,

    /// Create a new Codex Cloud task headlessly.
    New(NewArgs),

    /// Watch a task until completion.
    Watch(WatchArgs),

    /// List available Codex Cloud tasks.
    List(ListArgs),

    /// Show task metadata and variant summary.
    Show(ShowArgs),

    /// Print the unified diff for a task variant.
    Diff(DiffArgs),

    /// Export patches and reports for task variants.
    Export(ExportArgs),

    /// Apply task variants locally, creating branches/worktrees as needed.
    Apply(ApplyArgs),
}

#[derive(Debug, Args, Clone)]
pub struct NewArgs {
    #[arg(long)]
    pub title: String,

    #[arg(long, value_name = "FILE", conflicts_with = "prompt")]
    pub prompt_file: Option<PathBuf>,

    #[arg(long, conflicts_with = "prompt_file")]
    pub prompt: Option<String>,

    #[arg(long = "best-of", default_value_t = 1)]
    pub best_of: usize,

    #[arg(long, value_name = "DIR")]
    pub repo: Option<PathBuf>,

    #[arg(long, default_value = "main")]
    pub base: String,

    #[arg(long, value_name = "ENV")]
    pub env: Option<String>,

    #[arg(long = "type", value_name = "KIND", default_value = "code")]
    pub task_type: String,

    #[arg(long)]
    pub wait: bool,

    #[arg(long, value_name = "DIR")]
    pub export_dir: Option<PathBuf>,

    #[arg(long)]
    pub include_diffs: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long, value_name = "NAME")]
    pub branch_prefix: Option<String>,

    #[arg(long, value_name = "BRANCH")]
    pub apply_base: Option<String>,

    #[arg(long)]
    pub worktrees: bool,

    #[arg(long, value_name = "DIR")]
    pub worktree_dir: Option<PathBuf>,

    #[arg(long)]
    pub three_way: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,

    #[arg(long = "commit-msg-template", value_name = "TEMPLATE")]
    pub commit_msg_template: Option<String>,

    #[arg(long)]
    pub open_pr: bool,
}

#[derive(Debug, Args, Clone)]
pub struct WatchArgs {
    pub task_id: String,

    #[arg(long, value_name = "DURATION", default_value = "5s")]
    pub interval: String,

    #[arg(long, value_name = "DURATION")]
    pub timeout: Option<String>,

    #[arg(long, value_name = "DIR")]
    pub export_dir: Option<PathBuf>,

    #[arg(long)]
    pub include_diffs: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long, value_name = "BRANCH")]
    pub base: Option<String>,

    #[arg(long, value_name = "NAME")]
    pub branch_prefix: Option<String>,

    #[arg(long)]
    pub worktrees: bool,

    #[arg(long, value_name = "DIR")]
    pub worktree_dir: Option<PathBuf>,

    #[arg(long)]
    pub three_way: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,

    #[arg(long = "commit-msg-template", value_name = "TEMPLATE")]
    pub commit_msg_template: Option<String>,

    #[arg(long)]
    pub open_pr: bool,
}

#[derive(Debug, Args, Clone)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct ShowArgs {
    pub task_id: String,

    #[arg(long, conflicts_with = "all")]
    pub variant: Option<usize>,

    #[arg(long)]
    pub all: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args, Clone)]
pub struct DiffArgs {
    pub task_id: String,

    #[arg(long)]
    pub variant: Option<usize>,
}

#[derive(Debug, Args, Clone)]
pub struct ExportArgs {
    pub task_id: String,

    #[arg(long)]
    pub dir: Option<PathBuf>,

    #[arg(long, conflicts_with = "all")]
    pub variant: Option<usize>,

    #[arg(long)]
    pub all: bool,

    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,
}

#[derive(Debug, Args, Clone)]
pub struct ApplyArgs {
    pub task_id: String,

    #[arg(long, conflicts_with = "all")]
    pub variant: Option<usize>,

    #[arg(long)]
    pub all: bool,

    #[arg(long)]
    pub base: Option<String>,

    #[arg(long, value_name = "NAME")]
    pub branch_prefix: Option<String>,

    #[arg(long)]
    pub worktrees: bool,

    #[arg(long, value_name = "DIR")]
    pub worktree_dir: Option<PathBuf>,

    #[arg(long)]
    pub three_way: bool,

    #[arg(long = "commit-msg-template", value_name = "TEMPLATE")]
    pub commit_msg_template: Option<String>,

    #[arg(long)]
    pub open_pr: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,
}

pub async fn run_cloud_command(
    cli: CloudCli,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> Result<()> {
    let CloudCli {
        config_overrides,
        command,
    } = cli;

    match command {
        None | Some(CloudSubcommand::Interactive) => {
            let tui_cli = codex_cloud_tasks::Cli { config_overrides };
            codex_cloud_tasks::run_main(tui_cli, codex_linux_sandbox_exe).await
        }
        Some(CloudSubcommand::New(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_new(&context, &args).await
        }
        Some(CloudSubcommand::Watch(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_watch(&context, &args).await
        }
        Some(CloudSubcommand::List(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_list(&context, &args).await
        }
        Some(CloudSubcommand::Show(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_show(&context, &args).await
        }
        Some(CloudSubcommand::Diff(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_diff(&context, &args).await
        }
        Some(CloudSubcommand::Export(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_export(&context, &args).await
        }
        Some(CloudSubcommand::Apply(args)) => {
            let context = CloudContext::new(config_overrides).await?;
            run_apply(&context, &args).await
        }
    }
}

struct CloudContext {
    backend: Arc<dyn CloudBackend>,
    http_extras: Option<HttpExtras>,
    is_mock: bool,
}

struct HttpExtras {
    backend: BackendClient,
}

#[derive(Clone)]
struct TaskData {
    task_id: String,
    summary: Option<TaskSummary>,
    list_item: Option<TaskListItem>,
    variants: Vec<VariantData>,
    attempt_total_hint: Option<usize>,
}

#[derive(Clone)]
struct VariantData {
    index: usize,
    is_base: bool,
    attempt_placement: Option<i64>,
    status: AttemptStatus,
    diff: Option<String>,
    messages: Vec<String>,
    prompt: Option<String>,
    turn_id: Option<String>,
}

#[derive(Serialize)]
struct ShowJsonOutput {
    task_id: String,
    summary: Option<TaskSummary>,
    task: Option<TaskListItem>,
    attempt_total: Option<usize>,
    variants: Vec<VariantJson>,
}

#[derive(Serialize)]
struct VariantJson {
    variant_index: usize,
    is_base: bool,
    attempt_placement: Option<i64>,
    status: String,
    diff: Option<String>,
    messages: Vec<String>,
    prompt: Option<String>,
    turn_id: Option<String>,
}

#[derive(Serialize)]
struct ExportReport {
    task_id: String,
    variant_index: usize,
    status: String,
    attempt_placement: Option<i64>,
    prompt: Option<String>,
    messages: Vec<String>,
}

impl CloudContext {
    async fn new(config_overrides: CliConfigOverrides) -> Result<Self> {
        set_user_agent_suffix(UA_SUFFIX);
        let use_mock = matches!(
            std::env::var("CODEX_CLOUD_TASKS_MODE").ok().as_deref(),
            Some("mock") | Some("MOCK")
        );
        if use_mock {
            return Ok(Self {
                backend: Arc::new(MockClient),
                http_extras: None,
                is_mock: true,
            });
        }

        let override_pairs = config_overrides.parse_overrides().map_err(|e| anyhow!(e))?;
        let config = codex_core::config::Config::load_with_cli_overrides(
            override_pairs,
            codex_core::config::ConfigOverrides::default(),
        )
        .await
        .with_context(|| "Failed to load configuration with CLI overrides")?;

        let raw_base_url = match std::env::var("CODEX_CLOUD_TASKS_BASE_URL") {
            Ok(val) if !val.trim().is_empty() => val,
            _ => config.chatgpt_base_url.clone(),
        };
        let base_url = normalize_base_url(&raw_base_url);
        let ua = codex_core::default_client::get_codex_user_agent();

        let mut client = HttpClient::new(base_url.clone())?.with_user_agent(ua.clone());
        let mut backend_client = BackendClient::new(base_url)?.with_user_agent(ua);

        let codex_home = config.codex_home.clone();
        let auth_manager = AuthManager::new(codex_home, false);
        let auth = auth_manager
            .auth()
            .ok_or_else(|| anyhow!("Not signed in. Run 'codex login' to sign in with ChatGPT."))?;
        let token = auth
            .get_token()
            .await
            .context("Failed to load ChatGPT session token")?;
        if token.is_empty() {
            bail!("Not signed in. Run 'codex login' to sign in with ChatGPT.");
        }
        client = client.with_bearer_token(token.clone());
        backend_client = backend_client.with_bearer_token(token.clone());

        if let Some(account_id) = auth
            .get_account_id()
            .or_else(|| extract_chatgpt_account_id(&token))
        {
            client = client.with_chatgpt_account_id(account_id.clone());
            backend_client = backend_client.with_chatgpt_account_id(account_id);
        }

        Ok(Self {
            backend: Arc::new(client),
            http_extras: Some(HttpExtras {
                backend: backend_client,
            }),
            is_mock: false,
        })
    }

    fn is_mock(&self) -> bool {
        self.is_mock
    }

    async fn list_tasks(&self) -> Result<Vec<TaskSummary>> {
        self.backend.list_tasks(None).await.map_err(map_cloud_error)
    }

    async fn get_task_text(&self, task_id: &str) -> Result<TaskText> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .get_task_text(task_id)
            .await
            .map_err(map_cloud_error)
    }

    async fn get_task_diff(&self, task_id: &str) -> Result<Option<String>> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .get_task_diff(task_id)
            .await
            .map_err(map_cloud_error)
    }

    async fn list_sibling_attempts(
        &self,
        task_id: &str,
        turn_id: &str,
    ) -> Result<Vec<TurnAttempt>> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .list_sibling_attempts(task_id, turn_id.to_string())
            .await
            .map_err(map_cloud_error)
    }

    async fn fetch_task_list_item(&self, task_id: &str) -> Result<Option<TaskListItem>> {
        let Some(extras) = &self.http_extras else {
            return Ok(None);
        };

        let (_parsed, body, _ct) = extras
            .backend
            .get_task_details_with_body(task_id)
            .await
            .context("Failed to fetch task details")?;
        let value: serde_json::Value =
            serde_json::from_str(&body).context("Failed to parse task details JSON")?;
        if let Some(task) = value.get("task") {
            let item: TaskListItem = serde_json::from_value(task.clone())
                .context("Failed to deserialize task metadata")?;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    async fn find_task_summary(&self, task_id: &str) -> Result<Option<TaskSummary>> {
        let tasks = self.list_tasks().await?;
        Ok(tasks.into_iter().find(|t| t.id.0 == task_id))
    }

    async fn load_task_data(&self, task_id: &str) -> Result<TaskData> {
        let summary = self.find_task_summary(task_id).await?;
        let list_item = self.fetch_task_list_item(task_id).await?;
        let text = self
            .get_task_text(task_id)
            .await
            .context("Failed to fetch task conversation")?;
        let base_diff = self
            .get_task_diff(task_id)
            .await
            .context("Failed to fetch task diff")?;

        let mut variants = Vec::new();
        variants.push(VariantData {
            index: 1,
            is_base: true,
            attempt_placement: text.attempt_placement,
            status: text.attempt_status,
            diff: base_diff,
            messages: text.messages.clone(),
            prompt: text.prompt.clone(),
            turn_id: text.turn_id.clone(),
        });

        let mut siblings: Vec<VariantData> = Vec::new();
        if let Some(turn_id) = text.turn_id.clone() {
            let attempts = self
                .list_sibling_attempts(task_id, &turn_id)
                .await
                .context("Failed to load sibling attempts")?;
            let seen_turn: Option<String> = text.turn_id.clone();
            for attempt in attempts {
                if seen_turn
                    .as_ref()
                    .map(|base| base == &attempt.turn_id)
                    .unwrap_or(false)
                {
                    continue;
                }
                siblings.push(VariantData {
                    index: 0,
                    is_base: false,
                    attempt_placement: attempt.attempt_placement,
                    status: attempt.status,
                    diff: attempt.diff,
                    messages: attempt.messages,
                    prompt: None,
                    turn_id: Some(attempt.turn_id),
                });
            }
        }

        siblings.sort_by(|a, b| {
            compare_attempts(
                a.attempt_placement,
                b.attempt_placement,
                a.turn_id.as_deref(),
                b.turn_id.as_deref(),
            )
        });
        for (i, variant) in siblings.into_iter().enumerate() {
            let mut variant = variant;
            variant.index = i + 2;
            variants.push(variant);
        }

        let attempt_hint = summary
            .as_ref()
            .and_then(|s| s.attempt_total)
            .or_else(|| Some(text.sibling_turn_ids.len().saturating_add(1)))
            .filter(|v| *v != 0);

        Ok(TaskData {
            task_id: task_id.to_string(),
            summary,
            list_item,
            variants,
            attempt_total_hint: attempt_hint,
        })
    }

    async fn create_task(&self, request: CreateTaskReq) -> Result<TaskId> {
        let created = self
            .backend
            .create_task(request)
            .await
            .map_err(map_cloud_error)?;
        Ok(created.id)
    }
}

fn map_cloud_error(err: CloudTaskError) -> anyhow::Error {
    anyhow!(err)
}

fn compare_attempts(
    lhs: Option<i64>,
    rhs: Option<i64>,
    lhs_turn: Option<&str>,
    rhs_turn: Option<&str>,
) -> std::cmp::Ordering {
    match (lhs, rhs) {
        (Some(l), Some(r)) => l.cmp(&r),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => lhs_turn.cmp(&rhs_turn),
    }
}

fn resolve_repo_arg(repo: &Option<PathBuf>) -> Result<Option<String>> {
    match repo {
        Some(path) => {
            let resolved_path = if path == Path::new(".") {
                env::current_dir().context("Failed to resolve current directory for --repo")?
            } else {
                path.clone()
            };
            let canonical = fs::canonicalize(&resolved_path).with_context(|| {
                format!(
                    "Failed to resolve repository path {}",
                    resolved_path.display()
                )
            })?;
            Ok(Some(canonical.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

fn parse_task_kind(raw: &str) -> Result<TaskKind> {
    let normalized = raw.trim().to_lowercase();
    match normalized.as_str() {
        "code" => Ok(TaskKind::Code),
        "review" => Ok(TaskKind::Review),
        other => bail!("Unsupported --type value '{other}'. Expected 'code' or 'review'."),
    }
}

async fn run_new(context: &CloudContext, args: &NewArgs) -> Result<()> {
    let prompt = match (&args.prompt, &args.prompt_file) {
        (Some(inline), None) => inline.clone(),
        (None, Some(path)) => fs::read_to_string(path)
            .with_context(|| format!("Failed to read prompt file {path:?}"))?,
        (Some(_), Some(_)) => unreachable!("clap enforces mutual exclusivity"),
        (None, None) => bail!("Either --prompt or --prompt-file must be provided"),
    };

    let title = args.title.trim().to_string();
    if title.is_empty() {
        bail!("--title cannot be empty");
    }

    let best_of = args.best_of.max(1);
    let best_of_u32 =
        u32::try_from(best_of).context("--best-of must fit within a 32-bit unsigned integer")?;

    let repo = resolve_repo_arg(&args.repo)?;
    let task_kind = parse_task_kind(&args.task_type)?;
    let env_id = args
        .env
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let base_ref = args.base.trim().to_string();

    let request = CreateTaskReq {
        title,
        prompt,
        best_of: best_of_u32,
        repo,
        base: Some(base_ref),
        env: env_id,
        task_kind,
    };

    let mut export_dir = args.export_dir.clone();
    if args.include_diffs && export_dir.is_none() {
        export_dir = Some(
            env::current_dir()
                .context("Failed to determine export directory for --include-diffs")?,
        );
    }

    let created = context.create_task(request).await?;
    let task_id = created.0;
    println!("{task_id}");

    if !(args.wait || export_dir.is_some() || args.apply) {
        return Ok(());
    }

    if (export_dir.is_some() || args.apply) && !args.wait {
        bail!("--export-dir and --apply require --wait to be specified");
    }

    if context.is_mock() && !args.wait {
        return Ok(());
    }

    let interval = Duration::from_secs(5);
    let timeout = None;
    let status = wait_for_completion(context, &task_id, interval, timeout).await?;
    match status {
        AttemptStatus::Completed => {
            if let Some(dir) = export_dir.as_ref() {
                let export_args = ExportArgs {
                    task_id: task_id.clone(),
                    dir: Some(dir.clone()),
                    variant: None,
                    all: true,
                    jobs: args.jobs,
                };
                run_export(context, &export_args).await?;
            }
            if args.apply {
                let apply_args = ApplyArgs {
                    task_id: task_id.clone(),
                    variant: None,
                    all: true,
                    base: args.apply_base.clone().or_else(|| Some(args.base.clone())),
                    branch_prefix: args.branch_prefix.clone(),
                    worktrees: args.worktrees,
                    worktree_dir: args.worktree_dir.clone(),
                    three_way: args.three_way,
                    commit_msg_template: args.commit_msg_template.clone(),
                    open_pr: args.open_pr,
                    dry_run: args.dry_run,
                    jobs: args.jobs,
                };
                run_apply(context, &apply_args).await?;
            }
        }
        AttemptStatus::Failed | AttemptStatus::Cancelled => {
            bail!(
                "Task {task_id} did not complete successfully (status: {})",
                attempt_status_label(status)
            );
        }
        _ => {}
    }

    Ok(())
}

async fn wait_for_completion(
    context: &CloudContext,
    task_id: &str,
    interval: Duration,
    timeout: Option<Duration>,
) -> Result<AttemptStatus> {
    if context.is_mock() {
        return Ok(AttemptStatus::Completed);
    }

    let mut last_status: Option<AttemptStatus> = None;
    let start = Instant::now();

    loop {
        let text = context.get_task_text(task_id).await?;
        let status = text.attempt_status;
        if Some(status) != last_status {
            println!("status: {}", attempt_status_label(status));
            last_status = Some(status);
        }

        match status {
            AttemptStatus::Completed | AttemptStatus::Failed | AttemptStatus::Cancelled => {
                return Ok(status);
            }
            _ => {}
        }

        if let Some(limit) = timeout
            && start.elapsed() >= limit
        {
            bail!("Timed out waiting for task {task_id}");
        }

        sleep(interval).await;
    }
}

async fn run_watch(context: &CloudContext, args: &WatchArgs) -> Result<()> {
    let interval = parse_duration(&args.interval)
        .with_context(|| format!("Failed to parse --interval value '{}'.", args.interval))?;
    let timeout = match &args.timeout {
        Some(raw) => Some(
            parse_duration(raw)
                .with_context(|| format!("Failed to parse --timeout value '{raw}'."))?,
        ),
        None => None,
    };

    let mut export_dir = args.export_dir.clone();
    if args.include_diffs && export_dir.is_none() {
        export_dir = Some(
            env::current_dir()
                .context("Failed to determine export directory for --include-diffs")?,
        );
    }

    let status = wait_for_completion(context, &args.task_id, interval, timeout).await?;
    match status {
        AttemptStatus::Completed => {
            if let Some(dir) = export_dir.as_ref() {
                let export_args = ExportArgs {
                    task_id: args.task_id.clone(),
                    dir: Some(dir.clone()),
                    variant: None,
                    all: true,
                    jobs: args.jobs,
                };
                run_export(context, &export_args).await?;
            }
            if args.apply {
                let apply_args = ApplyArgs {
                    task_id: args.task_id.clone(),
                    variant: None,
                    all: true,
                    base: args.base.clone(),
                    branch_prefix: args.branch_prefix.clone(),
                    worktrees: args.worktrees,
                    worktree_dir: args.worktree_dir.clone(),
                    three_way: args.three_way,
                    commit_msg_template: args.commit_msg_template.clone(),
                    open_pr: args.open_pr,
                    dry_run: args.dry_run,
                    jobs: args.jobs,
                };
                run_apply(context, &apply_args).await?;
            }
        }
        AttemptStatus::Failed | AttemptStatus::Cancelled => {
            bail!(
                "Task {} did not complete successfully (status: {})",
                args.task_id,
                attempt_status_label(status)
            );
        }
        _ => {}
    }

    Ok(())
}

async fn run_list(context: &CloudContext, args: &ListArgs) -> Result<()> {
    let tasks = context.list_tasks().await?;
    let filtered: Vec<TaskSummary> = match &args.filter {
        Some(filter) => {
            let needle = filter.to_lowercase();
            tasks
                .into_iter()
                .filter(|task| {
                    task.id.0.to_lowercase().contains(&needle)
                        || task.title.to_lowercase().contains(&needle)
                })
                .collect()
        }
        None => tasks,
    };

    if args.json {
        let json = serde_json::to_string_pretty(&filtered)
            .context("Failed to render task list as JSON")?;
        println!("{json}");
        return Ok(());
    }

    if filtered.is_empty() {
        println!("No matching tasks.");
        return Ok(());
    }

    let mut id_width = 2_usize;
    let mut status_width = 6_usize;
    for task in &filtered {
        id_width = id_width.max(task.id.0.len());
        status_width = status_width.max(status_label(&task.status).len());
    }

    println!(
        "{:<id_width$}  {:<status_width$}  TITLE",
        "ID",
        "STATUS",
        id_width = id_width,
        status_width = status_width
    );
    for task in filtered {
        let id = task.id.0;
        let status = status_label(&task.status);
        let title = task.title;
        println!("{id:<id_width$}  {status:<status_width$}  {title}");
    }
    Ok(())
}

fn status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Ready => "ready",
        TaskStatus::Applied => "applied",
        TaskStatus::Error => "error",
    }
}

async fn run_show(context: &CloudContext, args: &ShowArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, args.all)?;

    if args.json {
        let json = ShowJsonOutput {
            task_id: data.task_id.clone(),
            summary: data.summary.clone(),
            task: data.list_item.clone(),
            attempt_total: data.attempt_total_hint,
            variants: selected
                .into_iter()
                .map(|variant| VariantJson {
                    variant_index: variant.index,
                    is_base: variant.is_base,
                    attempt_placement: variant.attempt_placement,
                    status: attempt_status_label(variant.status).to_string(),
                    diff: variant.diff.clone(),
                    messages: variant.messages.clone(),
                    prompt: variant.prompt.clone(),
                    turn_id: variant.turn_id.clone(),
                })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&json).context("Failed to render JSON output")?;
        println!("{json}");
        return Ok(());
    }

    let title = data
        .summary
        .as_ref()
        .map(|s| s.title.as_str())
        .or_else(|| data.list_item.as_ref().map(|t| t.title.as_str()))
        .unwrap_or("<untitled>");
    if let Some(summary) = &data.summary {
        let status = status_label(&summary.status);
        println!(
            "Task {id} — {title} [{status}]",
            id = summary.id.0,
            title = title,
            status = status
        );
        if let Some(env) = summary.environment_label.as_deref() {
            println!("Environment: {env}");
        }
        let DiffSummary {
            files_changed,
            lines_added,
            lines_removed,
        } = summary.summary;
        if files_changed > 0 || lines_added > 0 || lines_removed > 0 {
            println!("Diff summary: {files_changed} files (+{lines_added} / -{lines_removed})");
        }
    } else {
        println!("Task {id} — {title}", id = data.task_id, title = title);
    }
    if let Some(total) = data.attempt_total_hint {
        println!("Variants reported: {total}");
    }
    println!("Loaded variants: {}", data.variants.len());

    for variant in selected {
        let status = attempt_status_label(variant.status);
        let placement = variant
            .attempt_placement
            .map(|p| format!("placement {p}"))
            .unwrap_or_else(|| "placement n/a".to_string());
        let message_count = variant.messages.len();
        let diff_stats = diff_stats(variant.diff.as_deref().unwrap_or(""));
        let diff_summary = if diff_stats.is_empty() {
            Cow::Borrowed("no diff")
        } else {
            Cow::Owned(format!(
                "diff lines +{} / -{}",
                diff_stats.added, diff_stats.removed
            ))
        };
        let label = if variant.is_base {
            Cow::Borrowed("(base)")
        } else {
            Cow::Owned(String::new())
        };
        println!(
            "Variant {index} {label}: {status}, {placement}, {diff_summary}, {message_count} messages",
            index = variant.index,
            label = label,
            status = status,
            placement = placement,
            diff_summary = diff_summary,
            message_count = message_count
        );
    }

    println!(
        "Use 'codex cloud diff {id} --variant <n>' to view a specific patch.",
        id = data.task_id
    );
    Ok(())
}

async fn run_diff(context: &CloudContext, args: &DiffArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, false)?;
    let variant = selected
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Variant not found"))?;
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff", variant.index))?;
    print!("{diff}");
    if !diff.ends_with('\n') {
        println!();
    }
    Ok(())
}

async fn run_export(context: &CloudContext, args: &ExportArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, args.all)?;
    if selected.is_empty() {
        bail!("No variants available to export");
    }
    let base_dir = args
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    fs::create_dir_all(&base_dir)
        .with_context(|| format!("Failed to create destination {base_dir:?}"))?;

    let jobs = args.jobs.unwrap_or(1).max(1);
    let semaphore = Arc::new(Semaphore::new(jobs));

    let variants = selected.into_iter().cloned();
    let exports = stream::iter(variants.map(|variant| {
        let semaphore = Arc::clone(&semaphore);
        let base_dir = base_dir.clone();
        let data = data.clone();
        async move {
            let _permit = semaphore
                .acquire()
                .await
                .context("semaphore closed while exporting variants")?;
            export_variant(&data, variant, &base_dir).await
        }
    }))
    .buffer_unordered(jobs)
    .collect::<Vec<_>>()
    .await;

    for result in exports {
        result?;
    }

    Ok(())
}

async fn export_variant(data: &TaskData, variant: VariantData, base_dir: &Path) -> Result<()> {
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff to export", variant.index))?;
    let dir = base_dir.join(format!("var{}", variant.index));
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {dir:?}"))?;
    let patch_path = dir.join("patch.diff");
    fs::write(&patch_path, diff).with_context(|| format!("Failed to write {patch_path:?}"))?;

    let report = ExportReport {
        task_id: data.task_id.clone(),
        variant_index: variant.index,
        status: attempt_status_label(variant.status).to_string(),
        attempt_placement: variant.attempt_placement,
        prompt: variant.prompt.clone(),
        messages: variant.messages.clone(),
    };
    let report_json =
        serde_json::to_string_pretty(&report).context("Failed to serialize report JSON")?;
    let report_path = dir.join("report.json");
    fs::write(&report_path, report_json)
        .with_context(|| format!("Failed to write {report_path:?}"))?;
    println!(
        "Wrote variant {index} files to {dir}",
        index = variant.index,
        dir = dir.display()
    );
    Ok(())
}

async fn run_apply(context: &CloudContext, args: &ApplyArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected_refs = select_variants(&data.variants, args.variant, args.all)?;
    if selected_refs.is_empty() {
        bail!("No variants available to apply");
    }
    let variants: Vec<VariantData> = selected_refs.into_iter().cloned().collect();

    if args.dry_run {
        plan_apply(&data, &variants, args);
        return Ok(());
    }

    let repo_root = git_repo_root().await?;
    let current_branch = if args.worktrees {
        None
    } else {
        Some(current_branch(&repo_root).await?)
    };

    let base_branch = match &args.base {
        Some(base) => base.clone(),
        None => current_branch.clone().unwrap_or_else(|| "main".to_string()),
    };

    fetch_origin(&repo_root, &base_branch).await?;
    let base_ref = format!("origin/{base_branch}");

    let branch_prefix = args
        .branch_prefix
        .clone()
        .unwrap_or_else(|| default_branch_prefix(&data.task_id));
    let branch_prefix = sanitize_branch(&branch_prefix);

    let requested_jobs = args.jobs.unwrap_or(1).max(1);
    let jobs = if args.worktrees { requested_jobs } else { 1 };

    if jobs == 1 {
        for variant in variants {
            apply_variant(&repo_root, &branch_prefix, &base_ref, &data, variant, args).await?;
        }
    } else {
        let semaphore = Arc::new(Semaphore::new(jobs));
        let tasks = stream::iter(variants.into_iter().map(|variant| {
            let repo_root = repo_root.clone();
            let branch_prefix = branch_prefix.clone();
            let base_ref = base_ref.clone();
            let args = args.clone();
            let data = data.clone();
            let semaphore = Arc::clone(&semaphore);
            async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .context("semaphore closed while applying variants")?;
                apply_variant(&repo_root, &branch_prefix, &base_ref, &data, variant, &args).await
            }
        }))
        .buffer_unordered(jobs)
        .collect::<Vec<_>>()
        .await;

        for result in tasks {
            result?;
        }
    }

    if !args.worktrees
        && let Some(branch) = current_branch
    {
        checkout_branch(&repo_root, &branch, &branch).await.ok();
    }

    Ok(())
}

fn plan_apply(data: &TaskData, variants: &[VariantData], args: &ApplyArgs) {
    let branch_prefix = args
        .branch_prefix
        .clone()
        .unwrap_or_else(|| default_branch_prefix(&data.task_id));
    let branch_prefix = sanitize_branch(&branch_prefix);
    let base_branch = args.base.clone().unwrap_or_else(|| "main".to_string());
    println!("Planning apply for task {id}", id = data.task_id);
    println!("Base branch: {base_branch}");
    println!(
        "Branch prefix: {branch_prefix} (worktrees: {worktrees})",
        branch_prefix = branch_prefix,
        worktrees = args.worktrees
    );
    for variant in variants {
        let branch = format!("{branch_prefix}{index}", index = variant.index);
        let status = attempt_status_label(variant.status);
        let diff_lines = diff_stats(variant.diff.as_deref().unwrap_or("")).total_lines();
        let index = variant.index;
        println!("Variant {index}: branch {branch} ({status}), diff lines: {diff_lines}");
    }
    println!("Dry run complete.");
}

#[derive(Clone, Debug, Default)]
struct CommandOutput {
    stdout: String,
}

async fn apply_variant(
    repo_root: &Path,
    branch_prefix: &str,
    base_ref: &str,
    data: &TaskData,
    variant: VariantData,
    args: &ApplyArgs,
) -> Result<()> {
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff to apply", variant.index))?;
    let branch_name = format!("{branch_prefix}{index}", index = variant.index);
    if args.worktrees {
        let worktree_path = compute_worktree_path(repo_root, &branch_name)?;
        prepare_worktree(repo_root, &worktree_path, &branch_name, base_ref).await?;
        apply_in_path(
            &worktree_path,
            &branch_name,
            diff,
            variant.index,
            &data.task_id,
            args,
        )
        .await?;
    } else {
        checkout_branch(repo_root, &branch_name, base_ref).await?;
        apply_in_path(
            repo_root,
            &branch_name,
            diff,
            variant.index,
            &data.task_id,
            args,
        )
        .await?;
    }
    Ok(())
}

async fn apply_in_path(
    cwd: &Path,
    branch: &str,
    diff: &str,
    variant_index: usize,
    task_id: &str,
    args: &ApplyArgs,
) -> Result<()> {
    let request = ApplyGitRequest {
        cwd: cwd.to_path_buf(),
        diff: diff.to_string(),
        revert: false,
        preflight: false,
        three_way: args.three_way,
    };
    let result = apply_git_patch(&request)
        .with_context(|| format!("Failed to apply patch for variant {variant_index}"))?;
    if result.exit_code != 0 {
        bail!(
            "Patch for variant {variant_index} did not apply cleanly on branch {branch}.\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }

    run_git(cwd, ["add", "-A"]).await?;

    let message = commit_message(args, task_id, variant_index);
    run_git(cwd, ["commit", "-m", message.as_str()])
        .await
        .with_context(|| format!("git commit failed on branch {branch}"))?;
    println!("Applied variant {variant_index} onto branch {branch}");

    if args.open_pr {
        push_and_open_pr(cwd, branch, args, &message).await?;
    }

    Ok(())
}

async fn push_and_open_pr(cwd: &Path, branch: &str, args: &ApplyArgs, message: &str) -> Result<()> {
    run_git(cwd, ["push", "--set-upstream", "origin", branch])
        .await
        .with_context(|| format!("git push failed for branch {branch}"))?;
    if which("gh").is_err() {
        println!(
            "'gh' CLI not found; branch {branch} pushed. Run 'gh pr create --fill --base {base} --head {branch}'.",
            branch = branch,
            base = args.base.clone().unwrap_or_else(|| "main".to_string())
        );
        return Ok(());
    }
    let base = args.base.clone().unwrap_or_else(|| "main".to_string());
    let mut command = Command::new("gh");
    command.current_dir(cwd);
    command.args([
        "pr", "create", "--fill", "--base", &base, "--head", branch, "--title", message,
    ]);
    let output = command.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr create failed for branch {branch}: {stderr}");
    }
    println!("Opened PR for branch {branch}");
    Ok(())
}

fn commit_message(args: &ApplyArgs, task_id: &str, variant_index: usize) -> String {
    match &args.commit_msg_template {
        Some(template) => template
            .replace("{task_id}", task_id)
            .replace("{variant}", &variant_index.to_string()),
        None => format!("Apply {task_id} variant {variant_index}"),
    }
}

async fn run_git<I, S>(cwd: &Path, args: I) -> Result<CommandOutput>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args_vec: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut command = Command::new("git");
    command.current_dir(cwd);
    command.args(&args_vec);
    let output = command.output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let cmd = format!("git {}", args_vec.join(" "));
        bail!(
            "Command {cmd} failed with exit {}\nstdout:\n{}\nstderr:\n{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr
        );
    }
    Ok(CommandOutput { stdout })
}

#[derive(Default)]
struct DiffStats {
    added: usize,
    removed: usize,
}

impl DiffStats {
    fn total_lines(&self) -> usize {
        self.added + self.removed
    }

    fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

fn diff_stats(diff: &str) -> DiffStats {
    let mut added = 0_usize;
    let mut removed = 0_usize;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'+') => added += 1,
            Some(b'-') => removed += 1,
            _ => {}
        }
    }
    DiffStats { added, removed }
}

fn attempt_status_label(status: AttemptStatus) -> &'static str {
    match status {
        AttemptStatus::Pending => "pending",
        AttemptStatus::InProgress => "in_progress",
        AttemptStatus::Completed => "completed",
        AttemptStatus::Failed => "failed",
        AttemptStatus::Cancelled => "cancelled",
        AttemptStatus::Unknown => "unknown",
    }
}

fn select_variants(
    variants: &[VariantData],
    requested: Option<usize>,
    all: bool,
) -> Result<Vec<&VariantData>> {
    if variants.is_empty() {
        return Ok(Vec::new());
    }
    if all {
        return Ok(variants.iter().collect());
    }
    let idx = requested.unwrap_or(1);
    let variant = variants
        .iter()
        .find(|variant| variant.index == idx)
        .ok_or_else(|| anyhow!("Variant {idx} not found"))?;
    Ok(vec![variant])
}

fn sanitize_branch(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '/' => c,
            _ => '-',
        })
        .collect()
}

fn default_branch_prefix(task_id: &str) -> String {
    let slug = task_id
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c,
            _ => '-',
        })
        .collect::<String>();
    format!("cloud/{slug}/var")
}

fn compute_worktree_path(repo_root: &Path, branch_name: &str) -> Result<PathBuf> {
    let parent = repo_root.parent().ok_or_else(|| {
        anyhow!(
            "Unable to determine worktree parent for repository {}",
            repo_root.display()
        )
    })?;
    let canonical_parent = fs::canonicalize(parent).with_context(|| {
        format!(
            "Failed to resolve worktree parent directory {}",
            parent.display()
        )
    })?;
    if canonical_parent == Path::new("/") {
        bail!(
            "Refusing to create worktrees under filesystem root for repository {}",
            repo_root.display()
        );
    }
    let worktree_root = canonical_parent.join("_cloud");
    let slug = branch_name.replace('/', "_");
    Ok(worktree_root.join(slug))
}

async fn git_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let out = run_git(&cwd, ["rev-parse", "--show-toplevel"]).await?;
    Ok(PathBuf::from(out.stdout.trim()))
}

async fn current_branch(repo_root: &Path) -> Result<String> {
    let out = run_git(repo_root, ["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    Ok(out.stdout.trim().to_string())
}

async fn fetch_origin(repo_root: &Path, base: &str) -> Result<()> {
    let result = run_git(repo_root, ["fetch", "origin", base]).await;
    if let Err(err) = result {
        bail!("git fetch origin {base} failed: {err}");
    }
    sleep(Duration::from_millis(50)).await;
    let refspec = format!("origin/{base}");
    run_git(repo_root, ["rev-parse", "--verify", refspec.as_str()])
        .await
        .context(format!("Unable to resolve {refspec}"))?;
    Ok(())
}

async fn checkout_branch(repo_root: &Path, branch: &str, base_ref: &str) -> Result<()> {
    let result = run_git(repo_root, ["checkout", "-B", branch, base_ref]).await;
    if let Err(err) = result {
        bail!("git checkout -B {branch} {base_ref} failed: {err}");
    }
    Ok(())
}

async fn prepare_worktree(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<()> {
    if worktree_path.exists() {
        run_git(
            worktree_path,
            ["checkout", "-B", branch, base_ref, "--quiet"],
        )
        .await
        .ok();
        run_git(repo_root, ["worktree", "prune"]).await.ok();
        fs::remove_dir_all(worktree_path)
            .with_context(|| format!("Failed to remove stale worktree at {worktree_path:?}"))?;
    }
    if let Some(parent) = worktree_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to prepare worktree parent for {worktree_path:?}"))?;
    }
    let path_owned = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_root,
        [
            "worktree",
            "add",
            "--force",
            "-B",
            branch,
            path_owned.as_str(),
            base_ref,
        ],
    )
    .await
    .with_context(|| {
        format!(
            "Failed to create worktree {branch} at {path}",
            path = worktree_path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_task_kind_handles_case_insensitive_input() {
        assert!(matches!(
            parse_task_kind("Code").expect("parse"),
            TaskKind::Code
        ));
        assert!(matches!(
            parse_task_kind("REVIEW").expect("parse"),
            TaskKind::Review
        ));
        assert!(parse_task_kind("unknown").is_err());
    }

    #[test]
    fn resolve_repo_arg_returns_canonical_path() {
        let temp = TempDir::new().expect("temp dir");
        let nested = temp.path().join("repo");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        let resolved = resolve_repo_arg(&Some(nested.clone()))
            .expect("resolve repo")
            .expect("path");
        let expected = std::fs::canonicalize(&nested)
            .expect("canonicalize")
            .to_string_lossy()
            .to_string();
        assert_eq!(resolved, expected);
    }
}
