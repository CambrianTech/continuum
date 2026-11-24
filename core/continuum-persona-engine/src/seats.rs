//! [`Seats`]: a Core's minds as residency sees them (CONTINUUM-ENGINE 4.3). The engine's pool seats several
//! minds at once, each in place, and a switch between seated minds moves nothing. A mind not seated is
//! parked here, her blocks in RAM. Bringing her in takes a free sequence; when none is free, or the pool
//! has no cells for her, the seated mind used least recently leaves for RAM and is parked: the only move,
//! and only when the pool is full.
//!
//! Custody holds on every path: a mind being brought in is parked again on any refusal, and a mind
//! evicted stays in the engine's custody until her copy-out lands, then is parked or, when her owner no
//! longer keeps her, dropped.

use std::collections::HashMap;
use std::time::Instant;

use crate::engine::{PersonaEngine, RetireError, StageError, SwitchError};
use crate::persona::{Carries, PersonaId};
use crate::pipeline::Identified;
use crate::residency::{Execute, TierRefusal};

#[cfg(test)]
mod tests;

/// Why a switch or a room-making did not complete. Every mind it was handed is parked or seated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeatsError {
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Switch(#[from] SwitchError),
    #[error(transparent)]
    Retire(#[from] RetireError),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
    /// The pool is full and every seated mind must stay (the one coming in, or the one decoding).
    #[error("no seated mind can leave to make room for {0:?}")]
    NoRoom(PersonaId),
}

/// The minds parked in RAM and when each seated mind last stepped.
pub struct Seats<U> {
    parked: HashMap<PersonaId, U>,
    used: HashMap<PersonaId, Instant>,
}

/// What a Core's owner decides about its minds: whether one is kept once she leaves her seat (parked) or
/// dropped, and how a mind never seen starts.
pub trait Owner<U, T: Execute> {
    fn keeps(&self, mind: &U) -> bool;
    fn start(&self, engine: &mut PersonaEngine<U, T>, id: PersonaId) -> Result<U, TierRefusal>;
}

impl<U: Identified> Seats<U> {
    /// Seats over minds already parked (a restart's resumed minds).
    pub fn new(parked: HashMap<PersonaId, U>) -> Self {
        Self { parked, used: HashMap::new() }
    }

    pub fn parked(&self) -> &HashMap<PersonaId, U> {
        &self.parked
    }

    /// Give up a parked mind (a one-off whose turn ended).
    pub fn unpark(&mut self, id: PersonaId) -> Option<U> {
        self.parked.remove(&id)
    }

    /// She is done with her seat for now: the first to leave when the pool needs room.
    pub fn cooled(&mut self, id: PersonaId) {
        self.used.remove(&id);
    }

    /// Make `id` the active mind. Seated, she is only made active. Otherwise she is brought in, and the
    /// seated minds used least recently leave first while there is no room for her; the active mind may
    /// be one of them, once her decode has landed. Returns who left.
    pub fn switch_to<T>(
        &mut self,
        engine: &mut PersonaEngine<U, T>,
        id: PersonaId,
        owner: &impl Owner<U, T>,
    ) -> Result<Vec<PersonaId>, SeatsError>
    where
        U: Carries<T::Blocks>,
        T: Execute,
    {
        self.retire_previous(engine, owner)?;
        let left = if engine.is_seated(id) { Vec::new() } else { self.bring_in(engine, id, owner)? };
        engine.activate(id)?;
        self.used.insert(id, Instant::now());
        Ok(left)
    }

    /// Room for the active mind's next step: the seated mind used least recently, other than her,
    /// leaves. `None` when she is the only one seated.
    pub fn make_room_during<T>(&mut self, engine: &mut PersonaEngine<U, T>, owner: &impl Owner<U, T>) -> Result<Option<PersonaId>, SeatsError>
    where
        U: Carries<T::Blocks>,
        T: Execute,
    {
        let Some(active) = engine.active() else { return Ok(None) };
        match self.evict_coldest(engine, active, owner) {
            Ok(left) => Ok(Some(left)),
            Err(SeatsError::NoRoom(_)) => Ok(None),
            Err(refused) => Err(refused),
        }
    }

    /// Bring a retirement to its end: park her, or drop her when her owner no longer keeps her.
    pub fn retire_previous<T>(&mut self, engine: &mut PersonaEngine<U, T>, owner: &impl Owner<U, T>) -> Result<(), SeatsError>
    where
        U: Carries<T::Blocks>,
        T: Execute,
    {
        if let Some(previous) = engine.finish_retirement()? {
            if owner.keeps(&previous) {
                self.parked.insert(previous.persona(), previous);
            }
        }
        Ok(())
    }

    /// Her parked state paged in, or a new mind on an empty sequence. On any refusal she is parked.
    fn bring_in<T>(
        &mut self,
        engine: &mut PersonaEngine<U, T>,
        id: PersonaId,
        owner: &impl Owner<U, T>,
    ) -> Result<Vec<PersonaId>, SeatsError>
    where
        U: Carries<T::Blocks>,
        T: Execute,
    {
        let mut mind = match self.parked.remove(&id) {
            Some(parked) => parked,
            None => owner.start(engine, id)?,
        };
        let mut left = Vec::new();
        loop {
            match engine.stage(mind) {
                Ok(()) => break,
                Err(refused) => {
                    mind = refused.returned;
                    let room = match &refused.reason {
                        StageError::Tier(tier) if tier.wants_room() => self.evict_coldest(engine, id, owner),
                        _ => Err(refused.reason.into()),
                    };
                    match room {
                        Ok(gone) => left.push(gone),
                        Err(why) => {
                            self.parked.insert(mind.persona(), mind);
                            return Err(why);
                        }
                    }
                }
            }
        }
        engine.settle()?;
        engine.seat()?;
        Ok(left)
    }

    /// Evict the seated mind used least recently, other than `stays`, and finish her retirement.
    fn evict_coldest<T>(&mut self, engine: &mut PersonaEngine<U, T>, stays: PersonaId, owner: &impl Owner<U, T>) -> Result<PersonaId, SeatsError>
    where
        U: Carries<T::Blocks>,
        T: Execute,
    {
        let leaving = engine
            .seated()
            .filter(|seated| *seated != stays)
            .min_by_key(|seated| self.used.get(seated).copied())
            .ok_or(SeatsError::NoRoom(stays))?;
        match engine.evict(leaving) {
            // her copy-out did not begin: she stays retiring in the engine's custody, finished below or
            // retried by the next move
            Ok(()) | Err(SwitchError::CopyOut { .. }) => {}
            Err(refused) => return Err(refused.into()),
        }
        self.used.remove(&leaving);
        self.retire_previous(engine, owner)?;
        Ok(leaving)
    }
}
