use anyhow::Context;
use anyhow::Result;
use codex_cloud_tasks_client::TaskFeed;
use codex_cloud_tasks_client::TaskListPage;
use codex_cloud_tasks_client::TaskListRequest;
use codex_cloud_tasks_client::TaskStatus;
use codex_cloud_tasks_client::TaskSummary;
use serde::Serialize;
use serde_json;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::status_label;
use crate::cloud::types::HistoryArgs;
use crate::cloud::types::ListArgs;
use crate::cloud::types::PagedListArgs;

pub async fn run_list(context: &CloudContext, args: &ListArgs) -> Result<()> {
    run_feed(context, &args.options, TaskFeed::Current).await
}

pub async fn run_history(context: &CloudContext, args: &HistoryArgs) -> Result<()> {
    run_feed(context, &args.options, TaskFeed::History).await
}

async fn run_feed(context: &CloudContext, options: &PagedListArgs, feed: TaskFeed) -> Result<()> {
    let mut request = TaskListRequest {
        feed,
        sort: options.sort.to_backend(),
        ..TaskListRequest::default()
    };
    if let Some(env) = &options.environment
        && !env.is_empty()
    {
        request.environment_id = Some(env.clone());
    }
    if let Some(size) = options.page_size {
        request.limit = Some(size);
    }
    if let Some(cursor) = &options.page_cursor
        && !cursor.is_empty()
    {
        request.cursor = Some(cursor.clone());
    }
    let TaskListPage {
        mut tasks,
        next_cursor,
    } = context.fetch_task_page(request).await?;

    if let Some(filter) = &options.filter {
        let needle = filter.to_lowercase();
        tasks.retain(|task| {
            task.id.0.to_lowercase().contains(&needle)
                || task.title.to_lowercase().contains(&needle)
        });
    }

    if !options.statuses.is_empty() {
        let allowed: Vec<TaskStatus> = options
            .statuses
            .iter()
            .map(|filter| filter.to_status())
            .collect();
        tasks.retain(|task| allowed.iter().any(|status| status == &task.status));
    }

    if options.json {
        let output = TaskPageOutput {
            tasks: tasks.clone(),
            next_cursor: next_cursor.clone(),
        };
        let json =
            serde_json::to_string_pretty(&output).context("Failed to render task list as JSON")?;
        println!("{json}");
        return Ok(());
    }

    if tasks.is_empty() {
        println!("No matching tasks.");
        if let Some(cursor) = next_cursor {
            println!("\nNext cursor: {cursor}");
        }
        return Ok(());
    }

    let mut id_width = 2_usize;
    let mut status_width = 6_usize;
    for task in &tasks {
        id_width = id_width.max(task.id.0.len());
        status_width = status_width.max(status_label(&task.status).len());
    }

    println!(
        "{:<id_width$}  {:<status_width$}  TITLE",
        "ID",
        "STATUS",
        id_width = id_width,
        status_width = status_width
    );
    for task in tasks {
        let id = task.id.0;
        let status = status_label(&task.status);
        let title = task.title;
        println!("{id:<id_width$}  {status:<status_width$}  {title}");
    }

    if let Some(cursor) = next_cursor {
        println!("\nNext cursor: {cursor}");
    }

    Ok(())
}

#[derive(Serialize)]
struct TaskPageOutput {
    tasks: Vec<TaskSummary>,
    next_cursor: Option<String>,
}
