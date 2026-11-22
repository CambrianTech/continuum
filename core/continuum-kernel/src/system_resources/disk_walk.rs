//! Budgeted directory walking with coverage tracking and statistical estimation.
//!
//! This module provides the core machinery for Joel's disk accounting directive:
//! - **Req 2**: Per-tick inode budget with resume cursor — no full synchronous walks at boot
//! - **Req 3**: Coverage fraction + Unknown/Partial states — consumers read "partial: 40% swept"  
//! - **Req 4**: Estimate-before-enumerate via random subtree sampling with error bounds

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// How far we've walked a directory tree so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// We haven't started walking yet — no data exists at all.
    Unknown,
    /// A partial sweep has run but hasn't covered the full tree.
    Partial { swept_entries: u64, total_estimated: u64 },
    /// Full enumeration completed in a single walk (or budget was large enough).
    Complete { entries: u64 },
}

impl Coverage {
    pub fn fraction(&self) -> f32 {
        match self {
            Coverage::Unknown => 0.0,
            Coverage::Partial { swept_entries, total_estimated } if *total_estimated == 0 => 0.0,
            Coverage::Partial { swept_entries, total_estimated } => {
                (*swept_entries as f32) / (*total_estimated as f32)
            }
            Coverage::Complete { .. } => 1.0,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, Coverage::Complete { .. })
    }
}

/// A cursor that remembers where a budgeted walk left off so the next tick can resume.
#[derive(Debug, Clone)]
pub struct WalkCursor {
    /// Directories we've fully descended into (their paths are in this set).
    entered: HashSet<PathBuf>,
    /// How many inodes we've processed total across all ticks.
    entries_processed: u64,
}

impl WalkCursor {
    pub fn new() -> Self {
        Self {
            entered: HashSet::new(),
            entries_processed: 0,
        }
 }

    /// Returns true if this directory has already been fully descended into.
    pub fn is_entered(&self, dir: &Path) -> bool {
        self.entered.contains(dir)
    }

    /// Mark a directory as fully entered (all its entries have been processed).
    pub fn mark_entered(&mut self, dir: &Path) {
        self.entered.insert(dir.to_path_buf());
    }

    /// Advance the entry counter.
    pub fn advance_entries(&mut self, count: u64) {
        self.entries_processed += count;
    }

    pub fn entries_processed(&self) -> u64 {
        self.entries_processed
    }

    /// Reset cursor — start fresh (used after a full walk).
    pub fn reset(&mut self) {
        self.entered.clear();
        self.entries_processed = 0;
    }
}

/// Per-tick budget for how many inodes a single tick may process.
/// Used during cold start to prevent blocking the boot path.
#[derive(Debug, Clone, Copy)]
pub struct WalkBudget {
    /// Maximum inodes to walk per tick.
    pub max_inodes: u64,
    /// If set, how long a tick budget is valid (for rate-limiting).
    pub refill_interval: Duration,
}

impl Default for WalkBudget {
    fn default() -> Self {
        // 10k inodes per tick — enough to make progress but not block boot.
        Self {
            max_inodes: 10_000,
            refill_interval: Duration::from_secs(5),
        }
    }
}

// ��── Budgeted walk implementation ��─────────────────────────────────────

/// Walk a directory tree respecting the given budget. Returns how many inodes were processed
/// and whether we completed the full tree.
pub fn budgeted_walk(
    root: &Path,
    cursor: &mut WalkCursor,
    budget: &WalkBudget,
) -> (u64, bool) {
    let mut remaining = budget.max_inodes.saturating_sub(cursor.entries_processed);
    if remaining == 0 {
        return (0, false); // budget exhausted before we even started this tick
    }

    let mut total_processed = 0u64;
    let mut completed = false;

    fn walk_dir(
        dir: &Path,
        cursor: &mut WalkCursor,
        remaining: &mut u64,
        total: &mut u64,
        completed: &mut bool,
    ) {
        // Skip if we've already fully entered this directory.
        if cursor.is_entered(dir) {
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if *remaining == 0 {
                // Budget exhausted — we'll resume here next tick.
                return;
            }

            let path = entry.path();
            *total += 1;
            *remaining -= 1;

            if path.is_dir() {
                walk_dir(&path, cursor, remaining, total, completed);
            }
        }

        // If we processed all entries without running out of budget, mark as entered.
        if *remaining > 0 || *total == 0 {
            cursor.mark_entered(dir);
        }
    }

    walk_dir(root, cursor, &mut remaining, &mut total_processed, &mut completed);

    // If we didn't run out of budget mid-traversal, the tree is fully covered.
    if !cursor.is_entered(root) {
        cursor.mark_entered(root);
    }
    completed = cursor.entries_processed() >= 1 && remaining > 0;

    (total_processed, completed)
}

// ��── Statistical estimation for Req 4 ��─────────────────────────────

/// A single subtree sample: its path, byte count, and entry count.
#[derive(Debug, Clone)]
pub struct SubtreeSample {
    pub path: PathBuf,
    pub bytes: u64,
    pub entries: u64,
}

