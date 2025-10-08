#![allow(clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
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

#[test]
fn list_mock_reports_next_cursor() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "list", "--page-size", "2"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    assert!(
        stdout.contains("Next cursor: T-1001"),
        "stdout did not report cursor: {stdout}"
    );
}

#[test]
fn list_mock_status_filter_and_sort() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "list",
            "--json",
            "--status",
            "ready",
            "--sort",
            "updated-asc",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).expect("json output");
    let tasks = value
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    assert!(!tasks.is_empty(), "expected ready tasks");
    assert!(
        tasks
            .iter()
            .all(|task| { task.get("status").and_then(Value::as_str) == Some("ready") })
    );
    let first_id = tasks[0].get("id").and_then(Value::as_str).expect("task id");
    assert_eq!(first_id, "T-1002");
}

#[test]
fn list_mock_cursor_navigation() {
    let mut first = codex_command();
    let first_assert = first
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "list", "--json", "--page-size", "2"])
        .assert()
        .success();
    let first_stdout =
        String::from_utf8(first_assert.get_output().stdout.clone()).expect("utf8 stdout");
    let first_json: Value = serde_json::from_str(&first_stdout).expect("json");
    let cursor = first_json
        .get("next_cursor")
        .and_then(Value::as_str)
        .expect("cursor present")
        .to_string();
    assert_eq!(cursor, "T-1001");

    let mut second = codex_command();
    let second_assert = second
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "list",
            "--json",
            "--page-size",
            "2",
            "--page-cursor",
            &cursor,
        ])
        .assert()
        .success();
    let second_stdout =
        String::from_utf8(second_assert.get_output().stdout.clone()).expect("utf8 stdout");
    let second_json: Value = serde_json::from_str(&second_stdout).expect("json");
    let tasks = second_json
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    let ids: Vec<_> = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("id"))
        .collect();
    assert_eq!(ids, vec!["T-1002", "T-1003"]);
    assert!(second_json.get("next_cursor").is_none_or(Value::is_null));
}

#[test]
fn history_mock_json_feed() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "history", "--json", "--page-size", "2"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).expect("json output");
    let tasks = value
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    let ids: Vec<_> = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("id"))
        .collect();
    assert_eq!(ids, vec!["H-9000", "H-9001"]);
}
