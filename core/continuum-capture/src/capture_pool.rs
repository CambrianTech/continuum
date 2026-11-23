//! The captures' retention (card 699578cb): the one owner that frees capture bytes when the volume
//! holding them is under pressure. It has no budget of its own (disk relief asks it by its volume's
//! pressure, `paging::pool::ResourcePool::root`): asked for bytes, it removes closed generations
//! oldest first across every owner, and never a generation an open log is appending to. A generation's
//! index goes before its frames, so a removal cut short leaves frames no reader lists, never an index
//! pointing at nothing.

use std::path::{Path, PathBuf};

use continuum_kernel::paging::pool::{ResourcePool, ResourcePoolEntry};
use continuum_kernel::system_resources::disk_reporters::ClassWriter;

use crate::capture_log;

/// The captures held in one disk class, freed oldest first under pressure.
pub struct CapturesPool {
    class: ClassWriter,
}

/// One closed generation that may be removed: its files, their bytes and when it was last written.
struct Closed {
    index: PathBuf,
    frames: PathBuf,
    index_bytes: u64,
    frames_bytes: u64,
    modified: std::time::SystemTime,
}

/// A file's bytes; `None` for one already gone.
fn size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

impl CapturesPool {
    pub fn new(class: ClassWriter) -> Self {
        Self { class }
    }

    /// Every generation of every owner, live ones included, as (index, frames) paths.
    fn generations(&self) -> Vec<(PathBuf, PathBuf)> {
        let owners = self.class.owners().unwrap_or_default(); // unwrap_or_default: an unreadable class root holds nothing this pool can free
        owners
            .into_iter()
            .flat_map(|owner| {
                let scoped = self.class.owned_by(owner);
                let gens = capture_log::generations(&scoped).unwrap_or_default(); // unwrap_or_default: an unreadable owner holds nothing this pool can free
                gens.into_iter()
                    .filter_map(move |generation| {
                        let index = scoped.path(Path::new(&format!("{generation:016}.index"))).ok()?;
                        let frames = scoped.path(Path::new(&format!("{generation:016}.frames"))).ok()?;
                        Some((index, frames))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The closed generations, oldest written first.
    fn closed(&self) -> Vec<Closed> {
        let mut closed: Vec<Closed> = self
            .generations()
            .into_iter()
            .filter(|(_, frames)| !capture_log::is_live(frames))
            .filter_map(|(index, frames)| {
                let modified = std::fs::metadata(&frames).and_then(|m| m.modified()).ok()?;
                Some(Closed { index_bytes: size(&index)?, frames_bytes: size(&frames)?, index, frames, modified })
            })
            .collect();
        closed.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| a.frames.cmp(&b.frames))); // equal times: the lower generation first
        closed
    }
}

impl ResourcePool for CapturesPool {
    fn root(&self) -> Option<PathBuf> {
        self.class.path(Path::new(".")).ok()
    }

    fn tier_name(&self) -> &str {
        "captures"
    }

    /// No budget of its own: relief asks it by its volume's pressure.
    fn capacity_bytes(&self) -> u64 {
        0
    }

    fn usage_bytes(&self) -> u64 {
        self.generations().iter().flat_map(|(index, frames)| [size(index), size(frames)]).flatten().sum()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let mut freed = 0u64;
        for gen in self.closed() {
            if freed >= want_bytes {
                break;
            }
            // the index first: a removal cut short leaves frames no reader lists
            if std::fs::remove_file(&gen.index).is_err() {
                continue;
            }
            freed = freed.saturating_add(gen.index_bytes);
            match std::fs::remove_file(&gen.frames) {
                Ok(()) => freed = freed.saturating_add(gen.frames_bytes),
                // its index is gone, so no reader lists these frames, but their bytes are still held
                Err(error) => continuum_base::probe!(class = "capture.generation.frames_kept", frames = %gen.frames.display(), error = %error,
                    "a capture generation's index was removed but its frames file was not: unlisted bytes remain"),
            }
            continuum_base::probe!(class = "capture.generation.evicted", frames = %gen.frames.display(), bytes = gen.index_bytes + gen.frames_bytes,
                "a closed capture generation was removed under disk pressure, oldest first");
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture_log::CaptureLog;
    use crate::frame::FrameKind;
    use continuum_kernel::system_resources::disk_reporters::TrackedDir;

    // what this catches: retention that deletes the generation a live log is still appending to (its
    // frames would vanish beneath the sink), that frees the newest history before the oldest, or that
    // reports bytes it did not free.
    #[test]
    fn eviction_frees_closed_generations_oldest_first_and_never_a_live_one() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = TrackedDir::new("captures-pool-test", dir.path().to_path_buf()).writer();
        let owner = uuid::Uuid::new_v4();
        for _ in 0..2 {
            let mut closed = CaptureLog::open(class.owned_by(owner)).expect("test: open");
            closed.append(FrameKind::Submitted, &("r",), &[&[7u8; 1024]], 1).expect("test: append");
        } // generations 0 and 1, both closed
        let mut live = CaptureLog::open(class.owned_by(owner)).expect("test: live");
        live.append(FrameKind::Submitted, &("r",), &[&[7u8; 1024]], 2).expect("test: append");

        let pool = CapturesPool::new(TrackedDir::new("captures-pool-test", dir.path().to_path_buf()).writer());
        let before = pool.usage_bytes();
        let freed = pool.evict_at_least(1);
        assert!(freed > 0, "asked for bytes, it frees a closed generation");
        let left = capture_log::generations(&class.owned_by(owner)).expect("test: generations");
        assert_eq!(left, vec![1, 2], "the oldest closed generation went first");
        assert_eq!(pool.usage_bytes(), before - freed, "it frees exactly what it reports");

        pool.evict_at_least(u64::MAX);
        let left = capture_log::generations(&class.owned_by(owner)).expect("test: generations");
        assert_eq!(left, vec![2], "however much is asked, the live generation stays");
        drop(live);
    }
}
