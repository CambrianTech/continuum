//! Budgeted directory walking with coverage tracking.
//!
//! This module provides the core machinery for Joel's disk accounting directive:
//! - **Req 2**: Per-tick inode budget with resume cursor — no full synchronous walks at boot
//! - **Req 3**: Coverage fraction + Unknown/Partial states — consumers read "partial: 40% swept"

use std::collections::HashSet;
use std::path::{Path, PathBuf};
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
    /// How many unique directories we've encountered across all ticks.
    dirs_seen: u64,
    /// How many inodes we've processed total across all ticks.
    entries_processed: u64,
    /// Shallow probe of root's direct children on first tick — used as base for tree-size estimate.
    pub(crate) initial_root_count: u64,
}

impl WalkCursor {
    pub fn new() -> Self {
        Self {
            entered: HashSet::new(),
            dirs_seen: 0,
            initial_root_count: 0,
            entries_processed: 0,
        }
 }

    /// Returns true if this directory has already been fully descended into.
    pub fn is_entered(&self, dir: &Path) -> bool {
        self.entered.contains(dir)
    }

    /// Mark a directory as fully entered (all its entries have been processed).
    pub fn mark_entered(&mut self, dir: &Path) {
        let was_new = !self.entered.contains(dir);
        self.entered.insert(dir.to_path_buf());
        if was_new {
            self.dirs_seen += 1;
        }
    }

    /// Set the initial shallow-probe count (root direct children). Called once on first tick.
    pub(crate) fn set_initial_root_count(&mut self, count: u64) {
        if self.initial_root_count == 0 && count > 0 {
            self.initial_root_count = count;
        }
    }

    /// Estimate total tree size for partial coverage reporting.
    /// Uses a shallow root probe on first tick as the base, then extrapolates
    /// as more directories are discovered during subsequent ticks.
    pub fn estimate_total_size(&self) -> Option<u64> {
        if self.initial_root_count == 0 && self.dirs_seen <= 1 {
            return None; // haven't gathered enough info yet
        }

        let base = self.initial_root_count.max(1);
        if self.dirs_seen > 2 {
            // We've entered multiple directories — tree extends beyond root level.
            // Extrapolate: each doubling of dirs_seen adds roughly one more level.
            let extra_levels = (self.dirs_seen as f64).log2().max(0.5);
            Some((base as f64 * (1.0 + extra_levels)) as u64)
        } else {
            // First tick or just entering root — use shallow probe as lower bound.
            Some(base)
        }
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
        self.dirs_seen = 0;
        self.initial_root_count = 0;
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
}

