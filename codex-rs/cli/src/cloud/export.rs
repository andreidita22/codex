use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futures::stream::StreamExt;
use futures::stream::{self};
use serde_json;
use tokio::sync::Semaphore;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::helpers::select_variants;
use crate::cloud::types::ExportArgs;
use crate::cloud::types::ExportReport;
use crate::cloud::types::TaskData;
use crate::cloud::types::VariantData;

pub async fn run_export(context: &CloudContext, args: &ExportArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, args.all)?;
    if selected.is_empty() {
        bail!("No variants available to export");
    }

    let base_dir: PathBuf = if let Some(dir) = &args.dir {
        dir.clone()
    } else {
        std::env::current_dir().context("Failed to determine current directory for export")?
    };
    fs::create_dir_all(&base_dir)
        .with_context(|| format!("Failed to create destination {base_dir:?}"))?;

    let jobs = args.jobs.unwrap_or(1).max(1);
    let semaphore = Arc::new(Semaphore::new(jobs));

    let exports = stream::iter(selected.into_iter().map(|variant| {
        let semaphore = Arc::clone(&semaphore);
        let base_dir = base_dir.clone();
        let data = data.clone();
        async move {
            let variant = variant.clone();
            let _permit = semaphore
                .acquire()
                .await
                .context("semaphore closed while exporting variants")?;
            export_variant(&data, variant, &base_dir).await
        }
    }))
    .buffer_unordered(jobs)
    .collect::<Vec<_>>()
    .await;

    for result in exports {
        result?;
    }

    Ok(())
}

async fn export_variant(data: &TaskData, variant: VariantData, base_dir: &Path) -> Result<()> {
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff to export", variant.index))?;
    let dir = base_dir.join(format!("var{}", variant.index));
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {dir:?}"))?;
    let patch_path = dir.join("patch.diff");
    fs::write(&patch_path, diff).with_context(|| format!("Failed to write {patch_path:?}"))?;

    let report = ExportReport {
        task_id: data.task_id.clone(),
        variant_index: variant.index,
        status: attempt_status_label(variant.status).to_string(),
        attempt_placement: variant.attempt_placement,
        prompt: variant.prompt.clone(),
        messages: variant.messages.clone(),
    };
    let report_json =
        serde_json::to_string_pretty(&report).context("Failed to serialize report JSON")?;
    let report_path = dir.join("report.json");
    fs::write(&report_path, report_json)
        .with_context(|| format!("Failed to write {report_path:?}"))?;
    println!(
        "Wrote variant {index} files to {dir}",
        index = variant.index,
        dir = dir.display()
    );
    Ok(())
}
