//! This node's measured serving state, `<continuum home>/state/<kind>.json`: the one root every
//! engine kind saved through `continuum_saved_state` lives under (serving rates first; the decode
//! knee, prefill rate and lane footprint join with their observers, controller card 8a983e31),
//! tracked as the `state` disk class.

use continuum_saved_state::{OwnedFile, SavedState, Store};

/// The store for kind `K` under this node's state dir; unrooted when there is no continuum home.
pub(crate) fn store<K: SavedState<Medium = OwnedFile>>() -> Store<K> {
    Store::new(crate::paths::continuum_home().ok().map(|home| home.join("state")))
}
