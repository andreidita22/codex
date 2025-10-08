use crate::ApplyOutcome;
use crate::AttemptStatus;
use crate::CloudBackend;
use crate::CloudTaskError;
use crate::DiffSummary;
use crate::Result;
use crate::TaskFeed;
use crate::TaskId;
use crate::TaskListPage;
use crate::TaskListRequest;
use crate::TaskListSort;
use crate::TaskStatus;
use crate::TaskSummary;
use crate::TurnAttempt;
use crate::TurnHistoryEntry;
use crate::api::TaskText;
use chrono::DateTime;
use chrono::Utc;

#[derive(Clone, Default)]
pub struct MockClient;

#[async_trait::async_trait]
impl CloudBackend for MockClient {
    async fn list_tasks_page(&self, request: TaskListRequest) -> Result<TaskListPage> {
        let mut tasks = match request.feed {
            TaskFeed::Current => current_tasks(),
            TaskFeed::History => history_tasks(),
        };

        match request.environment_id.as_deref() {
            Some(env) => tasks.retain(|task| task.environment_id.as_deref() == Some(env)),
            None => tasks.retain(|task| task.environment_id.is_none()),
        }

        if !request.status_filters.is_empty() {
            tasks.retain(|task| {
                request
                    .status_filters
                    .iter()
                    .any(|status| status == &task.status)
            });
        }

        match request.sort {
            TaskListSort::UpdatedDesc => tasks.sort_by(|lhs, rhs| {
                rhs.updated_at
                    .cmp(&lhs.updated_at)
                    .then_with(|| lhs.id.0.cmp(&rhs.id.0))
            }),
            TaskListSort::UpdatedAsc => tasks.sort_by(|lhs, rhs| {
                lhs.updated_at
                    .cmp(&rhs.updated_at)
                    .then_with(|| lhs.id.0.cmp(&rhs.id.0))
            }),
        }

        let start_index = match request.cursor.as_ref() {
            Some(cursor) => {
                let position = tasks.iter().position(|task| task.id.0 == *cursor);
                match position {
                    Some(idx) => idx + 1,
                    None => {
                        return Err(CloudTaskError::Http {
                            message: format!("invalid cursor: {cursor}"),
                            request_id: Some("MOCK-CURSOR".to_string()),
                            source: anyhow::anyhow!("invalid cursor"),
                        });
                    }
                }
            }
            None => 0,
        };

        let limit = request.limit.unwrap_or(50);
        if limit == 0 {
            let next_cursor = tasks.get(start_index).map(|task| task.id.0.clone());
            return Ok(TaskListPage {
                tasks: Vec::new(),
                next_cursor,
            });
        }

        if start_index >= tasks.len() {
            return Ok(TaskListPage {
                tasks: Vec::new(),
                next_cursor: None,
            });
        }

        let end_index = (start_index + limit).min(tasks.len());
        let page_tasks = tasks[start_index..end_index].to_vec();
        let next_cursor = if end_index < tasks.len() {
            page_tasks.last().map(|task| task.id.0.clone())
        } else {
            None
        };

        Ok(TaskListPage {
            tasks: page_tasks,
            next_cursor,
        })
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
                created_at: Some(ts("2024-05-01T12:00:10Z")),
                status: AttemptStatus::Completed,
                diff: Some(mock_diff_for(&task)),
                messages: vec!["Mock alternate attempt".to_string()],
            }]);
        }
        Ok(Vec::new())
    }

    async fn list_turn_history(&self, task: TaskId) -> Result<Vec<TurnHistoryEntry>> {
        let first_created = ts("2024-05-01T11:00:00Z");
        let alt_created = ts("2024-05-01T11:00:05Z");
        let second_created = ts("2024-05-01T11:07:00Z");
        let base_diff = mock_diff_for(&task);
        let attempts = vec![
            TurnAttempt {
                turn_id: format!("{}-turn-1", task.0),
                attempt_placement: Some(0),
                created_at: Some(first_created),
                status: AttemptStatus::Completed,
                diff: Some(base_diff.clone()),
                messages: vec!["Base attempt".to_string()],
            },
            TurnAttempt {
                turn_id: format!("{}-turn-1-alt", task.0),
                attempt_placement: Some(1),
                created_at: Some(alt_created),
                status: AttemptStatus::Completed,
                diff: Some(base_diff.replace("Task:", "Alt variant")),
                messages: vec!["Alternate attempt".to_string()],
            },
        ];

        let first_turn = TurnHistoryEntry {
            turn_id: format!("{}-turn-1", task.0),
            created_at: Some(first_created),
            prompt: Some("Mock prompt: summarize changes".to_string()),
            attempts,
        };

        let second_turn = TurnHistoryEntry {
            turn_id: format!("{}-turn-2", task.0),
            created_at: Some(second_created),
            prompt: Some("Mock prompt: follow up".to_string()),
            attempts: vec![TurnAttempt {
                turn_id: format!("{}-turn-2", task.0),
                attempt_placement: Some(0),
                created_at: Some(second_created),
                status: AttemptStatus::Completed,
                diff: None,
                messages: vec!["Follow-up attempt".to_string()],
            }],
        };

        Ok(vec![first_turn, second_turn])
    }

    async fn create_task(&self, req: crate::CreateTaskReq) -> Result<crate::CreatedTask> {
        let _ = (
            req.title,
            req.prompt,
            req.repo,
            req.base,
            req.env,
            req.task_kind,
            req.best_of,
        );
        let id = format!("task_local_{}", chrono::Utc::now().timestamp_millis());
        Ok(crate::CreatedTask { id: TaskId(id) })
    }
}

