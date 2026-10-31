//! `DiskStore`: `blobs/sha256/<hex>` and `refs/<scope>/<name>` under one root. ONE write path
//! for every byte this layer persists: a temp file under `tmp/`, synced, published (a blob by
//! create-if-absent, a ref by replace), its directory synced, every error returned.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use continuum_host::durable;

use super::{drain, ByteRange, RefName, Store, Stored};
use crate::blob::{BlobRead, BlobSource};
use crate::error::StoreError;
use crate::id::{ContentId, HashingWriter};

/// A content-addressed store on a local file system.
pub struct DiskStore {
    root: PathBuf,
}

/// Distinguishes this process's temp files from each other (the pid separates processes).
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn io_err<'a>(op: &'static str, path: &'a Path) -> impl FnOnce(io::Error) -> StoreError + 'a {
    move |source| StoreError::Io {
        op,
        path: path.to_path_buf(),
        source,
    }
}

impl DiskStore {
    /// Open (creating if needed) a store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let store = Self { root: root.into() };
        for dir in [
            store.root.join("blobs").join("sha256"),
            store.root.join("refs"),
            store.root.join("tmp"),
        ] {
            ensure_dir(&dir)?;
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a blob lives (exposed so tests and tools can prove what is on disk).
    pub fn blob_path(&self, id: &ContentId) -> PathBuf {
        self.root.join("blobs").join("sha256").join(id.hex())
    }

    fn ref_path(&self, name: &RefName) -> PathBuf {
        self.root.join("refs").join(name.scope()).join(name.name())
    }

    /// A new, exclusively created temp file and the guard that removes it.
    fn temp(&self) -> Result<(Temp, File), StoreError> {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = self
            .root
            .join("tmp")
            .join(format!("{}-{seq}", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io_err("create temp", &path))?;
        Ok((Temp { path }, file))
    }
}

/// A temp file that removes itself. Removal on drop is best effort by necessity (drop cannot
/// return an error); the failure that caused the drop has already been returned, and a
/// leftover temp holds no reachable state.
struct Temp {
    path: PathBuf,
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Create `dir` (and parents) and make each new entry durable in its parent.
fn ensure_dir(dir: &Path) -> Result<(), StoreError> {
    if dir.as_os_str().is_empty() || dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        ensure_dir(parent)?;
    }
    match durable::create_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io_err("create dir", dir)(e)),
    }
    Ok(())
}

