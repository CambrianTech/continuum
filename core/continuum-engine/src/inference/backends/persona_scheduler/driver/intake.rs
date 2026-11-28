//! What reaches the frame loop: submissions taken and admitted, and a stop's suspend and preservation of every mind.

use super::*;

impl Driver<'_, '_> {
    /// Hand each stage what waited while it was full, before anything newer.
    pub(super) fn flush(&mut self) {
        self.emit.flush();
        self.record.flush();
    }

    /// Each outbox wakes on its own consumer's space. Both retain undelivered items
    /// when an incoming submission interrupts the drain.
    pub(super) async fn drain(&mut self) {
        tokio::join!(self.emit.drain(), self.record.drain());
    }

    /// Whether anything still waits for a stage that was full.
    pub(super) fn holding(&self) -> bool {
        self.emit.waiting() + self.record.waiting() > 0
    }

    /// Route what arrived: a turn into the frame loop (refused once the stop has suspended every
    /// mind), or the stop's suspend.
    /// A stop is handed back, not acknowledged here: the loop writes its last records first.
    #[must_use = "a stop must be acknowledged once its records are written"]
    pub(super) fn take(&mut self, submission: Submission, turns: &mut Vec<Turn>) -> Option<Stop> {
        match submission {
            Submission::Turn(req) if self.stopping => {
                tell(&req.response_tx, TokenEvent::Error("the engine is stopping: her minds are preserved, and her turn starts after it restarts".into()));
                None
            }
            Submission::Turn(req) => {
                self.admit(req, turns);
                None
            }
            Submission::Learning(job) if self.stopping => {
                job.refuse("the engine is stopping");
                None
            }
            Submission::Learning(job) => {
                self.learning.queue(job);
                None
            }
            Submission::Suspend(reply) => {
                self.end_learning("the engine is stopping");
                Some(Stop { reply, suspension: self.suspend(turns) })
            }
        }
    }

    /// The root suspend (CONTINUUM-ENGINE 4.16): no new turn is taken, turns in flight end (what her
    /// blocks computed stays hers), and every mind is preserved: bound, rewound to her view's
    /// checkpoint (so a restart resumes exactly at her view), retired through the pipeline, then her
    /// blocks and token history written. The manifest naming every preserved mind is published last:
    /// it is the commit point. A mind not preserved is reported, never silently dropped.
    pub(super) fn suspend(&mut self, turns: &mut Vec<Turn>) -> Suspension {
        self.stopping = true;
        for turn in std::mem::take(turns) {
            self.end(turn, Ending::Stopping);
        }
        let minds: Vec<_> = self.parked.values().chain(self.engine.units())
            .filter(|mind| resident(mind)).map(|mind| mind.id()).collect();
        let blocked = |cause: Uuid, why: String| {
            let retained = if minds.is_empty() {
                vec![(cause, why)]
            } else {
                minds.iter().map(|mind| (mind.0, why.clone())).collect()
            };
            Suspension { preserved: 0, retained }
        };
        if let Some(id) = self.engine.retiring() {
            if let Err(why) = self.retire_previous() {
                return blocked(id.0, format!("copy-out of {:?} blocked preservation: {why}", id.0));
            }
        }
        self.lane.save();
        let Some(mut store) = self.store.take() else {
            return blocked(Uuid::nil(), "this node has no home to preserve minds in".into());
        };
        // This stop's generation: every mind's blocks go to a file no published manifest names, and the
        // manifest naming them is published only once every write has landed (preserve.rs).
        let generation = preserve::Generation::fresh();
        let mut entries = Vec::new();
        let mut retained = Vec::new();
        for id in minds {
            match self.preserve(id, &mut store, generation) {
                Ok(entry) => entries.push(entry),
                Err(why) => retained.push((id.0, why.to_string())), // the stop's report: Display at the edge
            }
        }
        let preserved = entries.len();
        let manifest = preserve::next_manifest(store.last(), &entries);
        let served = preserve::ServedModel { model: self.model_id.clone() };
        match store.publish(&served, &manifest) {
            Ok(unremoved) => retained.extend(unremoved.into_iter().map(|(path, why)| (Uuid::nil(), format!("an unnamed block file {} was not removed: {why}", path.display())))),
            Err(why) => retained.push((Uuid::nil(), format!("this stop's manifest was not saved, so the last complete generation stands: {why}"))),
        }
        continuum_base::probe!(class = "persona.engine.suspended", preserved = preserved as u64, retained = retained.len() as u64, "the engine's stop preserved its minds");
        // logged too: a stop's probes can be lost as the process ends (none reached the probe file on
        // the M5's first suspend, 2026-11-19 03:57Z)
        runtime::logger("persona-scheduler").info(&format!("the stop preserved {preserved} minds; not preserved: {retained:?}"));
        self.store = Some(store);
        Suspension { preserved, retained }
    }

