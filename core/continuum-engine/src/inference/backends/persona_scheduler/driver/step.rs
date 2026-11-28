//! One step of a turn: her resume to the prefix she holds, her checkpoints, a prefill chunk or a decoded token.

use super::*;

impl Driver<'_, '_> {
    /// One frame for `turn`: bind her if another mind is bound, then one step (a prefill chunk, or
    /// one token).
    pub(super) fn step(&mut self, turn: &mut Turn) -> Result<Advanced, TurnError> {
        if turn.req.response_tx.is_closed() {
            return Err(TurnError::CallerReleased);
        }
        let gap = turn.ready_since.elapsed();
        turn.spent.longest_gap = turn.spent.longest_gap.max(gap);
        if turn.spent.first_step.is_none() {
            turn.spent.first_step = Some(turn.submitted.elapsed());
        }
        let switched = self.engine.active() != Some(turn.id);
        if switched {
            let switching = Instant::now();
            self.switch_to(turn.id, turn.one_off)?;
            let took = switching.elapsed();
            self.lane.observe(Observation::SwitchedIn { mind: turn.id, took });
            turn.spent.switch += took;
            turn.spent.switches += 1;
        }
        if matches!(turn.phase, Phase::Waiting) {
            let from = self.resume(turn)?;
            self.lane.observe(Observation::Reused { mind: turn.id, reused: from as u64, prompt: turn.prompt.len() as u64 });
            if from >= turn.prompt.len() {
                return Err(TurnError::NothingDecoded);
            }
            turn.phase = Phase::Prefilling { pos: from };
            if turn.spent.milestones.start() {
                let since_admit_ms = record::whole_ms(turn.submitted.elapsed());
                let started = transition::At::FrameStarted { since_admit_ms, reused_tokens: from, prompt_tokens: turn.prompt.len(), switched: turn.spent.switches > 0 };
                self.transition(turn, started);
            }
        }
        let stepping = Instant::now();
        let (advanced, decoding, prefill_from, at) = match std::mem::replace(&mut turn.phase, Phase::Waiting) { // at: where the step begins
            Phase::Waiting => (Err(TurnError::NothingDecoded), false, None, 0), // moves nothing, so its position is never read
            Phase::Prefilling { pos } => (self.prefill_chunk(turn, pos).map(|()| Advanced::Continues), false, Some(pos), pos),
            Phase::Decoding { token, pos, generated } => (self.decode_token(turn, token, pos, generated), true, None, usize::from(pos)),
        };
        let stepped = Instant::now();
        turn.ready_since = stepped;
        let took = stepped.saturating_duration_since(stepping);
        // what the step moved: one token decoded, or the prefill chunk up to where she now stands
        let tokens = match (prefill_from, &turn.phase) {
            (Some(from), Phase::Prefilling { pos }) => pos.saturating_sub(from),
            (Some(from), Phase::Decoding { .. }) => turn.prompt.len().saturating_sub(from),
            (Some(_), Phase::Waiting) | (None, _) => usize::from(decoding && advanced.is_ok()),
        };
        self.lane.observe(Observation::Stepped { mind: turn.id, took, tokens: tokens as u64, at: at as u64, prefill: !decoding, switched });
        if let Ok(Advanced::Done { tokens_generated, .. }) = &advanced {
            self.lane.observe(Observation::TurnEnded { mind: turn.id, decoded: *tokens_generated as u64 });
        }
        if decoding {
            turn.spent.decode += took;
        } else {
            turn.spent.prefill += took;
        }
        prefill_rate::observe(turn, prefill_from);
        self.record.send(record::Fact::Busy { started: stepping, ended: stepped });
        if turn.spent.milestones.step(advanced.is_ok()) {
            let progress = transition::At::FirstProgress { since_admit_ms: record::whole_ms(turn.submitted.elapsed()), step_ms: record::whole_ms(took), prefill: !decoding };
            self.transition(turn, progress);
        }
        if let Ok(Advanced::Done { .. }) = &advanced {
            self.report_footprint(turn.id, turn.one_off);
        }
        advanced
    }

