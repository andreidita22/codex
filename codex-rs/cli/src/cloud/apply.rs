use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_git_apply::ApplyGitRequest;
use codex_git_apply::apply_git_patch;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use once_cell::sync::Lazy;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Semaphore;
use which::which;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::helpers::checkout_branch;
use crate::cloud::helpers::compute_worktree_path;
use crate::cloud::helpers::current_branch;
use crate::cloud::helpers::default_branch_prefix;
use crate::cloud::helpers::diff_stats;
use crate::cloud::helpers::fetch_origin;
use crate::cloud::helpers::git_repo_root;
use crate::cloud::helpers::prepare_worktree;
use crate::cloud::helpers::run_git;
use crate::cloud::helpers::sanitize_branch;
use crate::cloud::helpers::select_variants;
use crate::cloud::types::ApplyArgs;
use crate::cloud::types::TaskData;
use crate::cloud::types::VariantData;

const GIT_ADD_CHUNK_SIZE: usize = 32;
static WORKTREE_GUARD: Lazy<AsyncMutex<()>> = Lazy::new(|| AsyncMutex::new(()));

pub async fn run_apply(context: &CloudContext, args: &ApplyArgs) -> Result<()> {
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
    let task_id = data.task_id.clone();

    let requested_jobs = args.jobs.unwrap_or(1).max(1);
    let jobs = if args.worktrees { requested_jobs } else { 1 };

    if !args.worktrees && requested_jobs > 1 {
        println!("Info: using jobs=1 because --worktrees is disabled (single checkout).");
    }

    if jobs == 1 {
        for variant in variants {
            apply_variant(
                &repo_root,
                &branch_prefix,
                &base_ref,
                &task_id,
                variant,
                args,
            )
            .await?;
        }
    } else {
        let semaphore = Arc::new(Semaphore::new(jobs));
        stream::iter(variants.into_iter().map(|variant| {
            let repo_root = repo_root.clone();
            let branch_prefix = branch_prefix.clone();
            let base_ref = base_ref.clone();
            let args = args.clone();
            let task_id = task_id.clone();
            let semaphore = Arc::clone(&semaphore);
            async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .context("semaphore closed while applying variants")?;
                apply_variant(
                    &repo_root,
                    &branch_prefix,
                    &base_ref,
                    &task_id,
                    variant,
                    &args,
                )
                .await
            }
        }))
        .buffer_unordered(jobs)
        .try_collect::<()>()
        .await?;
    }

    if !args.worktrees
        && let Some(branch) = current_branch
        && let Err(err) = checkout_branch(&repo_root, &branch, &branch).await
    {
        eprintln!("Warning: failed to switch back to original branch '{branch}': {err}");
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

async fn apply_variant(
    repo_root: &Path,
    branch_prefix: &str,
    base_ref: &str,
    task_id: &str,
    variant: VariantData,
    args: &ApplyArgs,
) -> Result<()> {
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff to apply", variant.index))?;
    let branch_name = format!("{branch_prefix}{index}", index = variant.index);
    if args.worktrees {
        let worktree_path = compute_worktree_path(args.worktree_dir.as_deref(), &branch_name)?;
        {
            let _guard = WORKTREE_GUARD.lock().await;
            prepare_worktree(repo_root, &worktree_path, &branch_name, base_ref).await?;
        }
        apply_in_path(
            &worktree_path,
            &branch_name,
            diff,
            variant.index,
            task_id,
            args,
        )
        .await?;
    } else {
        checkout_branch(repo_root, &branch_name, base_ref).await?;
        apply_in_path(repo_root, &branch_name, diff, variant.index, task_id, args).await?;
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
    let mut request = ApplyGitRequest {
        cwd: cwd.to_path_buf(),
        diff: diff.to_string(),
        revert: false,
        preflight: true,
        three_way: false,
    };
    let preflight = apply_git_patch(&request)
        .with_context(|| format!("Preflight failed for variant {variant_index}"))?;

    request.preflight = false;
    request.three_way = false;

    let devnull_patch = diff.contains("\n--- /dev/null\n") || diff.contains("\n+++ /dev/null\n");
    let preflight_error = (preflight.exit_code != 0).then(|| {
        format!(
            "Preflight failed for variant {variant_index} on branch {branch}.\ncommand: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            branch = branch,
            cmd = preflight.cmd_for_log,
            stdout = preflight.stdout.trim_end(),
            stderr = preflight.stderr.trim_end()
        )
    });

    let result = if preflight.exit_code == 0 {
        apply_git_patch(&request)
            .with_context(|| format!("Failed to apply patch for variant {variant_index}"))?
    } else if args.three_way && !devnull_patch {
        request.three_way = true;
        apply_git_patch(&request).with_context(|| {
            format!("Failed to three-way apply patch for variant {variant_index}")
        })?
    } else {
        bail!(preflight_error.unwrap_or_else(|| {
            format!(
                "Patch for variant {variant_index} did not apply cleanly on branch {branch} (try --three-way)"
            )
        }));
    };

    if result.exit_code != 0 {
        let mut message = format!(
            "Patch for variant {variant_index} did not apply cleanly on branch {branch}.\ncommand: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            cmd = result.cmd_for_log,
            stdout = result.stdout.trim_end(),
            stderr = result.stderr.trim_end()
        );
        if let Some(pre) = preflight_error {
            message.push_str("\nPreflight diagnostics:\n");
            message.push_str(&pre);
        }
        bail!(message);
    }

    let paths = extract_paths_from_diff(diff);
    if paths.is_empty() {
        bail!("Patch applied but no paths detected to stage for variant {variant_index}");
    }
    for chunk in paths.chunks(GIT_ADD_CHUNK_SIZE) {
        let mut args_vec = vec!["add".to_string(), "--".to_string()];
        args_vec.extend(chunk.iter().cloned());
        let args_iter: Vec<&str> = args_vec.iter().map(String::as_str).collect();
        run_git(cwd, args_iter).await?;
    }

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

fn extract_paths_from_diff(diff: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            if path != "/dev/null" {
                paths.insert(path.to_string());
            }
        } else if let Some(path) = line.strip_prefix("--- a/")
            && path != "/dev/null"
        {
            paths.insert(path.to_string());
        }
    }
    paths.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_paths_handles_creates_and_deletes() {
        let diff = "diff --git a/src/foo.rs b/src/foo.rs\n--- a/src/foo.rs\n+++ b/src/foo.rs\n@@\n+fn main() {}\n";
        let paths = extract_paths_from_diff(diff);
        assert_eq!(paths, vec!["src/foo.rs"]);

        let diff_delete = "diff --git a/src/old.rs b/src/old.rs\n--- a/src/old.rs\n+++ /dev/null\n@@\n-delete me\n";
        let paths = extract_paths_from_diff(diff_delete);
        assert_eq!(paths, vec!["src/old.rs"]);
    }
}
