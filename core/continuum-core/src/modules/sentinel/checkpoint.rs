//! Checkpoint persistence for durable pipeline resume.
//!
//! Atomic writes: serialize → write to .tmp → rename.
//! Storage: ~/.continuum/sentinel/checkpoints/{handle}.json
//! (or `$CONTINUUM_CHECKPOINT_DIR` when set — tests use this to
//! redirect writes to a per-process tempdir so `cargo test` never
//! touches the developer's real home.)

use std::path::PathBuf;

use super::types::{PipelineCheckpoint, PipelineStatus};

/// Base directory for checkpoint storage.
///
/// Production: `~/.continuum/sentinel/checkpoints/`.
/// Tests / dev tools: set `CONTINUUM_CHECKPOINT_DIR` to redirect.
fn checkpoints_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("CONTINUUM_CHECKPOINT_DIR") {
        return PathBuf::from(override_dir);
    }
    let home = dirs::home_dir().expect("Failed to resolve home directory");
    home.join(".continuum").join("sentinel").join("checkpoints")
}

/// Ensure the checkpoints directory exists
fn ensure_dir() -> Result<(), String> {
    let dir = checkpoints_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create checkpoints dir {}: {e}", dir.display()))
}

/// Checkpoint file path for a given handle
fn checkpoint_path(handle: &str) -> PathBuf {
    checkpoints_dir().join(format!("{handle}.json"))
}

/// Save a checkpoint atomically (write .tmp then rename)
pub fn save_checkpoint(handle: &str, checkpoint: &PipelineCheckpoint) -> Result<(), String> {
    ensure_dir()?;
    let path = checkpoint_path(handle);
    let tmp_path = path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(checkpoint)
        .map_err(|e| format!("Failed to serialize checkpoint: {e}"))?;

    std::fs::write(&tmp_path, json.as_bytes())
        .map_err(|e| format!("Failed to write checkpoint tmp file: {e}"))?;

    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename checkpoint file: {e}"))?;

    Ok(())
}

/// Load a checkpoint by handle. Returns None if not found.
pub fn load_checkpoint(handle: &str) -> Result<Option<PipelineCheckpoint>, String> {
    let path = checkpoint_path(handle);
    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read checkpoint {}: {e}", path.display()))?;

    let checkpoint: PipelineCheckpoint = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse checkpoint {}: {e}", path.display()))?;

    Ok(Some(checkpoint))
}

/// List all checkpoints (reads every JSON file in the checkpoints dir)
pub fn list_checkpoints() -> Result<Vec<PipelineCheckpoint>, String> {
    let dir = checkpoints_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut checkpoints = Vec::new();
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("Failed to read checkpoints dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str::<PipelineCheckpoint>(&data) {
                    Ok(cp) => checkpoints.push(cp),
                    Err(e) => {
                        let log = crate::runtime::logger("sentinel");
                        log.warn(&format!(
                            "Skipping corrupt checkpoint {}: {e}",
                            path.display()
                        ));
                    }
                },
                Err(e) => {
                    let log = crate::runtime::logger("sentinel");
                    log.warn(&format!(
                        "Failed to read checkpoint {}: {e}",
                        path.display()
                    ));
                }
            }
        }
    }

    Ok(checkpoints)
}

