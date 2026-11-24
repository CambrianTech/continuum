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

#[cfg(test)]
thread_local! {
    /// Have another pool's roll land between this thread's next eviction's first look and its own roll.
    static ANOTHER_ROLL_FIRST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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
    /// Its owner's newest generation: emptied, never removed, so the owner's next opens at a higher
    /// number. A number reused after eviction would let a stored cursor, or an index that already read
    /// that generation's first entries, point into different records.
    newest: bool,
    /// Its owner's class writer and its number: what empties the newest in place, through the class.
    scoped: ClassWriter,
    generation: u64,
}

/// A file's bytes; `None` for one already gone.
fn size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

impl CapturesPool {
    pub fn new(class: &ClassWriter) -> Self {
        Self { class: class.clone() }
    }

    /// Every generation of every owner, live ones included, as (index, frames, its owner's newest).
    fn generations(&self) -> Vec<(PathBuf, PathBuf, bool, ClassWriter, u64)> {
        let owners = self.class.owners().unwrap_or_default(); // unwrap_or_default: an unreadable class root holds nothing this pool can free
        owners
            .into_iter()
            .flat_map(|owner| {
                let scoped = self.class.owned_by(owner);
                let gens = capture_log::generations(&scoped).unwrap_or_default(); // unwrap_or_default: an unreadable owner holds nothing this pool can free
                let newest = gens.last().copied();
                gens.into_iter()
                    .filter_map(move |generation| {
                        let index = scoped.path(Path::new(&format!("{generation:016}.index"))).ok()?;
                        let frames = scoped.path(Path::new(&format!("{generation:016}.frames"))).ok()?;
                        Some((index, frames, Some(generation) == newest, scoped.clone(), generation))
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
            .filter(|(_, frames, ..)| !capture_log::is_live(frames))
            .filter_map(|(index, frames, newest, scoped, generation)| {
                let modified = std::fs::metadata(&frames).and_then(|m| m.modified()).ok()?;
                let (index_bytes, frames_bytes) = (size(&index)?, size(&frames)?);
                // an emptied newest holds nothing to free
                (index_bytes + frames_bytes > 0).then_some(Closed { index_bytes, frames_bytes, index, frames, modified, newest, scoped, generation })
            })
            .collect();
        closed.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| a.frames.cmp(&b.frames))); // equal times: the lower generation first
        closed
    }
}

impl ResourcePool for CapturesPool {
    fn root(&self) -> Option<PathBuf> {
        Some(self.class.dir())
    }

    fn tier_name(&self) -> &str {
        "captures"
    }

    /// No budget of its own: relief asks it by its volume's pressure.
    fn capacity_bytes(&self) -> u64 {
        0
    }

    fn usage_bytes(&self) -> u64 {
        self.generations().iter().flat_map(|(index, frames, ..)| [size(index), size(frames)]).flatten().sum()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let freed = self.evict_closed(want_bytes, 0);
        if freed >= want_bytes {
            return freed;
        }
        // not enough closed history: close what the running writer holds open, so its own
        // generations become evictable too, and take those (Codex on #6236)
        #[cfg(test)]
        if ANOTHER_ROLL_FIRST.with(|first| first.replace(false)) {
            capture_log::roll_open(); // a concurrent pool's roll, landing between this pool's two looks
        }
        let rolled = capture_log::roll_open();
        continuum_base::probe!(class = "capture.generation.rolled", rolled = rolled as u64, freed, want_bytes,
            "closed capture history did not cover the pressure: the open logs rolled to their next generations");
        // swept again whatever this roll closed: a concurrent pool's roll may already have closed this
        // class's logs, leaving them reclaimable though this call rolled none (Codex on #6236)
        self.evict_closed(want_bytes, freed)
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

impl CapturesPool {
    /// Remove closed generations oldest first until `want_bytes` (counting `freed` already) are free.
    fn evict_closed(&self, want_bytes: u64, mut freed: u64) -> u64 {
        for gen in self.closed() {
            if freed >= want_bytes {
                break;
            }
            // the index first: a removal cut short leaves frames no reader lists. The owner's newest is
            // emptied in place instead, so its number is never reused.
            let let_go = |path: &Path, suffix: &str| if gen.newest { gen.scoped.replace(Path::new(&format!("{:016}.{suffix}", gen.generation)), b"") } else { std::fs::remove_file(path) };
            if let_go(&gen.index, "index").is_err() {
                continue;
            }
            freed = freed.saturating_add(gen.index_bytes);
            match let_go(&gen.frames, "frames") {
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

        let pool = CapturesPool::new(&TrackedDir::new("captures-pool-test", dir.path().to_path_buf()).writer());
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

    // what this catches (Codex on #6236): two pools on two roots under pressure at once: the first's
    // roll closes every registered log, the second's own roll then closes none, and the second frees
    // nothing though its root's history just became reclaimable.
    #[test]
    fn a_pool_reclaims_history_another_pools_roll_closed() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = TrackedDir::new("captures-pool-concurrent-test", dir.path().to_path_buf()).writer();
        let owner = uuid::Uuid::new_v4();
        let logs = crate::capture_log::OpenLogs::shared();
        let record = crate::capture_log::CaptureRecord { owner, kind: FrameKind::Submitted, header: vec![0x90], runs: vec![std::sync::Arc::from(&[7u8; 1024][..])], captured_at_ms: 1 };
        logs.append(&class, &record).expect("test: a record");

        let pool = CapturesPool::new(&TrackedDir::new("captures-pool-concurrent-test", dir.path().to_path_buf()).writer());
        ANOTHER_ROLL_FIRST.with(|first| first.set(true)); // its first look finds the log open; the other roll lands next
        assert!(pool.evict_at_least(u64::MAX) > 1024, "this root's history, closed by the other roll, is still reclaimed");
    }

    // what this catches (Codex on #6236): a playback that has already read an owner's calls and,
    // after eviction emptied or removed their generations under pressure, keeps listing them (its
    // index kept the read count and places of history that is gone).
    #[test]
    fn a_warmed_playback_sees_eviction_on_its_next_page() {
        use crate::prompt_capture::{FramePromptCaptureSink, PromptCall, PromptCaptureSink};
        let dir = tempfile::tempdir().expect("test: dir");
        let persona = uuid::Uuid::new_v4();
        let sink = FramePromptCaptureSink::at(dir.path(), persona);
        let call = PromptCall { request_id: "r".into(), persona_id: persona, room_id: uuid::Uuid::new_v4(), cycle_id: Some(1), context_window: Some(8192), cause: "synthetic", cause_root: None, replay_of: None };
        let request = continuum_engine::ai::types::TextGenerationRequest::default();
        let token = sink.submitted(&call, &request).expect("test: submitted");
        sink.terminal(token, crate::call_frames::CallStatus::Completed, None, None, 1);

        let playback = crate::call_playback::Playback::at(dir.path(), persona);
        let ten = std::num::NonZeroUsize::new(10).expect("ten");
        assert_eq!(playback.page(None, false, ten).expect("test: page").entries.len(), 1, "warmed: it has read the call");
        assert!(CapturesPool::new(&crate::capture::fixture_class(dir.path())).evict_at_least(u64::MAX) > 0, "pressure took it");
        let after = playback.page(None, false, ten).expect("test: the next page reads");
        assert!(after.entries.is_empty() && after.issues.is_empty(), "the next page lists nothing gone: {after:?}");
    }

    // what this catches (Codex on #6236): a long-running node that can never reclaim its own capture
    // history, because the writer holds every owner's generation open for its whole life and the pool
    // skips every open generation; and a writer that cannot record again after a roll.
    #[test]
    fn under_pressure_a_running_writers_own_history_is_reclaimed_and_it_keeps_recording() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = TrackedDir::new("captures-pool-roll-test", dir.path().to_path_buf()).writer();
        let owner = uuid::Uuid::new_v4();
        let logs = crate::capture_log::OpenLogs::shared();
        let record = || crate::capture_log::CaptureRecord { owner, kind: FrameKind::Submitted, header: vec![0x90], runs: vec![std::sync::Arc::from(&[7u8; 1024][..])], captured_at_ms: 1 };
        logs.append(&class, &record()).expect("test: the running writer's first record");

        let pool = CapturesPool::new(&TrackedDir::new("captures-pool-roll-test", dir.path().to_path_buf()).writer());
        assert!(pool.evict_at_least(u64::MAX) > 1024, "the writer's own open generation rolled and was reclaimed");
        assert_eq!(capture_log::generations(&class.owned_by(owner)).expect("test: generations"), vec![0], "its bytes are gone; its number stays, emptied, so it is never reused");

        logs.append(&class, &record()).expect("test: the writer records again after the roll");
        assert_eq!(capture_log::generations(&class.owned_by(owner)).expect("test: generations"), vec![0, 1], "into its next generation, the emptied one keeping its number");
    }
}
