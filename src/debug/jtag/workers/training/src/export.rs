/// Export Module - Training Data JSONL Export
///
/// This module handles exporting training data to JSONL format for fine-tuning.
/// Supports multiple formats: OpenAI, LLaMA, Alpaca.
///
/// PHASE 1: Stub implementation that creates empty JSONL file.
/// PHASE 2: Will integrate with TypeScript to fetch actual training data.
use crate::messages::ExportTrainingPayload;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

/// Export training data to JSONL format.
///
/// Returns (examples_exported, bytes_written, average_quality, duration_ms)
pub fn export_training_data(
    payload: &ExportTrainingPayload,
) -> std::io::Result<(usize, usize, f64, u64)> {
    let start = Instant::now();

    // Create output file
    let file = File::create(&payload.output_path)?;
    let mut writer = BufWriter::new(file);

    // PHASE 1: Write header comment only (no actual data yet)
    // PHASE 2: Will fetch TrainingExampleEntity records and export
    let header = format!(
        "# Training data export\n# Format: {}\n# Min quality: {}\n# Limit: {}\n",
        payload.format,
        payload.min_quality,
        if payload.limit == 0 {
            "unlimited".to_string()
        } else {
            payload.limit.to_string()
        }
    );

    let bytes_written = header.len();
    writer.write_all(header.as_bytes())?;
    writer.flush()?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Return stub stats (0 examples for now)
    Ok((0, bytes_written, 0.0, duration_ms))
}

/// Validate output path is writable.
pub fn validate_output_path(path: &str) -> Result<(), String> {
    // Check parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.exists() {
            return Err(format!("Parent directory does not exist: {:?}", parent));
        }
    }

    Ok(())
}
