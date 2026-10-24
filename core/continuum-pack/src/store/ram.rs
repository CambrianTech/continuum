//! `RamStore`: blobs as shared bytes, refs in a map, bounded by a byte capacity the caller
//! sizes. Full is a typed refusal; eviction belongs to the tier that composes stores.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard};

use bytes::Bytes;

use super::{drain, ByteRange, RefName, Store, Stored};
use crate::blob::{BlobRead, BlobSource};
use crate::error::StoreError;
use crate::id::{ContentId, HashingWriter};

pub struct RamStore {
    capacity: u64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    blobs: HashMap<ContentId, Bytes>,
    refs: BTreeMap<RefName, ContentId>,
    bytes: u64,
}

impl RamStore {
    /// A store that holds at most `capacity` bytes of blobs.
    pub fn with_capacity(capacity: u64) -> Self {
        Self {
            capacity,
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, State>, StoreError> {
        self.state.lock().map_err(|_| StoreError::Poisoned)
    }

    /// How many distinct blobs are held.
    pub fn blob_count(&self) -> Result<usize, StoreError> {
        Ok(self.state()?.blobs.len())
    }

    /// How many bytes of blobs are held.
    pub fn bytes(&self) -> Result<u64, StoreError> {
        Ok(self.state()?.bytes)
    }
}

impl Store for RamStore {
    fn put(&self, src: BlobSource<'_>) -> Result<Stored, StoreError> {
        let stored = |id, b: &Bytes| Stored {
            id,
            size: b.len() as u64,
        };
        let bytes = match src {
            // A buffer is hashed and copied once; there is nothing to stream.
            BlobSource::Bytes(b) => {
                let id = ContentId::of(b);
                if let Some(have) = self.state()?.blobs.get(&id) {
                    return Ok(stored(id, have));
                }
                (id, Bytes::copy_from_slice(b))
            }
            other => {
                let mut w = HashingWriter::new(Vec::new());
                drain(other, &mut w)?;
                let (id, _, buf) = w.finish();
                (id, Bytes::from(buf))
            }
        };
        let (id, data) = bytes;
        let mut st = self.state()?;
        if let Some(have) = st.blobs.get(&id) {
            return Ok(stored(id, have));
        }
        let len = data.len() as u64;
        if st.bytes.saturating_add(len) > self.capacity {
            return Err(StoreError::Full {
                capacity: self.capacity,
                needed: len,
            });
        }
        st.bytes += len;
        let out = stored(id, &data);
        st.blobs.insert(id, data);
        Ok(out)
    }

    fn get(&self, id: &ContentId, range: ByteRange) -> Result<BlobRead, StoreError> {
        let st = self.state()?;
        let b = st.blobs.get(id).ok_or(StoreError::NotFound(*id))?;
        let (start, len) = range.within(id, b.len() as u64)?;
        // `within` proved start + len <= b.len(), so both fit in usize.
        Ok(BlobRead::Bytes(
            b.slice(start as usize..(start + len) as usize),
        ))
    }

    fn has(&self, id: &ContentId) -> Result<bool, StoreError> {
        Ok(self.state()?.blobs.contains_key(id))
    }

    fn set_ref(
        &self,
        name: &RefName,
        to: ContentId,
        expect: Option<ContentId>,
    ) -> Result<(), StoreError> {
        let mut st = self.state()?;
        let found = st.refs.get(name).copied();
        if found != expect {
            return Err(StoreError::RefConflict {
                name: name.clone(),
                expected: expect,
                found,
            });
        }
        st.refs.insert(name.clone(), to);
        Ok(())
    }

    fn get_ref(&self, name: &RefName) -> Result<Option<ContentId>, StoreError> {
        Ok(self.state()?.refs.get(name).copied())
    }
}