fn current_tasks() -> Vec<TaskSummary> {
    const ROWS: &[MockTaskRow] = &[
        MockTaskRow {
            id: "T-1000",
            title: "Update README formatting",
            status: TaskStatus::Ready,
            updated_at: "2024-05-01T12:00:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 6,
            lines_removed: 1,
            is_review: false,
            attempt_total: Some(2),
        },
        MockTaskRow {
            id: "T-1001",
            title: "Fix clippy warnings in core",
            status: TaskStatus::Pending,
            updated_at: "2024-04-30T18:30:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 4,
            lines_removed: 2,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "T-1002",
            title: "Add contributing guide",
            status: TaskStatus::Ready,
            updated_at: "2024-04-29T09:45:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 8,
            lines_removed: 0,
            is_review: true,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "T-1003",
            title: "Tidy docs sections",
            status: TaskStatus::Applied,
            updated_at: "2024-04-28T15:15:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 3,
            lines_removed: 1,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "T-2000",
            title: "Env A: Upgrade scripts",
            status: TaskStatus::Ready,
            updated_at: "2024-04-30T10:00:00Z",
            environment_id: Some("env-A"),
            environment_label: Some("Env A"),
            files_changed: 1,
            lines_added: 5,
            lines_removed: 1,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "T-3000",
            title: "Env B: One",
            status: TaskStatus::Ready,
            updated_at: "2024-04-29T21:00:00Z",
            environment_id: Some("env-B"),
            environment_label: Some("Env B"),
            files_changed: 1,
            lines_added: 2,
            lines_removed: 0,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "T-3001",
            title: "Env B: Two",
            status: TaskStatus::Pending,
            updated_at: "2024-04-29T20:00:00Z",
            environment_id: Some("env-B"),
            environment_label: Some("Env B"),
            files_changed: 1,
            lines_added: 1,
            lines_removed: 1,
            is_review: false,
            attempt_total: Some(1),
        },
    ];

    ROWS.iter()
        .cloned()
        .map(MockTaskRow::into_summary)
        .collect()
}

fn history_tasks() -> Vec<TaskSummary> {
    const ROWS: &[MockTaskRow] = &[
        MockTaskRow {
            id: "H-9000",
            title: "Archive README cleanup",
            status: TaskStatus::Applied,
            updated_at: "2024-03-15T13:00:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 12,
            lines_removed: 3,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "H-9001",
            title: "Rollback feature toggle",
            status: TaskStatus::Error,
            updated_at: "2024-03-10T09:00:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 6,
            lines_removed: 7,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "H-9002",
            title: "Investigate slow tests",
            status: TaskStatus::Applied,
            updated_at: "2024-03-05T16:30:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 4,
            lines_removed: 1,
            is_review: false,
            attempt_total: Some(1),
        },
        MockTaskRow {
            id: "H-9003",
            title: "Docs sync from main",
            status: TaskStatus::Applied,
            updated_at: "2024-02-28T12:00:00Z",
            environment_id: None,
            environment_label: Some("Global"),
            files_changed: 1,
            lines_added: 3,
            lines_removed: 0,
            is_review: false,
            attempt_total: Some(1),
        },
    ];

    ROWS.iter()
        .cloned()
        .map(MockTaskRow::into_summary)
        .collect()
}

#[derive(Clone)]
struct MockTaskRow {
    id: &'static str,
    title: &'static str,
    status: TaskStatus,
    updated_at: &'static str,
    environment_id: Option<&'static str>,
    environment_label: Option<&'static str>,
    files_changed: usize,
    lines_added: usize,
    lines_removed: usize,
    is_review: bool,
    attempt_total: Option<usize>,
}

