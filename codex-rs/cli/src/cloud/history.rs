use std::fs;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use serde::Serialize;

use crate::cloud::context::CloudContext;
use crate::cloud::helpers::attempt_status_label;
use crate::cloud::types::HistoryArgs;

pub async fn run_history(context: &CloudContext, args: &HistoryArgs) -> Result<()> {
    let history = context.fetch_turn_history(&args.task_id).await?;
    let total_turns = history.len();
    let bounds = parse_turn_bounds(args.turns.as_deref(), total_turns)?;
    let range = bounds.clamp(total_turns);

    let base_dir = if args.include_diffs {
        let root = args
            .out_dir
            .as_ref()
            .ok_or_else(|| anyhow!("--include-diffs requires --out <dir>"))?;
        let dir = root.join(sanitize_for_filename(&args.task_id));
        fs::create_dir_all(&dir).with_context(|| {
            format!(
                "Failed to create history output directory {}",
                dir.display()
            )
        })?;
        Some(dir)
    } else {
        None
    };

    let output_mode = if args.jsonl {
        HistoryOutputMode::Jsonl
    } else {
        HistoryOutputMode::Json
    };

    let mut selected_turns = 0usize;
    let mut records: Vec<HistoryRecord> = Vec::new();

    for (turn_idx_zero, turn) in history.iter().enumerate() {
        let turn_index = turn_idx_zero + 1;
        if turn_index < range.start {
            continue;
        }
        if let Some(end) = range.end
            && turn_index > end
        {
            break;
        }

        selected_turns += 1;
        if turn.attempts.is_empty() {
            continue;
        }

        for (variant_idx_zero, attempt) in turn.attempts.iter().enumerate() {
            let has_diff = attempt.diff.as_ref().is_some_and(|d| !d.trim().is_empty());
            let diff_path = if has_diff && args.include_diffs {
                if let (Some(diff_dir), Some(diff_content)) =
                    (base_dir.as_ref(), attempt.diff.as_ref())
                {
                    let file_name = format!(
                        "turn{:03}_variant{:02}.diff",
                        turn_index,
                        variant_idx_zero + 1
                    );
                    let path = diff_dir.join(file_name);
                    fs::write(&path, diff_content)
                        .with_context(|| format!("Failed to write diff {}", path.display()))?;
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            };

            let created_at = attempt
                .created_at
                .or(turn.created_at)
                .map(|ts| ts.to_rfc3339());
            let updated_at = attempt
                .created_at
                .or(turn.created_at)
                .map(|ts| ts.to_rfc3339());

            let record = HistoryRecord {
                task_id: args.task_id.clone(),
                turn_index,
                turn_id: turn.turn_id.clone(),
                variant_index: variant_idx_zero + 1,
                attempt_placement: attempt.attempt_placement,
                status: attempt_status_label(attempt.status).to_string(),
                has_diff,
                created_at,
                updated_at,
                diff_path: diff_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            };

            match output_mode {
                HistoryOutputMode::Json => records.push(record),
                HistoryOutputMode::Jsonl => {
                    println!("{}", serde_json::to_string(&record)?);
                }
            }
        }
    }

    if matches!(output_mode, HistoryOutputMode::Json) {
        let output = HistoryJsonOutput {
            task_id: args.task_id.clone(),
            total_turns,
            selected_turns,
            range: HistoryRangeOutput {
                requested: args.turns.clone(),
                start: range.start,
                end: range.end.unwrap_or(total_turns),
            },
            records,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&output).context("Failed to render history as JSON")?
        );
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct TurnBounds {
    start: Option<usize>,
    end: Option<usize>,
}

impl TurnBounds {
    fn clamp(self, total: usize) -> TurnRange {
        let start = self.start.unwrap_or(1).max(1);
        let mut end = self.end;
        if let Some(value) = end.as_mut()
            && *value > total
        {
            *value = total;
        }
        TurnRange { start, end }
    }
}

#[derive(Clone, Copy)]
struct TurnRange {
    start: usize,
    end: Option<usize>,
}

fn parse_turn_bounds(spec: Option<&str>, total_turns: usize) -> Result<TurnBounds> {
    let Some(raw) = spec else {
        return Ok(TurnBounds {
            start: None,
            end: None,
        });
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(TurnBounds {
            start: None,
            end: None,
        });
    }
    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.len() != 2 {
        bail!("Invalid --turns value '{trimmed}'. Expected format <start>:<end>.");
    }
    let start = if parts[0].is_empty() {
        None
    } else {
        Some(parse_index(parts[0])?)
    };
    let end = if parts[1].is_empty() {
        None
    } else {
        Some(parse_index(parts[1])?)
    };
    if let (Some(s), Some(e)) = (start, end)
        && s > e
    {
        bail!("Invalid --turns range: start {s} is greater than end {e}.");
    }
    if let Some(s) = start
        && s == 0
    {
        bail!("Turn indices are 1-based; got 0.");
    }
    if let Some(e) = end
        && e == 0
    {
        bail!("Turn indices are 1-based; got 0.");
    }
    if let Some(s) = start
        && s > total_turns
        && end.unwrap_or(s) > total_turns
    {
        // allow empty result but keep bounds
    }
    Ok(TurnBounds { start, end })
}

fn parse_index(raw: &str) -> Result<usize> {
    raw.trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("Invalid turn index '{raw}'"))
}

fn sanitize_for_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

#[derive(Serialize)]
struct HistoryRecord {
    task_id: String,
    turn_index: usize,
    turn_id: String,
    variant_index: usize,
    attempt_placement: Option<i64>,
    status: String,
    has_diff: bool,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_path: Option<String>,
}

#[derive(Serialize)]
struct HistoryJsonOutput {
    task_id: String,
    total_turns: usize,
    selected_turns: usize,
    range: HistoryRangeOutput,
    records: Vec<HistoryRecord>,
}

#[derive(Serialize)]
struct HistoryRangeOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    requested: Option<String>,
    start: usize,
    end: usize,
}

enum HistoryOutputMode {
    Json,
    Jsonl,
}