    /// One mind handed over in `generation`: bound, rewound to her view, retired through the
    /// pipeline, her files written and moved (preserve/mind_files.rs). Her entry names them; it is
    /// published with the stop's manifest.
    pub(super) fn preserve(&mut self, id: PersonaId, store: &mut preserve::Preserved, generation: preserve::Generation) -> Result<preserve::PreservedMind, PreserveError> {
        self.switch_to(id, false)?;
        let end = self.engine.active_mut().and_then(|her| her.parts().tokens.deepest());
        if let Some(end) = end {
            self.rewind_to_checkpoint(end)?;
            self.record(|her| her.tokens.cut(end))?;
        }
        if self.engine.retire_active()?.is_some() {
            self.retire_previous()?;
        }
        let her = self.parked.remove(&id).ok_or(PreserveError::NotReturned)?;
        let (epoch, parts) = (her.epoch().0, her.into_parts());
        let leaving = preserve::Leaving { checkpoint: parts.tokens.view_checkpoint(&parts.genome), persona: id.0, epoch, basis: parts.genome.genes, genes: &parts.genome.sources, blocks: parts.blocks, held: &parts.tokens.held };
        Ok(store.hand_over(leaving, generation)?)
    }

    /// Take a submission into the frame loop: its token ids and its sampler, both built on her own
    /// thread. A turn the stream cannot hold is refused at once, before it waits for a frame.
    pub(super) fn admit(&mut self, mut req: GenerationRequest, turns: &mut Vec<Turn>) {
        let (id, one_off) = match req.persona_id {
            Some(persona) => (PersonaId(persona), false),
            None => (PersonaId(Uuid::new_v4()), true),
        };
        // A turn this Core's stream cannot hold, prompt and asked-for reply, is refused at the door
        // with the numbers, before any prefill is spent on it; her caller refits to the stream.
        if let Err(infeasible) = self.capacity.admit(req.prompt.len()) {
            let at = transition::At::Infeasible { prompt_tokens: infeasible.prompt_tokens, capacity: infeasible.capacity };
            self.record.send(record::Fact::Transition(transition::Transition { request: req.request.clone(), persona: id.0, one_off, at }));
            tell(&req.response_tx, TokenEvent::Exceeded(super::super::Exceeded { prompt_tokens: infeasible.prompt_tokens, capacity: infeasible.capacity }));
            return;
        }
        let prompt = std::mem::take(&mut req.prompt);
        let (basis, genes) = Basis::of(&req.active_loras);
        let boundary = req.view_end;
        self.answers += 1;
        let answer = emit::AnswerSeq(self.answers);
        let cancel = Arc::new(AtomicBool::new(false));
        self.emit.send(emit::Out::Open { answer, sink: req.response_tx.clone(), stops: req.stop_sequences.clone().into(), cancel: Arc::clone(&cancel) });
        let submitted = Instant::now();
        if one_off { self.live_one_offs.insert(id); }
        let admitted = transition::Transition {
            request: req.request.clone(),
            persona: id.0,
            one_off,
            at: transition::At::Admitted { queue_depth: turns.len() + 1, urgency: req.urgency, prompt_tokens: prompt.len(), since_submit_ms: record::whole_ms(req.submitted.elapsed()) },
        };
        if let Some(r) = &req.request { crate::inference::llama_server::engine_begin(crate::inference::llama_server::GenerationKey::of(r.as_str())); }
        turns.push(Turn { req, id, one_off, prompt: prompt.into(), basis, genes, phase: Phase::Waiting, submitted, ready_since: submitted, spent: Spent::default(), boundaries: Boundaries { view: boundary, learned: None }, answer, cancel });
        self.record.send(record::Fact::Transition(admitted));
    }

}
