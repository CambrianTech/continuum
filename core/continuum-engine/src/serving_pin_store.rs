//! The operator's serving pin, on disk (cards 3160b3d0 / 9552a01e).
//!
//! `serving/pin` is durable intent — "this host serves THIS model" — but the
//! daemon held it only in a `watch` channel, so every reboot lost it and the
//! boot planner picked whatever fit the outgoing server's memory (a 14B coder,
//! a 27B on two lanes, an embedding model on the 5090 — measured 2026-09-06,
//! five re-pins by hand on the M5 in one day). One saved-state file under the
//! state dir (`state/serving-pin.json`), written on pin, removed on unpin, read
//! ONCE at daemon construction so the very first plan is computed under the pin.
//! Anything but a matched pin (corrupt, another format) boots unpinned, never a
//! guessed model; the saved-state trunk probes what it found.

use std::path::Path;

use continuum_saved_state::{OwnedFile, Retention, SavedState, Singleton, Store};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredServingPin {
    pub model_id: String,
    pub set_at_ms: u64,
}

/// The pin as the saved-state contract keeps it: operator intent, so never rebuildable.
struct Pin;
impl SavedState for Pin {
    const KIND: &'static str = "serving-pin";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one file per node, removed on unpin" };
    type Key = Singleton;
    type Contract = ();
    type Value = StoredServingPin;
    type Medium = OwnedFile;
    /// The file before the envelope was this value: adopted once, rewritten by the next save.
    fn upgrade(found: u32, old: serde_json::Value) -> Option<Self::Value> {
        continuum_saved_state::adopt_bare(found, old)
    }
}

/// The pin file as a VALUE the daemon and the `serving/pin` · `serving/unpin`
/// commands hold — never a path resolved from a global at the moment of the write.
///
/// The global was the bug (2026-09-16): `save()` resolved `~/.continuum` on its own,
/// so the `serving/pin` unit test — which drives the production command — wrote the
/// REAL machine's pin file on every `cargo test`, to whichever model sorts first in
/// the catalog (`AtomicChat/Qwen3.8-Flash-Next-GGUF`). Every test run on every dev
/// box re-pinned an undownloaded model for that box's next boot: the #3955 brick,
/// the M5's "five re-pins by hand in one day", the 18:23Z write ten minutes before
/// the 5090 crashed. A handle has no such reach: production builds it under the
/// continuum home once; a test builds it under a tempdir and cannot touch the box.
#[derive(Debug, Clone)]
pub struct ServingPinStore {
    store: Store<Pin>,
}

impl ServingPinStore {
    /// The one production constructor: the pin file under the continuum home.
    pub fn under_home(home: &Path) -> Self {
        Self { store: Store::at(home.join("state")) }
    }

    /// The stored pin; `None` for anything but a matched one (the trunk probed which).
    pub fn load(&self) -> Option<StoredServingPin> {
        self.store.load(&Singleton, &()).matched().ok()
    }

    /// Persist the pin. A failed save is probed by the trunk and non-fatal: the live pin is set
    /// either way; only the next boot loses it.
    pub fn save(&self, model_id: &str) {
        let pin = StoredServingPin {
            model_id: model_id.to_string(),
            set_at_ms: continuum_base::time::EpochMs::now().millis(), // pre-1970 reads 0: a clock before 1970 is impossible on a running host; 0 only labels the record
        };
        if self.store.save(&Singleton, &(), &pin).is_ok() {
            continuum_base::probe!(class = "serving.pin.stored", model_id = %model_id, "serving pin persisted — survives the next reboot");
        }
    }

    /// Remove the stored pin (unpin). Missing = fine; a failure is probed by the trunk.
    pub fn clear(&self) {
        if self.store.remove(&Singleton).is_ok() {
            continuum_base::probe!(class = "serving.pin.cleared", "stored serving pin removed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (3160b3d0 / 9552a01e): a pin that does not come back after a
    // restart, a corrupt file becoming a model, or an unpin that leaves the pin: every
    // outcome of the saved-state contract for this kind, then clear → unpinned.
    #[test]
    fn a_pin_survives_a_restart_and_a_corrupt_file_never_becomes_a_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pin = StoredServingPin { model_id: "ornith-ai/Ornith-1.5-35B-A3B-GGUF".into(), set_at_ms: 5 };
        continuum_saved_state::round_trip::every_outcome::<Pin>(dir.path(), Singleton, (), None, pin);
        let store = ServingPinStore::under_home(dir.path());
        std::fs::create_dir_all(dir.path().join("state")).expect("state dir");
        std::fs::write(dir.path().join("state/serving-pin.json"), b"{ not json").expect("corrupt");
        assert!(store.load().is_none(), "corrupt = unpinned, not guessed");
        store.save("x/y");
        assert_eq!(store.load().map(|p| p.model_id).as_deref(), Some("x/y"));
        store.clear();
        assert!(store.load().is_none(), "cleared = unpinned");
    }

    // what this catches (2026-09-16): the store reaching for a global home. The
    // production command's unit test wrote the REAL machine's pin file on every
    // `cargo test` because `save()` resolved `~/.continuum` itself. A handle built
    // from a path has no way to do that; this pins the absence of the reach.
    #[test]
    fn the_store_never_resolves_a_home_on_its_own() {
        let src = include_str!("serving_pin_store.rs");
        let production = src.split("#[cfg(test)]").next().unwrap_or(src);
        for reach in [
            "continuum_home",
            "home_dir",
            "CONTINUUM_HOME",
            "CONTINUUM_ROOT",
        ] {
            assert!(
                !production.contains(reach),
                "the pin store must be handed its path, never resolve one: found `{reach}`"
            );
        }
    }
}
