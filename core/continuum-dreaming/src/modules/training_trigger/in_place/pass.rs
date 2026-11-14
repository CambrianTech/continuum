//! A bucket's pass: learn each bucket's pending examples on the live engine, one step at a time,
//! and settle (publish, then page her out) when the pass ends.

use super::*;

impl TrainingTriggerState {
    /// Learn every pending submission whose base `engine` serves (`served_model`), one bucket at a
    /// time, each through the trigger's owned operations. Pages the whole pending journal, so rows
    /// for other bases never hide eligible ones behind them.
    pub(crate) async fn learn_in_place(self: &Arc<Self>, engine: EngineIncarnation, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        self.in_place.authority.custody.settle_orphans().await;
        let buckets = eligible_buckets(&store, served_model).await?;
        // learning and candidates progress together: a bucket waiting (for her act, her memory, her
        // slot) never holds a ready candidate from its trial (Codex on #5905). Both are joined here,
        // in this pass; each candidate is taken under its own bucket's gate.
        let served = served_model.to_owned();
        let learning = self.learn_each(buckets, move |state, key, rows| {
            let served = served.clone();
            async move {
                let pass = state.learn_bucket(key.clone(), rows, engine).await?;
                if pass.checkpointed {
                    // a generation published in this pass is offered as a candidate now, not a pass later
                    state.sweep_bucket(&key, &served).await?;
                }
                Ok(pass)
            }
        });
        let (learned, swept) = tokio::join!(learning, self.sweep_all(&store, served_model));
        learned?;
        swept
    }

    /// `key`'s newest generation, offered as a candidate the way the sweep offers every bucket's.
    async fn sweep_bucket(self: &Arc<Self>, key: &BucketKey, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        match store.checkpoint_of(key).await? {
            Some(current) => self.sweep_candidates(vec![current], served_model).await,
            None => Ok(()),
        }
    }

    /// Every bucket's newest generation is candidate work of its own, retried until taken: a
    /// candidate interrupted after its generation, or a bucket with nothing pending, is not lost.
    async fn sweep_all(self: &Arc<Self>, store: &super::super::durable::DurableStore, served_model: &str) -> Result<(), String> {
        const PAGE: usize = 64;
        let mut after: Option<String> = None;
        loop {
            let page = store.checkpoints(after.as_deref(), PAGE).await?;
            let full = page.len() == PAGE;
            after = page.last().map(|row| row.cursor().to_owned());
            self.sweep_candidates(page, served_model).await?;
            if !full {
                return Ok(());
            }
        }
    }

    /// Run `step` on each bucket under its owned gate, in order. The owner refusing (shutdown) ends
    /// the pass; one bucket's own failure never does: it is probed, its durable work stays, and the
    /// next bucket runs (Codex on 086d60123: a persistent bucket error starved every later bucket).
    /// Every bucket's pass at once, joined here: one bucket waiting for her act or her slot never
    /// holds another's. A bucket's failure is its own; the others' passes still run.
    pub(crate) async fn learn_each<F, Fut>(self: &Arc<Self>, buckets: Vec<(BucketKey, Vec<Arrival>)>, step: F) -> Result<(), String>
    where
        F: Fn(Arc<Self>, BucketKey, Vec<Arrival>) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Result<BucketPass, String>> + Send + 'static,
    {
        let passes = buckets.into_iter().map(|(key, rows)| {
            let persona = key.persona_id;
            let pass = step.clone()(Arc::clone(self), key, rows);
            async move { (persona, pass.await) }
        });
        for (persona, pass) in futures::future::join_all(passes).await {
            match pass {
                Ok(pass) if pass.learned > 0 || pass.quarantined || pass.checkpointed => {
                    continuum_base::probe!(class = "training.in_place.pass", learned = u64::from(pass.learned), quarantined = pass.quarantined,
                        checkpointed = pass.checkpointed, deferred = pass.deferred, "accepted examples learned in place");
                }
                Ok(_) => {}
                Err(why) => continuum_base::probe!(class = "training.in_place.bucket_failed", persona = %persona, reason = %why,
                    "one bucket's pass failed; its durable work stays, the other buckets' passes ran and the next pass tries it again"),
            }
        }
        Ok(())
    }