    /// Where her prefill starts once bound (`reuse::choose`): what she holds when the prompt extends it, her
    /// checkpoint when it keeps her view and changes what follows, else zero for her probe row's reason.
    pub(super) fn resume(&mut self, turn: &mut Turn) -> Result<usize, TurnError> {
        let basis = turn.basis.clone();
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        let reuse::Decision { shared, action: decided } = reuse_for(her, &turn.prompt, &basis);
        turn.spent.shared_prefix_tokens = shared;
        divergence::probe(self.model, turn.id, &her.parts().tokens.held, &turn.prompt, shared);
        // Prefill crosses this matching prefix again: checkpoint there, for the next turn's use.
        let can_learn = match decided {
            reuse::Reuse::Rewind { from } => shared > from,
            reuse::Reuse::Restart(reuse::Miss::Diverged | reuse::Miss::NoCheckpoint) => shared > 0,
            _ => false,
        };
        if can_learn && her.parts().genome == basis && turn.boundaries.view.is_some_and(|view| shared < view) {
            turn.boundaries.learned = Some(shared);
        }
        let (from, start) = match decided {
            reuse::Reuse::Append { from } => (from, "append"),
            reuse::Reuse::Rewind { from } => match self.rewind_to_checkpoint(from) {
                Ok(()) => (from, "rewind"),
                Err(error) => {
                    continuum_base::probe!(class = "persona.engine.rewind_failed", persona = %turn.id.0, checkpoint_end = from as u64,
                        shared_tokens = shared as u64, error = %error, "native rewind failed; turn must prefill");
                    (0, "rewind_refused")
                }
            },
            reuse::Reuse::Restart(miss) => (0, miss.reason()),
        };
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        her.parts_mut().tokens.cut(from);
        her.parts_mut().genome = basis;
        turn.spent.reused = from;
        turn.spent.start = start;
        Ok(from)
    }

    /// Rewind the active mind to her kept checkpoint (taken out and put back, so the engine can read
    /// it while it moves her).
    pub(super) fn rewind_to_checkpoint(&mut self, end: usize) -> Result<(), TurnError> {
        let snapshot = self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut().tokens.take_at(end).ok_or(TurnError::NoneActive)?;
        let rewound = self.engine.rewind_active(snapshot.blocks());
        self.record(|her| her.tokens.put_back(snapshot))?;
        Ok(rewound?)
    }

    /// Checkpoint the active mind at her view's end (`end` tokens held), reusing her checkpoint's
    /// buffers. A refused take keeps none, so a later act restarts instead of rewinding to a stale one.
    pub(super) fn keep_checkpoint(&mut self, turn: &mut Turn, kind: Boundary, end: usize) -> Result<(), TurnError> {
        let taking = Instant::now();
        let history = &mut self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut().tokens;
        let Some(mut blocks) = history.capture_buffer(kind, end) else {
            continuum_base::probe!(class = "persona.engine.checkpoint_allocation_failed", persona = %turn.id.0,
                expected_end = end as u64, "native checkpoint buffer allocation failed");
            turn.spent.checkpoint_ms += record::whole_ms(taking.elapsed());
            return Ok(());
        };
        match self.engine.checkpoint_active(&mut blocks) {
            Ok(pos) if pos as usize + 1 == end => {
                self.record(|her| her.tokens.save(kind, blocks, end, &turn.basis))?;
            }
            Ok(pos) => continuum_base::probe!(class = "persona.engine.checkpoint_misaligned", persona = %turn.id.0,
                expected_end = end as u64, native_position = i64::from(pos), "discarded misaligned native checkpoint"),
            Err(error) => continuum_base::probe!(class = "persona.engine.checkpoint_failed", persona = %turn.id.0,
                expected_end = end as u64, error = %error, "native checkpoint capture failed"),
        }
        turn.spent.checkpoint_ms += record::whole_ms(taking.elapsed());
        Ok(())
    }

