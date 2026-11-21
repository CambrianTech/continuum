//! Filesystem event watcher for disk accounting.
//!
//! Observes filesystem events via the `notify` crate and feeds budgeted size deltas into each
//! [`TrackedDir`](super::disk_reporters::TrackedDir), so reporters see a live **estimate**
//! (last_walk baseline + accumulated delta) rather than a frozen number.
//!
//! Budget: each processing tick may spend at most [`EVENT_BUDGET_MS`] ms doing `stat` calls.
//! Events received when the budget is exhausted are still counted in `event_count`, so reporters
//! can report "N changes since last scan" alongside the estimate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use notify::{Event as NotifyEvent, RecursiveMode, RecommendedWatcher, Watcher};
use parking_lot::RwLock;

use super::disk_reporters::TrackedDir;

/// Maximum time spent on stat calls per processing tick (100 ms).
const EVENT_BUDGET_MS: u64 = 100;

/// Tracks when the budget was last refreshed (in ms since UNIX_EPOCH).
static LAST_BUDGET_REFRESH: AtomicU64 = AtomicU64::new(0);

/// Watches filesystem events on tracked directories and feeds size deltas into TrackedDirs.
pub struct EventWatcher {
    dirs: Vec<Arc<TrackedDir>>,
    /// Last known sizes for each path we've seen (for delta calculation).
    prev_sizes: Arc<RwLock<HashMap<PathBuf, u64>>>,
}

impl EventWatcher {
    pub fn new(dirs: Vec<Arc<TrackedDir>>) -> Self {
        Self {
            dirs,
            prev_sizes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Spawns the watcher as an async task.
    pub fn spawn(self) {
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<NotifyEvent, _>| {
                if let Err(e) = res {
                    eprintln!("EventWatcher: notify error: {}", e);
                }
            },
            notify::Config::default(),
        )
        .expect("Failed to create EventWatcher");

        // Watch all paths
        for dir in &self.dirs {
            if let Err(e) = watcher.watch(dir.path(), RecursiveMode::Recursive) {
                eprintln!("EventWatcher: failed to watch {:?}: {}", dir.path(), e);
            }
        }

        tokio::spawn(async move { self.run(watcher).await });
    }

    async fn run(mut self, mut watcher: RecommendedWatcher) {
        let mut rx = watcher.receiver();

        loop {
            match rx.recv() {
                Ok(event) => self.handle_event(&event).await,
                Err(_) => {
                    // Watcher was stopped or error occurred; restart if dirs still exist.
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    for dir in &self.dirs {
                        let _ = watcher.watch(dir.path(), RecursiveMode::Recursive);
                    }
                }
            }
        }
    }

    async fn handle_event(&self, event: &NotifyEvent) {
        // Filter to relevant events (write, create, remove).
        if !event.kind.is_write() && !event.kind.is_remove() && !event.kind.is_create() {
            return;
        }

        let mut matched_dirs = Vec::new();
        for dir in &self.dirs {
            if event.paths.iter().any(|p| p.starts_with(dir.path())) {
                matched_dirs.push(dir);
            }
        }

        // Always increment event counts (even when budget exhausted).
        for dir in &matched_dirs {
            dir.event_count.fetch_add(1, Ordering::Relaxed);
        }

        // Budget check: use a tick counter based on elapsed time since UNIX_EPOCH.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if LAST_BUDGET_REFRESH.load(Ordering::Relaxed) == 0
            || (now_ms - LAST_BUDGET_REFRESH.load(Ordering::Relaxed)) >= EVENT_BUDGET_MS
        {
            // Budget refreshed or first tick.
            LAST_BUDGET_REFRESH.store(now_ms, Ordering::Relaxed);
        } else {
            // Still in same tick — skip stats to stay within budget.
            return;
        }

        // Perform budgeted stat calls for each changed path.
        for path in &event.paths {
            if !path.exists() {
                // File was deleted or moved away.
                let mut sizes = self.prev_sizes.write();
                if let Some(prev_size) = sizes.remove(path) {
                    for dir in &matched_dirs {
                        dir.observe_event(-(prev_size as i64));
                    }
                }
                continue;
            }

            match tokio::fs::metadata(path).await {
                Ok(metadata) => {
                    let size = metadata.len() as i64;
                    let mut sizes = self.prev_sizes.write();
                    let prev_size = sizes.insert(path.clone(), size as u64);
                    if let Some(prev) = prev_size {
                        let delta = size - (prev as i64);
                        for dir in &matched_dirs {
                            dir.observe_event(delta);
                        }
                    } else {
                        // First time seeing this file — treat as new growth.
                        for dir in &matched_dirs {
                            dir.observe_event(size);
                        }
                    }
                }
                Err(_) => {
                    // stat failed (permissions, etc.) — skip silently.
                }
            }
        }
    }
}
