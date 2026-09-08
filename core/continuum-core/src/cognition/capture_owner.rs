//! Construction-time leases for the existing capture owners. Concurrent replay
//! forks of one persona share a writer; they must not rotate beneath a live sink.
//! Like the warm eval-lane lease, Weak ownership closes at the last user and
//! creates no reaper, polling task, persistent transcript cache, or rival writer.
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

pub(crate) struct CaptureOwners<T> {
    owners: Mutex<HashMap<PathBuf, Weak<Mutex<T>>>>,
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
        path: &Path,
        open: impl FnOnce(&Path) -> io::Result<T>,
    ) -> io::Result<Arc<Mutex<T>>> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("capture directory missing"))?;
        std::fs::create_dir_all(parent)?;
        let key = parent.canonicalize()?.join(
            path.file_name()
                .ok_or_else(|| io::Error::other("capture file name missing"))?,
        );
        let mut owners = self
            .owners
            .lock()
            .map_err(|_| io::Error::other("capture owner lease lock poisoned"))?;
        if let Some(owner) = owners.get(&key).and_then(Weak::upgrade) {
            return Ok(owner);
        }
        owners.retain(|_, owner| owner.strong_count() > 0);
        let owner = Arc::new(Mutex::new(open(&key)?));
        owners.insert(key, Arc::downgrade(&owner));
        Ok(owner)
    }
}
