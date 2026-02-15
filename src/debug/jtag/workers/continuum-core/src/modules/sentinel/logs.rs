//! Sentinel log management — list, read, tail log streams

use serde_json::{json, Value};
use std::path::Path;

use crate::runtime::CommandResult;
use super::types::LogStreamInfo;

/// List log streams for a sentinel handle
pub async fn list_logs(logs_base_dir: &Path, params: Value) -> Result<CommandResult, String> {
    let handle_id = params.get("handle")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: handle")?;

    let logs_dir = logs_base_dir.join(handle_id);

    if !logs_dir.exists() {
        return Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "streams": [],
        })));
    }

    let mut streams = Vec::new();
    let mut entries = tokio::fs::read_dir(&logs_dir)
        .await
        .map_err(|e| format!("Failed to read logs dir: {e}"))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| format!("Read error: {e}"))? {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if ext == Some("log") || ext == Some("jsonl") {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| format!("Metadata error: {e}"))?;

            let modified = metadata.modified()
                .map(|t| {
                    let datetime: chrono::DateTime<chrono::Utc> = t.into();
                    datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
                })
                .unwrap_or_default();

            streams.push(LogStreamInfo {
                name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                size: metadata.len(),
                modified_at: modified,
            });
        }
    }

    Ok(CommandResult::Json(json!({
        "handle": handle_id,
        "logsDir": logs_dir.to_string_lossy(),
        "streams": streams,
    })))
}

/// Read a log stream with offset and limit
pub async fn read_log(logs_base_dir: &Path, params: Value) -> Result<CommandResult, String> {
    let handle_id = params.get("handle")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: handle")?;

    let stream = params.get("stream")
        .and_then(|v| v.as_str())
        .unwrap_or("combined");

    let offset = params.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let limit = params.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;

    let logs_dir = logs_base_dir.join(handle_id);
    let log_path = logs_dir.join(format!("{stream}.log"));
    let jsonl_path = logs_dir.join(format!("{stream}.jsonl"));

    let actual_path = if log_path.exists() {
        log_path
    } else if jsonl_path.exists() {
        jsonl_path
    } else {
        return Err(format!("Log stream not found: {stream}"));
    };

    let content = tokio::fs::read_to_string(&actual_path)
        .await
        .map_err(|e| format!("Failed to read log: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let selected_lines: Vec<&str> = lines
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    let truncated = offset + selected_lines.len() < total_lines;

    Ok(CommandResult::Json(json!({
        "handle": handle_id,
        "stream": stream,
        "content": selected_lines.join("\n"),
        "lineCount": selected_lines.len(),
        "totalLines": total_lines,
        "offset": offset,
        "truncated": truncated,
    })))
}

/// Tail a log stream (last N lines)
pub async fn tail_log(logs_base_dir: &Path, params: Value) -> Result<CommandResult, String> {
    let handle_id = params.get("handle")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: handle")?;

    let stream = params.get("stream")
        .and_then(|v| v.as_str())
        .unwrap_or("combined");

    let lines_count = params.get("lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

    let logs_dir = logs_base_dir.join(handle_id);
    let log_path = logs_dir.join(format!("{stream}.log"));
    let jsonl_path = logs_dir.join(format!("{stream}.jsonl"));

    let actual_path = if log_path.exists() {
        log_path
    } else if jsonl_path.exists() {
        jsonl_path
    } else {
        return Err(format!("Log stream not found: {stream}"));
    };

    let content = tokio::fs::read_to_string(&actual_path)
        .await
        .map_err(|e| format!("Failed to read log: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = total_lines.saturating_sub(lines_count);
    let tail_lines: Vec<&str> = lines.into_iter().skip(start).collect();

    Ok(CommandResult::Json(json!({
        "handle": handle_id,
        "stream": stream,
        "content": tail_lines.join("\n"),
        "lineCount": tail_lines.len(),
        "totalLines": total_lines,
    })))
}
