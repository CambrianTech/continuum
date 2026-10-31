//! The type's side: what a package of this kind is made of. Usually derived.

use crate::error::PackError;
use crate::manifest::Manifest;
use crate::packer::Packer;
use crate::unpacker::Unpacker;

/// A package kind and the schema this build writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    pub schema: u32,
}

impl Kind {
    pub const fn new(name: &'static str, schema: u32) -> Self {
        Self { name, schema }
    }
}

/// One step of a kind's migration chain: a pure rewrite of a schema-`from` manifest into
/// schema `from + 1` (the chain bumps the number). Run on load, before requirements are
/// checked and before any part is read.
#[derive(Clone, Copy)]
pub struct Migration {
    pub from: u32,
    pub apply: fn(&mut Manifest) -> Result<(), PackError>,
}

/// A type that packs itself into parts and unpacks itself from them.
pub trait Packable: Sized + Send + Sync + 'static {
    const KIND: Kind;
    /// What loading needs from the host (a model, an embedder, keys); `()` for most kinds.
    type Context: ?Sized;

    fn pack(&self, p: &mut Packer<'_>) -> Result<(), PackError>;
    fn unpack(u: &mut Unpacker<'_>, cx: &Self::Context) -> Result<Self, PackError>;

    /// The recorded migrations, one per past schema; empty for a kind still on schema 1.
    fn migrations() -> &'static [Migration] {
        &[]
    }
}
