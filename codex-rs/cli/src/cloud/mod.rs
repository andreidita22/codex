use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use codex_common::CliConfigOverrides;

pub mod apply;
pub mod context;
pub mod diff;
pub mod export;
pub mod helpers;
pub mod list;
pub mod new;
pub mod show;
pub mod types;
pub mod watch;

use apply::run_apply;
use diff::run_diff;
use export::run_export;
use list::run_list;
use new::run_new;
use show::run_show;
use types::CloudSubcommand;
use watch::run_watch;

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
            let context = context::build_context(config_overrides).await?;
            run_new(&context, &args).await
        }
        Some(CloudSubcommand::Watch(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_watch(&context, &args).await
        }
        Some(CloudSubcommand::List(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_list(&context, &args).await
        }
        Some(CloudSubcommand::Show(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_show(&context, &args).await
        }
        Some(CloudSubcommand::Diff(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_diff(&context, &args).await
        }
        Some(CloudSubcommand::Export(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_export(&context, &args).await
        }
        Some(CloudSubcommand::Apply(args)) => {
            let context = context::build_context(config_overrides).await?;
            run_apply(&context, &args).await
        }
    }
}
