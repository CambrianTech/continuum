//! Which weights file an engine opened, stamped when its lane launched (card 3fd98325, S3 of the
//! cross-grid training lease).
//!
//! A lane names its model by id; the registry may resolve that id to another file later (a pull,
//! a replaced quantization). An adapter trained in the lane must name the weights the ENGINE
//! read, so the lane records the file it was launched on (path, size, modification time, and the
//! file's identity on its volume), and a later reader proves the path still names that file
//! before trusting its bytes (Codex on #5418).
//!
//! The bound, stated: a stat cannot see a rewrite in place that keeps the same file, size and
//! modification time (it would also rewrite the running engine's mapped weights under it), nor a
//! replacement in the instant between this stamp and the engine opening the file.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The weights file a lane was launched on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightsStamp {
    pub path: PathBuf,
    pub bytes: u64,
    /// Modification time, nanoseconds since the Unix epoch.
    pub modified_ns: u64,
    /// The file's identity on its volume (`continuum_host::file_identity`).
    pub volume: u64,
    pub index: u64,
}

/// Why a stamped file's bytes cannot be vouched for.
#[derive(Debug, PartialEq, Eq)]
pub enum StampRefusal {
    /// The path no longer names the stamped file, or it changed while being read: final.
    Changed(String),
    /// The file could not be read now.
    Unreadable(String),
}

impl WeightsStamp {
    /// Stamp the file at `path` as it is now.
    pub fn of(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Self::of_open(path, &file)
    }

    fn of_open(path: &Path, file: &File) -> io::Result<Self> {
        let meta = file.metadata()?;
        let modified_ns = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| io::Error::other(format!("modification time before the epoch: {e}")))?
            .as_nanos();
        let id = continuum_host::file_identity::of(file)?;
        Ok(Self {
            path: path.to_path_buf(),
            bytes: meta.len(),
            modified_ns: u64::try_from(modified_ns).map_err(|_| io::Error::other("modification time past 2554"))?,
            volume: id.volume,
            index: id.index,
        })
    }

    /// Read the stamped file through ONE handle that is proved to be the stamped file before
    /// `read` runs and unchanged after it. A path that names another file, or none, is
    /// `Changed` whatever `read` reported, so a replacement is never mistaken for a read error;
    /// only then is `read`'s own failure `Unreadable`.
    pub fn read_verified<T>(&self, read: impl FnOnce(&mut File) -> Result<T, String>) -> Result<T, StampRefusal> {
        let unreadable = |e: io::Error| StampRefusal::Unreadable(format!("{}: {e}", self.path.display()));
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StampRefusal::Changed(format!("{} is gone", self.path.display())))
            }
            Err(e) => return Err(unreadable(e)),
        };
        self.same_as(&file).map_err(unreadable)??;
        let outcome = read(&mut file);
        self.same_as(&file).map_err(unreadable)??;
        outcome.map_err(|e| StampRefusal::Unreadable(format!("{}: {e}", self.path.display())))
    }

    fn same_as(&self, file: &File) -> io::Result<Result<(), StampRefusal>> {
        let now = Self::of_open(&self.path, file)?;
        Ok(if now == *self {
            Ok(())
        } else {
            Err(StampRefusal::Changed(format!("{} is not the file the lane was launched on", self.path.display())))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(stamp: &WeightsStamp) -> Result<String, StampRefusal> {
        stamp.read_verified(|file| {
            let mut hasher = Sha256::new();
            io::copy(file, &mut hasher).map_err(|e| e.to_string())?;
            Ok(format!("{:x}", hasher.finalize()))
        })
    }

    // what this catches (Codex on #5418): an adapter trained on file A credited to whatever the
    // path holds later. The stamped file reads; the path replaced by another file (same size)
    // is Changed, never read as the training base, even when the reader would have failed on
    // it; a missing file is Changed, not Unreadable.
    #[test]
    fn only_the_stamped_file_is_vouched_for() {
        let dir = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        let (path, other) = (dir.path().join("base.gguf"), dir.path().join("other.gguf"));
        std::fs::write(&path, b"weights-a").expect("write"); // JUSTIFIED: test fixture
        std::fs::write(&other, b"weights-b").expect("write"); // JUSTIFIED: test fixture
        let stamp = WeightsStamp::of(&path).expect("stamp"); // JUSTIFIED: test fixture
        assert_eq!(sha(&stamp), Ok(format!("{:x}", Sha256::digest(b"weights-a"))));
        assert!(matches!(stamp.read_verified(|_| Err::<(), _>("parse failed".into())), Err(StampRefusal::Unreadable(_))), "the stamped file's own read failure may pass");

        std::fs::rename(&other, &path).expect("replace"); // JUSTIFIED: test fixture
        assert!(matches!(sha(&stamp), Err(StampRefusal::Changed(_))), "a replaced path is another file");
        assert!(matches!(stamp.read_verified(|_| Err::<(), _>("parse failed".into())), Err(StampRefusal::Changed(_))), "a replacement outranks the reader's error");
        std::fs::remove_file(&path).expect("remove"); // JUSTIFIED: test fixture
        assert!(matches!(sha(&stamp), Err(StampRefusal::Changed(_))));
    }
}
