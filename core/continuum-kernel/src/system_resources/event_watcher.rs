//! Filesystem event watcher for disk accounting.
//!
//! Observes filesystem events via the `notify` crate and feeds budgeted size deltas into each
//! [`TrackedDir`](super::disk_reporters::TrackedDir), so reporters see a live **estimate**
//! (last_walk baseline + accumulated delta) rather than a frozen number.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::{Event as NotifyEvent, EventKind, RecommendedWatcher};
use parking_lot::RwLock;

use super::disk_reporters::TrackedDir;

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

    /// Spawns the notify watcher and returns it (caller must keep alive).
    pub fn spawn(self) {
        let dirs = self.dirs.clone();
        let prev_sizes = self.prev_sizes.clone();

        let mut watcher = match notify::recommended_watcher(move |event: notify::Result<NotifyEvent>| {
            if let Ok(ev) = event {
                process_event(&dirs, &prev_sizes, &ev);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("EventWatcher: failed to create watcher: {}", e);
                return;
            }
        };

        for dir in &self.dirs {
            if let Err(e) = watcher.watch(dir.path(), notify::RecursiveMode::Recursive) {
                eprintln!(
                    "EventWatcher: failed to watch {:?}: {}",
                    dir.path(), e
                );
            }
        }

        // Keep the watcher alive — it runs on its own internal receiver thread.
        std::mem::forget(watcher);
    }
}

/// Check if an event is relevant for size tracking (create, modify, remove).
fn is_relevant(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Process a single filesystem event, updating TrackedDir deltas.
fn process_event(
    dirs: &[Arc<TrackedDir>],
    prev_sizes: &Arc<RwLock<HashMap<PathBuf, u64>>>,
    event: &NotifyEvent,
) {
    if !is_relevant(&event.kind) {
        return;
    }

    let path = match event.paths.first() {
        Some(p) => p.clone(),
        None => return,
    };

    // Skip noise directories
    if path.file_name().map_or(false, |n| n == ".git" || n == "target") {
        return;
    }

    let meta = std::fs::metadata(&path);

    match (meta, &event.kind) {
        (Ok(m), EventKind::Remove(_)) => {
            // File was deleted — record negative delta.
            let mut sizes = prev_sizes.write();
            if let Some(prev_size) = sizes.remove(&path) {
                for dir in dirs {
                    if path.starts_with(dir.path()) {
                        dir.observe_event(-(prev_size as i64));
                    }
                }
            }
        }
        (Ok(m), _) => {
            // File created or modified — record size change.
            let mut sizes = prev_sizes.write();
            let new_size = m.len();
            if let Some(prev_size) = sizes.insert(path.clone(), new_size) {
                for dir in dirs {
                    if path.starts_with(dir.path()) {
                        let delta = new_size as i64 - prev_size as i64;
                        dir.observe_event(delta);
                    }
                }
            } else {
                // First time seeing this file — no previous size, so delta is 0
                sizes.insert(path.clone(), new_size);
            }
        }
        (Err(_), EventKind::Remove(_)) => {
            // File already gone when we tried to stat it — treat as full deletion.
            let mut sizes = prev_sizes.write();
            if let Some(prev_size) = sizes.remove(&path) {
                for dir in dirs {
                    if path.starts_with(dir.path()) {
                        dir.observe_event(-(prev_size as i64));
                    }
                }
            }
        }
        _ => {} // Other errors or non-relevant events — ignore.
    }
}
