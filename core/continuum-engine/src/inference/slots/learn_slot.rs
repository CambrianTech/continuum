//! The slot one learning step runs on (card 4114a2c6): custody taken the way a grant handoff
//! takes it (`grant_handoff::prepare`), never a second pool or registry.
//!
//! Learning an example she lived is background work of HER mind, so it yields and never evicts:
//! - while she is mid-act it does not wait, it is deferred (her act comes first);
//! - when her page is still warm in a slot, it learns there, pinned, so the engine serves the
//!   prompt from her cached prefix and restores her tail after the step;
//! - otherwise it learns on the endpoint's scratch slot (one prefill, nobody's page touched);
//! - with neither, it is deferred: taking a citizen slot would evict another mind's page.
//!
//! The endpoint's admission is held through the step, bound to the engine incarnation the learner
//! seated on, so the engine cannot be replaced underneath it.

use super::{directory, EndpointAdmission, MindTurn, PageKey, SlotPermit, SlotPin};
use crate::inference::engine_residency::EngineIncarnation;

/// Custody of the slot a learning step runs on, for as long as this value lives.
pub struct LearnSlot {
    slot: u32,
    warm: bool,
    // drop order is declaration order: the pin goes before the permit, whose drop wakes waiting
    // citizens, so a woken waiter can evict this page (TurnAdmission's order; Codex on 199214535)
    _pin: Option<SlotPin>,
    _permit: SlotPermit,
    _turn: MindTurn,
    _endpoint: EndpointAdmission,
}

impl LearnSlot {
    /// The engine slot to learn on.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// Whether the slot holds her page (her prefix is served from cache), or is the scratch slot.
    pub fn warm(&self) -> bool {
        self.warm
    }
}

/// Why no slot is held for a learning step now. Every one defers the step; none is a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnSlotRefusal {
    /// She is mid-act, or the slot she would learn on is busy: her act and residents come first.
    Busy,
    /// Her page is not warm and this endpoint keeps no scratch slot: learning would evict a page.
    NoScratch,
    /// The endpoint is not this engine any more (relaunched or replaced since she was seated).
    Replaced,
    /// The endpoint is not admitting work (suspended, unowned or without a pool).
    Unadmitted(String),
}

impl std::fmt::Display for LearnSlotRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(f, "she is mid-act or the slot is busy"),
            Self::NoScratch => write!(f, "her page is not warm and the endpoint keeps no scratch slot"),
            Self::Replaced => write!(f, "the endpoint is another engine incarnation now"),
            Self::Unadmitted(why) => write!(f, "the endpoint is not admitting: {why}"),
        }
    }
}

/// Take custody of the slot `persona`'s next learning step runs on, in `engine`.
pub async fn learn_slot(engine: EngineIncarnation, persona: uuid::Uuid) -> Result<LearnSlot, LearnSlotRefusal> {
    let key = PageKey::mind(persona).ok_or_else(|| LearnSlotRefusal::Unadmitted("no resident identity".into()))?;
    let turn = MindTurn::try_begin(key).ok_or(LearnSlotRefusal::Busy)?;
    let admission = directory().endpoint(&engine.root_url()).admit().await.map_err(LearnSlotRefusal::Unadmitted)?;
    if admission.incarnation() != Some((engine.pid, engine.started_s)) {
        return Err(LearnSlotRefusal::Replaced);
    }
    admission.check_ready().map_err(LearnSlotRefusal::Unadmitted)?;
    let pool = admission.pool.clone().ok_or_else(|| LearnSlotRefusal::Unadmitted("no owned pool".into()))?;
    let warm = pool.holders.lock().iter().find_map(|(slot, resident)| (*resident == key).then_some(*slot));
    let (slot, pin) = match warm {
        Some(slot) => (slot, pool.pin(&key)),
        None => (pool.scratch_slot().ok_or(LearnSlotRefusal::NoScratch)?, None),
    };
    let permit = pool.try_acquire_slot(slot).ok_or(LearnSlotRefusal::Busy)?;
    if warm.is_some() && pool.holders.lock().get(&slot) != Some(&key) {
        return Err(LearnSlotRefusal::Busy); // her page moved before custody: try again next pass
    }
    Ok(LearnSlot { slot, warm: warm.is_some(), _pin: pin, _permit: permit, _turn: turn, _endpoint: admission })
}

#[cfg(test)]
mod tests {
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
        assert_eq!((held.slot(), held.warm()), (slot, true));
        assert!(pool.try_acquire_slot(slot).is_none(), "no turn takes the slot mid-step");
        drop(held);
        assert!(pool.try_acquire_slot(slot).is_some(), "released with the custody");
    }

    // what this catches: learning that waits on, or runs beside, her own act. It defers.
    #[tokio::test]
    async fn her_act_comes_first() {
        let (engine, _pool) = endpoint(2).await;
        let persona = uuid::Uuid::new_v4();
        let _acting = MindTurn::try_begin(PageKey::mind(persona).expect("test: a mind")).expect("test: her act");
        assert_eq!(learn_slot(engine, persona).await.err(), Some(LearnSlotRefusal::Busy));
    }

    // what this catches: a cold learning step evicting another mind's page. Without her page warm
    // it takes the scratch slot, and without one it defers.
    #[tokio::test]
    async fn a_cold_page_takes_scratch_or_defers_never_a_citizen_slot() {
        let (engine, pool) = endpoint(2).await;
        let persona = uuid::Uuid::new_v4();
        match pool.scratch_slot() {
            Some(scratch) => {
                let held = learn_slot(engine, persona).await.expect("test: custody");
                assert_eq!((held.slot(), held.warm()), (scratch, false));
            }
            None => assert_eq!(learn_slot(engine, persona).await.err(), Some(LearnSlotRefusal::NoScratch)),
        }
    }

    // what this catches: custody taken at an address whose engine is not the one she was seated on.
    #[tokio::test]
    async fn another_incarnation_at_the_address_is_refused() {
        let (engine, _pool) = endpoint_stamped(2, 1).await;
        assert_eq!(learn_slot(engine, uuid::Uuid::new_v4()).await.err(), Some(LearnSlotRefusal::Replaced));
    }
}
