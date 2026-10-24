//! Loading: top-down and lazy. The root manifest is read and verified, its kind and schema
//! checked and migrated, its requirements checked; only then does `unpack` see a part, and
//! blobs and handled children are read only when asked for.

use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::blob::Blob;
use crate::error::PackError;
use crate::handle::Handle;
use crate::id::ContentId;
use crate::manifest::{Descriptor, Manifest, Part};
use crate::packable::Packable;
use crate::requires::{Audience, Offers, Requirement};
use crate::store::{read_verified, RefName, Store};

/// A store and what this host offers: everything a load needs besides the kind's context.
pub struct Loader {
    store: Arc<dyn Store>,
    offers: Offers,
}

impl Loader {
    pub fn new(store: Arc<dyn Store>, offers: Offers) -> Arc<Self> {
        Arc::new(Self { store, offers })
    }

    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// Load the package at `id` as a `T`.
    pub fn load<T: Packable>(
        self: &Arc<Self>,
        id: &ContentId,
        cx: &T::Context,
    ) -> Result<T, PackError> {
        let manifest = self.open::<T>(id)?;
        let mut u = Unpacker {
            loader: self,
            manifest,
        };
        T::unpack(&mut u, cx)
    }

    /// Load the package `name` points at.
    pub fn load_ref<T: Packable>(
        self: &Arc<Self>,
        name: &RefName,
        cx: &T::Context,
    ) -> Result<T, PackError> {
        let id = self
            .store
            .get_ref(name)?
            .ok_or_else(|| PackError::RefAbsent(name.clone()))?;
        self.load(&id, cx)
    }

    /// The manifest at `id`, verified, kind-checked, migrated to `T`'s schema, requirements met.
    fn open<T: Packable>(&self, id: &ContentId) -> Result<Manifest, PackError> {
        let bytes = read_verified(self.store.as_ref(), id)?;
        let mut m = Manifest::decode(&bytes)?;
        let kind = T::KIND;
        if m.kind != kind.name {
            return Err(PackError::KindMismatch {
                expected: kind.name,
                found: m.kind,
            });
        }
        if m.schema > kind.schema {
            continuum_base::probe!(
                class = "decision",
                verdict = "refuse-newer-schema",
                kind = kind.name,
                found = m.schema,
                known = kind.schema
            );
            return Err(PackError::UnknownNewerSchema {
                kind: kind.name,
                found: m.schema,
                known: kind.schema,
            });
        }
        while m.schema < kind.schema {
            let from = m.schema;
            let step = T::migrations().iter().find(|g| g.from == from).ok_or(
                PackError::MissingMigration {
                    kind: kind.name,
                    from,
                },
            )?;
            (step.apply)(&mut m)?;
            m.schema = from + 1;
        }
        if let Some((part, requirement)) =
            m.requires.iter().find(|(_, r)| !self.offers.satisfies(r))
        {
            continuum_base::probe!(class = "decision", verdict = "refuse-unmet-requirement", kind = kind.name, part = %part);
            return Err(PackError::UnmetRequirement {
                part: part.clone(),
                requirement: requirement.clone(),
            });
        }
        Ok(m)
    }
}

/// The verbs a type unpacks itself with; the mirror of [`crate::Packer`].
pub struct Unpacker<'l> {
    loader: &'l Arc<Loader>,
    manifest: Manifest,
}

impl Unpacker<'_> {
    /// The (migrated) manifest being unpacked.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// A field. Missing is a typed refusal: a default would hide a schema change that
    /// should have been a migration.
    pub fn field<T: DeserializeOwned>(&self, name: &str) -> Result<T, PackError> {
        let v = self
            .manifest
            .fields
            .get(name)
            .ok_or_else(|| missing(name))?;
        T::deserialize(v).map_err(|source| PackError::Field {
            part: name.to_owned(),
            source,
        })
    }

    /// A blob, lazily: no bytes are read here.
    pub fn blob(&self, name: &str, media: &str) -> Result<Blob, PackError> {
        match self.part(name)? {
            Part::Blob(d) if d.media_type == media => {
                Ok(Blob::stored(self.loader.store().clone(), d.digest, d.size))
            }
            Part::Blob(d) => Err(PackError::WrongMedia {
                part: name.to_owned(),
                expected: media.to_owned(),
                found: d.media_type.clone(),
            }),
            _ => Err(wrong(name, "blob")),
        }
    }

    /// A child, eagerly loaded.
    pub fn child<C: Packable>(&self, name: &str, cx: &C::Context) -> Result<C, PackError> {
        let d = self.one_child(name)?;
        self.loader.load(&d.digest, cx)
    }

    /// An ordered list of children, eagerly loaded.
    pub fn children<C: Packable>(&self, name: &str, cx: &C::Context) -> Result<Vec<C>, PackError> {
        self.many_children(name)?
            .iter()
            .map(|d| self.loader.load(&d.digest, cx))
            .collect()
    }

    /// A child by handle: nothing is read until the handle is loaded.
    pub fn handle<C: Packable>(&self, name: &str) -> Result<Handle<C>, PackError> {
        let d = self.one_child(name)?;
        Ok(Handle::stored(d.clone(), self.loader.clone()))
    }

    /// An ordered list of children by handle.
    pub fn handles<C: Packable>(&self, name: &str) -> Result<Vec<Handle<C>>, PackError> {
        Ok(self
            .many_children(name)?
            .iter()
            .map(|d| Handle::stored(d.clone(), self.loader.clone()))
            .collect())
    }

    /// A recorded requirement (already proven met).
    pub fn requirement(&self, name: &str) -> Result<&Requirement, PackError> {
        self.manifest
            .requires
            .get(name)
            .ok_or_else(|| missing(name))
    }

    /// A sealed part. Opening lands with sealing in slice 5; until then this refuses.
    pub fn secret<T: DeserializeOwned>(
        &self,
        name: &str,
        _audience: &Audience,
    ) -> Result<T, PackError> {
        Err(PackError::SealingUnavailable {
            part: name.to_owned(),
        })
    }

    fn part(&self, name: &str) -> Result<&Part, PackError> {
        self.manifest.parts.get(name).ok_or_else(|| missing(name))
    }

    fn one_child(&self, name: &str) -> Result<&Descriptor, PackError> {
        match self.part(name)? {
            Part::Child(d) => Ok(d),
            _ => Err(wrong(name, "child")),
        }
    }

    fn many_children(&self, name: &str) -> Result<&[Descriptor], PackError> {
        match self.part(name)? {
            Part::Children(ds) => Ok(ds),
            _ => Err(wrong(name, "children")),
        }
    }
}

fn missing(name: &str) -> PackError {
    PackError::MissingPart {
        part: name.to_owned(),
    }
}

fn wrong(name: &str, expected: &'static str) -> PackError {
    PackError::WrongPart {
        part: name.to_owned(),
        expected,
    }
}
