//! Packing: a value describes itself as parts; the packer stores blobs and children
//! bottom-up and writes the manifest last, so a manifest that exists has every part below it.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::blob::{Blob, BlobSource, BlobState};
use crate::error::PackError;
use crate::handle::Handle;
use crate::manifest::{Descriptor, Manifest, Part, MANIFEST_MEDIA};
use crate::packable::Packable;
use crate::requires::{Audience, Requirement};
use crate::store::{copy_blob, RefName, Store};

/// The verbs a type packs itself with. Every part name is used once, across all verbs.
pub struct Packer<'s> {
    store: &'s dyn Store,
    manifest: Manifest,
    names: BTreeSet<String>,
}

impl<'s> Packer<'s> {
    fn new(store: &'s dyn Store, name: &str, schema: u32) -> Self {
        Self {
            store,
            manifest: Manifest::new(name, schema),
            names: BTreeSet::new(),
        }
    }

    fn claim(&mut self, name: &str) -> Result<String, PackError> {
        if self.names.insert(name.to_owned()) {
            Ok(name.to_owned())
        } else {
            Err(PackError::DuplicatePart(name.to_owned()))
        }
    }

    /// Small structured data, inline in the manifest.
    pub fn field<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<(), PackError> {
        let part = self.claim(name)?;
        let v = serde_json::to_value(value) // boundary: a manifest field is the package's stored/wire format
            .map_err(|source| PackError::Field { part: part.clone(), source })?;
        self.manifest.fields.insert(part, v);
        Ok(())
    }

    /// Large bytes from any source, stored as their own blob and hashed in flight.
    pub fn blob(
        &mut self,
        name: &str,
        media: &str,
        src: BlobSource<'_>,
    ) -> Result<Descriptor, PackError> {
        let part = self.claim(name)?;
        let stored = self.store.put(src)?;
        let d = Descriptor {
            media_type: media.to_owned(),
            digest: stored.id,
            size: stored.size,
        };
        self.manifest.parts.insert(part, Part::Blob(d.clone()));
        Ok(d)
    }

    /// A [`Blob`] value: written if resident, referenced if this store has it, copied (and
    /// verified) from its own store otherwise.
    pub fn blob_value(&mut self, name: &str, media: &str, blob: &Blob) -> Result<(), PackError> {
        let part = self.claim(name)?;
        let (digest, size) = match blob.state() {
            BlobState::Resident { bytes, id } => match id.get() {
                Some(known) if self.store.has(known)? => (*known, bytes.len() as u64),
                _ => {
                    let stored = self.store.put(BlobSource::Bytes(bytes))?;
                    // A resident id already set is the same content's hash; nothing to record.
                    let _ = id.set(stored.id);
                    (stored.id, stored.size)
                }
            },
            BlobState::Stored { store, id, size } => {
                if !self.store.has(&id)? {
                    copy_blob(store.as_ref(), self.store, &id)?;
                }
                (id, size)
            }
        };
        let d = Descriptor {
            media_type: media.to_owned(),
            digest,
            size,
        };
        self.manifest.parts.insert(part, Part::Blob(d));
        Ok(())
    }

    /// Another package, packed eagerly into the same store.
    pub fn child<C: Packable>(&mut self, name: &str, value: &C) -> Result<Descriptor, PackError> {
        let part = self.claim(name)?;
        let d = pack_node(self.store, value)?;
        self.manifest.parts.insert(part, Part::Child(d.clone()));
        Ok(d)
    }

    /// An ordered list of packages.
    pub fn children<'c, C: Packable>(
        &mut self,
        name: &str,
        values: impl IntoIterator<Item = &'c C>,
    ) -> Result<(), PackError> {
        let part = self.claim(name)?;
        let ds = values
            .into_iter()
            .map(|v| pack_node(self.store, v))
            .collect::<Result<Vec<_>, _>>()?;
        self.manifest.parts.insert(part, Part::Children(ds));
        Ok(())
    }

    /// A lazy child: referenced if stored here, otherwise packed (fresh) or copied (stored).
    pub fn handle<C: Packable>(&mut self, name: &str, handle: &Handle<C>) -> Result<(), PackError> {
        let part = self.claim(name)?;
        let d = handle.descriptor_in(self.store)?;
        self.manifest.parts.insert(part, Part::Child(d));
        Ok(())
    }

    /// An ordered list of lazy children.
    pub fn handles<'h, C: Packable>(
        &mut self,
        name: &str,
        handles: impl IntoIterator<Item = &'h Handle<C>>,
    ) -> Result<(), PackError> {
        let part = self.claim(name)?;
        let ds = handles
            .into_iter()
            .map(|h| h.descriptor_in(self.store))
            .collect::<Result<Vec<_>, _>>()?;
        self.manifest.parts.insert(part, Part::Children(ds));
        Ok(())
    }

    /// What a loader must offer before any part of this package is read.
    pub fn requires(&mut self, name: &str, requirement: Requirement) -> Result<(), PackError> {
        let part = self.claim(name)?;
        self.manifest.requires.insert(part, requirement);
        Ok(())
    }

    /// A part only `audience` may read. Sealing lands in slice 5; until then this refuses,
    /// so no secret is ever stored in plaintext.
    pub fn secret<T: Serialize + ?Sized>(
        &mut self,
        name: &str,
        _value: &T,
        _audience: &Audience,
    ) -> Result<(), PackError> {
        let part = self.claim(name)?;
        Err(PackError::SealingUnavailable { part })
    }
}

/// Pack `value` and everything under it into `store`; return its manifest's descriptor.
pub(crate) fn pack_node<T: Packable>(
    store: &dyn Store,
    value: &T,
) -> Result<Descriptor, PackError> {
    let mut p = Packer::new(store, T::KIND.name, T::KIND.schema);
    value.pack(&mut p)?;
    let bytes = p.manifest.encode()?;
    let stored = store.put(BlobSource::Bytes(&bytes))?;
    Ok(Descriptor {
        media_type: MANIFEST_MEDIA.to_owned(),
        digest: stored.id,
        size: stored.size,
    })
}

/// Store `value`'s whole tree; return the root manifest's id. Names nothing: see [`commit`].
pub fn save<T: Packable>(store: &dyn Store, value: &T) -> Result<crate::ContentId, PackError> {
    Ok(pack_node(store, value)?.digest)
}

/// Store `value`'s whole tree, then flip `name` onto its root iff `name` is at `expect`.
/// The flip is the one mutation: a failure anywhere before it leaves the old root current.
pub fn commit<T: Packable>(
    store: &dyn Store,
    name: &RefName,
    value: &T,
    expect: Option<crate::ContentId>,
) -> Result<crate::ContentId, PackError> {
    let root = save(store, value)?;
    store.set_ref(name, root, expect)?;
    continuum_base::probe!(class = "decision", verdict = "commit", kind = T::KIND.name, reference = %name, root = %root);
    Ok(root)
}
