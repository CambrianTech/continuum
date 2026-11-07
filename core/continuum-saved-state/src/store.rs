//! The store for [`OwnedFile`] kinds: one directory, one envelope file per key, every write under
//! the key's lock and numbered by generation.

use crate::envelope::{decode, Envelope};
use crate::kind::{Key, OwnedFile, Rebuildable, SavedState};
use crate::loaded::{CorruptCause, Loaded};
use continuum_base::time::EpochMs;
use sha2::{Digest, Sha256};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// What a save wrote: the generation it placed and the digest of exactly those bytes, so a reader
/// can tell this write from any other (never a bare "saved").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub generation: u64,
    /// SHA-256 of the placed file, lowercase hex.
    pub digest: String,
}

/// Why a save did not land. The store keeps no handle, so the next save tries afresh.
#[derive(Debug)]
pub enum SaveError {
    /// No directory to save in (no continuum home).
    Unrooted,
    /// Another writer placed a newer generation since this one read: the write is refused, and
    /// the caller reads again and reapplies its change.
    StaleGeneration { read: u64, found: u64 },
    /// The value did not encode.
    Encode(serde_json::Error),
    /// The directory, the lock or the atomic placement failed.
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Unrooted => f.write_str("no continuum home: nothing can be saved"),
            SaveError::StaleGeneration { read, found } => write!(f, "read at generation {read}, but generation {found} was saved since"),
            SaveError::Encode(e) => write!(f, "the value did not encode: {e}"),
            SaveError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<SaveError> for std::io::Error {
    fn from(e: SaveError) -> Self {
        match e {
            SaveError::Io(e) => e,
            other => std::io::Error::other(other.to_string()),
        }
    }
}

/// Where one kind's files live, and the trunk's save and load for it.
pub struct Store<K: SavedState<Medium = OwnedFile>> {
    dir: Option<PathBuf>,
    /// Saves failed in a row: reported on each failure and on the recovery, never a latch.
    failures: AtomicU32,
    kind: PhantomData<fn() -> K>,
}

impl<K: SavedState<Medium = OwnedFile>> Store<K> {
    /// The kind's files in `dir`; `None` when there is no continuum home to root them in (every
    /// load is then `Unrooted` and every save refuses).
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, failures: AtomicU32::new(0), kind: PhantomData }
    }

    pub fn at(dir: PathBuf) -> Self {
        Self::new(Some(dir))
    }

    /// The file `key` lives in.
    pub fn path(&self, key: &K::Key) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(key.file_name(K::KIND)))
    }

    /// The verdict on what is saved under `key`, judged against the `live` contract.
    pub fn load(&self, key: &K::Key, live: &K::Contract) -> Loaded<K::Value> {
        self.load_at_generation(key, live).0
    }

    /// The verdict and the generation on disk (0: nothing of this kind there), for a caller that
    /// changes what it read and saves it back with [`Store::save_over`]. Probed once, under the
    /// verdict's own class.
    pub fn load_at_generation(&self, key: &K::Key, live: &K::Contract) -> (Loaded<K::Value>, u64) {
        let (loaded, generation) = match self.path(key) {
            None => (Loaded::Unrooted, 0),
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => decode::<K>(&bytes, key, live, EpochMs::now().millis()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Loaded::Absent, 0),
                Err(e) => (Loaded::Corrupt { cause: CorruptCause::Unreadable(e.kind()) }, 0),
            },
        };
        continuum_base::probe!(class = loaded.probe_class(), kind = K::KIND, path = ?self.path(key), generation, found = %loaded, "saved state loaded");
        (loaded, generation)
    }

    /// Save `value` under `key`, valid under `contract`, as the next generation whatever is on
    /// disk: for a writer that owns the whole value (a measured curve).
    pub fn save(&self, key: &K::Key, contract: &K::Contract, value: &K::Value) -> Result<Saved, SaveError> {
        self.reported(key, self.place(key, contract, value, None))
    }

    /// Save a value changed from what was read at generation `read`; refused with
    /// [`SaveError::StaleGeneration`] if another writer saved since, so a read-modify-write never
    /// erases a change it did not see.
    pub fn save_over(&self, key: &K::Key, contract: &K::Contract, value: &K::Value, read: u64) -> Result<Saved, SaveError> {
        self.reported(key, self.place(key, contract, value, Some(read)))
    }

    fn reported(&self, key: &K::Key, placed: Result<Saved, SaveError>) -> Result<Saved, SaveError> {
        let path = self.path(key);
        match &placed {
            Ok(saved) => {
                let failed = self.failures.swap(0, Ordering::Relaxed);
                if failed > 0 {
                    continuum_base::probe!(class = "state.save.recovered", kind = K::KIND, path = ?path, generation = saved.generation, after = u64::from(failed), "saved state writes again");
                }
            }
            Err(error) => {
                let in_a_row = self.failures.fetch_add(1, Ordering::Relaxed).saturating_add(1);
                continuum_base::probe!(class = "state.save.failed", kind = K::KIND, path = ?path, in_a_row = u64::from(in_a_row), %error, "saved state not written; the next save tries again");
            }
        }
        placed
    }

    fn place(&self, key: &K::Key, contract: &K::Contract, value: &K::Value, read: Option<u64>) -> Result<Saved, SaveError> {
        let (Some(dir), Some(path)) = (self.dir.as_deref(), self.path(key)) else { return Err(SaveError::Unrooted) };
        std::fs::create_dir_all(dir).map_err(SaveError::Io)?;
        let _writer = continuum_host::durable::lock_path(&dir.join(format!(".{}.lock", key.file_name(K::KIND))), true).map_err(SaveError::Io)?;
        let on_disk = match std::fs::read(&path) {
            Ok(bytes) => crate::envelope::generation_of(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(SaveError::Io(e)),
        };
        if let Some(read) = read.filter(|read| *read != on_disk) {
            return Err(SaveError::StaleGeneration { read, found: on_disk });
        }
        let generation = on_disk.saturating_add(1);
        let bytes = serde_json::to_vec_pretty(&Envelope::<K>::new(key, contract, value, generation, EpochMs::now().millis())).map_err(SaveError::Encode)?;
        continuum_host::durable::write_replace(&path, &bytes).map_err(SaveError::Io)?;
        Ok(Saved { generation, digest: Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect() })
    }

    /// Remove what is saved under `key` (an unpin). Nothing saved is not an error.
    pub fn remove(&self, key: &K::Key) -> Result<(), SaveError> {
        let Some(path) = self.path(key) else { return Err(SaveError::Unrooted) };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                continuum_base::probe!(class = "state.removed", kind = K::KIND, path = %path.display(), "saved state removed");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                continuum_base::probe!(class = "state.save.failed", kind = K::KIND, path = %path.display(), error = %e, "saved state not removed");
                Err(SaveError::Io(e))
            }
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }
}

