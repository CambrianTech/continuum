//! Large bytes: what a store takes (`BlobSource`), what it gives back (`BlobRead`), and the
//! value a packed type holds (`Blob`): resident bytes, or a lazy reference read on demand.

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::{Arc, OnceLock};

use bytes::Bytes;

use crate::error::StoreError;
use crate::id::ContentId;
use crate::store::{read_all, read_verified, ByteRange, Store};

/// Bytes on their way into a store. A buffer (or an mmap's slice), a reader, or a writer
/// closure the producer fills (the engine writing a KV page); never copied first.
pub enum BlobSource<'a> {
    Bytes(&'a [u8]),
    Reader(&'a mut (dyn Read + 'a)),
    Writer(&'a mut Fill<'a>),
}

/// A producer that writes a blob's bytes into the store's (hashing) sink.
pub type Fill<'a> = dyn FnMut(&mut dyn Write) -> io::Result<()> + 'a;

/// Bytes on their way out of a store: shared bytes (RAM), or a stream of known length (disk).
pub enum BlobRead {
    Bytes(Bytes),
    Stream {
        len: u64,
        reader: Box<dyn Read + Send>,
    },
}

impl BlobRead {
    /// Everything this read yields, in memory.
    pub fn into_bytes(self) -> Result<Bytes, StoreError> {
        match self {
            BlobRead::Bytes(b) => Ok(b),
            BlobRead::Stream { len, reader } => read_all(reader, len),
        }
    }
}

/// A blob part's value. Resident when built in memory; lazy (an id and a store) when
/// unpacked, so loading a package reads no blob until someone asks for its bytes.
#[derive(Clone)]
pub struct Blob(Arc<Inner>);

enum Inner {
    /// In memory; the id is computed once, on first ask or by the store's in-flight hash.
    Resident {
        bytes: Bytes,
        id: OnceLock<ContentId>,
    },
    /// In a store, known by id and length.
    Stored {
        store: Arc<dyn Store>,
        id: ContentId,
        size: u64,
    },
}

impl Blob {
    /// Bytes held in memory, to be stored when packed.
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self(Arc::new(Inner::Resident {
            bytes: bytes.into(),
            id: OnceLock::new(),
        }))
    }

    pub(crate) fn stored(store: Arc<dyn Store>, id: ContentId, size: u64) -> Self {
        Self(Arc::new(Inner::Stored { store, id, size }))
    }

    /// Length in bytes.
    pub fn len(&self) -> u64 {
        match &*self.0 {
            Inner::Resident { bytes, .. } => bytes.len() as u64,
            Inner::Stored { size, .. } => *size,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the bytes are in memory (false = a lazy reference not yet read).
    pub fn is_resident(&self) -> bool {
        matches!(*self.0, Inner::Resident { .. })
    }

    /// The content id. A resident blob is hashed once, on first ask (packing it sets this
    /// from the store's in-flight hash, so a packed blob is never hashed twice).
    pub fn content_id(&self) -> ContentId {
        match &*self.0 {
            Inner::Resident { bytes, id } => *id.get_or_init(|| ContentId::of(bytes)),
            Inner::Stored { id, .. } => *id,
        }
    }

    /// All the bytes, verified against the id when they come from a store.
    pub fn read_all(&self) -> Result<Bytes, StoreError> {
        match &*self.0 {
            Inner::Resident { bytes, .. } => Ok(bytes.clone()),
            Inner::Stored { store, id, .. } => read_verified(store.as_ref(), id),
        }
    }

    /// A span of the bytes, NOT verified: a span cannot be checked against a whole-blob
    /// digest without reading the whole blob. Use [`Blob::read_all`] where integrity matters.
    pub fn read_range_unverified(&self, start: u64, len: u64) -> Result<Bytes, StoreError> {
        let range = ByteRange::Span { start, len };
        match &*self.0 {
            Inner::Resident { bytes, .. } => {
                let (s, n) = range.within(&self.content_id(), bytes.len() as u64)?;
                // `within` proved s + n <= bytes.len(), so both fit in usize.
                Ok(bytes.slice(s as usize..(s + n) as usize))
            }
            Inner::Stored { store, id, .. } => store.get(id, range)?.into_bytes(),
        }
    }

    /// The id if it is known without hashing (stored, or resident and already hashed).
    pub(crate) fn known_id(&self) -> Option<ContentId> {
        match &*self.0 {
            Inner::Resident { id, .. } => id.get().copied(),
            Inner::Stored { id, .. } => Some(*id),
        }
    }

    pub(crate) fn state(&self) -> BlobState<'_> {
        match &*self.0 {
            Inner::Resident { bytes, id } => BlobState::Resident { bytes, id },
            Inner::Stored { store, id, size } => BlobState::Stored {
                store,
                id: *id,
                size: *size,
            },
        }
    }
}

/// A packer's view of a blob: what it must write, or where it can copy from.
pub(crate) enum BlobState<'b> {
    Resident {
        bytes: &'b Bytes,
        id: &'b OnceLock<ContentId>,
    },
    Stored {
        store: &'b Arc<dyn Store>,
        id: ContentId,
        size: u64,
    },
}

/// Two blobs are equal when their contents are: the id is the content.
impl PartialEq for Blob {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.content_id() == other.content_id()
    }
}

impl Eq for Blob {}

impl fmt::Debug for Blob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Blob")
            .field("len", &self.len())
            .field("resident", &self.is_resident())
            .field("id", &self.known_id())
            .finish()
    }
}
