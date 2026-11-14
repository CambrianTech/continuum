//! The slot one learning step runs on (card 4114a2c6), admitted the way her turn is admitted
//! (card 23069702 part B): `turn_admission::admit_turn` as a keyed dream, never a second path.
//!
//! Learning an example she lived is background work of HER mind, so it yields and never evicts
//! without saving:
//! - her act comes first: while she is mid-act the step is refused (`HER_ACT_FIRST`), holding
//!   nothing;
//! - it holds the ambient serving lane, so a waiting turn is admitted ahead of it;
//! - it takes the endpoint's decode permit (`DecodeBound`), the one aggregate bound every
//!   admission shares;
//! - her slot is leased and pinned: warm, she learns in place on her cached prefix; cold, the
//!   paging plan saves any evictee and restores her page first, exactly as for her turn.
//!
//! The admission binds the engine incarnation it was granted on and is held through the step, so
//! the engine cannot be replaced underneath it. A training POST is not a generation: her page
//! attribution survives only the engine's confirmation that the served tail was restored.

use super::{directory, PageKey, Probed, SlotClass};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::llama_server::LearnOutcome;
use crate::inference::turn_admission::{admit_turn, TurnAdmission, HER_ACT_FIRST};

/// Custody of the slot a learning step runs on, for as long as this value lives.
pub struct LearnSlot {
    slot: u32,
    admission: TurnAdmission,
}

impl LearnSlot {
    /// The engine slot to learn on.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// The step's `POST /learn` is about to be sent: from here on the slot holds whatever the
    /// engine left, so a cancellation, panic or lost answer forgets her residency.
    pub fn dispatching(&mut self) {
        self.admission.dispatching();
    }

    /// The engine answered the step. Only `success` proves the exact served tail is back in the
    /// slot (the fork truncates to her prefix when a step did not begin or finish); any other
    /// answer leaves her residency to be forgotten, and her next turn restores her page.
    pub fn answered(&mut self, outcome: &LearnOutcome) {
        if outcome.success {
            self.admission.served_tail_restored();
        }
    }
}

/// Why no slot is held for a learning step now. Every one defers the step; none is a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnSlotRefusal {
    /// She is mid-act: her act comes first.
    Busy,
    /// The endpoint is not this engine any more (relaunched or replaced since she was seated).
    Replaced,
    /// The endpoint is not admitting work (suspended, unowned, without a pool, or paging refused).
    Unadmitted(String),
}

impl std::fmt::Display for LearnSlotRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(f, "{HER_ACT_FIRST}"),
            Self::Replaced => write!(f, "the endpoint is another engine incarnation now"),
            Self::Unadmitted(why) => write!(f, "the endpoint is not admitting: {why}"),
        }
    }
}

/// Take custody of the slot `persona`'s next learning step runs on, in `engine`. Waits, holding
/// nothing, for the lane, the permit and her slot, exactly as her turn would.
pub async fn learn_slot(engine: EngineIncarnation, persona: uuid::Uuid) -> Result<LearnSlot, LearnSlotRefusal> {
    let key = PageKey::mind(persona).ok_or_else(|| LearnSlotRefusal::Unadmitted("no resident identity".into()))?;
    let root = engine.root_url();
    let pool = match directory().get(&root) {
        Probed::Known(pool) => pool,
        Probed::Absent(_) => return Err(LearnSlotRefusal::Unadmitted("the endpoint keeps no slots".into())),
        Probed::NotAsked => return Err(LearnSlotRefusal::Unadmitted("the endpoint has no installed pool".into())),
    };
    let bound = directory().endpoint(&root).concurrency().bound();
    let client = crate::inference::engine_client();
    let admission = admit_turn(bound, SlotClass::Dream, Some(key), Some(pool), client, &root, 0, std::time::Duration::ZERO)
        .await
        .map_err(|why| if why == HER_ACT_FIRST { LearnSlotRefusal::Busy } else { LearnSlotRefusal::Unadmitted(why) })?;
    if admission.engine_incarnation() != Some((engine.pid, engine.started_s)) {
        return Err(LearnSlotRefusal::Replaced);
    }
    let slot = admission.placed_slot().ok_or_else(|| LearnSlotRefusal::Unadmitted("admitted without placing her page".into()))?;
    Ok(LearnSlot { slot, admission })
}

#[cfg(test)]
mod tests {
    use super::super::MindTurn;
    use super::*;

    /// A pool for `slots` slots at a fresh endpoint, stamped as this test process's engine.
    async fn endpoint(slots: u32) -> (EngineIncarnation, std::sync::Arc<super::super::KvSlotPool>) {
        endpoint_stamped(slots, 0).await
    }

