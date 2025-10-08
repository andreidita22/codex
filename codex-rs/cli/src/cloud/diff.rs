use anyhow::Result;
use anyhow::anyhow;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::select_variants;
use crate::cloud::types::DiffArgs;

pub async fn run_diff(context: &CloudContext, args: &DiffArgs) -> Result<()> {
    let data = context.load_task_data(&args.task_id).await?;
    let selected = select_variants(&data.variants, args.variant, false)?;
    let variant = selected
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Variant not found"))?;
    let diff = variant
        .diff
        .as_ref()
        .ok_or_else(|| anyhow!("Variant {} has no diff", variant.index))?;
    print!("{diff}");
    if !diff.ends_with('\n') {
        println!();
    }
    Ok(())
}