impl Store for DiskStore {
    fn put(&self, src: BlobSource<'_>) -> Result<Stored, StoreError> {
        let (temp, file) = self.temp()?;
        let mut w = HashingWriter::new(BufWriter::with_capacity(1 << 20, file));
        drain(src, &mut w)?;
        let (id, size, buf) = w.finish();
        let file = buf
            .into_inner()
            .map_err(|e| io_err("flush temp", &temp.path)(e.into_error()))?;
        file.sync_all().map_err(io_err("sync temp", &temp.path))?;
        drop(file);
        let dest = self.blob_path(&id);
        match durable::publish_new(&temp.path, &dest) {
            Ok(()) => {}
            // Same content is already here (an earlier put, or a racing one): finish its
            // fence rather than trusting a name a racer may not have synced yet.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let existing = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&dest)
                    .map_err(io_err("open blob", &dest))?;
                durable::fence(&existing, &dest).map_err(io_err("sync blob", &dest))?;
            }
            Err(e) => return Err(io_err("publish blob", &dest)(e)),
        }
        Ok(Stored { id, size })
    }

    fn get(&self, id: &ContentId, range: ByteRange) -> Result<BlobRead, StoreError> {
        let path = self.blob_path(id);
        let mut file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(StoreError::NotFound(*id)),
            Err(e) => return Err(io_err("open blob", &path)(e)),
        };
        let size = file.metadata().map_err(io_err("stat blob", &path))?.len();
        let (start, len) = range.within(id, size)?;
        if start > 0 {
            file.seek(SeekFrom::Start(start))
                .map_err(io_err("seek blob", &path))?;
        }
        Ok(BlobRead::Stream {
            len,
            reader: Box::new(BufReader::with_capacity(1 << 20, file).take(len)),
        })
    }

    fn has(&self, id: &ContentId) -> Result<bool, StoreError> {
        let path = self.blob_path(id);
        path.try_exists().map_err(io_err("stat blob", &path))
    }

    fn set_ref(
        &self,
        name: &RefName,
        to: ContentId,
        expect: Option<ContentId>,
    ) -> Result<(), StoreError> {
        let path = self.ref_path(name);
        let dir = self.root.join("refs").join(name.scope());
        ensure_dir(&dir)?;
        // One writer per ref across processes: an exclusive lock on a sibling that is never
        // deleted (deleting a lock file races the next locker).
        let lock_path = dir.join(format!(".{}.lock", name.name()));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(io_err("open ref lock", &lock_path))?;
        lock.lock().map_err(io_err("lock ref", &lock_path))?;
        let found = self.get_ref(name)?;
        if found != expect {
            continuum_base::probe!(class = "decision", verdict = "ref-conflict", reference = %name);
            return Err(StoreError::RefConflict {
                name: name.clone(),
                expected: expect,
                found,
            });
        }
        let (temp, mut file) = self.temp()?;
        file.write_all(format!("{to}\n").as_bytes())
            .map_err(io_err("write ref", &temp.path))?;
        file.sync_all().map_err(io_err("sync ref", &temp.path))?;
        drop(file);
        durable::replace(&temp.path, &path).map_err(io_err("publish ref", &path))?;
        Ok(())
    }

    fn get_ref(&self, name: &RefName) -> Result<Option<ContentId>, StoreError> {
        let path = self.ref_path(name);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err("read ref", &path)(e)),
        };
        text.trim_end_matches('\n')
            .parse()
            .map(Some)
            .map_err(|e| StoreError::RefCorrupt {
                name: name.clone(),
                reason: format!("{e}"),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the on-disk layout the design names (blobs/sha256/<hex>,
    // refs/<scope>/<name>), an idempotent put that leaves no temp behind, a range read, a
    // compare-and-swap that refuses a stale expectation, and a corrupt ref file reported as
    // corrupt rather than read as "no ref".
    #[test]
    fn layout_idempotent_put_ranges_and_ref_cas() {
        let dir = tempfile::tempdir().unwrap();
        let s = DiskStore::open(dir.path().join("store")).unwrap();
        let a = s.put(BlobSource::Bytes(b"hello world")).unwrap();
        let again = s.put(BlobSource::Bytes(b"hello world")).unwrap();
        assert_eq!(a, again);
        assert_eq!(fs::read(s.blob_path(&a.id)).unwrap(), b"hello world");
        assert_eq!(
            fs::read_dir(s.root().join("tmp")).unwrap().count(),
            0,
            "no temp left behind"
        );
        let span = s
            .get(&a.id, ByteRange::Span { start: 6, len: 5 })
            .unwrap()
            .into_bytes()
            .unwrap();
        assert_eq!(&span[..], b"world");
        assert!(matches!(
            s.get(&a.id, ByteRange::Span { start: 6, len: 6 }),
            Err(StoreError::RangeOutOfBounds { .. })
        ));

        let name = RefName::new("persona", "helper").unwrap();
        let b = s.put(BlobSource::Bytes(b"other")).unwrap();
        s.set_ref(&name, a.id, None).unwrap();
        assert!(matches!(
            s.set_ref(&name, b.id, None),
            Err(StoreError::RefConflict { .. })
        ));
        s.set_ref(&name, b.id, Some(a.id)).unwrap();
        assert_eq!(s.get_ref(&name).unwrap(), Some(b.id));
        assert_eq!(
            fs::read_to_string(dir.path().join("store/refs/persona/helper")).unwrap(),
            format!("{}\n", b.id)
        );
        fs::write(dir.path().join("store/refs/persona/helper"), "garbage").unwrap();
        assert!(matches!(
            s.get_ref(&name),
            Err(StoreError::RefCorrupt { .. })
        ));
        assert!(RefName::new("..", "x").is_err() && RefName::new("a/b", "x").is_err());
    }
}