    /// The same, its pool stamped `later` seconds after this process started (another engine).
    async fn endpoint_stamped(slots: u32, later: u64) -> (EngineIncarnation, std::sync::Arc<super::super::KvSlotPool>) {
        let port = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).expect("test: a free port").port();
        let engine = EngineIncarnation::of(std::process::id(), port).expect("test: this process's start time is readable");
        let pool = directory().ensure_pool(&engine.root_url(), slots);
        pool.mark_incarnation((engine.pid, engine.started_s + later));
        (engine, pool)
    }

    fn outcome(success: bool) -> LearnOutcome {
        serde_json::from_value(serde_json::json!({
            "success": success, "committed_steps": i64::from(success), "effects_uncertain": false,
            "refusal": "none", "loss": 0.0, "n_labels": 1, "t_ms": 1.0, "max_chunk_ms": 1.0,
            "restoration_ms": 1.0, "entry_ms": 0.0, "max_graph_ms": 1.0, "max_restore_ms": 1.0, "cleanup_ms": 0.0,
        }))
        .expect("test: a learn outcome")
    }

    // what this catches: a learning step that re-prefills on some slot while her page is warm in
    // another, or that releases the slot to a turn mid-step. Her warm slot is used and held.
    #[tokio::test]
    async fn a_warm_page_is_learned_on_in_place_and_held() {
        let (engine, pool) = endpoint(2).await;
        let persona = uuid::Uuid::new_v4();
        let key = PageKey::mind(persona).expect("test: a mind");
        let slot = pool.lease(key).await.expect("test: leased");
        pool.holders.lock().insert(slot, key);
        let held = learn_slot(engine, persona).await.expect("test: custody");
        assert_eq!(held.slot(), slot);
        assert!(pool.try_acquire_slot(slot).is_none(), "no turn takes the slot mid-step");
        drop(held);
        assert!(pool.try_acquire_slot(slot).is_some(), "released with the custody");
    }

    // what this catches: learning that waits on, or runs beside, her own act. It is refused, holding
    // nothing.
    #[tokio::test]
    async fn her_act_comes_first() {
        let (engine, _pool) = endpoint(2).await;
        let persona = uuid::Uuid::new_v4();
        let _acting = MindTurn::try_begin(PageKey::mind(persona).expect("test: a mind")).expect("test: her act");
        assert_eq!(learn_slot(engine, persona).await.err(), Some(LearnSlotRefusal::Busy));
    }

    // what this catches (NoScratch deleted): a cold page deferred forever on a node without a scratch
    // slot, or placed over another mind's page. Her page is placed by the paging plan on a free slot.
    #[tokio::test]
    async fn a_cold_page_is_placed_by_paging_never_over_a_resident() {
        let (engine, pool) = endpoint(3).await;
        let other = PageKey::mind(uuid::Uuid::new_v4()).expect("test: a mind");
        let theirs = pool.lease(other).await.expect("test: leased");
        pool.holders.lock().insert(theirs, other);
        let persona = uuid::Uuid::new_v4();
        let held = learn_slot(engine, persona).await.expect("test: custody on a free slot");
        assert_ne!(held.slot(), theirs, "never over another mind's page");
        assert_eq!(pool.holders.lock().get(&theirs), Some(&other), "their residency is untouched");
    }

    // what this catches: custody taken at an address whose engine is not the one she was seated on.
    #[tokio::test]
    async fn another_incarnation_at_the_address_is_refused() {
        let (engine, _pool) = endpoint_stamped(2, 1).await;
        assert_eq!(learn_slot(engine, uuid::Uuid::new_v4()).await.err(), Some(LearnSlotRefusal::Replaced));
    }

    // what this catches (Codex on B): a step cancelled or lost after its POST keeping her page
    // attributed to a slot whose KV the engine may have truncated, and an answered step forgetting
    // a page the engine proved it restored. Only `success` keeps it.
    #[tokio::test]
    async fn only_a_successful_answer_keeps_her_residency() {
        for (answer, kept) in [(Some(true), true), (Some(false), false), (None, false)] {
            let (engine, pool) = endpoint(2).await;
            let persona = uuid::Uuid::new_v4();
            let key = PageKey::mind(persona).expect("test: a mind");
            let slot = pool.lease(key).await.expect("test: leased");
            pool.holders.lock().insert(slot, key);
            let mut held = learn_slot(engine, persona).await.expect("test: custody");
            held.dispatching();
            if let Some(success) = answer {
                held.answered(&outcome(success));
            }
            drop(held); // None: cancelled or lost after the POST
            assert_eq!(pool.holders.lock().get(&slot) == Some(&key), kept, "answer {answer:?}");
        }
    }
}
