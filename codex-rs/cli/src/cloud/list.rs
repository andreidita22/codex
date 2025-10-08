use anyhow::Context;
use anyhow::Result;
use codex_cloud_tasks_client::TaskSummary;
use serde_json;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::status_label;
use crate::cloud::types::ListArgs;

pub async fn run_list(context: &CloudContext, args: &ListArgs) -> Result<()> {
    let tasks = context.list_tasks().await?;
    let filtered: Vec<TaskSummary> = match &args.filter {
        Some(filter) => {
            let needle = filter.to_lowercase();
            tasks
                .into_iter()
                .filter(|task| {
                    task.id.0.to_lowercase().contains(&needle)
                        || task.title.to_lowercase().contains(&needle)
                })
                .collect()
        }
        None => tasks,
    };

    if args.json {
        let json = serde_json::to_string_pretty(&filtered)
            .context("Failed to render task list as JSON")?;
        println!("{json}");
        return Ok(());
    }

    if filtered.is_empty() {
        println!("No matching tasks.");
        return Ok(());
    }

    let mut id_width = 2_usize;
    let mut status_width = 6_usize;
    for task in &filtered {
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
    for task in filtered {
        let id = task.id.0;
        let status = status_label(&task.status);
        let title = task.title;
        println!("{id:<id_width$}  {status:<status_width$}  {title}");
    }
    Ok(())
}
