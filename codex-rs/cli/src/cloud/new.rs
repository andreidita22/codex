use std::env;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_cloud_tasks_client::CreateTaskReq;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::PostComplete;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::helpers::on_completed;
use crate::cloud::helpers::parse_task_kind;
use crate::cloud::helpers::resolve_repo_arg;
use crate::cloud::helpers::wait_for_completion;
use crate::cloud::types::ApplyArgs;
use crate::cloud::types::NewArgs;

pub async fn run_new(context: &CloudContext, args: &NewArgs) -> Result<()> {
    let prompt = match (&args.prompt, &args.prompt_file) {
        (Some(inline), None) => inline.clone(),
        (None, Some(path)) => std::fs::read_to_string(path)
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

    let apply_template = ApplyArgs {
        task_id: String::new(),
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

    let post_complete = PostComplete {
        export_dir: export_dir.clone(),
        apply: args.apply,
        apply_args: apply_template,
        jobs: args.jobs,
    };

    let created = context.create_task(request).await?;
    let task_id = created.0;
    println!("{task_id}");

    if !(args.wait || post_complete.export_dir.is_some() || post_complete.apply) {
        return Ok(());
    }

    if (post_complete.export_dir.is_some() || post_complete.apply) && !args.wait {
        bail!("--export-dir and --apply require --wait to be specified");
    }

    if context.is_mock() && !args.wait {
        return Ok(());
    }

    let interval = Duration::from_secs(5);
    let timeout = None;
    let status = wait_for_completion(context, &task_id, interval, timeout).await?;
    match status {
        codex_cloud_tasks_client::AttemptStatus::Completed => {
            on_completed(context, &task_id, &post_complete).await?;
        }
        codex_cloud_tasks_client::AttemptStatus::Failed
        | codex_cloud_tasks_client::AttemptStatus::Cancelled => {
            bail!(
                "Task {task_id} did not complete successfully (status: {})",
                attempt_status_label(status)
            );
        }
        _ => {}
    }

    Ok(())
}
