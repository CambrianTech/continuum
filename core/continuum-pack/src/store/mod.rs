//! The medium's side: five verbs. Blobs are immutable and named by content, so they need no
//! commit protocol; `set_ref` (a compare-and-swap of a name onto a manifest) is the only
//! mutation.

mod disk;
#[cfg(any(test, feature = "conformance"))]
mod fault;
mod ram;

use std::fmt;
use std::io::{self, Read};

use bytes::Bytes;

pub use disk::DiskStore;
#[cfg(any(test, feature = "conformance"))]
pub use fault::{FaultMode, FaultStore, Op};
pub use ram::RamStore;

use crate::blob::{BlobRead, BlobSource};
use crate::error::StoreError;
use crate::id::{ContentId, HashingWriter};

/// Where packages' bytes live. Every implementation verifies nothing on the caller's behalf
/// except what it hashed itself at `put`; readers verify with [`read_verified`].
pub trait Store: Send + Sync {
    /// Store bytes under their content id, hashing in flight. Idempotent.
    fn put(&self, src: BlobSource<'_>) -> Result<Stored, StoreError>;
    /// Read a blob, whole or a span.
    fn get(&self, id: &ContentId, range: ByteRange) -> Result<BlobRead, StoreError>;
    /// Whether the blob is here. An I/O failure is an error, never "absent".
    fn has(&self, id: &ContentId) -> Result<bool, StoreError>;
    /// Point `name` at `to` iff it currently points at `expect` (`None` = it must not exist).
    fn set_ref(
        &self,
        name: &RefName,
        to: ContentId,
        expect: Option<ContentId>,
    ) -> Result<(), StoreError>;
    /// Where `name` points, or `None` if it was never set.
    fn get_ref(&self, name: &RefName) -> Result<Option<ContentId>, StoreError>;
}

/// The result of a `put`: the content's id and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stored {
    pub id: ContentId,
    pub size: u64,
}

/// Which bytes of a blob to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteRange {
    Full,
    Span { start: u64, len: u64 },
}

impl ByteRange {
    /// The `(start, len)` this range selects from a blob of `size` bytes, or a typed refusal.
    pub(crate) fn within(self, id: &ContentId, size: u64) -> Result<(u64, u64), StoreError> {
        match self {
            ByteRange::Full => Ok((0, size)),
            ByteRange::Span { start, len } => match start.checked_add(len) {
                Some(end) if end <= size => Ok((start, len)),
                _ => Err(StoreError::RangeOutOfBounds {
                    id: *id,
                    start,
                    len,
                    size,
                }),
            },
        }
    }
}

/// A ref's typed name: `<scope>/<name>`, each one path-safe segment.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefName {
    /// `scope/name`, held as one string (the error types that carry a name stay small).
    path: String,
    scope_len: usize,
}

impl RefName {
    pub fn new(scope: &str, name: &str) -> Result<Self, StoreError> {
        let (scope, name) = (segment(scope)?, segment(name)?);
        Ok(Self {
            path: format!("{scope}/{name}"),
            scope_len: scope.len(),
        })
    }

    // Slicing cannot fail: `new` is the only constructor, and it put an ASCII `/` at
    // `scope_len` with both segments non-empty ASCII.
    pub fn scope(&self) -> &str {
        &self.path[..self.scope_len]
    }

    pub fn name(&self) -> &str {
        &self.path[self.scope_len + 1..]
    }
}

impl fmt::Display for RefName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path)
    }
}

/// One path segment a ref may use on every store: no separators, no dot-files (so no `..` and
/// no collision with a store's own `.lock`/temp names), no empty name.
fn segment(s: &str) -> Result<&str, StoreError> {
    let ok = !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(s)
    } else {
        Err(StoreError::InvalidRefName(s.to_owned()))
    }
}

/// Read a whole blob and prove its bytes hash to `id`. Whatever store or peer served it, a
/// mismatch is a typed refusal: stores are trusted for availability, never integrity.
pub fn read_verified(store: &dyn Store, id: &ContentId) -> Result<Bytes, StoreError> {
    let bytes = store.get(id, ByteRange::Full)?.into_bytes()?;
    let actual = ContentId::of(&bytes);
    if actual != *id {
        continuum_base::probe!(class = "decision", verdict = "digest-mismatch", expected = %id, actual = %actual);
        return Err(StoreError::DigestMismatch {
            expected: *id,
            actual,
        });
    }
    Ok(bytes)
}

/// Copy one blob from `src` to `dst` if `dst` lacks it, verifying the bytes in flight.
pub(crate) fn copy_blob(
    src: &dyn Store,
    dst: &dyn Store,
    id: &ContentId,
) -> Result<Stored, StoreError> {
    match src.get(id, ByteRange::Full)? {
        BlobRead::Bytes(b) => check(dst.put(BlobSource::Bytes(&b))?, id),
        BlobRead::Stream { mut reader, .. } => check(dst.put(BlobSource::Reader(&mut reader))?, id),
    }
}

fn check(stored: Stored, expected: &ContentId) -> Result<Stored, StoreError> {
    if stored.id == *expected {
        Ok(stored)
    } else {
        Err(StoreError::DigestMismatch {
            expected: *expected,
            actual: stored.id,
        })
    }
}

/// Drain any source into `w` (the store's sink, wrapped in a hasher).
pub(crate) fn drain<W: io::Write>(
    src: BlobSource<'_>,
    w: &mut HashingWriter<W>,
) -> Result<(), StoreError> {
    let r = match src {
        BlobSource::Bytes(b) => io::Write::write_all(w, b),
        BlobSource::Reader(r) => io::copy(r, w).map(|_| ()),
        BlobSource::Writer(f) => f(w),
    };
    r.map_err(|source| StoreError::Stream {
        op: "write blob",
        source,
    })
}

/// Read a whole stream (used by `BlobRead::into_bytes`).
pub(crate) fn read_all(mut reader: impl Read, len: u64) -> Result<Bytes, StoreError> {
    let cap = usize::try_from(len).map_err(|_| StoreError::Stream {
        op: "read blob",
        source: io::Error::new(
            io::ErrorKind::OutOfMemory,
            "blob larger than the address space",
        ),
    })?;
    let mut buf = Vec::with_capacity(cap);
    reader
        .read_to_end(&mut buf)
        .map_err(|source| StoreError::Stream {
            op: "read blob",
            source,
        })?;
    Ok(Bytes::from(buf))
}
