#![allow(clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;

fn codex_command() -> Command {
    let mut cmd = Command::cargo_bin("codex").expect("codex binary");
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd
}

#[test]
fn new_mock_creates_task_id() {
    let mut cmd = codex_command();
    cmd.env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "new",
            "--title",
            "demo",
            "--prompt",
            "Mock prompt",
            "--best-of",
            "4",
            "--base",
            "main",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("task_local_"));
}

#[test]
fn watch_mock_succeeds() {
    let mut cmd = codex_command();
    cmd.env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "watch", "TASK-123", "--interval", "0.1s"])
        .assert()
        .success();
}
