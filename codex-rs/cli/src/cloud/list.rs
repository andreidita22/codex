use std::collections::HashSet;
use std::io::{self, Write};

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use codex_cloud_tasks_client::CloudTaskError;
use codex_cloud_tasks_client::TaskListPage;
use codex_cloud_tasks_client::TaskListRequest;
use codex_cloud_tasks_client::TaskListSort;
use codex_cloud_tasks_client::TaskStatus;
use codex_cloud_tasks_client::TaskSummary;
use serde::Serialize;
use serde_json;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::status_label;
use crate::cloud::types::ListArgs;
use crate::cloud::types::ListFeedArg;
use crate::cloud::types::ListSortArg;
use crate::cloud::types::PagedListArgs;

const JSON_WARN_THRESHOLD: usize = 10_000;

pub async fn run_list(context: &CloudContext, args: &ListArgs) -> Result<()> {
    run_feed(context, &args.options, ListFeedArg::Current).await
}

async fn run_feed(
    context: &CloudContext,
    options: &PagedListArgs,
    default_feed: ListFeedArg,
) -> Result<()> {
    let output_mode = if options.json {
        OutputMode::Json
    } else if options.jsonl {
        OutputMode::Jsonl
    } else {
        OutputMode::Table
    };

    let feed_arg = options.feed.unwrap_or(default_feed);
    let mut active_sort = options.sort;

    let mut request_template = TaskListRequest {
        feed: feed_arg.to_backend(),
        sort: active_sort.to_backend(),
        ..TaskListRequest::default()
    };
    request_template.limit = Some(options.page_size);
    request_template.environment_id = options
        .environment
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string);
    request_template.cursor = options
        .page_cursor
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string);

    let status_vector: Vec<TaskStatus> = options
        .statuses
        .iter()
        .map(|filter| filter.to_status())
        .collect();
    request_template.status_filters = status_vector.clone();

    let substring_filter = options.filter.as_ref().map(|needle| needle.to_lowercase());
    let status_filters: Option<HashSet<TaskStatus>> = if status_vector.is_empty() {
        None
    } else {
        Some(status_vector.iter().copied().collect())
    };
    let status_labels: Vec<String> = options
        .statuses
        .iter()
        .map(|filter| filter.label().to_string())
        .collect();

    let mut seen_ids: HashSet<String> = HashSet::new();

    let fetch_all = options.all;
    let max_pages = if fetch_all {
        options.max_pages.max(1)
    } else {
        1_usize
    };

    let start_cursor = request_template.cursor.clone();
    let mut page_cursor = start_cursor.clone();
    let mut processed_pages = 0_usize;
    let mut final_next_cursor: Option<String> = None;
    let mut truncated = false;
    let mut fallback_warned = false;

    let mut printed_any = false;
    let mut header_printed = false;
    let mut id_width = 2_usize;
    let mut status_width = 6_usize;
    let mut review_width = REVIEW_HEADER.len();

    let mut json_tasks: Vec<TaskSummary> = Vec::new();
    let mut warned_large_json = false;

    let effective_filter = ListEffectiveFilter {
        feed: feed_arg.display().to_string(),
        environment: request_template.environment_id.clone(),
        statuses: status_labels.clone(),
        substring: options.filter.clone(),
        include_reviews: options.include_reviews,
        page_size: options.page_size,
        start_cursor: start_cursor.clone(),
        all_pages: fetch_all,
        max_pages: if fetch_all { options.max_pages } else { 1 },
    };

    loop {
        if processed_pages >= max_pages {
            if fetch_all {
                truncated = final_next_cursor.is_some();
            }
            break;
        }

        let mut page_request = request_template.clone();
        page_request.cursor = page_cursor.clone();

        let (page, fallback_used) =
            match fetch_page_with_sort_fallback(context, page_request, active_sort).await {
                Ok(result) => result,
                Err(err) => {
                    emit_error(&output_mode, &err)?;
                    return Err(anyhow!(err));
                }
            };

        if fallback_used && !fallback_warned {
            eprintln!(
                "Warning: backend rejected sort {}; falling back to updated_at:desc",
                active_sort.display()
            );
            fallback_warned = true;
            active_sort = ListSortArg::UpdatedDesc;
            request_template.sort = TaskListSort::UpdatedDesc;
        }

        processed_pages += 1;
        final_next_cursor = page.next_cursor.clone();

        let filtered = filter_tasks(
            page.tasks,
            options.include_reviews,
            substring_filter.as_deref(),
            status_filters.as_ref(),
            if fetch_all { Some(&mut seen_ids) } else { None },
        );

        if filtered.is_empty()
            && fetch_all
            && let Some(next) = &final_next_cursor
        {
            page_cursor = Some(next.clone());
            continue;
        }

        match output_mode {
            OutputMode::Json => {
                if !filtered.is_empty() {
                    printed_any = true;
                    let new_len = json_tasks.len() + filtered.len();
                    if new_len >= JSON_WARN_THRESHOLD && !warned_large_json {
                        eprintln!(
                            "Warning: rendering large JSON output ({new_len} tasks); consider --jsonl or adjusting filters."
                        );
                        warned_large_json = true;
                    }
                    json_tasks.extend(filtered.into_iter());
                }
            }
            OutputMode::Jsonl => {
                if !filtered.is_empty() {
                    printed_any = true;
                }
                for task in filtered.into_iter() {
                    let record = ListJsonlRecord {
                        task,
                        page_index: processed_pages,
                        feed: effective_filter.feed.clone(),
                        sort: active_sort.display().to_string(),
                        effective_filter: effective_filter.clone(),
                    };
                    println!("{}", serde_json::to_string(&record)?);
                }
            }
            OutputMode::Table => {
                if filtered.is_empty() {
                    // nothing to print for this page
                } else {
                    printed_any = true;
                    update_widths(
                        &filtered,
                        &mut id_width,
                        &mut status_width,
                        &mut review_width,
                    );
                    if !header_printed {
                        print_table_context(
                            &effective_filter,
                            active_sort,
                            &status_labels,
                            substring_filter.as_deref(),
                            fetch_all,
                        );
                        print_header(
                            options.include_reviews,
                            id_width,
                            status_width,
                            review_width,
                        );
                        header_printed = true;
                    }
                    for task in filtered.iter() {
                        print_task_row(
                            task,
                            options.include_reviews,
                            id_width,
                            status_width,
                            review_width,
                        );
                    }
                }
            }
        }

        if !fetch_all {
            break;
        }
        match &final_next_cursor {
            Some(next) => {
                if processed_pages >= max_pages {
                    truncated = true;
                    break;
                }
                page_cursor = Some(next.clone());
            }
            None => break,
        }
    }

    match output_mode {
        OutputMode::Json => {
            let output = ListJsonOutput {
                tasks: &json_tasks,
                next_cursor: final_next_cursor.as_deref(),
                truncated,
                sort: active_sort.display(),
                pages: processed_pages,
                effective_filter: &effective_filter,
            };
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            serde_json::to_writer_pretty(&mut handle, &output)
                .context("Failed to render task list as JSON")?;
            handle.write_all(b"\n")?;
        }
        OutputMode::Jsonl => {
            if truncated {
                eprintln!("Info: truncated after {max_pages} pages; use --max-pages to adjust.");
            }
            if let Some(cursor) = final_next_cursor {
                eprintln!("Info: next_cursor={cursor}");
            }
        }
        OutputMode::Table => {
            if !printed_any {
                println!("No matching tasks.");
            }
            match final_next_cursor {
                Some(cursor) => {
                    let suffix = if truncated { ", truncated" } else { "" };
                    println!("\n(next_cursor: {cursor}{suffix})");
                }
                None if truncated => {
                    println!("\n(truncated)");
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn filter_tasks(
    tasks: Vec<TaskSummary>,
    include_reviews: bool,
    substring: Option<&str>,
    status_filters: Option<&HashSet<TaskStatus>>,
    mut dedupe: Option<&mut HashSet<String>>,
) -> Vec<TaskSummary> {
    let mut filtered = Vec::with_capacity(tasks.len());
    for task in tasks.into_iter() {
        if !include_reviews && task.is_review {
            continue;
        }
        if let Some(status_filters) = status_filters
            && !status_filters.contains(&task.status)
        {
            continue;
        }
        if let Some(substring) = substring {
            let id_match = task.id.0.to_lowercase().contains(substring);
            let title_match = task.title.to_lowercase().contains(substring);
            if !id_match && !title_match {
                continue;
            }
        }
        if let Some(set) = dedupe.as_deref_mut()
            && !set.insert(task.id.0.clone())
        {
            continue;
        }
        filtered.push(task);
    }
    filtered
}

fn update_widths(
    tasks: &[TaskSummary],
    id_width: &mut usize,
    status_width: &mut usize,
    review_width: &mut usize,
) {
    for task in tasks {
        *id_width = (*id_width).max(task.id.0.len());
        *status_width = (*status_width).max(status_label(task.status).len());
        *review_width = (*review_width).max(if task.is_review { 3 } else { 2 });
    }
}

fn print_header(include_review: bool, id_width: usize, status_width: usize, review_width: usize) {
    if include_review {
        println!(
            "{:<id_width$}  {:<status_width$}  {:<review_width$}  TITLE",
            "ID",
            "STATUS",
            REVIEW_HEADER,
            id_width = id_width,
            status_width = status_width,
            review_width = review_width,
        );
    } else {
        println!(
            "{:<id_width$}  {:<status_width$}  TITLE",
            "ID",
            "STATUS",
            id_width = id_width,
            status_width = status_width,
        );
    }
}

fn print_task_row(
    task: &TaskSummary,
    include_review: bool,
    id_width: usize,
    status_width: usize,
    review_width: usize,
) {
    let status = status_label(task.status);
    if include_review {
        let review_mark = if task.is_review { "yes" } else { "no" };
        println!(
            "{:<id_width$}  {:<status_width$}  {:<review_width$}  {}",
            task.id.0,
            status,
            review_mark,
            task.title,
            id_width = id_width,
            status_width = status_width,
            review_width = review_width,
        );
    } else {
        println!(
            "{:<id_width$}  {:<status_width$}  {}",
            task.id.0,
            status,
            task.title,
            id_width = id_width,
            status_width = status_width,
        );
    }
}

async fn fetch_page_with_sort_fallback(
    context: &CloudContext,
    request: TaskListRequest,
    requested_sort: ListSortArg,
) -> std::result::Result<(TaskListPage, bool), CloudTaskError> {
    match context.try_fetch_task_page(request.clone()).await {
        Ok(page) => Ok((page, false)),
        Err(err) => {
            if requested_sort == ListSortArg::UpdatedDesc || !is_sort_error(&err) {
                return Err(err);
            }
            let mut fallback_request = request;
            fallback_request.sort = TaskListSort::UpdatedDesc;
            let page = context.try_fetch_task_page(fallback_request).await?;
            Ok((page, true))
        }
    }
}

fn is_sort_error(err: &CloudTaskError) -> bool {
    match err {
        CloudTaskError::Http { message, .. } => {
            message.contains("sort") || message.contains("Sort")
        }
        _ => false,
    }
}

fn emit_error(mode: &OutputMode, err: &CloudTaskError) -> Result<()> {
    if matches!(mode, OutputMode::Json | OutputMode::Jsonl) {
        let payload = ListErrorOutput {
            error: err.to_string(),
            request_id: err.request_id().map(std::string::ToString::to_string),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&payload)
                .context("Failed to render error payload as JSON")?
        );
    }
    Ok(())
}

const REVIEW_HEADER: &str = "REVIEW";

enum OutputMode {
    Table,
    Json,
    Jsonl,
}

#[derive(Clone, Serialize)]
struct ListEffectiveFilter {
    feed: String,
    environment: Option<String>,
    statuses: Vec<String>,
    substring: Option<String>,
    include_reviews: bool,
    page_size: usize,
    start_cursor: Option<String>,
    all_pages: bool,
    max_pages: usize,
}

#[derive(Serialize)]
struct ListJsonOutput<'a> {
    tasks: &'a [TaskSummary],
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<&'a str>,
    truncated: bool,
    sort: &'a str,
    pages: usize,
    effective_filter: &'a ListEffectiveFilter,
}

#[derive(Serialize)]
struct ListJsonlRecord {
    task: TaskSummary,
    page_index: usize,
    feed: String,
    sort: String,
    effective_filter: ListEffectiveFilter,
}

#[derive(Serialize)]
struct ListErrorOutput {
    error: String,
    request_id: Option<String>,
}

fn print_table_context(
    filter: &ListEffectiveFilter,
    sort: ListSortArg,
    status_labels: &[String],
    substring: Option<&str>,
    fetch_all: bool,
) {
    let mut parts = vec![
        format!("Feed: {}", filter.feed),
        format!("Sort: {}", sort.display()),
        format!("Page size: {}", filter.page_size),
    ];
    if let Some(env) = &filter.environment {
        parts.push(format!("Env: {env}"));
    }
    if !status_labels.is_empty() {
        parts.push(format!("Statuses: {}", status_labels.join(",")));
    }
    if let Some(substring) = substring {
        parts.push(format!("Substring: \"{substring}\""));
    }
    if filter.include_reviews {
        parts.push("Include reviews".to_string());
    }
    if fetch_all {
        parts.push(format!("All pages (max {})", filter.max_pages));
    }
    if let Some(cursor) = &filter.start_cursor
        && !cursor.is_empty()
    {
        parts.push(format!("Start cursor: {cursor}"));
    }
    println!("{}", parts.join(" | "));
}
