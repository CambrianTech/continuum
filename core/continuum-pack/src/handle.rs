//! `Handle<T>`: a child package by reference. Fresh (a value to be packed) or stored (an id
//! and a loader); a stored handle reads nothing until `.load()`.

use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::error::PackError;
use crate::id::ContentId;
use crate::manifest::Descriptor;
use crate::packable::Packable;
use crate::packer::{pack_node, Packer};
use crate::store::Store;
use crate::transport;
use crate::unpacker::{Loader, Unpacker};

/// A lazily loaded child. Clones share one state, so a child held by many parents is packed
/// once and loaded once.
pub struct Handle<T: Packable>(Arc<State<T>>);

enum State<T: Packable> {
    Fresh {
        value: Arc<T>,
        packed: OnceLock<Descriptor>,
    },
    Stored {
        at: Descriptor,
        loader: Arc<Loader>,
        loaded: OnceLock<Arc<T>>,
    },
}

impl<T: Packable> Handle<T> {
    /// A value not yet stored; packing the parent packs it.
    pub fn new(value: T) -> Self {
        Self(Arc::new(State::Fresh {
            value: Arc::new(value),
            packed: OnceLock::new(),
        }))
    }

    pub(crate) fn stored(at: Descriptor, loader: Arc<Loader>) -> Self {
        Self(Arc::new(State::Stored {
            at,
            loader,
            loaded: OnceLock::new(),
        }))
    }

    /// The child's id, once it is stored (always, for a handle that came from a load).
    pub fn id(&self) -> Option<ContentId> {
        match &*self.0 {
            State::Fresh { packed, .. } => packed.get().map(|d| d.digest),
            State::Stored { at, .. } => Some(at.digest),
        }
    }

    /// Whether the value is in memory (a fresh value, or a stored one already loaded).
    pub fn is_loaded(&self) -> bool {
        match &*self.0 {
            State::Fresh { .. } => true,
            State::Stored { loaded, .. } => loaded.get().is_some(),
        }
    }

    /// The value: fetched, verified and unpacked on first use, then shared.
    pub fn load(&self, cx: &T::Context) -> Result<Arc<T>, PackError> {
        match &*self.0 {
            State::Fresh { value, .. } => Ok(value.clone()),
            State::Stored { at, loader, loaded } => {
                if let Some(v) = loaded.get() {
                    return Ok(v.clone());
                }
                let v = Arc::new(loader.load::<T>(&at.digest, cx)?);
                // A concurrent first load set the same content; either Arc is the value.
                Ok(loaded.get_or_init(|| v).clone())
            }
        }
    }

    /// This child's descriptor in `store`: referenced if present, else packed or copied.
    pub(crate) fn descriptor_in(&self, store: &dyn Store) -> Result<Descriptor, PackError> {
        match &*self.0 {
            State::Fresh { value, packed } => {
                if let Some(d) = packed.get() {
                    if store.has(&d.digest)? {
                        return Ok(d.clone());
                    }
                }
                let d = pack_node(store, value.as_ref())?;
                Ok(packed.get_or_init(|| d).clone())
            }
            State::Stored { at, loader, .. } => {
                if !store.has(&at.digest)? {
                    transport::copy(loader.store().as_ref(), store, &at.digest)?;
                }
                Ok(at.clone())
            }
        }
    }
}

impl<T: Packable> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Packable> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle")
            .field("kind", &T::KIND.name)
            .field("id", &self.id())
            .field("loaded", &self.is_loaded())
            .finish()
    }
}

/// A field type the derive can store as a child part (`#[pack(child)]`).
pub trait ChildPart: Sized {
    fn pack_part(&self, name: &str, p: &mut Packer<'_>) -> Result<(), PackError>;
    fn unpack_part(name: &str, u: &mut Unpacker<'_>) -> Result<Self, PackError>;
}

impl<T: Packable> ChildPart for Handle<T> {
    fn pack_part(&self, name: &str, p: &mut Packer<'_>) -> Result<(), PackError> {
        p.handle(name, self)
    }

    fn unpack_part(name: &str, u: &mut Unpacker<'_>) -> Result<Self, PackError> {
        u.handle(name)
    }
}

impl<T: Packable> ChildPart for Vec<Handle<T>> {
    fn pack_part(&self, name: &str, p: &mut Packer<'_>) -> Result<(), PackError> {
        p.handles(name, self.iter())
    }

    fn unpack_part(name: &str, u: &mut Unpacker<'_>) -> Result<Self, PackError> {
        u.handles(name)
    }
}