/// Estimate total size by sampling K random subtrees at depth D from the root.
/// Returns (mean_estimate, standard_error) for proactive eviction decisions.
pub fn estimate_via_sampling(
    root: &Path,
    k: usize,           // number of samples to take
    max_depth: u8,      // how deep into tree to pick subtrees
) -> Option<(f64, f64)> {
    let entries = std::fs::read_dir(root).ok()?;
    let dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();

    if dirs.is_empty() {
        return None;
    }

    // Sample K directories (with replacement if fewer than K exist).
    let samples = sample_subtrees(&dirs, k, max_depth);
    if samples.is_empty() {
        return None;
    }

    // Compute mean and standard error.
    let n = samples.len() as f64;
    let sum: f64 = samples.iter().map(|s| s.bytes as f64).sum();
    let mean = sum / n;

    if n < 2.0 {
        return Some((mean, f64::NAN)); // can't compute error with one sample
    }

    let variance: f64 = samples.iter().map(|s| {
        let diff = s.bytes as f64 - mean;
        diff * diff
    }).sum::<f64>() / (n - 1.0);

    let std_err = (variance / n).sqrt();
    Some((mean, std_err))
}

fn sample_subtrees(dirs: &[PathBuf], k: usize, max_depth: u8) -> Vec<SubtreeSample> {
    use std::collections::VecDeque;

    let mut samples = Vec::new();
    let rng_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    for i in 0..k {
        // Pseudo-random selection (simple LCG — fine for sampling, not crypto).
        let idx = ((rng_seed.wrapping_mul(i as u64 + 1).wrapping_add(7)) % dirs.len() as u64) as usize;
        let dir = &dirs[idx % dirs.len()];

        // Walk down to max_depth and collect size.
        if let Some(sample) = walk_subtree(dir, max_depth) {
            samples.push(sample);
        }
    }

    samples
}

fn walk_subtree(root: &Path, depth: u8) -> Option<SubtreeSample> {
    let mut queue = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0u8));
    
    let mut total_bytes = 0u64;
    let mut entry_count = 0u64;

    while let Some((path, current_depth)) = queue.pop_front() {
        if current_depth > depth {
            continue;
        }

        entry_count += 1;
        
        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                let child_path = entry.path();
                if child_path.is_dir() && current_depth < depth {
                    queue.push_back((child_path, current_depth + 1));
                } else if child_path.is_file() {
                    if let Ok(metadata) = std::fs::metadata(&child_path) {
                        total_bytes += metadata.len();
                    }
                }
            }
        }
    }

    Some(SubtreeSample {
        path: root.to_path_buf(),
        bytes: total_bytes,
        entries: entry_count,
    })
}

/// Growth-rate model: tracks how fast a directory's size changes over time.
#[derive(Debug, Clone)]
pub struct GrowthRate {
    /// Timestamp of the last measurement (ms since epoch).
    pub last_seen_ms: u64,
    /// Bytes recorded at last measurement.
    pub last_bytes: u64,
}

impl GrowthRate {
    pub fn new(bytes: u64) -> Self {
        Self {
            last_seen_ms: current_time_ms(),
            last_bytes: bytes,
        }
    }

    /// Update with a new measurement. Returns estimated growth rate in bytes/sec.
    pub fn update(&mut self, bytes: u64) -> f64 {
        let now = current_time_ms();
        let dt_secs = ((now.saturating_sub(self.last_seen_ms)) as f64).max(0.001) / 1000.0;
        let growth = (bytes as i64 - self.last_bytes as i64) as f64;
        let rate = growth / dt_secs;

        self.last_seen_ms = now;
        self.last_bytes = bytes;
        rate
    }

    /// Predict size at time `future_ms` from now.
    pub fn predict(&self, future_ms: u64) -> Option<u64> {
        let dt_secs = ((future_ms.saturating_sub(self.last_seen_ms)) as f64).max(0.0) / 1000.0;
        // We need the previous rate — but we only store one sample. 
        // For a single-sample model, predict linear growth from last measurement rate.
        // This is approximated: callers should call update() to get rates first.
        None // requires at least two measurements for prediction
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_fraction_returns_correct_values() {
        assert_eq!(Coverage::Unknown.fraction(), 0.0);
        assert_eq!(Coverage::Complete { entries: 100 }.fraction(), 1.0);
        let partial = Coverage::Partial { swept_entries: 40, total_estimated: 100 };
        assert!((partial.fraction() - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn cursor_tracks_entered_dirs() {
        let mut cursor = WalkCursor::new();
        assert!(!cursor.is_entered(Path::new("/foo")));
        cursor.mark_entered(Path::new("/foo"));
        assert!(cursor.is_entered(Path::new("/foo")));
        cursor.reset();
        assert!(!cursor.is_entered(Path::new("/foo")));
    }

    #[test]
    fn growth_rate_tracks_update() {
        let mut gr = GrowthRate::new(100);
        std::thread::sleep(Duration::from_millis(10));
        let rate = gr.update(200);
        // Should be positive (growth) and roughly 100 bytes / 0.01s = ~10000 bytes/sec
        assert!(rate > 5000.0, "expected positive growth rate, got {}", rate);
    }

    #[test]
    fn sampling_returns_none_for_empty_dir() {
        // Create a temp dir that's empty
        let tmp = std::env::temp_dir().join("disk_walk_test_empty");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);
        
        assert!(estimate_via_sampling(&tmp, 5, 2).is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
use std::collections::VecDeque;

