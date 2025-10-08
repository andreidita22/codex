use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_cloud_tasks_client::AttemptStatus;
use codex_cloud_tasks_client::TaskStatus;
use tokio::process::Command;
use tokio::time::sleep;

use crate::cloud::context::CloudContext;
use crate::cloud::types::ApplyArgs;
use crate::cloud::types::CommandOutput;
use crate::cloud::types::DiffStats;
use crate::cloud::types::ExportArgs;
use crate::cloud::types::VariantData;

pub fn status_label(status: TaskStatus) -> &'static str {
    status.as_label()
}

pub fn compare_attempts(
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

pub fn resolve_repo_arg(repo: &Option<PathBuf>) -> Result<Option<String>> {
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

pub fn parse_task_kind(raw: &str) -> Result<codex_cloud_tasks_client::TaskKind> {
    let normalized = raw.trim().to_lowercase();
    match normalized.as_str() {
        "code" => Ok(codex_cloud_tasks_client::TaskKind::Code),
        "review" => Ok(codex_cloud_tasks_client::TaskKind::Review),
        other => bail!("Unsupported --type value '{other}'. Expected 'code' or 'review'."),
    }
}

pub fn attempt_status_label(status: AttemptStatus) -> &'static str {
    match status {
        AttemptStatus::Pending => "pending",
        AttemptStatus::InProgress => "in_progress",
        AttemptStatus::Completed => "completed",
        AttemptStatus::Failed => "failed",
        AttemptStatus::Cancelled => "cancelled",
        AttemptStatus::Unknown => "unknown",
    }
}

pub fn select_variants(
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

pub fn sanitize_branch(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '/' => c,
            _ => '-',
        })
        .collect()
}

pub fn default_branch_prefix(task_id: &str) -> String {
    let slug = task_id
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c,
            _ => '-',
        })
        .collect::<String>();
    format!("cloud/{slug}/var")
}

pub fn compute_worktree_path(worktree_parent: Option<&Path>, branch_name: &str) -> Result<PathBuf> {
    let parent = worktree_parent
        .ok_or_else(|| anyhow!("worktree parent not set; pass --worktree-dir <DIR>"))?;
    let canonical_parent = fs::canonicalize(parent).with_context(|| {
        format!(
            "Failed to resolve worktree parent directory {}",
            parent.display()
        )
    })?;
    if canonical_parent == Path::new("/") {
        bail!("Refusing to create worktrees under filesystem root (/)");
    }
    let worktree_root = canonical_parent.join("_cloud");
    let slug = branch_name.replace('/', "_");
    Ok(worktree_root.join(slug))
}

pub async fn git_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let out = run_git(&cwd, ["rev-parse", "--show-toplevel"]).await?;
    Ok(PathBuf::from(out.stdout.trim()))
}

pub async fn current_branch(repo_root: &Path) -> Result<String> {
    let out = run_git(repo_root, ["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    Ok(out.stdout.trim().to_string())
}

pub async fn fetch_origin(repo_root: &Path, base: &str) -> Result<()> {
    run_git(repo_root, ["fetch", "origin", base])
        .await
        .with_context(|| format!("git fetch origin {base} failed"))?;

    let refspec = format!("origin/{base}");
    let mut last_err: Option<anyhow::Error> = None;
    for delay in [0_u64, 50, 100, 200, 400] {
        if delay > 0 {
            sleep(Duration::from_millis(delay)).await;
        }
        match run_git(repo_root, ["rev-parse", "--verify", refspec.as_str()]).await {
            Ok(_) => return Ok(()),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Unable to resolve {}", refspec)))
        .with_context(|| format!("Unable to resolve {refspec} after fetch"))
}

pub async fn checkout_branch(repo_root: &Path, branch: &str, base_ref: &str) -> Result<()> {
    let result = run_git(repo_root, ["checkout", "-B", branch, base_ref]).await;
    if let Err(err) = result {
        bail!("git checkout -B {branch} {base_ref} failed: {err}");
    }
    Ok(())
}

pub async fn prepare_worktree(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<()> {
    if worktree_path.exists() {
        if let Err(err) = run_git(
            worktree_path,
            ["checkout", "-B", branch, base_ref, "--quiet"],
        )
        .await
        {
            eprintln!(
                "Info: failed to reset existing worktree {}: {err}",
                worktree_path.display()
            );
        }
        if let Err(err) = run_git(repo_root, ["worktree", "prune"]).await {
            eprintln!("Info: failed to prune git worktrees: {err}");
        }
        fs::remove_dir_all(worktree_path).with_context(|| {
            format!(
                "Failed to remove stale worktree at {}",
                worktree_path.display()
            )
        })?;
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

pub async fn run_git<I, S>(cwd: &Path, args: I) -> Result<CommandOutput>
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

pub fn diff_stats(diff: &str) -> DiffStats {
    let mut stats = DiffStats::default();
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'+') => stats.added += 1,
            Some(b'-') => stats.removed += 1,
            _ => {}
        }
    }
    stats
}

pub async fn wait_for_completion(
    context: &CloudContext,
    task_id: &str,
    interval: Duration,
    timeout: Option<Duration>,
) -> Result<AttemptStatus> {
    if context.is_mock() {
        return Ok(AttemptStatus::Completed);
    }

    let mut last_status: Option<AttemptStatus> = None;
    let start = tokio::time::Instant::now();

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

pub async fn on_completed(
    context: &CloudContext,
    task_id: &str,
    opts: &PostComplete,
) -> Result<()> {
    if opts.export_dir.is_none() && !opts.apply {
        return Ok(());
    }

    if let Some(dir) = opts.export_dir.as_ref() {
        let args = ExportArgs {
            task_id: task_id.to_string(),
            dir: Some(dir.clone()),
            variant: None,
            all: true,
            jobs: opts.jobs,
        };
        crate::cloud::export::run_export(context, &args).await?;
    }
    if opts.apply {
        let mut apply_args = opts.apply_args.clone();
        apply_args.task_id = task_id.to_string();
        apply_args.variant = None;
        apply_args.all = true;
        crate::cloud::apply::run_apply(context, &apply_args).await?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct PostComplete {
    pub export_dir: Option<PathBuf>,
    pub apply: bool,
    pub apply_args: ApplyArgs,
    pub jobs: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_cloud_tasks_client::TaskKind;
    use std::path::Path;
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

    #[test]
    fn compute_worktree_path_requires_parent() {
        let err = compute_worktree_path(None, "branch-1").expect_err("expected error");
        assert!(
            err.to_string()
                .contains("worktree parent not set; pass --worktree-dir <DIR>")
        );
    }

    #[test]
    fn compute_worktree_path_rejects_root() {
        let err = compute_worktree_path(Some(Path::new("/")), "branch").expect_err("expected err");
        assert!(
            err.to_string()
                .contains("Refusing to create worktrees under filesystem root")
        );
    }
}
