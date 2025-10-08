#![allow(clippy::expect_used)]

use anyhow::Result;
use assert_cmd::Command;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use tempfile::tempdir;

#[test]
fn apply_new_file_creates_branch_and_commit() -> Result<()> {
    let setup = GitFixture::with_files(&[("README.md", "Intro\nHello\n")])?;
    let repo = setup.repo_path();

    let mut cmd = Command::cargo_bin("codex")?;
    cmd.current_dir(repo)
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .env("CODEX_APPLY_GIT_CFG", "core.protectNTFS=false")
        .args([
            "cloud",
            "apply",
            "T-1002",
            "--variant",
            "1",
            "--base",
            "main",
            "--commit-msg-template",
            "Test {task_id}#{variant}",
        ])
        .assert()
        .success();

    let branch = "cloud/T-1002/var1";
    git(repo, ["rev-parse", "--verify", branch])?;
    let commit_subject = git(repo, ["log", "-1", "--pretty=%s", branch])?;
    assert_eq!(commit_subject.trim(), "Test T-1002#1");

    let file_contents = git(repo, ["show", &format!("{branch}:CONTRIBUTING.md")])?;
    assert!(file_contents.contains("Contributing"));

    Ok(())
}

#[test]
fn apply_three_way_fallback_requires_flag() -> Result<()> {
    let setup = GitFixture::with_files(&[("README.md", "Intro\nHello\n")])?;
    let repo = setup.repo_path();

    // Modify README so the patch does not apply cleanly without a 3-way merge.
    fs::write(repo.join("README.md"), "Intro\nHello there!\n")?;
    git(repo, ["add", "README.md"])?;
    git(repo, ["commit", "-m", "Tweak greeting"])?;
    git(repo, ["push", "origin", "main"])?;

    let mut no_three_way = Command::cargo_bin("codex")?;
    let assert = no_three_way
        .current_dir(repo)
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .env("CODEX_APPLY_GIT_CFG", "core.protectNTFS=false")
        .args([
            "cloud",
            "apply",
            "T-1000",
            "--variant",
            "1",
            "--base",
            "main",
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("Preflight failed for variant"),
        "stderr missing preflight diagnostics: {stderr}"
    );

    // Now retry with --three-way and expect success.
    let mut with_three_way = Command::cargo_bin("codex")?;
    let failure = with_three_way
        .current_dir(repo)
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .env("CODEX_APPLY_GIT_CFG", "core.protectNTFS=false")
        .args([
            "cloud",
            "apply",
            "T-1000",
            "--variant",
            "1",
            "--base",
            "main",
            "--three-way",
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&failure.get_output().stderr).into_owned();
    assert!(
        stderr.contains("Applied patch to 'README.md' with conflicts."),
        "stderr missing three-way diagnostics: {stderr}"
    );
    assert!(stderr.contains("Preflight failed for variant"));

    Ok(())
}

#[test]
fn apply_worktrees_supports_jobs_and_custom_dir() -> Result<()> {
    let setup = GitFixture::with_files(&[("README.md", "Intro\nHello\n")])?;
    let repo = setup.repo_path();
    let worktree_parent = tempdir().expect("worktree parent");

    let mut cmd = Command::cargo_bin("codex")?;
    cmd.current_dir(repo)
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .env("CODEX_APPLY_GIT_CFG", "core.protectNTFS=false")
        .args([
            "cloud",
            "apply",
            "T-1000",
            "--all",
            "--base",
            "main",
            "--jobs",
            "2",
            "--worktrees",
            "--worktree-dir",
            worktree_parent.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let root = worktree_parent.path().join("_cloud");
    assert!(root.exists(), "missing worktree root");

    let branch_one = root.join("cloud_T-1000_var1");
    let branch_two = root.join("cloud_T-1000_var2");
    assert!(branch_one.join("README.md").exists());
    assert!(branch_two.join("README.md").exists());

    Ok(())
}

#[derive(Debug)]
struct GitFixture {
    #[allow(dead_code)]
    remote: tempfile::TempDir,
    repo: tempfile::TempDir,
}

impl GitFixture {
    fn with_files(files: &[(&str, &str)]) -> Result<Self> {
        let remote = tempdir().expect("remote");
        git(remote.path(), ["init", "--bare"])?;

        let repo = tempdir().expect("repo");
        git(repo.path(), ["init"])?;
        git(repo.path(), ["config", "user.name", "Codex Test"])?;
        git(repo.path(), ["config", "user.email", "codex@example.com"])?;

        for (path, contents) in files {
            let file_path = repo.path().join(path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(file_path, contents)?;
        }

        git(repo.path(), ["add", "."])?;
        git(repo.path(), ["commit", "-m", "initial"])?;
        git(repo.path(), ["branch", "-M", "main"])?;
        git(
            repo.path(),
            [
                "remote",
                "add",
                "origin",
                remote.path().to_str().expect("remote path"),
            ],
        )?;
        git(repo.path(), ["push", "-u", "origin", "main"])?;

        Ok(Self { remote, repo })
    }

    fn repo_path(&self) -> &Path {
        self.repo.path()
    }
}

fn git<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let output = std::process::Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .current_dir(cwd)
        .args(&args_vec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git command failed: git {:?}\nstdout:{}\nstderr:{}",
            args_vec,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