/// Delete a checkpoint file
pub fn delete_checkpoint(handle: &str) -> Result<(), String> {
    let path = checkpoint_path(handle);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete checkpoint {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Scan for orphaned pipelines (status=Running on disk means process died mid-execution).
/// Returns handles that were marked as Interrupted.
pub fn recover_interrupted() -> Result<Vec<String>, String> {
    let checkpoints = list_checkpoints()?;
    let mut resumable = Vec::new();

    for mut cp in checkpoints {
        if cp.status == PipelineStatus::Running {
            // Was running when process died — mark as interrupted
            cp.status = PipelineStatus::Interrupted;
            cp.last_checkpoint_at = chrono::Utc::now().to_rfc3339();
            save_checkpoint(&cp.sentinel_handle, &cp)?;
            resumable.push(cp.sentinel_handle);
        } else if cp.status == PipelineStatus::Interrupted
            || cp.status == PipelineStatus::BudgetExhausted
        {
            // Already interrupted or budget-exhausted from a previous restart — still resumable
            resumable.push(cp.sentinel_handle);
        }
    }

    Ok(resumable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::*;
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// Per-process tempdir installed into `CONTINUUM_CHECKPOINT_DIR` the
    /// first time any test in this module runs. Kept alive (and unique
    /// per `cargo test` invocation) by the `OnceLock` — `TempDir`'s
    /// Drop deletes it when the process exits. Without this isolation
    /// the tests below polluted `~/.continuum/sentinel/checkpoints/`
    /// on the developer's real home and flaked on machines where that
    /// directory was root-owned from prior docker-compose bind-mounts.
    fn ensure_checkpoint_dir_isolated() {
        static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
        let dir = DIR.get_or_init(|| tempfile::tempdir().expect("create checkpoint test tempdir"));
        // Set every call: cargo runs tests in parallel and any other
        // test that clears CONTINUUM_CHECKPOINT_DIR could race us;
        // re-setting per test keeps the contract local.
        std::env::set_var("CONTINUUM_CHECKPOINT_DIR", dir.path());
    }

    fn make_test_checkpoint(handle: &str) -> PipelineCheckpoint {
        PipelineCheckpoint {
            sentinel_handle: handle.to_string(),
            pipeline_name: Some("test-pipeline".to_string()),
            step_index: 2,
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "shell".to_string(),
                success: true,
                duration_ms: 100,
                output: Some("ok".to_string()),
                error: None,
                exit_code: Some(0),
                data: serde_json::Value::Null,
            }],
            budget_consumed: BudgetConsumed {
                elapsed_secs: 30,
                cost_usd: 0.05,
                tokens_used: 5000,
                iterations: 1,
            },
            budget_limits: BudgetLimits {
                max_time_secs: Some(3600),
                max_cost_usd: Some(5.0),
                max_tokens: Some(1_000_000),
                max_iterations: Some(50),
            },
            started_at: "2026-03-15T00:00:00Z".to_string(),
            last_checkpoint_at: "2026-03-15T00:00:30Z".to_string(),
            status: PipelineStatus::Running,
            pipeline: Pipeline {
                name: Some("test".to_string()),
                steps: vec![PipelineStep::Shell {
                    cmd: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    timeout_secs: None,
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                }],
                working_dir: Some("/tmp".to_string()),
                timeout_secs: None,
                inputs: HashMap::new(),
            },
            working_dir: "/tmp".to_string(),
            escalation: None,
        }
    }

    #[test]
    fn test_save_load_checkpoint() {
        ensure_checkpoint_dir_isolated();
        let handle = format!(
            "test-ckpt-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        );
        let cp = make_test_checkpoint(&handle);

        save_checkpoint(&handle, &cp).unwrap();
        let loaded = load_checkpoint(&handle).unwrap().unwrap();
        assert_eq!(loaded.sentinel_handle, handle);
        assert_eq!(loaded.step_index, 2);
        assert_eq!(loaded.budget_consumed.elapsed_secs, 30);

        // Cleanup
        delete_checkpoint(&handle).unwrap();
        assert!(load_checkpoint(&handle).unwrap().is_none());
    }

    #[test]
    fn test_list_checkpoints() {
        ensure_checkpoint_dir_isolated();
        let handle = format!(
            "test-list-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        );
        let cp = make_test_checkpoint(&handle);
        save_checkpoint(&handle, &cp).unwrap();

        let all = list_checkpoints().unwrap();
        assert!(all.iter().any(|c| c.sentinel_handle == handle));

        delete_checkpoint(&handle).unwrap();
    }

    #[test]
    fn test_recover_interrupted() {
        ensure_checkpoint_dir_isolated();
        let handle = format!(
            "test-recover-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        );
        let cp = make_test_checkpoint(&handle);
        save_checkpoint(&handle, &cp).unwrap();

        let interrupted = recover_interrupted().unwrap();
        assert!(interrupted.contains(&handle));

        // Verify it was changed to Interrupted
        let loaded = load_checkpoint(&handle).unwrap().unwrap();
        assert_eq!(loaded.status, PipelineStatus::Interrupted);

        delete_checkpoint(&handle).unwrap();
    }
}
