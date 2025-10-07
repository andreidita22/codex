use crate::ApplyOutcome;
use crate::AttemptStatus;
use crate::CloudBackend;
use crate::DiffSummary;
use crate::Result;
use crate::TaskId;
use crate::TaskStatus;
use crate::TaskSummary;
use crate::TurnAttempt;
use crate::TurnHistoryEntry;
use crate::api::TaskText;
use chrono::Utc;

#[derive(Clone, Default)]
pub struct MockClient;

#[async_trait::async_trait]
impl CloudBackend for MockClient {
    async fn list_tasks(&self, _env: Option<&str>) -> Result<Vec<TaskSummary>> {
        // Slightly vary content by env to aid tests that rely on the mock
        let rows = match _env {
            Some("env-A") => vec![("T-2000", "A: First", TaskStatus::Ready)],
            Some("env-B") => vec![
                ("T-3000", "B: One", TaskStatus::Ready),
                ("T-3001", "B: Two", TaskStatus::Pending),
            ],
            _ => vec![
                ("T-1000", "Update README formatting", TaskStatus::Ready),
                ("T-1001", "Fix clippy warnings in core", TaskStatus::Pending),
                ("T-1002", "Add contributing guide", TaskStatus::Ready),
            ],
        };
        let environment_id = _env.map(str::to_string);
        let environment_label = match _env {
            Some("env-A") => Some("Env A".to_string()),
            Some("env-B") => Some("Env B".to_string()),
            Some(other) => Some(other.to_string()),
            None => Some("Global".to_string()),
        };
        let mut out = Vec::new();
        for (idx, (id_str, title, status)) in rows.into_iter().enumerate() {
            let id = TaskId(id_str.to_string());
            let diff = mock_diff_for(&id);
            let (a, d) = count_from_unified(&diff);
            let updated_at = Utc::now() - chrono::Duration::minutes(idx as i64 * 10);
            let is_review = id_str == "T-1002";
            out.push(TaskSummary {
                id,
                title: title.to_string(),
                status,
                updated_at,
                environment_id: environment_id.clone(),
                environment_label: environment_label.clone(),
                summary: DiffSummary {
                    files_changed: 1,
                    lines_added: a,
                    lines_removed: d,
                },
                is_review,
                attempt_total: Some(if id_str == "T-1000" { 2 } else { 1 }),
            });
        }
        Ok(out)
    }

    async fn get_task_diff(&self, id: TaskId) -> Result<Option<String>> {
        Ok(Some(mock_diff_for(&id)))
    }

    async fn get_task_messages(&self, _id: TaskId) -> Result<Vec<String>> {
        Ok(vec![
            "Mock assistant output: this task contains no diff.".to_string(),
        ])
    }

    async fn get_task_text(&self, _id: TaskId) -> Result<TaskText> {
        Ok(TaskText {
            prompt: Some("Why is there no diff?".to_string()),
            messages: vec!["Mock assistant output: this task contains no diff.".to_string()],
            turn_id: Some("mock-turn".to_string()),
            sibling_turn_ids: Vec::new(),
            attempt_placement: Some(0),
            attempt_status: AttemptStatus::Completed,
        })
    }

    async fn apply_task(&self, id: TaskId, _diff_override: Option<String>) -> Result<ApplyOutcome> {
        Ok(ApplyOutcome {
            applied: true,
            status: crate::ApplyStatus::Success,
            message: format!("Applied task {} locally (mock)", id.0),
            skipped_paths: Vec::new(),
            conflict_paths: Vec::new(),
        })
    }

    async fn apply_task_preflight(
        &self,
        id: TaskId,
        _diff_override: Option<String>,
    ) -> Result<ApplyOutcome> {
        Ok(ApplyOutcome {
            applied: false,
            status: crate::ApplyStatus::Success,
            message: format!("Preflight passed for task {} (mock)", id.0),
            skipped_paths: Vec::new(),
            conflict_paths: Vec::new(),
        })
    }

    async fn list_sibling_attempts(
        &self,
        task: TaskId,
        _turn_id: String,
    ) -> Result<Vec<TurnAttempt>> {
        if task.0 == "T-1000" {
            return Ok(vec![TurnAttempt {
                turn_id: "T-1000-attempt-2".to_string(),
                attempt_placement: Some(1),
                created_at: Some(Utc::now()),
                status: AttemptStatus::Completed,
                diff: Some(mock_diff_for(&task)),
                messages: vec!["Mock alternate attempt".to_string()],
            }]);
        }
        Ok(Vec::new())
    }

