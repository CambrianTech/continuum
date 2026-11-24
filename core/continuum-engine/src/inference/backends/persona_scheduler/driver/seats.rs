//! Who holds the Core's seats (CONTINUUM-ENGINE 4.3). The context's KV is one pool and several minds are
//! seated in it at once, each in place: a switch to a seated mind moves nothing. A mind who is not seated
//! comes in through a free sequence (her blocks paged back in from RAM, or a new mind starting empty).
//! When no sequence is free, or the pool has no cells for her or for the active mind's next step, the
//! seated mind used least recently leaves for RAM and is parked: the only move the pool makes, and only
//! when it is full. The active mind is never evicted for her own room.

use std::collections::HashMap;
use std::time::Instant;

use continuum_persona_engine::engine::{DecodeError, StageError, SwitchError};
use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_llama::Step;

use super::{resident, Basis, Driver, History, Mind, Parts, Tenure, TurnError};

/// When each seated mind last stepped. A mind with no entry (a one-off whose turn ended) is the coldest.
#[derive(Default)]
pub(super) struct Recency(HashMap<PersonaId, Instant>);

impl Recency {
    pub(super) fn used(&mut self, id: PersonaId) {
        self.0.insert(id, Instant::now());
    }

    /// She is done with her seat: the first to leave when the pool needs room.
    pub(super) fn forget(&mut self, id: PersonaId) {
        self.0.remove(&id);
    }

    /// The mind among `seated` used least recently.
    fn coldest(&self, seated: impl Iterator<Item = PersonaId>) -> Option<PersonaId> {
        seated.min_by_key(|id| self.0.get(id).copied())
    }
}

impl Driver<'_, '_> {
    /// Make `id` the active mind. Seated, she is only made active. Otherwise she comes in through a free
    /// sequence, and the seated mind used least recently leaves first when none is free.
    ///
    /// A retirement a refused copy-out left behind is finished first, so it is retried rather than skipped
    /// and the retiring mind is never staged while still in custody (Codex on #6140).
    pub(super) fn switch_to(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        self.retire_previous()?;
        if !self.engine.is_seated(id) {
            self.bring_in(id, one_off)?;
        }
        self.engine.activate(id)?;
        self.recency.used(id);
        Ok(())
    }

    /// Bring `id` into the pool: her parked state paged back in, or a new mind on an empty sequence.
    fn bring_in(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        let mut mind = match self.parked.remove(&id) {
            Some(parked) => parked,
            None => {
                let first = MindId { persona: id, epoch: Epoch(1) };
                let blocks = self.engine.blocks_for(first)?;
                let tenure = if one_off { Tenure::OneOff } else { Tenure::Resident };
                Mind::new(id, first.epoch, Parts { blocks, tokens: History::default(), genome: Basis::default(), learning: (), decode: tenure })
            }
        };
        loop {
            match self.engine.stage(mind) {
                Ok(()) => break,
                Err(refused) => {
                    let wants_room = matches!(&refused.reason, StageError::Tier(tier) if tier.wants_room());
                    mind = refused.returned;
                    if wants_room && self.make_room(id)? {
                        continue;
                    }
                    self.parked.insert(mind.id(), mind);
                    return Err(refused.reason.into());
                }
            }
        }
        self.engine.settle()?;
        self.engine.seat()?;
        Ok(())
    }

    /// Evict the seated mind used least recently, other than the active one and `keep`, and park her.
    /// `false` when no seated mind can leave (the pool holds only the minds that must stay).
    fn make_room(&mut self, keep: PersonaId) -> Result<bool, TurnError> {
        let active = self.engine.active();
        let leaving = self.recency.coldest(self.engine.seated().filter(|seated| Some(*seated) != active && *seated != keep));
        let Some(leaving) = leaving else { return Ok(false) };
        match self.engine.evict(leaving) {
            // her copy-out did not begin: she stays retiring in the engine's custody, retried next
            Ok(()) | Err(SwitchError::CopyOut { .. }) => {}
            Err(refused) => return Err(refused.into()),
        }
        self.recency.forget(leaving);
        self.retire_previous()?;
        continuum_base::probe!(class = "persona.engine.evicted", persona = ?leaving, for_mind = ?keep, "the pool needed room: the seated mind used least recently left for RAM");
        Ok(true)
    }

    /// One step of the active mind. When the pool has no cells for it, a seated mind leaves and the step
    /// runs again; a decode refused for want of room decoded nothing.
    pub(super) fn submit(&mut self, step: Step) -> Result<(), TurnError> {
        loop {
            match self.engine.submit(&step) {
                Err(DecodeError::Tier(refusal)) if refusal.wants_room() => {
                    let active = self.engine.active().ok_or(TurnError::NoneActive)?;
                    if !self.make_room(active)? {
                        return Err(DecodeError::Tier(refusal).into());
                    }
                }
                done => return Ok(done?),
            }
        }
    }

    /// Bring a retirement to its end and park her; a one-off whose turn ended is dropped, blocks and all.
    pub(super) fn retire_previous(&mut self) -> Result<(), TurnError> {
        let Some(previous) = self.engine.finish_retirement()? else { return Ok(()) };
        if resident(&previous) || self.live_one_offs.contains(&previous.id()) {
            self.parked.insert(previous.id(), previous);
        }
        Ok(())
    }
}
