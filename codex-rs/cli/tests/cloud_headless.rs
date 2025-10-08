#![allow(clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
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
fn list_table_shows_next_cursor() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "list", "--page-size", "2"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    assert!(
        stdout.contains("(next_cursor: T-1001)"),
        "missing cursor: {stdout}"
    );
    assert!(
        !stdout.contains("Add contributing guide"),
        "review task should be hidden"
    );
}

#[test]
fn list_table_includes_review_column_when_requested() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "list", "--include-reviews", "--page-size", "3"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    assert!(stdout.contains("REVIEW"));
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("Add contributing guide") && line.contains("yes"))
    );
}

#[test]
fn list_json_metadata_and_filters() {
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
            "updated_at:asc",
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
    assert!(tasks.iter().all(|task| {
        task.get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status.eq_ignore_ascii_case("ready"))
    }));
    assert_eq!(
        value.get("sort").and_then(Value::as_str),
        Some("updated_at:asc")
    );
    assert_eq!(value.get("truncated").and_then(Value::as_bool), Some(false));
    let effective = value
        .get("effective_filter")
        .and_then(Value::as_object)
        .expect("effective filter");
    assert_eq!(
        effective
            .get("statuses")
            .and_then(Value::as_array)
            .expect("statuses")
            .len(),
        1
    );
}

#[test]
fn list_jsonl_all_dedupes() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "list",
            "--feed",
            "current",
            "--all",
            "--max-pages",
            "4",
            "--page-size",
            "1",
            "--jsonl",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 unique tasks");
    let mut seen = HashSet::new();
    for line in lines {
        let value: Value = serde_json::from_str(line).expect("json line");
        let task_id = value
            .get("task")
            .and_then(|task| task.get("id"))
            .and_then(Value::as_str)
            .expect("task id");
        assert!(seen.insert(task_id.to_string()), "duplicate task {task_id}");
    }
}

#[test]
fn list_invalid_cursor_reports_request_id() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "list", "--json", "--page-cursor", "invalid-cursor"])
        .assert()
        .failure();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).expect("json error");
    assert!(
        value
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|msg| msg.contains("invalid cursor"))
    );
    assert_eq!(
        value.get("request_id").and_then(Value::as_str),
        Some("MOCK-CURSOR")
    );
}

#[test]
fn list_cursor_navigation_json() {
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
    assert_eq!(ids, vec!["T-1003"]);
    assert!(second_json.get("next_cursor").is_none_or(Value::is_null));
}

#[test]
fn history_default_json_contains_normalized_records() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "history", "T-1000"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).expect("json output");
    assert_eq!(value.get("task_id").and_then(Value::as_str), Some("T-1000"));
    assert_eq!(value.get("total_turns").and_then(Value::as_u64), Some(2));
    assert_eq!(value.get("selected_turns").and_then(Value::as_u64), Some(2));
    let records = value
        .get("records")
        .and_then(Value::as_array)
        .expect("records array");
    assert!(records.len() >= 2);
    assert!(records.iter().any(|record| {
        record
            .get("has_diff")
            .and_then(Value::as_bool)
            .is_some_and(|flag| flag)
    }));
    assert!(records.iter().any(|record| {
        record
            .get("has_diff")
            .and_then(Value::as_bool)
            .is_some_and(|flag| !flag)
    }));
}

#[test]
fn history_turn_range_limits_and_jsonl() {
    let mut cmd = codex_command();
    let assert = cmd
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "history", "T-1000", "--turns", ":1"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).expect("json output");
    assert_eq!(value.get("selected_turns").and_then(Value::as_u64), Some(1));
    let records = value
        .get("records")
        .and_then(Value::as_array)
        .expect("records array");
    assert_eq!(records.len(), 2); // two variants for first turn

    let mut jsonl = codex_command();
    let jsonl_assert = jsonl
        .env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args(["cloud", "history", "T-1000", "--turns", "2:2", "--jsonl"])
        .assert()
        .success();
    let jsonl_stdout =
        String::from_utf8(jsonl_assert.get_output().stdout.clone()).expect("utf8 stdout");
    let lines: Vec<&str> = jsonl_stdout.lines().collect();
    assert_eq!(lines.len(), 1);
    let record: Value = serde_json::from_str(lines[0]).expect("jsonl record");
    assert_eq!(record.get("turn_index").and_then(Value::as_u64), Some(2));
    assert_eq!(record.get("has_diff").and_then(Value::as_bool), Some(false));
}

#[test]
fn history_include_diffs_writes_files() {
    let temp = tempdir().expect("temp dir");
    let mut cmd = codex_command();
    cmd.env("CODEX_CLOUD_TASKS_MODE", "mock")
        .args([
            "cloud",
            "history",
            "T-1000",
            "--turns",
            "1:1",
            "--include-diffs",
            "--out",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let history_dir = temp.path().join(sanitize("T-1000"));
    assert!(history_dir.exists(), "history directory missing");
    let variant_one = history_dir.join("turn001_variant01.diff");
    let variant_two = history_dir.join("turn001_variant02.diff");
    assert!(variant_one.exists(), "variant 1 diff missing");
    assert!(variant_two.exists(), "variant 2 diff missing");
    assert!(!history_dir.join("turn002_variant01.diff").exists());
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}
