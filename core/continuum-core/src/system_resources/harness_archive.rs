//! The retired benchmark harness's on-disk state, moved ONCE into one archive directory.
//!
//! Card c4f4b8cd step 2 deleted the detached runner (`agent/solve`), the round tracker's
//! persistence and the verbs that read their files. Joel, 2026-10-19: archive the old state,
//! never delete it. On the first boot of a core without the harness, every path below that
//! still exists is renamed under [`archive_root`], keeping its path relative to the
//! continuum home, and a receipt naming each moved path is written beside it. The receipt
//! is the once-gate: a later boot finds it and moves nothing, so live files written after
//! the archive (the verdict store the grader still keeps) are never swept up.
//!
//! Runs off the main thread (`spawn_blocking`), at boot, once; outcomes on the
//! `benchmark.harness.archive` probe.

use std::path::{Path, PathBuf};

/// The archive directory, under the continuum home. Registered as the
/// `benchmark-harness-archive` cache class (`disk_reporters::standard_tracked_dirs`).
pub fn archive_root(continuum_home: &Path) -> PathBuf {
    continuum_home.join("archive").join("benchmark-harness")
}

/// The receipt the once-gate reads, inside [`archive_root`].
const RECEIPT: &str = "archived.json";

/// Whole directories the harness wrote, relative to the continuum home: the round
/// tracker's files, the per-instance verdicts and env refusals, `benchmark/validate`'s
/// coverage maps, the detached solve's attempt patches.
const HARNESS_DIRS: &[&str] = &[
    "state/bench-rounds",
    "benchmarks/swe/verdicts",
    "benchmarks/swe/coverage",
    "benchmarks/swe/captures",
];

/// Single files the harness wrote: `benchmark/record`'s results ledger.
const HARNESS_FILES: &[&str] = &["benchmarks/ledger.jsonl"];

/// The progress dir, where the detached solve journaled each run beside other ledgers.
const PROGRESS_DIR: &str = "progress";

/// A progress-dir file the detached solve wrote: its run ledger
/// (`agent-solve-<run>.json`) or any run's grade (`<run>.grade.json`).
fn is_harness_progress_file(name: &str) -> bool {
    name.starts_with("agent-solve-") || name.ends_with(".grade.json")
}

/// What one archive pass did.
#[derive(Debug, PartialEq, Eq)]
pub enum ArchiveOutcome {
    /// The receipt already stands: archived on an earlier boot.
    AlreadyArchived,
    /// Moved these paths (relative to the continuum home); the receipt records them.
    Moved(Vec<String>),
}

/// Archive the harness state under `continuum_home`, once. A failed rename stops the pass
/// before the receipt is written, so the next boot retries what is left.
pub fn archive_once_in(continuum_home: &Path, archived_at_ms: u64) -> Result<ArchiveOutcome, String> {
    let root = archive_root(continuum_home);
    if root.join(RECEIPT).exists() {
        return Ok(ArchiveOutcome::AlreadyArchived);
    }
    let mut relative: Vec<String> = HARNESS_DIRS
        .iter()
        .chain(HARNESS_FILES)
        .filter(|rel| continuum_home.join(rel).exists())
        .map(|rel| rel.to_string())
        .collect();
    if let Ok(entries) = std::fs::read_dir(continuum_home.join(PROGRESS_DIR)) {
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| is_harness_progress_file(name))
            .map(|name| format!("{PROGRESS_DIR}/{name}"))
            .collect();
        names.sort();
        relative.extend(names);
    }
    for rel in &relative {
        let to = root.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::rename(continuum_home.join(rel), &to).map_err(|e| format!("move {rel}: {e}"))?;
    }
    std::fs::create_dir_all(&root).map_err(|e| format!("create {}: {e}", root.display()))?;
    let receipt = serde_json::json!({ // file format on disk: the archive's own receipt, read by the once-gate's existence check and by hand
        "archivedAtMs": archived_at_ms,
        "card": "c4f4b8cd",
        "moved": relative,
    });
    std::fs::write(root.join(RECEIPT), receipt.to_string())
        .map_err(|e| format!("write receipt: {e}"))?;
    Ok(ArchiveOutcome::Moved(relative))
}

/// Boot entry: archive the node's harness state once, off the main thread.
pub fn spawn_archive(rt: &tokio::runtime::Handle) {
    rt.spawn_blocking(|| {
        let outcome = crate::paths::continuum_home()
            .and_then(|home| archive_once_in(&home, continuum_base::time::EpochMs::now().millis()));
        match outcome {
            Ok(ArchiveOutcome::AlreadyArchived) => {}
            Ok(ArchiveOutcome::Moved(moved)) => continuum_base::probe!(
                class = "benchmark.harness.archive",
                moved = moved.len() as u64,
                paths = %moved.join(","),
                "the retired benchmark harness's state was archived once, nothing deleted"
            ),
            Err(error) => continuum_base::probe!(
                class = "benchmark.harness.archive_failed",
                error = %error,
                "archiving the retired harness's state stopped; the next boot retries what is left"
            ),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the archive deleting or skipping harness state, sweeping a live
    // progress ledger that is not the harness's, or running twice and taking files written
    // after it (the verdict store stays live).
    #[test]
    fn harness_state_moves_once_and_nothing_else_moves() {
        let home = tempfile::tempdir().expect("tmp");
        let h = home.path();
        for dir in ["state/bench-rounds", "benchmarks/swe/verdicts", "progress", "benchmarks/swe/datasets"] {
            std::fs::create_dir_all(h.join(dir)).expect("mkdir");
        }
        std::fs::write(h.join("state/bench-rounds/r1.json"), "{}").expect("round");
        std::fs::write(h.join("benchmarks/swe/verdicts/sympy-1.json"), "{}").expect("verdict");
        std::fs::write(h.join("benchmarks/ledger.jsonl"), "{}\n").expect("ledger");
        std::fs::write(h.join("progress/agent-solve-a.json"), "{}").expect("solve ledger");
        std::fs::write(h.join("progress/agent-solve-a.grade.json"), "{}").expect("grade");
        std::fs::write(h.join("progress/eval-run-b.json"), "{}").expect("eval ledger");

        let ArchiveOutcome::Moved(moved) = archive_once_in(h, 1).expect("archive") else {
            panic!("first pass must move");
        };
        assert_eq!(moved.len(), 5, "{moved:?}");
        let root = archive_root(h);
        assert!(root.join("state/bench-rounds/r1.json").exists());
        assert!(root.join("benchmarks/swe/verdicts/sympy-1.json").exists());
        assert!(root.join("progress/agent-solve-a.grade.json").exists());
        assert!(!h.join("state/bench-rounds").exists());
        assert!(h.join("progress/eval-run-b.json").exists(), "another ledger stays");
        assert!(h.join("benchmarks/swe/datasets").exists(), "the rest of the benchmarks cache stays");

        std::fs::create_dir_all(h.join("benchmarks/swe/verdicts")).expect("live store");
        std::fs::write(h.join("benchmarks/swe/verdicts/new.json"), "{}").expect("new verdict");
        assert_eq!(archive_once_in(h, 2).expect("second"), ArchiveOutcome::AlreadyArchived);
        assert!(h.join("benchmarks/swe/verdicts/new.json").exists(), "a later verdict is live, never archived");
    }
}