    /// One chunk of her prompt from `pos`. The last chunk asks for logits and samples her first
    /// token; only it is sampled, since the sampler accepts what it samples (Codex on #6140).
    pub(super) fn prefill_chunk(&mut self, turn: &mut Turn, pos: usize) -> Result<(), TurnError> {
        // A chunk never runs past her view's end, so she can be checkpointed exactly there.
        let limit = turn.boundaries.next_end(pos, turn.prompt.len());
        // As many tokens as the lane allows (`lane::step_tokens`): within every other mind's slack.
        let end = pos.saturating_add(self.lane.step_tokens()).min(limit);
        let chunk = &turn.prompt[pos..end];
        let last = end == turn.prompt.len();
        let from = Pos::try_from(pos).map_err(|_| TurnError::TooLong)?;
        let len = chunk.len();
        self.engine.submit(Step { tokens: Tokens::Span { of: Arc::clone(&turn.prompt), from: pos, to: end }, from, logits_at_end: last, genes: Arc::clone(&turn.genes) })?;
        if !last {
            self.engine.settle()?;
            self.record(|her| her.tokens.held.extend_from_slice(chunk))?;
            if let Some(kind) = turn.boundaries.at(end) {
                self.keep_checkpoint(turn, kind, end)?;
            }
            turn.phase = Phase::Prefilling { pos: end };
            return Ok(());
        }
        let at = i32::try_from(len - 1).map_err(|_| TurnError::TooLong)?;
        let token = self.engine.tier().sample(&mut turn.req.sampler, at)?;
        self.record(|her| her.tokens.held.extend_from_slice(chunk))?;
        self.report_footprint(turn.id, turn.one_off);
        let pos = Pos::try_from(end).map_err(|_| TurnError::TooLong)?;
        turn.phase = Phase::Decoding { token, pos, generated: 0 };
        Ok(())
    }

    /// Emit her sampled token and, unless she stopped, decode it and sample the next.
    pub(super) fn decode_token(&mut self, turn: &mut Turn, token: i32, pos: Pos, generated: usize) -> Result<Advanced, TurnError> {
        let done = |end| Ok(Advanced::Done { tokens_generated: generated, end });
        if self.model.is_eog_token(token) {
            return done(super::super::TurnEnd::Finished);
        }
        // Emit raised her cancel flag: a stop sequence ended her answer, or her caller hung up. What
        // her blocks computed stays hers.
        if turn.cancel.load(Ordering::Acquire) {
            return done(super::super::TurnEnd::Stopped);
        }
        // The token goes to Emit (text, stop sequences, her caller) and the engine goes on.
        self.emit.send(emit::Out::Token { answer: turn.answer, token });
        let generated = generated + 1;
        let done = |end| Ok(Advanced::Done { tokens_generated: generated, end });
        if generated >= turn.req.max_tokens {
            return done(super::super::TurnEnd::AtCap);
        }
        // Her reply grows into the context; at its last position her turn ends, everything kept.
        if !self.capacity.holds(pos) {
            return done(super::super::TurnEnd::CapacityReached);
        }
        let forward = Instant::now();
        let reused_before = self.engine.tier().graphs_reused();
        self.engine.submit(Step { tokens: Tokens::One(token), from: pos, logits_at_end: true, genes: Arc::clone(&turn.genes) })?;
        turn.spent.forward += forward.elapsed();
        turn.spent.graphs_reused += self.engine.tier().graphs_reused().saturating_sub(reused_before);
        self.record(|her| her.tokens.held.push(token))?;
        // Wait for the forward pass here, where reading her logits would wait anyway, so the GPU's
        // time and the sampler's are measured apart.
        let landing = Instant::now();
        self.engine.settle()?;
        turn.spent.gpu += landing.elapsed();
        let sampling = Instant::now();
        let next = self.engine.tier().sample(&mut turn.req.sampler, 0)?;
        turn.spent.sample += sampling.elapsed();
        turn.phase = Phase::Decoding { token: next, pos: pos.advanced(1).map_err(|_| TurnError::TooLong)?, generated };
        Ok(Advanced::Continues)
    }

}
