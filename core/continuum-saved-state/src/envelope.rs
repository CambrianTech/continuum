//! The one envelope every saved value travels in, and its decode into a [`Loaded`] verdict.

use crate::kind::{Retention, SavedState};
use crate::loaded::{CorruptCause, Loaded};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

/// What is written: the payload and the facts that say whose it is and when it holds.
#[derive(Serialize)]
pub(crate) struct Envelope<'a, K: SavedState> {
    kind: &'static str,
    format: u32,
    /// Numbered per key; a writer that read an older one is refused (`Store::save_over`).
    generation: u64,
    saved_at_ms: u64,
    /// Provenance: the build that wrote it, recorded and never compared (`crate::record_build`).
    build: Option<&'static str>,
    key: &'a K::Key,
    contract: &'a K::Contract,
    value: &'a K::Value,
}

impl<'a, K: SavedState> Envelope<'a, K> {
    pub(crate) fn new(key: &'a K::Key, contract: &'a K::Contract, value: &'a K::Value, generation: u64, saved_at_ms: u64) -> Self {
        Self { kind: K::KIND, format: K::FORMAT, generation, saved_at_ms, build: crate::provenance::build(), key, contract, value }
    }
}

/// The generation of the envelope in `bytes`; 0 for anything that is not one (absent, torn, a file
/// from before the envelope), which the next write supersedes.
pub(crate) fn generation_of(bytes: &[u8]) -> u64 {
    serde_json::from_slice::<Value>(bytes).ok().and_then(|doc| doc.get("generation").and_then(Value::as_u64)).unwrap_or(0) // unwrap_or: not an envelope = generation 0
}

/// The verdict on `bytes` read for `key` under the `live` contract at `now_ms`, and the generation
/// on disk (0 when there is no envelope of this kind).
pub(crate) fn decode<K: SavedState>(bytes: &[u8], key: &K::Key, live: &K::Contract, now_ms: u64) -> (Loaded<K::Value>, u64) {
    let generation = generation_of(bytes);
    let loaded = verdict::<K>(bytes, key, live, now_ms);
    let generation = match loaded {
        Loaded::Corrupt { cause: CorruptCause::OtherKind } | Loaded::Older { .. } | Loaded::Upgraded { .. } => 0,
        _ => generation,
    };
    (loaded, generation)
}

/// Checked in the order a reader can trust: shape, kind, format, key, contract, age, then the
/// value itself.
fn verdict<K: SavedState>(bytes: &[u8], key: &K::Key, live: &K::Contract, now_ms: u64) -> Loaded<K::Value> {
    let mut doc = match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(doc)) => doc,
        // JSON, not an envelope: a file written before the contract.
        Ok(bare) => return older::<K>(0, bare),
        Err(_) => return Loaded::Corrupt { cause: CorruptCause::NotJson },
    };
    let (Some(Value::String(kind)), Some(format)) = (doc.get("kind"), doc.get("format").and_then(Value::as_u64)) else {
        return older::<K>(0, Value::Object(doc));
    };
    if kind != K::KIND {
        return Loaded::Corrupt { cause: CorruptCause::OtherKind };
    }
    let found = u32::try_from(format).unwrap_or(u32::MAX); // unwrap_or: a format past u32 is newer than any build
    if found > K::FORMAT {
        return Loaded::Newer { found, known: K::FORMAT };
    }
    if found < K::FORMAT {
        return older::<K>(found, doc.remove("value").unwrap_or(Value::Null)); // unwrap_or: an envelope with no value upgrades from nothing, which the kind refuses
    }
    let stored_key = doc.remove("key").unwrap_or(Value::Null); // unwrap_or: a missing key decodes as no key and is refused below
    match serde_json::from_value::<K::Key>(stored_key) { // disk boundary: the key field of a saved-state envelope
        Ok(stored) if stored == *key => {}
        Ok(_) => return Loaded::Corrupt { cause: CorruptCause::OtherKey },
        Err(_) => return Loaded::Corrupt { cause: CorruptCause::Undecodable },
    }
    let Ok(live) = serde_json::to_value(live) else { // disk boundary: the live contract in the envelope's own shape, compared field by field
        return Loaded::Corrupt { cause: CorruptCause::Undecodable };
    };
    if let Some(what) = first_divergence(doc.get("contract").unwrap_or(&Value::Null), &live) { // unwrap_or: an envelope with no contract diverges from any live one
        return Loaded::Diverged { what };
    }
    if let Retention::Aged { max_age, .. } = K::RETENTION {
        let max_age = max_age();
        let saved_at = doc.get("saved_at_ms").and_then(Value::as_u64).unwrap_or(0); // unwrap_or: an envelope with no time is as old as the epoch
        let age = Duration::from_millis(now_ms.saturating_sub(saved_at));
        if age >= max_age {
            return Loaded::Stale { age, max_age };
        }
    }
    match doc.remove("value").map(serde_json::from_value::<K::Value>) { // disk boundary: the value field of a saved-state envelope
        Some(Ok(value)) => Loaded::Matched(value),
        Some(Err(_)) | None => Loaded::Corrupt { cause: CorruptCause::Undecodable },
    }
}

/// A file of format `found` (0: before the envelope, `old` the whole file; else its value): the
/// kind's upgrade, or `Older` when it has none.
fn older<K: SavedState>(found: u32, old: Value) -> Loaded<K::Value> {
    match K::upgrade(found, old) {
        Some(value) => Loaded::Upgraded { value, from: found },
        None => Loaded::Older { found, known: K::FORMAT },
    }
}

/// The first field of `live` the `saved` contract disagrees with, by name; `contract` for a
/// contract that is not a struct.
fn first_divergence(saved: &Value, live: &Value) -> Option<String> {
    match (saved, live) {
        (Value::Object(saved), Value::Object(live)) => live
            .iter()
            .find(|(field, value)| saved.get(*field) != Some(value))
            .map(|(field, _)| field.clone())
            .or_else(|| saved.keys().find(|field| !live.contains_key(*field)).cloned()),
        (saved, live) if saved == live => None,
        _ => Some("contract".into()),
    }
}
