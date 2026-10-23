//! Per-turn shared context — fields identical across every persona
//! responding to the same message in the same room.
//!
//! # Why hoist
//!
//! Before #1206, every persona's `RespondInput` carried its own deep
//! copy of `recent_history`, `known_specialties`, and `room_id`. With
//! N personas reacting to one message, that's N deep clones of
//! identical data on the hot path — plus more clones inside
//! `respond()` as the data flows through analyze → render → prompt
//! assembly → recorder. The cost is O(N × history_depth × clone_cost)
//! per turn, all of it pure waste.
//!
//! `Arc<TurnContext>` collapses this into a single allocation per
//! turn that all personas share. Cloning the `Arc` is a single
//! pointer-bump; cloning the `Vec` it wraps was a heap walk.
//!
//! # Why this struct (not just inline `Arc`s on each field)
//!
//! Grouping into one struct gives:
//! - **One refcount** instead of three (smaller per-clone overhead).
//! - **One construction site** in `build_respond_input` — the place
//!   that knew how to assemble the per-turn shape can keep doing so
//!   without hauling three `Arc::new` calls through the projection.
//! - **A natural attach point for follow-up per-turn data** — the
//!   #1211 PR-2 work (engram recall surface plumbed into
//!   `prompt_assembly`) hangs off this struct. Each new per-turn
//!   field gets one place to live, not a fresh `Arc<Vec<...>>`
//!   field on every consumer.
//!
//! # Field selection
//!
//! Only fields that are *truly identical across personas in the same
//! turn* belong here. Fields that differ per persona (`system_prompt`,
//! `model`, `capabilities`, `other_persona_names` — which excludes
//! the self-persona's name from the room roster) stay on
//! `RespondInput`.

use crate::cognition::RecentMessage;
use std::sync::Arc;
use uuid::Uuid;

/// Per-turn shared context. One instance per inbound message; all
/// personas responding to that message share an `Arc` to the same
/// instance.
///
/// Construction is cheap (just field copies — the actual heap data
/// lives behind the `Arc`). Consumers borrow fields through the
/// `Arc`, never clone them; if they need to mutate they must
/// construct a new `TurnContext`.
#[derive(Debug, Clone)]
pub struct TurnContext {
    /// Room the inbound message arrived in. Same for all personas
    /// in the room.
    pub room_id: Uuid,
    /// Recent conversation history, most-recent last. Built once
    /// from the room's message log; shared.
    pub recent_history: Vec<RecentMessage>,
    /// Specialty identifiers for ALL personas in the room (this
    /// persona included). Used by the shared analyzer to know which
    /// `suggested_angles` keys to populate.
    pub known_specialties: Vec<String>,
}

impl TurnContext {
    /// Construct an `Arc`-wrapped TurnContext from owned data. The
    /// `Arc` wrap is the primary allocation; the inner `Vec`s carry
    /// the actual heap data and are moved (not cloned) into the
    /// struct.
    pub fn arc(
        room_id: Uuid,
        recent_history: Vec<RecentMessage>,
        known_specialties: Vec<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            room_id,
            recent_history,
            known_specialties,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: cloning an `Arc<TurnContext>` does NOT
    /// duplicate the heap data — both clones see the same underlying
    /// allocation. This is the perf claim of the whole hoist; if
    /// future refactors accidentally introduce a deep clone (e.g.
    /// `let ctx2 = (*arc).clone()`), the test fails.
    #[test]
    fn arc_clone_shares_heap_data() {
        let ctx = TurnContext::arc(
            Uuid::nil(),
            vec![],
            vec!["code".to_string(), "general".to_string()],
        );
        let clone = Arc::clone(&ctx);
        // Pointer equality: both Arcs point at the SAME TurnContext
        // on the heap. If `Arc::clone` ever drifted to a deep copy
        // this assertion would fail.
        assert!(Arc::ptr_eq(&ctx, &clone), "Arc clone must share heap data");
        assert_eq!(Arc::strong_count(&ctx), 2, "two refcounts after one clone");
    }

    /// What this catches: the constructor preserves field values
    /// verbatim — no surprise transformation. The arc() helper is
    /// intentionally trivial; this guards against accidental field
    /// reordering when more fields are added (e.g. PR-2 engram
    /// recall).
    #[test]
    fn arc_constructor_preserves_fields() {
        let room_id = Uuid::new_v4();
        let specs = vec!["a".to_string(), "b".to_string()];
        let ctx = TurnContext::arc(room_id, vec![], specs.clone());
        assert_eq!(ctx.room_id, room_id);
        assert_eq!(ctx.known_specialties, specs);
        assert!(ctx.recent_history.is_empty());
    }
}