impl<K: Rebuildable<Medium = OwnedFile>> Store<K> {
    /// The saved value, or an empty one the owner measures again: anything but `Matched` (already
    /// probed with what was found) starts from nothing, never from defaults read as measured.
    pub fn load_rebuildable(&self, key: &K::Key, live: &K::Contract) -> K::Value {
        self.rebuildable_at_generation(key, live).0
    }

    /// [`Store::load_rebuildable`] with the generation on disk, for a read-modify-write that saves
    /// back with [`Store::save_over`].
    pub fn rebuildable_at_generation(&self, key: &K::Key, live: &K::Contract) -> (K::Value, u64) {
        let (loaded, generation) = self.load_at_generation(key, live);
        match loaded.matched() {
            Ok(value) => (value, generation),
            Err(_found_and_probed) => (K::Value::default(), generation),
        }
    }

    /// Save a value that can be measured again: a failure is probed by [`Store::save`] and the next
    /// save tries afresh, so the measuring caller has nothing to do with it.
    pub fn save_rebuildable(&self, key: &K::Key, contract: &K::Contract, value: &K::Value) {
        match self.save(key, contract, value) {
            Ok(_) => {}
            Err(_probed_and_retried_on_the_next_save) => {}
        }
    }
}

/// A handle to the same files; the copy counts its own failures.
impl<K: SavedState<Medium = OwnedFile>> Clone for Store<K> {
    fn clone(&self) -> Self {
        Self::new(self.dir.clone())
    }
}

impl<K: SavedState<Medium = OwnedFile>> std::fmt::Debug for Store<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("kind", &K::KIND).field("dir", &self.dir).finish()
    }
}
