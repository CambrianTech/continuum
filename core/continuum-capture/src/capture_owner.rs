//! Construction-time leases for the existing capture owners. Concurrent replay
//! forks of one persona share a writer; they must not rotate beneath a live sink.
//! Like the warm eval-lane lease, Weak ownership closes at the last user and
//! creates no reaper, polling task, persistent transcript cache, or rival writer.
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

pub struct CaptureOwners<T> {
    owners: Mutex<HashMap<(PathBuf, uuid::Uuid), Weak<Mutex<T>>>>,
}
impl<T> Default for CaptureOwners<T> {
    fn default() -> Self {
        Self {
            owners: Mutex::new(HashMap::new()),
        }
    }
}
impl<T> CaptureOwners<T> {
    pub fn acquire(
        &self,
        directory: &Path,
        persona: uuid::Uuid,
        open: impl FnOnce(&Path, uuid::Uuid) -> io::Result<T>,
    ) -> io::Result<Arc<Mutex<T>>> {
        std::fs::create_dir_all(directory)?;
        let key = (directory.canonicalize()?, persona);
        let mut owners = self
            .owners
            .lock()
            .map_err(|_| io::Error::other("capture owner lease lock poisoned"))?;
        if let Some(owner) = owners.get(&key).and_then(Weak::upgrade) {
            return Ok(owner);
        }
        owners.retain(|_, owner| owner.strong_count() > 0);
        let owner = Arc::new(Mutex::new(open(&key.0, key.1)?));
        owners.insert(key, Arc::downgrade(&owner));
        Ok(owner)
    }
}
