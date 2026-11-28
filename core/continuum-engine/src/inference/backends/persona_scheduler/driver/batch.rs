//! A batch step: several bound minds' next tokens decoded in one engine step, the lane's `Batch` executed (S6, slice
//! 2b; THE-LANE-ON-SAGE.md, open question 1). The active mind leads; each mate is a mind decoding under the same
//! genes (adapters are per context), bound beside her in a stream of her own, or bound there now from the parked
//! minds. Every mind's step is booked as a single step's is (`Driver::account`): her end checks before the decode and
//! her sample after it are the single step's own (`before_decode`, `after_decode`), from her own output of the one
//! decode.

use super::*;

impl Driver<'_, '_> {
    /// Step `picked` (the active mind first, then her mates) as one decode. Each turn's result is returned with its
    /// index, the active mind's first; a mate that cannot join this step (another phase, other genes, no stream for
    /// her) is left for a later one, unstepped.
    pub(super) fn step_batch(&mut self, turns: &mut [Turn]) -> Vec<(usize, Result<Advanced, TurnError>)> {
        // the lane's buffer, borrowed for this step and handed back: never a copy
        let picked = std::mem::take(&mut self.lane.batch);
        let stepped = self.step_picked(turns, &picked);
        self.lane.batch = picked;
        stepped
    }

    fn step_picked(&mut self, turns: &mut [Turn], picked: &[usize]) -> Vec<(usize, Result<Advanced, TurnError>)> {
        let Some((&lead, mates)) = picked.split_first() else { return Vec::new() };
        let leads = self.engine.active() == Some(turns[lead].id) && matches!(turns[lead].phase, Phase::Decoding { .. });
        if !leads {
            // her switch or her prefill is a single step's: the batch forms once she decodes on the Core
            return vec![(lead, self.step(&mut turns[lead]))];
        }
        let genes = Arc::clone(&turns[lead].genes);
        let mut minds = vec![lead];
        for &mate in mates {
            let turn = &turns[mate];
            if matches!(turn.phase, Phase::Decoding { .. }) && continuum_persona_llama::same_genes(&turn.genes, &genes) && self.bound_beside(turn.id) {
                minds.push(mate);
            }
        }

        let stepping = Instant::now();
        let mut results = Vec::with_capacity(minds.len());
        let mut going: Vec<(usize, i32, Pos, usize)> = Vec::with_capacity(minds.len());
        let mut began: Vec<(usize, Pos)> = Vec::with_capacity(minds.len()); // where each mind's step began, for her account
        for &at in &minds {
            let turn = &mut turns[at];
            let Phase::Decoding { token, pos, generated } = std::mem::replace(&mut turn.phase, Phase::Waiting) else { continue };
            began.push((at, pos));
            match self.before_decode(turn, token, pos, generated) {
                Before::Ends(advanced) => results.push((at, Ok(advanced))),
                Before::Goes(generated) => going.push((at, token, pos, generated)),
            }
        }
        if !going.is_empty() {
            self.decode_together(turns, &going, &mut results);
        }
        for (at, advanced) in &results {
            let Some(&(_, pos)) = began.iter().find(|(began_at, _)| began_at == at) else { continue }; // every result is a mind that began
            self.account(&mut turns[*at], advanced, Kind { moved: Moved::Decode, at: usize::from(pos), switched: false }, stepping);
        }
        results
    }

    /// The one decode for `going` (index, token, position, generated), each her own output in order.
    fn decode_together(&mut self, turns: &mut [Turn], going: &[(usize, i32, Pos, usize)], results: &mut Vec<(usize, Result<Advanced, TurnError>)>) {
        let steps: Vec<(PersonaId, Step)> = going
            .iter()
            .map(|&(at, token, pos, _)| (turns[at].id, Step { tokens: Tokens::One(token), from: pos, logits_at_end: true, genes: Arc::clone(&turns[at].genes) }))
            .collect();
        let work: Vec<(PersonaId, &Step)> = steps.iter().map(|(id, step)| (*id, step)).collect();
        let forward = Instant::now();
        let reused_before = self.engine.tier().graphs_reused();
        if let Err(refused) = self.engine.submit_batch(&work) {
            for &(at, ..) in going {
                results.push((at, Err(refused.clone().into())));
            }
            return;
        }
        let forwarded = forward.elapsed();
        let reused = self.engine.tier().graphs_reused().saturating_sub(reused_before);
        for &(at, token, ..) in going {
            if let Some(her) = self.engine.bound_mut(turns[at].id) {
                her.parts_mut().tokens.held.push(token);
            }
        }
        let landing = Instant::now();
        let settled = self.engine.settle();
        let landed = landing.elapsed();
        for (output, &(at, _, pos, generated)) in going.iter().enumerate() {
            let turn = &mut turns[at];
            // each mind waited for the whole step: it is booked to each as her own
            turn.spent.forward += forwarded;
            turn.spent.gpu += landed;
            turn.spent.graphs_reused += reused;
            turn.spent.batched += u64::from(going.len() > 1);
            let advanced = match &settled {
                Err(refused) => Err(refused.clone().into()),
                Ok(()) => i32::try_from(output).map_err(|_| TurnError::TooLong).and_then(|output| self.after_decode(turn, output, pos, generated)),
            };
            results.push((at, advanced));
        }
    }

    /// Whether `id` is bound beside the active mind, binding her there from the parked minds if she is not and a
    /// stream is free for her. A mind the tier cannot bind (no free stream) is parked again, unchanged.
    fn bound_beside(&mut self, id: PersonaId) -> bool {
        if self.engine.beside().any(|beside| beside == id) {
            return true;
        }
        let Some(parked) = self.parked.remove(&id) else { return false };
        match self.engine.bind_beside(parked) {
            Ok(()) => true,
            Err(refused) => {
                self.parked.insert(id, refused.returned);
                false
            }
        }
    }
}
