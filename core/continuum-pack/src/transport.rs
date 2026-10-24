//! The DAG between stores: walk from a root, verify or copy what is missing. Kind-agnostic,
//! so one function moves any package between any two stores.
//!
//! Invariant every writer keeps: a manifest is written after every part below it, so a store
//! that HAS a manifest has its whole subtree. `copy` relies on it to skip present subtrees.

use std::collections::HashSet;

use crate::blob::BlobSource;
use crate::error::{PackError, StoreError};
use crate::id::ContentId;
use crate::manifest::Manifest;
use crate::store::{copy_blob, read_verified, Store};

/// The shape of a tree: references counted with multiplicity, nodes counted once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeStats {
    pub references: u64,
    pub distinct: u64,
    pub bytes: u64,
}

/// Read every node under `root` and prove each hashes to its id.
pub fn verify(store: &dyn Store, root: &ContentId) -> Result<TreeStats, PackError> {
    let mut stats = TreeStats {
        references: 1,
        ..TreeStats::default()
    };
    let mut seen = HashSet::new();
    verify_node(store, root, &mut seen, &mut stats)?;
    Ok(stats)
}

fn verify_node(
    store: &dyn Store,
    id: &ContentId,
    seen: &mut HashSet<ContentId>,
    stats: &mut TreeStats,
) -> Result<(), PackError> {
    if !seen.insert(*id) {
        return Ok(());
    }
    let bytes = read_verified(store, id)?;
    stats.distinct += 1;
    stats.bytes += bytes.len() as u64;
    let m = Manifest::decode(&bytes)?;
    for (is_child, d) in m.links() {
        stats.references += 1;
        if is_child {
            verify_node(store, &d.digest, seen, stats)?;
        } else if seen.insert(d.digest) {
            stats.distinct += 1;
            stats.bytes += read_verified(store, &d.digest)?.len() as u64;
        }
    }
    Ok(())
}

/// Copy the tree at `root` from `src` to `dst`, skipping whatever `dst` has, children before
/// parents, every byte re-hashed on arrival. Returns how many nodes were written.
pub fn copy(src: &dyn Store, dst: &dyn Store, root: &ContentId) -> Result<u64, PackError> {
    if dst.has(root)? {
        return Ok(0);
    }
    let bytes = read_verified(src, root)?;
    let m = Manifest::decode(&bytes)?;
    let mut written = 0;
    for (is_child, d) in m.links() {
        if is_child {
            written += copy(src, dst, &d.digest)?;
        } else if !dst.has(&d.digest)? {
            copy_blob(src, dst, &d.digest)?;
            written += 1;
        }
    }
    let stored = dst.put(BlobSource::Bytes(&bytes))?;
    if stored.id != *root {
        return Err(StoreError::DigestMismatch {
            expected: *root,
            actual: stored.id,
        }
        .into());
    }
    Ok(written + 1)
}
