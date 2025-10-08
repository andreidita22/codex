use std::path::PathBuf;

use clap::ArgGroup;
use clap::Args;
use clap::Subcommand;
use clap::ValueEnum;
use codex_backend_openapi_models::models::TaskListItem;
use codex_cloud_tasks_client::AttemptStatus;
use codex_cloud_tasks_client::TaskListSort;
use codex_cloud_tasks_client::TaskStatus;
use codex_cloud_tasks_client::TaskSummary;
use serde::Serialize;

#[derive(Debug, Args, Clone)]
#[command(
    group = ArgGroup::new("prompt_src")
        .required(true)
        .args(["prompt", "prompt_file"])
)]
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

    /// Wait for task completion; see --timeout to bound the wait duration
    #[arg(long)]
    pub wait: bool,

    /// Maximum time to wait when --wait is supplied (e.g. "5m", "90s")
    #[arg(long, value_name = "DURATION")]
    pub timeout: Option<String>,

    #[arg(long, value_name = "DIR")]
    pub export_dir: Option<PathBuf>,

    #[arg(long, help = "Alias for --export-dir=<cwd>; prefer --export-dir")]
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

    #[arg(long, help = "Alias for --export-dir=<cwd>; prefer --export-dir")]
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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ListSortArg {
    #[value(alias = "desc")]
    UpdatedDesc,
    #[value(alias = "asc")]
    UpdatedAsc,
}

impl ListSortArg {
    pub fn to_backend(self) -> TaskListSort {
        match self {
            Self::UpdatedDesc => TaskListSort::UpdatedDesc,
            Self::UpdatedAsc => TaskListSort::UpdatedAsc,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StatusFilter {
    Pending,
    Ready,
    Applied,
    Error,
}

impl StatusFilter {
    pub fn to_status(self) -> TaskStatus {
        match self {
            Self::Pending => TaskStatus::Pending,
            Self::Ready => TaskStatus::Ready,
            Self::Applied => TaskStatus::Applied,
            Self::Error => TaskStatus::Error,
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct PagedListArgs {
    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub filter: Option<String>,

    #[arg(long = "env", value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(
        long = "status",
        value_enum,
        value_delimiter = ',',
        value_name = "STATUS"
    )]
    pub statuses: Vec<StatusFilter>,

    #[arg(
        long = "sort",
        value_enum,
        default_value_t = ListSortArg::UpdatedDesc,
        value_name = "ORDER"
    )]
    pub sort: ListSortArg,

    #[arg(long = "page-size", value_name = "N")]
    pub page_size: Option<usize>,

    #[arg(long = "page-cursor", value_name = "CURSOR")]
    pub page_cursor: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct ListArgs {
    #[command(flatten)]
    pub options: PagedListArgs,
}

#[derive(Debug, Args, Clone)]
pub struct HistoryArgs {
    #[command(flatten)]
    pub options: PagedListArgs,
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

    /// List recently completed Codex Cloud tasks.
    History(HistoryArgs),

    /// Show task metadata and variant summary.
    Show(ShowArgs),

    /// Print the unified diff for a task variant.
    Diff(DiffArgs),

    /// Export patches and reports for task variants.
    Export(ExportArgs),

    /// Apply task variants locally, creating branches/worktrees as needed.
    Apply(ApplyArgs),
}

#[derive(Clone)]
pub struct TaskData {
    pub task_id: String,
    pub summary: Option<TaskSummary>,
    pub list_item: Option<TaskListItem>,
    pub variants: Vec<VariantData>,
    pub attempt_total_hint: Option<usize>,
}

#[derive(Clone)]
pub struct VariantData {
    pub index: usize,
    pub is_base: bool,
    pub attempt_placement: Option<i64>,
    pub status: AttemptStatus,
    pub diff: Option<String>,
    pub messages: Vec<String>,
    pub prompt: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Serialize)]
pub struct ShowJsonOutput {
    pub task_id: String,
    pub summary: Option<TaskSummary>,
    pub task: Option<TaskListItem>,
    pub attempt_total: Option<usize>,
    pub variants: Vec<VariantJson>,
}

#[derive(Serialize)]
pub struct VariantJson {
    pub variant_index: usize,
    pub is_base: bool,
    pub attempt_placement: Option<i64>,
    pub status: String,
    pub diff: Option<String>,
    pub messages: Vec<String>,
    pub prompt: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Serialize)]
pub struct ExportReport {
    pub task_id: String,
    pub variant_index: usize,
    pub status: String,
    pub attempt_placement: Option<i64>,
    pub prompt: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CommandOutput {
    pub stdout: String,
}

#[derive(Default, Clone, Copy)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
}

impl DiffStats {
    pub fn total_lines(&self) -> usize {
        self.added + self.removed
    }

    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}
