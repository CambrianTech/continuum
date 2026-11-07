//! What an owner declares about one kind of saved state. Implemented once per kind; the trunk
//! ([`crate::Store`]) owns placement, the envelope, validation, the verdict and its probe.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;

/// One kind of state that outlives a process.
pub trait SavedState {
    /// The kind's name: its file stem when it has one file, and the `kind` field of every probe.
    const KIND: &'static str;
    /// The envelope format. Bumped on any incompatible change to `Value`, `Key` or `Contract`; a
    /// file of another format loads as `Newer` or `Older`, never as a guess.
    const FORMAT: u32;
    /// How long a saved value stays valid and what bounds the files.
    const RETENTION: Retention;
    /// Whose state this is. Typed, never a formatted string.
    type Key: Key;
    /// The facts the value is valid under (model revision, engine incarnation, a cursor). Saved
    /// with the value and compared field by field at load: a mismatch names the field. `()` for a
    /// kind whose value holds under any world the node can be in.
    type Contract: Serialize;
    /// The state itself.
    type Value: Serialize + DeserializeOwned;
    /// Who places the bytes. Only [`OwnedFile`] exists in this slice.
    type Medium: Medium;

    /// The one migration seam: a file of format `found` (0 is a file from before the envelope,
    /// `old` the whole file; otherwise `old` is its value) read into the current value, or `None`
    /// to leave it `Older`. Loaded as `Upgraded`, probed, and rewritten at the current format by
    /// the next save; deleted once no node holds a file that old.
    fn upgrade(_found: u32, _old: serde_json::Value) -> Option<Self::Value> {
        None
    }
}

/// An upgrade for a kind whose file before the envelope was exactly its value: adopts format 0,
/// refuses every other. One line in each kind that moved onto the contract without changing shape.
/// Removed by card bf0574a4 once no node reports `state.load.upgraded` over a deploy cycle.
pub fn adopt_bare<V: DeserializeOwned>(found: u32, old: serde_json::Value) -> Option<V> {
    if found != 0 {
        return None;
    }
    serde_json::from_value(old).ok() // disk boundary: a pre-envelope file is the value itself; undecodable stays Older
}

/// A kind whose value can be measured again from nothing: a curve, a rate, a demand the next
/// partition re-records. Only such a kind may read anything but `Matched` as an empty value
/// ([`crate::Store::load_rebuildable`]); the trunk has already probed what was found. Every
/// other kind matches [`crate::Loaded`] itself, so the compiler, not a comment, decides who may
/// start empty.
pub trait Rebuildable: SavedState<Value: Default> {}

/// Whose state a file holds, and the file it lives in under the kind's directory.
pub trait Key: Serialize + DeserializeOwned + PartialEq {
    fn file_name(&self, kind: &'static str) -> String;
}

/// The key of a kind with one value per node: its file is `<kind>.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Singleton;

impl Key for Singleton {
    fn file_name(&self, kind: &'static str) -> String {
        format!("{kind}.json")
    }
}

/// A persona's (or any uuid-identified owner's) state: its file is `<uuid>.json`.
impl Key for uuid::Uuid {
    fn file_name(&self, _kind: &'static str) -> String {
        format!("{self}.json")
    }
}

/// Who places a kind's bytes. Each medium is one store type; a kind names its medium and only
/// that store accepts it.
pub trait Medium {}

/// The core writes the whole value as one small file, replaced atomically on every save.
pub struct OwnedFile;
impl Medium for OwnedFile {}

/// How long a saved value is valid and what bounds its files: declared data, never a constant
/// in the owner's code.
#[derive(Debug, Clone, Copy)]
pub enum Retention {
    /// One file per key, replaced in place: the files are bounded by the keys `bound` names, and
    /// a value never ages out by time alone.
    PerKey { bound: &'static str },
    /// One file per key, and a value older than `max_age()` loads as `Stale`. `max_age` answers
    /// from a measurement (a resident's return gap, a rotation window); `source` names it.
    Aged { bound: &'static str, max_age: fn() -> Duration, source: &'static str },
}

impl Retention {
    /// What bounds the files, for a reader of the registry.
    pub const fn bound(&self) -> &'static str {
        match self {
            Retention::PerKey { bound } | Retention::Aged { bound, .. } => bound,
        }
    }
}
