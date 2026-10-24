//! `FaultStore`: wraps any store, logs every operation, and fails the Nth one (before or after
//! delegating it) or flips a byte in reads of one blob. Compiled only for tests and the
//! `conformance` feature.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::{Bytes, BytesMut};

use super::{ByteRange, RefName, Store, Stored};
use crate::blob::{BlobRead, BlobSource};
use crate::error::StoreError;
use crate::id::ContentId;

/// One logged operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Put,
    Get(ContentId),
    Has(ContentId),
    SetRef,
    GetRef,
}

/// Where the injected fault lands relative to the real operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultMode {
    /// The operation never reaches the inner store.
    Before,
    /// The operation completes, then reports failure (a sync that failed after a rename).
    After,
}

pub struct FaultStore {
    inner: Arc<dyn Store>,
    fail_at: Option<(u64, FaultMode)>,
    tamper: Option<ContentId>,
    count: AtomicU64,
    ops: Mutex<Vec<Op>>,
}

impl FaultStore {
    /// A pass-through that only logs.
    pub fn new(inner: Arc<dyn Store>) -> Self {
        Self {
            inner,
            fail_at: None,
            tamper: None,
            count: AtomicU64::new(0),
            ops: Mutex::new(Vec::new()),
        }
    }

    /// Fail operation number `n` (1-based).
    pub fn failing_at(inner: Arc<dyn Store>, n: u64, mode: FaultMode) -> Self {
        Self {
            fail_at: Some((n, mode)),
            ..Self::new(inner)
        }
    }

    /// Flip one byte of every read of `id`.
    pub fn tampering(inner: Arc<dyn Store>, id: ContentId) -> Self {
        Self {
            tamper: Some(id),
            ..Self::new(inner)
        }
    }

    /// The operations so far, in order.
    pub fn ops(&self) -> Result<Vec<Op>, StoreError> {
        Ok(self.ops.lock().map_err(|_| StoreError::Poisoned)?.clone())
    }

    fn run<T>(
        &self,
        op: Op,
        name: &'static str,
        f: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.ops.lock().map_err(|_| StoreError::Poisoned)?.push(op);
        let at = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        match self.fail_at {
            Some((n, FaultMode::Before)) if n == at => Err(StoreError::Injected { op: name, at }),
            Some((n, FaultMode::After)) if n == at => {
                f().and(Err(StoreError::Injected { op: name, at }))
            }
            _ => f(),
        }
    }
}

impl Store for FaultStore {
    fn put(&self, src: BlobSource<'_>) -> Result<Stored, StoreError> {
        self.run(Op::Put, "put", || self.inner.put(src))
    }

    fn get(&self, id: &ContentId, range: ByteRange) -> Result<BlobRead, StoreError> {
        self.run(Op::Get(*id), "get", || {
            let read = self.inner.get(id, range)?;
            if self.tamper != Some(*id) {
                return Ok(read);
            }
            let mut b = BytesMut::from(&read.into_bytes()?[..]);
            if let Some(first) = b.first_mut() {
                *first ^= 0x01;
            }
            Ok(BlobRead::Bytes(Bytes::from(b)))
        })
    }

    fn has(&self, id: &ContentId) -> Result<bool, StoreError> {
        self.run(Op::Has(*id), "has", || self.inner.has(id))
    }

    fn set_ref(
        &self,
        name: &RefName,
        to: ContentId,
        expect: Option<ContentId>,
    ) -> Result<(), StoreError> {
        self.run(Op::SetRef, "set_ref", || {
            self.inner.set_ref(name, to, expect)
        })
    }

    fn get_ref(&self, name: &RefName) -> Result<Option<ContentId>, StoreError> {
        self.run(Op::GetRef, "get_ref", || self.inner.get_ref(name))
    }
}
