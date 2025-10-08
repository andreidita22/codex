#![allow(clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

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

#[test]
fn watch_mock_exports_when_requested() {
    let temp = tempdir().expect("temp dir");
    let mut cmd = codex_command();
    cmd.env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "watch",
            "TASK-123",
            "--interval",
            "0.1s",
            "--include-diffs",
            "--export-dir",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let export_root = temp.path().join("var1");
    assert!(export_root.join("patch.diff").exists());
    assert!(
        fs::read_to_string(export_root.join("report.json"))
            .expect("report contents")
            .contains("variant_index")
    );
}
