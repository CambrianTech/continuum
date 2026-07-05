//! Per-citizen substrate scoping for PER-USER view kinds (nav slice: scoping A).
//!
//! The node's one shared [`Substrate`] holds per-ROOM kinds — `chat`, `wall`,
//! `kanban` — because those describe *the room*, and every session viewing that
//! room reads the same envelope. But a per-USER view (`nav`, and later settings,
//! the costume-per-activity pick) is different: two citizens' nav can't share one
//! `kind="nav"` slot without overwriting each other (the cache is keyed by kind
//! alone). So per-user kinds get a **citizen-scoped substrate**, handed out here.
//!
//! ## Citizen-agnostic BY CONSTRUCTION
//!
//! There is exactly one method, [`PerUserSubstrates::for_citizen`], and it takes a
//! citizen id — a `Uuid`. It does NOT know or care whether that citizen is a human
//! at a browser or a persona like Asha: a human session and a persona session take
//! the IDENTICAL path to their own nav substrate. That's `[[persona-is-a-client]]`
//! made literal — the scoping code has no is-human branch, so the two can never
//! drift apart. Minimizing the human/persona gap starts with refusing to encode it.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::Substrate;

/// A registry of per-citizen substrates for per-user view kinds. One substrate
/// per citizen, created on first use. Per-room kinds do NOT come here — they stay
/// on the node's shared substrate.
#[derive(Default)]
pub struct PerUserSubstrates {
    by_citizen: Mutex<HashMap<Uuid, Substrate>>,
}

impl PerUserSubstrates {
    pub fn new() -> Self {
        Self::default()
    }

    /// The citizen's own substrate, created on first use. Returns a clone —
    /// [`Substrate`] is `Arc`-shared, so the clone points at the SAME underlying
    /// cache/broadcast; the projector that writes this citizen's nav and the
    /// session that reads it get the same store. Human or persona: identical path.
    pub fn for_citizen(&self, citizen: Uuid) -> Substrate {
        self.by_citizen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(citizen)
            .or_insert_with(Substrate::new)
            .clone()
    }

    /// How many citizens have a per-user substrate. Ops/telemetry read.
    pub fn citizen_count(&self) -> usize {
        self.by_citizen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateBuilder;

    // A minimal per-user view to store — we only need something with a kind to
    // prove the per-citizen isolation, so a tiny local ViewState stands in.
    #[derive(Debug, Clone, serde::Serialize)]
    struct TinyNav {
        current: String,
    }
    impl positron_core::ViewState for TinyNav {
        fn kind(&self) -> &'static str {
            "nav"
        }
    }

    // what this catches: two handles for the SAME citizen share one underlying
    // store (write via one, read via the other) — so the projector and the session
    // agree — while DIFFERENT citizens are isolated (no nav collision under one kind).
    #[test]
    fn same_citizen_shares_store_different_citizens_isolated() {
        let reg = PerUserSubstrates::new();
        let asha = Uuid::from_u128(0xa54a);
        let joel = Uuid::from_u128(0x101);

        // Two handles for Asha — the projector's and the session's.
        let asha_writer = reg.for_citizen(asha);
        let asha_reader = reg.for_citizen(asha);
        asha_writer.store(StateBuilder::standalone().session(TinyNav {
            current: "room-a".into(),
        }));
        assert!(
            asha_reader.cache().get("nav").is_some(),
            "same citizen: a write on one handle is visible on the other (shared store)"
        );

        // Joel's substrate is a different store — Asha's nav does not leak in.
        let joel_reader = reg.for_citizen(joel);
        assert!(
            joel_reader.cache().get("nav").is_none(),
            "different citizens are isolated — no nav collision under kind=nav"
        );
        assert_eq!(reg.citizen_count(), 2);
    }

    // what this catches: the API is citizen-agnostic — a persona id and a human id
    // are both just Uuids down the identical path; nothing branches on who they are.
    #[test]
    fn human_and_persona_take_the_identical_path() {
        let reg = PerUserSubstrates::new();
        let human = Uuid::from_u128(1);
        let persona = Uuid::from_u128(2);
        // Same call, same type, same behaviour — no is-human branch exists to test,
        // which is the point: both get an isolated substrate the same way.
        let _h = reg.for_citizen(human);
        let _p = reg.for_citizen(persona);
        assert_eq!(reg.citizen_count(), 2);
    }
}
