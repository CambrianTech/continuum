//! Who is on the Core: binding a mind, retiring the previous one, her record and her footprint.

use super::*;

impl Driver<'_, '_> {
    /// Make `id` the active mind: the occupant moves out to the RAM tier first and is parked (or
    /// forgotten if she was a one-off), then her parked state comes back (or a new mind starts) into
    /// the stream the occupant emptied. The Core's context is one stream, whole to whoever is in it
    /// (`SchedulerConfig::streams`), so nobody moves in until the occupant is out.
    ///
    /// A retirement a refused copy-out left behind is finished first, so it is retried rather than
    /// skipped and the retiring mind is never staged while still in custody (Codex on #6140). A
    /// refused copy-out keeps the occupant's stream bound: the next mind waits, and the copy-out is
    /// retried at the start of the next turn.
    pub(super) fn switch_to(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        self.retire_previous()?;
        // a mind bound beside the active one (a batch mate) trades places with her: both stay bound, no transfer
        if self.engine.beside().any(|beside| beside == id) {
            return self.engine.swap_active(id).map_err(TurnError::from);
        }
        if self.engine.active() == Some(id) {
            return Ok(());
        }
        if self.engine.active().is_some() {
            match self.engine.retire_active() {
                Ok(_) | Err(HandoffError::CopyOut { .. }) => {}
                Err(refused) => return Err(refused.into()),
            }
            self.retire_previous()?;
        }
        let mind = match self.parked.remove(&id) {
            Some(parked) => parked,
            None => {
                let first = MindId { persona: id, epoch: Epoch(1) };
                let blocks = self.engine.blocks_for(first)?;
                let tenure = if one_off { Tenure::OneOff } else { Tenure::Resident };
                Mind::new(id, first.epoch, Parts { blocks, tokens: History::default(), genome: Basis::default(), learning: (), decode: tenure })
            }
        };
        if let Err(refused) = self.engine.stage(mind) {
            self.parked.insert(refused.returned.id(), refused.returned);
            return Err(refused.reason.into());
        }
        self.engine.settle()?;
        match self.engine.handoff() {
            Ok(_) | Err(HandoffError::CopyOut { .. }) => {}
            Err(refused) => return Err(refused.into()),
        }
        self.retire_previous()
    }

    /// Report that `turn` reached `at`; its record stage writes it.
    pub(super) fn transition(&mut self, turn: &Turn, at: transition::At) {
        self.record.send(record::Fact::Transition(transition::Transition { request: turn.req.request.clone(), persona: turn.id.0, one_off: turn.one_off, at }));
    }

    pub(super) fn finish_one_off(&mut self, turn: &Turn) {
        if turn.one_off {
            // a one-off bound beside gives her stream back: she leaves through the one copy-out, then is forgotten
            if self.engine.beside().any(|beside| beside == turn.id) {
                let left = self.engine.unbind_beside(turn.id).map_err(TurnError::from).and_then(|()| self.engine.finish_retirement().map_err(TurnError::from));
                if let Err(refused) = left {
                    continuum_base::probe!(class = "persona.engine.beside_kept", persona = %turn.id.0, refused = %refused,
                        "a finished one-off bound beside could not leave yet: she keeps her stream until a switch frees it");
                }
            }
            self.parked.remove(&turn.id);
            self.live_one_offs.remove(&turn.id);
            self.lane.observe(Observation::Left { mind: turn.id });
        }
    }

    /// A paused one-off keeps her blocks until she finishes, just like a paused resident.
    pub(super) fn retire_previous(&mut self) -> Result<(), TurnError> {
        let Some(previous) = self.engine.finish_retirement()? else { return Ok(()) };
        if resident(&previous) || self.live_one_offs.contains(&previous.id()) {
            self.parked.insert(previous.id(), previous);
        }
        Ok(())
    }

    /// Update the active mind's own record (what her blocks now hold).
    pub(super) fn record(&mut self, change: impl FnOnce(&mut Parts)) -> Result<(), TurnError> {
        change(self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut());
        Ok(())
    }

    /// Attribute what the active mind holds to her in the footprint registry: her own footprint
    /// (tokens, checkpoint) and her blocks in the context, each asked of its owner (one-offs are not
    /// attributed).
    pub(super) fn report_footprint(&mut self, id: PersonaId, one_off: bool) {
        if one_off {
            return;
        }
        let Some(her) = self.engine.active_mut().filter(|her| her.id() == id) else { return };
        let (mind, own) = (MindId { persona: id, epoch: her.epoch() }, her.parts().tokens.held());
        let blocks = Bound { tier: self.engine.tier(), mind }.held();
        if blocks == Held::NOTHING {
            return;
        }
        let bytes = (blocks + own).total();
        footprint_registry::global().report_authoritative(FootprintKey::for_persona(id.0, ResourceType::KvCache, Residency::Active), bytes);
        // her blocks' bytes for the lane; which of them are cold the block layer does not report yet
        self.lane.observe(Observation::Resident { mind: id, kv_bytes: bytes as u64, cold_bytes: Measured::Unmeasured });
    }
}