    async fn list_turn_history(&self, task: TaskId) -> Result<Vec<TurnHistoryEntry>> {
        let now = Utc::now();
        let base_diff = mock_diff_for(&task);
        let attempts = vec![
            TurnAttempt {
                turn_id: format!("{}-turn-1", task.0),
                attempt_placement: Some(0),
                created_at: Some(now),
                status: AttemptStatus::Completed,
                diff: Some(base_diff.clone()),
                messages: vec!["Base attempt".to_string()],
            },
            TurnAttempt {
                turn_id: format!("{}-turn-1-alt", task.0),
                attempt_placement: Some(1),
                created_at: Some(now + chrono::Duration::seconds(5)),
                status: AttemptStatus::Completed,
                diff: Some(base_diff.replace("Task:", "Alt variant")),
                messages: vec!["Alternate attempt".to_string()],
            },
        ];

        let first_turn = TurnHistoryEntry {
            turn_id: format!("{}-turn-1", task.0),
            created_at: Some(now),
            prompt: Some("Mock prompt: summarize changes".to_string()),
            attempts,
        };

        let second_turn = TurnHistoryEntry {
            turn_id: format!("{}-turn-2", task.0),
            created_at: Some(now + chrono::Duration::minutes(5)),
            prompt: Some("Mock prompt: follow up".to_string()),
            attempts: vec![TurnAttempt {
                turn_id: format!("{}-turn-2", task.0),
                attempt_placement: Some(0),
                created_at: Some(now + chrono::Duration::minutes(5)),
                status: AttemptStatus::Completed,
                diff: Some(mock_diff_for(&task)),
                messages: vec!["Follow-up attempt".to_string()],
            }],
        };

        Ok(vec![first_turn, second_turn])
    }

    async fn create_task(
        &self,
        env_id: &str,
        prompt: &str,
        git_ref: &str,
        qa_mode: bool,
        best_of_n: usize,
    ) -> Result<crate::CreatedTask> {
        let _ = (env_id, prompt, git_ref, qa_mode, best_of_n);
        let id = format!("task_local_{}", chrono::Utc::now().timestamp_millis());
        Ok(crate::CreatedTask { id: TaskId(id) })
    }
}

fn mock_diff_for(id: &TaskId) -> String {
    match id.0.as_str() {
        "T-1000" => {
            "diff --git a/README.md b/README.md\nindex 000000..111111 100644\n--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,3 @@\n Intro\n-Hello\n+Hello, world!\n+Task: T-1000\n".to_string()
        }
        "T-1001" => {
            "diff --git a/core/src/lib.rs b/core/src/lib.rs\nindex 000000..111111 100644\n--- a/core/src/lib.rs\n+++ b/core/src/lib.rs\n@@ -1,2 +1,1 @@\n-use foo;\n use bar;\n".to_string()
        }
        _ => {
            "diff --git a/CONTRIBUTING.md b/CONTRIBUTING.md\nindex 000000..111111 100644\n--- /dev/null\n+++ b/CONTRIBUTING.md\n@@ -0,0 +1,3 @@\n+## Contributing\n+Please open PRs.\n+Thanks!\n".to_string()
        }
    }
}

fn count_from_unified(diff: &str) -> (usize, usize) {
    if let Ok(patch) = diffy::Patch::from_str(diff) {
        patch
            .hunks()
            .iter()
            .flat_map(diffy::Hunk::lines)
            .fold((0, 0), |(a, d), l| match l {
                diffy::Line::Insert(_) => (a + 1, d),
                diffy::Line::Delete(_) => (a, d + 1),
                _ => (a, d),
            })
    } else {
        let mut a = 0;
        let mut d = 0;
        for l in diff.lines() {
            if l.starts_with("+++") || l.starts_with("---") || l.starts_with("@@") {
                continue;
            }
            match l.as_bytes().first() {
                Some(b'+') => a += 1,
                Some(b'-') => d += 1,
                _ => {}
            }
        }
        (a, d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_turn_history_returns_mock_data() {
        let client = MockClient;
        let turns = client
            .list_turn_history(TaskId("T-1000".to_string()))
            .await
            .expect("mock history");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].attempts.len(), 2);
        assert_eq!(turns[1].attempts.len(), 1);
        assert!(
            turns[0]
                .prompt
                .as_ref()
                .is_some_and(|prompt| prompt.contains("summarize"))
        );
    }

    #[tokio::test]
    async fn list_tasks_marks_review_items() {
        let client = MockClient;
        let tasks = client.list_tasks(None).await.expect("list tasks");
        assert!(tasks.iter().any(|task| task.is_review));
    }
}
