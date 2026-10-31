//! Packages: one load/store for every layer, any depth (docs/architecture/PACKAGES.md).
//!
//! A type says WHAT it is ([`Packable`], usually `#[derive(Packable)]`); a store says WHERE
//! bytes go ([`Store`]). A package is an RFC 8785 (JCS) manifest whose parts are inline fields,
//! content-addressed blobs and child packages, so a tree is a Merkle DAG: one root id commits
//! to every byte below it, identical subtrees are stored once, and a save of any depth
//! commits by ONE ref flip ([`commit`]). Loads are lazy ([`Handle`], [`Blob`]) and every byte
//! read is verified against the id that named it.
//!
//! A base-layer crate: it depends on `continuum-base` and `continuum-host` and nothing else in
//! Continuum, so every crate above may use it with one line.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

// The derive emits `::continuum_pack::` paths; this lets them resolve inside this crate too.
extern crate self as continuum_pack;

mod blob;
mod canonical;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod error;
mod handle;
mod id;
mod manifest;
mod packable;
mod packer;
mod requires;
mod store;
mod transport;
mod unpacker;

pub use blob::{Blob, BlobRead, BlobSource, Fill};
pub use canonical::{canonicalize, CanonicalError};
pub use continuum_pack_derive::Packable;
pub use error::{PackError, StoreError};
pub use handle::{ChildPart, Handle};
pub use id::{ContentId, IdError};
pub use manifest::{Descriptor, Manifest, Part, FORMAT, MANIFEST_MEDIA, PROVENANCE_MEDIA};
pub use packable::{Kind, Migration, Packable};
pub use packer::{commit, save, Packer};
pub use requires::{Audience, Offers, Required, Requirement};
pub use store::{read_verified, ByteRange, DiskStore, RamStore, RefName, Store, Stored};
#[cfg(any(test, feature = "conformance"))]
pub use store::{FaultMode, FaultStore, Op};
pub use transport::{copy, verify, TreeStats};
pub use unpacker::{Loader, Unpacker};

#[cfg(test)]
mod tests;
