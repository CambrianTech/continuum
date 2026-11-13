//! The process's copy of every saved value it has read or written, one slot per file.
//!
//! The M5, 2026-11-07 (build 839e7a08d): serving sized each mind's lane from her training demand
//! on every pass, through a fresh store, so one resident's file was read, decoded and probed
//! 1,748 times in 23 minutes (`state.load.absent`, 9,224 lines an hour across her lines). The
//! store is the only writer of an [`crate::OwnedFile`] kind's files (no other process places
//! them; every save and remove in this process goes through a store, whichever handle), so the
//! first load of a key reads the disk and every later one answers from here; a save or a remove
//! replaces the copy with what it placed. A failed save or remove forgets the copy, so the next
//! load reads the disk again. A file this build cannot trust (corrupt, a newer or an older format)
//! is never copied: an operator repairs it while the core runs, and the next load reads the
//! repair. A file placed behind a running core otherwise is read at the next restart. A medium written by another process (KV pages' `ExternalBlob`,
//! captures) is never copied here: it revalidates against its writer's receipt instead.
//!
//! Slots are shared by every handle in the process (a fresh `Store::at` for the same directory
//! sees the same copy) and bounded by the keys each kind's `Retention` names. The map's lock is
//! held only to find a slot; a slot's lock is held across that one file's read or placement, so
//! two first loads of a key read it once. Nothing here awaits.

use crate::loaded::Loaded;
use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

/// A slot's file: the kind and its path (`None`: unrooted, one slot per kind).
type SlotKey = (&'static str, Option<PathBuf>);

static SLOTS: LazyLock<Mutex<HashMap<SlotKey, Arc<Mutex<Slot>>>>> = LazyLock::new(Default::default);

/// The slot for `kind`'s file at `path`, made empty on first use.
pub(crate) fn slot(kind: &'static str, path: Option<PathBuf>) -> Arc<Mutex<Slot>> {
    let mut slots = SLOTS.lock();
    Arc::clone(slots.entry((kind, path)).or_default())
}

/// A new process's view of every file under `dir`: their slots are dropped, so the next load of
/// each reads the disk. For a test that places a file behind the store, as another life or a
/// hand edit would have before this process started.
#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn restart(dir: &Path) {
    SLOTS.lock().retain(|(_, path), _| !path.as_deref().is_some_and(|path| path.starts_with(dir)));
}

/// What a file holds, as this process last read or placed it.
pub(crate) enum File {
    Missing,
    Bytes(Vec<u8>),
}

/// The verdict last reached and the live contract it was judged under.
pub(crate) struct Judged {
    pub(crate) live: serde_json::Value,
    /// `(Loaded<K::Value>, generation)`, typed by the kind that judged it.
    pub(crate) verdict: Box<dyn Any + Send>,
}

#[derive(Default)]
pub(crate) struct Slot {
    /// `None`: not read in this life.
    pub(crate) file: Option<File>,
    /// Reused while the live contract is the same; an aged kind is judged again each load.
    pub(crate) judged: Option<Judged>,
    /// The verdict last probed, by class and generation: a load probes only when it changes.
    reported: Option<(&'static str, u64)>,
    /// Loads answered without reading the disk since the last probe (reported with the next).
    hits: u64,
    /// Reads of the file in this life, and probes of its verdict: what the tests count.
    pub(crate) disk_reads: u64,
    pub(crate) probes: u64,
}

impl Slot {
    pub(crate) fn hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    /// Probe `loaded` when its class or generation differs from the last one probed here.
    pub(crate) fn report<T>(&mut self, kind: &'static str, path: Option<&Path>, loaded: &Loaded<T>, generation: u64) {
        let now = (loaded.probe_class(), generation);
        if self.reported == Some(now) {
            return;
        }
        continuum_base::probe!(class = now.0, kind, path = ?path, generation, found = %loaded, served_from_memory = self.hits, "saved state loaded");
        self.reported = Some(now);
        self.hits = 0;
        self.probes = self.probes.saturating_add(1);
    }

    /// The store placed `file`: the copy is what is on disk now.
    pub(crate) fn placed(&mut self, file: File) {
        self.file = Some(file);
        self.judged = None;
    }

    /// The disk may not hold what the copy says (a failed save or remove): read it again next.
    pub(crate) fn forget(&mut self) {
        self.file = None;
        self.judged = None;
    }
}