impl MockTaskRow {
    fn into_summary(self) -> TaskSummary {
        TaskSummary {
            id: TaskId(self.id.to_string()),
            title: self.title.to_string(),
            status: self.status,
            updated_at: ts(self.updated_at),
            environment_id: self.environment_id.map(std::string::ToString::to_string),
            environment_label: self.environment_label.map(std::string::ToString::to_string),
            summary: DiffSummary {
                files_changed: self.files_changed,
                lines_added: self.lines_added,
                lines_removed: self.lines_removed,
            },
            is_review: self.is_review,
            attempt_total: self.attempt_total,
        }
    }
}

fn ts(input: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(input)
        .unwrap_or_else(|err| panic!("invalid mock timestamp {input}: {err}"))
        .with_timezone(&Utc)
}

fn mock_diff_for(id: &TaskId) -> String {
    match id.0.as_str() {
        "T-1000" => {
            "diff --git a/README.md b/README.md\nindex 1e764d8896d5907795cdbc6ca2128229e8886a13..17aa8506ec863dd448b431837d222d0beb857853 100644\n--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,3 @@\n Intro\n-Hello\n+Hello, world!\n+Task: T-1000\n".to_string()
        }
        "T-1001" => {
            "diff --git a/core/src/lib.rs b/core/src/lib.rs\nindex ad721843f177487dc93e1f8113d2de8ed180bb46..2e1a89ad78eb93ae5e9217779e8acbffe172dced 100644\n--- a/core/src/lib.rs\n+++ b/core/src/lib.rs\n@@ -1,2 +1,1 @@\n-use foo;\n use bar;\n".to_string()
        }
        _ => {
            "diff --git a/CONTRIBUTING.md b/CONTRIBUTING.md\nnew file mode 100644\nindex 0000000000000000000000000000000000000000..9a2ff8aa1310388bde122d8ea824eea25a741a3b\n--- /dev/null\n+++ b/CONTRIBUTING.md\n@@ -0,0 +1,3 @@\n+## Contributing\n+Please open PRs.\n+Thanks!\n".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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

    #[tokio::test]
    async fn list_tasks_page_supports_pagination() {
        let client = MockClient;
        let mut request = TaskListRequest {
            limit: Some(2),
            ..TaskListRequest::default()
        };

        let first = client
            .list_tasks_page(request.clone())
            .await
            .expect("first page");
        assert_eq!(
            first
                .tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1000", "T-1001"]
        );
        assert_eq!(first.next_cursor.as_deref(), Some("T-1001"));
        let cursor = first.next_cursor.clone().expect("next cursor");

        request.cursor = Some(cursor);
        let second = client.list_tasks_page(request).await.expect("second page");
        assert_eq!(
            second
                .tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1002", "T-1003"]
        );
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn list_tasks_page_respects_sorting() {
        let client = MockClient;
        let request = TaskListRequest {
            limit: Some(4),
            sort: TaskListSort::UpdatedAsc,
            ..TaskListRequest::default()
        };

        let page = client.list_tasks_page(request).await.expect("sorted page");
        assert_eq!(
            page.tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1003", "T-1002", "T-1001", "T-1000"]
        );
    }

    #[tokio::test]
    async fn list_tasks_page_filters_environment() {
        let client = MockClient;
        let request = TaskListRequest {
            environment_id: Some("env-B".to_string()),
            ..TaskListRequest::default()
        };

        let page = client.list_tasks_page(request).await.expect("env page");
        assert_eq!(
            page.tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["T-3000", "T-3001"]
        );
    }

    #[tokio::test]
    async fn history_feed_returns_expected_items() {
        let client = MockClient;
        let mut request = TaskListRequest {
            feed: TaskFeed::History,
            limit: Some(2),
            ..TaskListRequest::default()
        };

        let first = client
            .list_tasks_page(request.clone())
            .await
            .expect("history page");
        assert_eq!(
            first
                .tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["H-9000", "H-9001"]
        );
        assert_eq!(first.next_cursor.as_deref(), Some("H-9001"));
        let cursor = first.next_cursor.clone().expect("history cursor");

        request.cursor = Some(cursor);
        let second = client
            .list_tasks_page(request)
            .await
            .expect("history page 2");
        assert_eq!(
            second
                .tasks
                .iter()
                .map(|task| task.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["H-9002", "H-9003"]
        );
        assert!(second.next_cursor.is_none());
    }
}