    /// Learn bucket `key`'s pending `rows` on `engine` until it stops, then publish what it learned.
    /// Each step: (1) her acts finish, holding nothing; (2) her learner is seated and the example's
    /// memory covered under the bucket's gate and the engine, holding no serving resource, every
    /// memory lease one attempt, a shortfall ending the pass holding nothing; (3) her slot is admitted as
    /// a keyed dream (lane, permit, page); (4) the gate and the engine are only tried, and on
    /// contention everything is let go and waited for holding no serving resource; (5) the step
    /// revalidates its memory, marks custody and posts, inside the owned operations (Codex on B).
    /// The pass ends by settling: publish, then page her out (`settle`).
    pub(crate) async fn learn_bucket(self: &Arc<Self>, key: BucketKey, rows: Vec<Arrival>, engine: EngineIncarnation) -> Result<BucketPass, String> {
        let mut pass = BucketPass::default();
        if rows.iter().all(|row| row.learned as usize >= row.examples.len()) {
            return Ok(pass); // nothing pending: no slot is taken for nothing
        }
        let rows = Arc::new(rows);
        let mut renewed = false;
        // seated in this pass: from here she holds a shadow, so she never waits out an act
        let mut seated = false;
        // a failure after seating still settles first, so her learner is never left seated
        let mut failure = None;
        loop {
            // 1. her acts first, holding nothing (only before she is seated)
            if waits_for_her_act(seated) {
                if let Err(why) = crate::inference::slots::her_acts_done(key.persona_id).await {
                    continuum_base::probe!(class = "training.in_place.step_deferred", persona = %key.persona_id, reason = %why,
                        "her act gate could not be waited on; the examples stay pending");
                    pass.deferred = true;
                    break;
                }
            }
            // 2. seat and cover her next example, holding no serving resource
            let prepare_rows = Arc::clone(&rows);
            let prepared = match self
                .run_owned(Work::Learning, key.clone(), move |state, key| async move { state.in_place.prepare(&state, &key, &prepare_rows, engine).await })
                .await
            {
                Ok(Ok(prepared)) => prepared,
                Ok(Err(why)) | Err(why) => {
                    failure = Some(why);
                    break;
                }
            };
            match prepared {
                Prepared::Ready => {
                    seated = true;
                    #[cfg(test)]
                    self.pause_operation(super::super::durable::OperationPoint::SeatedBeforeSlot).await;
                }
                Prepared::Nothing => break,
                Prepared::Deferred => {
                    pass.deferred = true;
                    break;
                }
                Prepared::MemoryShort(wait) => {
                    memory_short(&key, &wait);
                    pass.deferred = true;
                    break;
                }
            }
            // 3. her slot as a keyed dream (an act begun since step 1 refuses it: back to step 1)
            let slot = match crate::inference::slots::learn_slot(engine, key.persona_id).await {
                Ok(slot) => slot,
                Err(crate::inference::slots::LearnSlotRefusal::Busy) => {
                    // her act began after the wait: it comes first, and she is seated, so she
                    // lets go rather than wait it out holding her shadow (settle pages her out)
                    continuum_base::probe!(class = "training.in_place.yielded_to_her_act", persona = %key.persona_id,
                        "her act began while she was seated: the pass ends and pages her out; the next pass resumes her");
                    pass.deferred = true;
                    break;
                }
                Err(refusal) => {
                    // every deferral says why: a step that never runs is a miss, never a quiet slower path
                    continuum_base::probe!(class = "training.in_place.step_deferred", persona = %key.persona_id, reason = %refusal,
                        "her learning step was not given a slot; the examples stay pending");
                    pass.deferred = true;
                    break;
                }
            };
            let Some(lease) = self.submit_gates.try_acquire(&key) else {
                drop(slot);
                drop(self.submit_gates.acquire(&key).await);
                continue;
            };
            let engine_learning = self.in_place.engine(engine);
            let Some(held) = engine_learning.try_acquire(Some(key.persona_id), super::engine_lock::Phase::Step) else {
                drop((slot, lease));
                drop(engine_learning.acquire(Some(key.persona_id), super::engine_lock::Phase::Step).await);
                continue;
            };
            let step_rows = Arc::clone(&rows);
            let (next, stepped, renewed_now) = match self
                .run_leased(Work::Learning, lease, key.clone(), move |state, key| async move {
                    let (mut stepped, mut renewed) = (BucketPass::default(), renewed);
                    let next = state.in_place.step(&state, held, &key, &step_rows, engine, slot, &mut renewed, &mut stepped).await;
                    (next, stepped, renewed)
                })
                .await
            {
                Ok(stepped) => stepped,
                Err(why) => {
                    failure = Some(why);
                    break;
                }
            };
            renewed = renewed_now;
            pass.learned += stepped.learned;
            pass.deferred |= stepped.deferred;
            pass.quarantined |= stepped.quarantined;
            let next = match next {
                Ok(next) => next,
                Err(why) => {
                    failure = Some(why);
                    break;
                }
            };
            match next {
                Next::Again => continue,
                Next::Stop => break,
                Next::Fenced => return Ok(pass),
                Next::MemoryShort(wait) => {
                    memory_short(&key, &wait);
                    pass.deferred = true;
                    break;
                }
            }
        }
        pass.checkpointed = self.run_owned(Work::Learning, key, move |state, key| async move { state.in_place.settle(&state, &key, engine).await }).await??;
        match failure {
            Some(why) => Err(why),
            None => Ok(pass),
        }
    }

    /// Take each due candidate in `page` whose base `served_model` serves, under its bucket's gate.
    async fn sweep_candidates(self: &Arc<Self>, page: Vec<super::super::durable::LearnCheckpoint>, served_model: &str) -> Result<(), String> {
        for generation in page {
            if !crate::model_registry::artifacts::names_the_same_base(served_model, &generation.base_model) {
                continue;
            }
            // under the bucket's owned gate, so it is ordered with that bucket's learning
            let key = BucketKey { persona_id: generation.persona_id, trait_kind: generation.trait_kind.clone(), base_model: generation.base_model.clone() };
            let persona = key.persona_id;
            let swept = self
                .run_owned(Work::Learning, key, move |state, key| async move {
                    let store = state.durable.require()?;
                    if let Some(current) = store.checkpoint_of(&key).await? {
                        snapshot::maybe(&store, &current).await;
                        state.in_place.evict(&key, &current).await;
                    }
                    Ok::<(), String>(())
                })
                .await?;
            if let Err(why) = swept {
                continuum_base::probe!(class = "training.in_place.candidate_sweep_failed", persona = %persona, reason = %why,
                    "one bucket's candidate could not be read; the sweep goes on and the next pass takes it again");
            }
        }
        Ok(())
    }

    /// Run `operation` as one of the trigger's owned learning operations, outliving the caller;
    /// a stop hands it back like every other ([`Work::Learning`]).
    pub(in crate::modules::training_trigger) async fn spawn_owned(&self, operation: impl std::future::Future<Output = ()> + Send + 'static) {
        self.operations.lock().await.spawn(Work::Learning, operation);
    }
}
