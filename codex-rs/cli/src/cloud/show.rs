use std::borrow::Cow;

use anyhow::Context;
use anyhow::Result;
use codex_cloud_tasks_client::DiffSummary;
use serde_json;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::helpers::diff_stats;
use crate::cloud::helpers::select_variants;
use crate::cloud::helpers::status_label;
use crate::cloud::types::ShowArgs;
use crate::cloud::types::ShowJsonOutput;
use crate::cloud::types::VariantJson;

pub async fn run_show(context: &CloudContext, args: &ShowArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, args.all)?;

    if args.json {
        let json = ShowJsonOutput {
            task_id: data.task_id.clone(),
            summary: data.summary.clone(),
            task: data.list_item.clone(),
            attempt_total: data.attempt_total_hint,
            variants: selected
                .into_iter()
                .map(|variant| VariantJson {
                    variant_index: variant.index,
                    is_base: variant.is_base,
                    attempt_placement: variant.attempt_placement,
                    status: attempt_status_label(variant.status).to_string(),
                    diff: variant.diff.clone(),
                    messages: variant.messages.clone(),
                    prompt: variant.prompt.clone(),
                    turn_id: variant.turn_id.clone(),
                })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&json).context("Failed to render JSON output")?;
        println!("{json}");
        return Ok(());
    }

    let title = data
        .summary
        .as_ref()
        .map(|s| s.title.as_str())
        .or_else(|| data.list_item.as_ref().map(|t| t.title.as_str()))
        .unwrap_or("<untitled>");
    if let Some(summary) = &data.summary {
        let status = status_label(summary.status);
        println!(
            "Task {id} — {title} [{status}]",
            id = summary.id.0,
            title = title,
            status = status
        );
        if let Some(env) = summary.environment_label.as_deref() {
            println!("Environment: {env}");
        }
        let DiffSummary {
            files_changed,
            lines_added,
            lines_removed,
        } = summary.summary;
        if files_changed > 0 || lines_added > 0 || lines_removed > 0 {
            println!("Diff summary: {files_changed} files (+{lines_added} / -{lines_removed})");
        }
    } else {
        println!("Task {id} — {title}", id = data.task_id, title = title);
    }
    if let Some(total) = data.attempt_total_hint {
        println!("Variants reported: {total}");
    }
    println!("Loaded variants: {}", data.variants.len());

    for variant in selected {
        let status = attempt_status_label(variant.status);
        let placement = variant
            .attempt_placement
            .map(|p| format!("placement {p}"))
            .unwrap_or_else(|| "placement n/a".to_string());
        let message_count = variant.messages.len();
        let stats = diff_stats(variant.diff.as_deref().unwrap_or(""));
        let diff_summary = if stats.is_empty() {
            Cow::Borrowed("no diff")
        } else {
            Cow::Owned(format!("diff lines +{} / -{}", stats.added, stats.removed))
        };
        let label = if variant.is_base {
            Cow::Borrowed("(base)")
        } else {
            Cow::Borrowed("")
        };
        println!(
            "Variant {index} {label}: {status}, {placement}, {diff_summary}, {message_count} messages",
            index = variant.index,
            label = label,
            status = status,
            placement = placement,
            diff_summary = diff_summary,
            message_count = message_count
        );
    }

    println!(
        "Use 'codex cloud diff {id} --variant <n>' to view a specific patch.",
        id = data.task_id
    );
    Ok(())
}
