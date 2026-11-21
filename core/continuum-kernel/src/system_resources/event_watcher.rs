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
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event as NotifyEvent, EventType, RecursiveMode, RecommendedWatcher};
use parking_lot::RwLock;
use tokio::sync::mpsc;

use super::disk_reporters::TrackedDir;

/// Maximum time spent on stat calls per processing tick (100 ms).
const EVENT_BUDGET_MS: u64 = 100;

/// Watches filesystem events on tracked directories and feeds size deltas into TrackedDirs.
pub struct EventWatcher {
    dirs: Vec<Arc<TrackedDir>>,
    /// Last known sizes for each path we've seen (for delta calculation).
    prev_sizes: Arc<RwLock<HashMap<std::path::PathBuf, u64>>>,
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
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<NotifyEvent, notify::Error>>(1024);

        let mut watcher = RecommendedWatcher::new_with_receiver(
            move |res: Result<NotifyEvent, _>| {
                if let Err(e) = res {
                    eprintln!("EventWatcher: notify error: {}", e);
                }
            },
            rx,
            notify::Config::default(),
        )
        .expect("Failed to create EventWatcher");

        // Watch all paths
        for dir in &self.dirs {
            if let Err(e) = watcher.watch(dir.path(), RecursiveMode::Recursive) {
                eprintln!("EventWatcher: failed to watch {:?}: {}", dir.path(), e);
            }
        }

        tokio::spawn(async move { Self::run(self, watcher).await });
    }

    async fn run(mut self, mut watcher: RecommendedWatcher) {
        let mut rx = mpsc::channel::<Result<NotifyEvent, notify::Error>>(1024);
        // Reconnect the receiver to our channel for receiving
        drop(rx); // We don't need this one; the watcher's internal receiver is used internally

        loop {
            match tokio::time::timeout(Duration::from_secs(1), async {
                // This won't work directly — we need to access the internal receiver.
                // Let me restructure.
                unimplemented!()
            })
            .await
            {
                _ => {}
            }
        }
    }
}
