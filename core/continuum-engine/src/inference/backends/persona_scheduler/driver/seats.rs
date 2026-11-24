//! The driver's side of the Core's seats (CONTINUUM-ENGINE 4.3; the policy and its custody are
//! `continuum_persona_engine::seats::Seats`): this driver as the minds' [`Owner`] (which minds it keeps
//! once they leave a seat, how a new mind starts), and making room when the pool has no cells for the
//! active mind's next step.

use std::collections::HashSet;

use continuum_persona_engine::engine::DecodeError;
use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_engine::seats::Owner;
use continuum_persona_llama::{Step, UnifiedTier};

use super::{resident, Basis, Driver, Engine, History, Mind, Parts, Tenure, TurnError};

/// This driver as the owner of its minds: residents are kept, and a one-off while her turn lives.
struct Minds<'a> {
    live_one_offs: &'a HashSet<PersonaId>,
    /// How a mind never seen starts: a resident, or a one-off dropped when her turn ends.
    tenure: Tenure,
}

impl<'c, 'm> Owner<Mind, UnifiedTier<'c, 'm>> for Minds<'_> {
    fn keeps(&self, her: &Mind) -> bool {
        resident(her) || self.live_one_offs.contains(&her.id())
    }

    fn start(&self, engine: &mut Engine<'c, 'm>, id: PersonaId) -> Result<Mind, TierRefusal> {
        let first = MindId { persona: id, epoch: Epoch(1) };
        let blocks = engine.blocks_for(first)?;
        Ok(Mind::new(id, first.epoch, Parts { blocks, tokens: History::default(), genome: Basis::default(), learning: (), decode: self.tenure }))
    }
}

impl Driver<'_, '_> {
    /// Make `id` the active mind: seated, she is only made active; otherwise she is brought in, the
    /// coldest seated minds leaving while there is no room. A retirement a refused copy-out left behind is
    /// finished first, so it is retried rather than skipped (Codex on #6140).
    pub(super) fn switch_to(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        let minds = Minds { live_one_offs: &self.live_one_offs, tenure: if one_off { Tenure::OneOff } else { Tenure::Resident } };
        for leaving in self.seats.switch_to(&mut self.engine, id, &minds)? {
            evicted(leaving, id);
        }
        Ok(())
    }

    /// One step of the active mind. When the pool has no cells for it, the coldest other seated mind
    /// leaves and the step runs again; a decode refused for want of room decoded nothing.
    pub(super) fn submit(&mut self, step: Step) -> Result<(), TurnError> {
        loop {
            match self.engine.submit(&step) {
                Err(DecodeError::Tier(refusal)) if refusal.wants_room() => {
                    let minds = Minds { live_one_offs: &self.live_one_offs, tenure: Tenure::Resident };
                    let Some(leaving) = self.seats.make_room_during(&mut self.engine, &minds)? else {
                        return Err(DecodeError::Tier(refusal).into());
                    };
                    evicted(leaving, self.engine.active().ok_or(TurnError::NoneActive)?);
                }
                done => return Ok(done?),
            }
        }
    }

    /// Bring a retirement to its end: park her, or drop a one-off whose turn ended.
    pub(super) fn retire_previous(&mut self) -> Result<(), TurnError> {
        let minds = Minds { live_one_offs: &self.live_one_offs, tenure: Tenure::Resident };
        Ok(self.seats.retire_previous(&mut self.engine, &minds)?)
    }
}

fn evicted(leaving: PersonaId, for_mind: PersonaId) {
    continuum_base::probe!(class = "persona.engine.evicted", persona = ?leaving, for_mind = ?for_mind, "the pool needed room: the seated mind used least recently left for RAM");
}
