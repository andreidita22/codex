use std::env;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_cloud_tasks_client::AttemptStatus;
use humantime::parse_duration;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::PostComplete;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::helpers::on_completed;
use crate::cloud::helpers::wait_for_completion;
use crate::cloud::types::ApplyArgs;
use crate::cloud::types::WatchArgs;

pub async fn run_watch(context: &CloudContext, args: &WatchArgs) -> Result<()> {
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

    let post_complete = PostComplete {
        export_dir: export_dir.clone(),
        apply: args.apply,
        apply_args: ApplyArgs {
            task_id: String::new(),
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
        },
        jobs: args.jobs,
    };

    let status = wait_for_completion(context, &args.task_id, interval, timeout).await?;
    match status {
        AttemptStatus::Completed => {
            on_completed(context, &args.task_id, &post_complete).await?;
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
