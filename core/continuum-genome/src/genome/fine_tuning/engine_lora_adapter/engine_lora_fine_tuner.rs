use super::*;

#[async_trait]
impl FineTuningAdapter for EngineLoraFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: self.lane.provider().into(),
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact: true,
            // any base its lane source serves (`serves_base`); create_job acquires the lane
            supported_base_model_prefixes: vec![],
            requires: TrainerHardware::Any,
            place: self.lane.place(),
        }
    }

    fn serves_base(&self, base_model: &str) -> bool {
        self.lane.serves(base_model)
    }

    async fn create_job(&self, mut request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        // The request as it arrived, before the provider's defaults fill it in: what the job's
        // provenance records (card 04679d57), so defaults never read back as the producer's policy.
        let received = request.clone();
        let schedule = request.schedule.get_or_insert_with(default_schedule).clone();
        let lora = request.lora.get_or_insert_with(default_lora).clone();
        if request.dataset.examples.is_empty()
            || schedule.epochs == 0
            || schedule.epochs > 100
            || schedule.sequence_length < 16
            || !schedule.learning_rate.is_finite()
            || schedule.learning_rate <= 0.0
            || schedule.learning_rate > 1.0
            || !(1..=256).contains(&lora.rank)
            || lora.alpha == 0
        {
            return Err(FineTuningError::InvalidRequest(
                "invalid engine training data/schedule/LoRA geometry (epochs 1-100, sequence_length >= 16, lr (0,1], rank 1-256)".into(),
            ));
        }
        let targets = gguf_targets(&lora.target_modules).map_err(FineTuningError::InvalidRequest)?;
        let LaneLease { choice, hold: lane_hold, still_bound } =
            self.lane.acquire(&request.base_model).await?;
        let LaneChoice { url: lane, window: served_window, engine, build, memory, weights } = choice;
        let train_dir = self
            .train_dir
            .clone()
            .ok_or_else(|| failure("no engine train dir (no home directory): /train is off on every lane"))?;
        let id = Uuid::new_v4();
        let out = format!("{id}.gguf");
        if let Some(parent) = &request.parent {
            // A fork trains from the base today: the engine's /train has no warm start from
            // an existing adapter (the fork's init file went in #19). The lineage is still
            // recorded on the child's signature; the weights are not inherited. The row
            // that says so is this probe, until the engine takes `init_adapter`.
            continuum_base::probe!(
                class = "genome.fork.cold_start",
                persona = %request.persona_id,
                trait_kind = %request.trait_kind,
                parent = %parent,
                job = %id,
                "a fork trains from the base: the engine has no warm start from the parent yet; lineage recorded, weights not inherited"
            );
        }
        // LEARNING SEES WHAT SERVING SEES (Joel, 2026-09-28: "stupidly low token sizes are
        // idiotic ... the same as inference"; "you're not supposed to make learning so different
        // from reality"). The window is the per-slot window the matched lane was LAUNCHED with,
        // from the same record that chose the lane: her turns run in it, so a lived example
        // trains whole with its system and tool head. No request sets it (§9.1): a record that
        // predates the field is refused, never trained at a guessed window (Cormac on #4498:
        // a fallback to the request's length is attempt #1 again). The engine measures the
        // training graph at this window before allocating and refuses past the lease. This is the
        // CEILING: when the measured graph says it cannot fit beside serving, the window is
        // calculated down from it ([`choose_window`], interim until the attention memory stops
        // growing with the cached prefix); never from the request's length.
        if served_window == 0 {
            return Err(FineTuningError::InvalidRequest(format!(
                "the live lane serving {} has no recorded served window (a record from before the field): not training at a guessed window",
                request.base_model
            )));
        }
        // the engine's context granularity is 256; rounding a smaller served window UP would
        // train on more context than serving holds (Codex on #4498)
        if served_window < 256 {
            return Err(FineTuningError::InvalidRequest(format!(
                "the live lane serving {} holds {served_window} tokens a slot, under the engine's 256-token training granularity: not training past what serving holds",
                request.base_model
            )));
        }
        let window = train_window(served_window);
        continuum_base::probe!(
            class = "training.job.window",
            requested = schedule.sequence_length as u64,
            served = u64::from(served_window),
            sent = window as u64,
            max_slowdown_ppm = u64::from(TRAINING_MAX_SLOWDOWN_PPM),
            "the training window: the matched lane's served per-slot window as the CEILING (the engine sizes the context to the longest example, fork #30), rounded to the engine's 256 granularity; the request's length never decides it; and the bound on her decode slowdown while training runs"
        );
        // 0 blocks is no depth at all: every block, as omitted (the engine refuses 0 at parse)
        let depth = lora.top_layers.filter(|&k| k > 0);
        let val = request.dataset.validation_split.clamp(0.0, 0.5);
        let mut body = TrainRequest {
            examples: request.dataset.examples.iter().map(engine_example).collect(),
            out: out.clone(),
            rank: lora.rank,
            alpha: lora.alpha as f32,
            targets: targets.clone(),
            window,
            epochs: schedule.epochs,
            lr: schedule.learning_rate,
            val_split: val,
            seed: 42,
            top_layers: depth,
            memory_budget_mib: None,
            max_slowdown_ppm: TRAINING_MAX_SLOWDOWN_PPM,
            fit: Fit::Middle,
            exact: false,
            walk_host_budget_mib: None,
            chunk: None,
            recompute: true,
        };
        // THE EXACT WALK ONLY WITH A KNOWN BUDGET: unread memory trains the plain walk
        (body.exact, body.walk_host_budget_mib) =
            walk_request(crate::system_resources::memory_pressure::current_available_bytes());
        if !body.exact {
            continuum_base::probe!(
                class = "training.job.exact_walk_skipped",
                "the host memory monitor has not read the host: the job trains the plain (chunk-bounded) walk rather than an exact walk with no budget"
            );
        }
        // THE CHUNK FITS THE LEASE (card d6dae498): the shape is keyed by the walk it trains
        // and the chunk it trains at, and the chunk is the largest whose measured footprint
        // fits the governed VRAM now; a chunk never measured calibrates. Before this, the
        // engine took 512 for every job and a plain-walk footprint stood in for the exact
        // walk's (the 5090, 2026-10-10: 3418 MiB leased, 12.6 GB needed, refused in 8 s).
        let governed_free = self.free_for(&format!("genome-train:{id}"), memory);
        let mut walk_reason = "the host memory monitor had not read the host, so no exact-walk budget could be set".to_owned();
        let mut family = Shape {
            model: request.base_model.clone(),
            window,
            rank: lora.rank,
            targets: targets.clone(),
            depth,
            exact: body.exact,
            chunk: TRAINING_CHUNK_MIN,
            recompute: body.recompute,
        };
        // Evidence about THIS lane's engine only (Codex on #4988): a bound another engine measured
        // must not park her on this one, or a fixed engine could never wake her.
        let footprints = self.footprints.for_engine(build.clone());
        // rows filed before refusals had their own key are classified once, by the refusal their
        // own job recorded, so a refusal filed as a whole run leaves the fit (Codex on #5548).
        // File I/O that may wait out a Windows sharing violation: off the runtime.
        {
            let (store, roots) = (footprints.clone(), [request.local_artifact_dir.clone(), crate::genome::fine_tuning::job_dir::genome_root()]);
            let roots: Vec<PathBuf> = roots.into_iter().flatten().collect();
            let classified = tokio::task::spawn_blocking(move || store.classify_legacy(&roots))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            match classified {
                Ok(done) => {
                    if done.refusals > 0 {
                        continuum_base::probe!(
                            class = "training.footprints.legacy_refusals",
                            job = %id,
                            refusals = done.refusals as u64,
                            "footprint rows filed as whole runs were refusals by their own job's record: moved to their refusal keys, out of the fit"
                        );
                    }
                    if !done.unknown.is_empty() {
                        continuum_base::probe!(class = "training.footprints.legacy_unclassified", job = %id, rows = done.unknown.len() as u64, error = %done.unknown.join("; "),
                            "rows from before provenance whose job record could not be read stay unclassified; they are tried again on the next dispatch");
                    }
                }
                Err(e) => continuum_base::probe!(class = "training.footprints.legacy_unclassified", job = %id, error = %e,
                    "rows from before provenance could not be classified; they stay as they were and are classified on the next dispatch"),
            }
        }
        let mut rows = footprints.rows_like(&family);
        let mut model = GraphModel::fit(&rows);
        // THE WINDOW THE DATA NEEDS: every example measured at the served window by the engine's
        // render gives its whole length; the run plans for the longest (every example trains
        // whole, as serving saw it), then the largest chunk that fits it. Only when even the
        // smallest chunk cannot hold that window does the window shrink, and the partition parks
        // what no longer fits. Unmeasurable examples plan at the served window, and the
        // partition below ends the job before any run.
        let served_sent = window;
        // the render time travels with this, the one measure: /train renders the same examples
        // before it answers, so the POST's wait is sized by it (post_wait_for; Cormac on #4946)
        let measure_started = std::time::Instant::now();
        let at_served = measure_examples(&self.http, &lane, &body.examples, body.fit, served_sent).await;
        // AN ENGINE STILL LOADING DECIDES NOTHING: nothing of this job exists yet (no request.json,
        // no run, no partition), so the dispatch refuses as certain and retries the batch once the
        // engine serves, rather than failing a job whose examples then belong to no one.
        if let Err(MeasureError::NotReady(why)) = &at_served {
            return Err(FineTuningError::TrainerUnavailable(format!(
                "job {id}: the engine is not ready to measure her examples ({why}); nothing was created, and the dispatch retries once it serves"
            )));
        }
        let render_per_example = measure_started.elapsed().div_f64(body.examples.len().max(1) as f64);
        let need = at_served.as_ref().map_or(served_sent, |rows| window_for_need(rows, served_sent));
        // THE WALK THE WINDOW ALLOWS (Cormac, (iii)): the exact walk carries its gradient across
        // the whole window, so its graph does not shrink with the chunk (the 5090, 2026-10-11:
        // 18,198 MiB at window 67,584 for chunks 512, 256 and 128 alike). When a measurement
        // already rules it out for the window her examples need, she trains them WHOLE on the
        // plain walk instead of truncated or parked on the exact one: a step at cosine 0.81 on her
        // whole turns over one at 0.998 on a fraction of them (Joel: working training beats data
        // purity; she can unlearn).
        if body.exact && exact_ruled_out(&footprints.measured_rows_like(&family), need, governed_free) {
            continuum_base::probe!(
                class = "training.job.walk_plain",
                job = %id,
                needed = u64::from(need),
                governed_free = governed_free,
                "the exact walk is measured not to fit the window her examples need; the plain walk trains them whole"
            );
            body.exact = false;
            body.walk_host_budget_mib = None;
            walk_reason = format!("the exact walk was measured not to fit window {need} in {governed_free} governed free bytes");
            family.exact = false;
            rows = footprints.rows_like(&family);
            model = GraphModel::fit(&rows);
        }
        // CHUNK, THEN DEPTH, THEN WINDOW (card 0d21e644; Kimi's job 598e183c, refused at window
        // 45,056 with only a smaller window left, which parks her longest turns): each depth's
        // plan reads its own rows, so a depth is chosen by what was measured there, or calibrates
        let (stepped, plan) = choose_depth(family.depth, footprints.layers(&family.model), need, |depth| {
            let at = Shape { depth, ..family.clone() };
            choose_window(need, governed_free, &footprints.rows_like(&at), &footprints.bound_rows_like(&at))
        });
        if stepped != family.depth {
            continuum_base::probe!(
                class = "training.job.depth_stepped",
                job = %id,
                needed = u64::from(need),
                asked = family.depth.map_or(0, u64::from), // probe field: 0 = every block
                depth = stepped.map_or(0, u64::from), // probe field: 0 = every block
                governed_free = governed_free,
                "the asked depth cannot hold the window her examples need even at the smallest chunk; a shallower one can (or is unmeasured, and this run measures it), so every example trains whole"
            );
            family.depth = stepped;
            body.top_layers = stepped;
            rows = footprints.rows_like(&family);
            model = GraphModel::fit(&rows);
        }
        let window = plan.window;
        // ONE evaluation, ONE probe (Cormac on #4972): a predicted no-fit floor is reported as the
        // prediction it is, and never as window_calculated's "what trains her examples whole". The window is set
        // either way: the probe changes, the flow does not.
        let no_fit = if let crate::inference::planning::training_plan::WindowFit::PredictedNoFit { predicted_at_floor } = plan.fit {
            continuum_base::probe!(
                class = "training.job.no_window_fits_predicted",
                job = %id,
                needed = u64::from(need),
                governed_free = governed_free,
                predicted_at_floor = predicted_at_floor,
                floor_window = u64::from(window),
                "the fitted model PREDICTS the graph exceeds governed free even at the floor window and chunk (extrapolated from the measured windows, not measured there), so her examples park"
            );
            true
        } else {
            false
        };
        if window != body.window && !no_fit {
            continuum_base::probe!(
                class = "training.job.window_calculated",
                served = u64::from(body.window),
                needed = u64::from(need),
                window = u64::from(window),
                governed_free = governed_free,
                measured_rows = rows.len() as u64,
                kv_term = model.is_some_and(|m| m.kv_term_identified()),
                "the training window: what trains her examples whole (the longest of them), shrunk only if even the smallest chunk cannot hold it beside serving"
            );
        }
        // THE WINDOW NEVER SHRINKS BELOW WHAT TRAINS ONE OF HER EXAMPLES (card de9e1342; Codex's
        // corrections): a window none of them trains in teaches nothing, and the run would end
        // Failed with nothing to return. When memory plans one, the smallest window that trains
        // one of them decides. Measured not to fit what the trainer is governed to, the job is
        // not created: its examples wait, owned by the trigger, with what would wake them. Only
        // predicted not to fit, it calibrates there, and its refusal is the measurement the next
        // plan parks on. Her examples are never cut to a smaller window.
        // WHOLE length, never `tokens_min` (head plus last exchange: a middle-cut example; Codex on
        // #4988, her canonical shortest is 38,012 whole against 15,966 min)
        let useful = at_served
            .as_ref()
            .ok()
            .and_then(|rows| rows.iter().map(|r| r.tokens_full).min())
            .and_then(|tokens| u32::try_from(tokens).ok())
            .map(|tokens| train_window(tokens.saturating_add(255)))
            .filter(|&useful| useful <= served_sent);
        // Checked whatever the plan chose (Codex on #4988): a measured no-fit at a window equal to
        // `useful`, the floor included, would otherwise dispatch again.
        let window = match useful {
            Some(useful) => {
                let at_useful = Shape { window: useful, chunk: TRAINING_CHUNK_MIN, ..family.clone() };
                match footprints.need(&at_useful, model.map(|m| m.bytes(useful, TRAINING_CHUNK_MIN))) {
                    Some(n) if n.source != NeedSource::Predicted && n.rules_out(governed_free) => {
                        let wait = super::super::types::CapacityWait {
                            window: useful,
                            need_bytes: n.bytes,
                            need_source: n.source,
                            governed_free,
                            engine_stamp: build.clone(),
                        };
                        continuum_base::probe!(
                            class = "training.job.capacity_parked",
                            job = %id,
                            window = u64::from(useful),
                            need_bytes = n.bytes,
                            need_source = ?n.source,
                            governed_free = governed_free,
                            "the smallest window that trains one of her examples is measured not to fit the governed training memory: no job; her examples wait with what wakes them"
                        );
                        return Err(FineTuningError::CapacityParked(wait));
                    }
                    _ => window.max(useful),
                }
            }
            None => window,
        };
        body.window = window;
        let shape_at = |chunk: u32| Shape { window, chunk, ..family.clone() };
        // The chunk is chosen against the measured graph, else the fitted model's prediction for
        // an unmeasured (window, chunk): a calculated window with a chunk left to the default
        // would refuse by construction (Cormac on #4920). The LEASE is only ever a measurement:
        // a chunk known only by prediction is a calibration run, leasing all governed free VRAM.
        let (chunk, _) = choose_chunk(window, governed_free, |c| {
            footprints.need(&shape_at(c), model.map(|m| m.bytes(window, c))).and_then(|n| n.decides(governed_free))
        });
        let measured = footprints.get(&shape_at(chunk));
        let shape = shape_at(chunk);
        body.chunk = Some(chunk);
        let job_footprints = footprints.clone();
        // what this candidate cannot be under, by measurement (Astra and Cormac on #4949): a
        // calibration grant below it is a guaranteed refusal
        let calibration_floor = calibration_floor(&footprints.measured_rows_like(&family), window, chunk);
        let walk = if body.exact {
            super::super::types::TrainingWalk::Exact
        } else {
            super::super::types::TrainingWalk::Plain { reason: walk_reason.clone() }
        };
        let post_wait_floor = self.post_wait_floor;
        let http = self.http.clone();
        let holds = self.holds.clone();
        let hold_store = self.hold_store.clone();
        let residency_store = self.residency_store.clone();
        let base_model = request.base_model.clone();
        let governed = matches!(self.admission, Admission::Governed);
        let job_dir = job_dir_for(&request, id);
        // THE REQUEST BEFORE THE RUN (card 04679d57): the examiner learns what a gene trained on
        // from its job's request (`job_dir::read_job_request`), and proves the unseen tasks from it.
        // This trainer never wrote one, so every in-engine gene was unexaminable by construction
        // (the windows 5090: both of Kimi's exam receipts, 2026-10-24, "no training job recorded").
        // Published once, before anything can train; a job whose provenance cannot be written is
        // not started, since its gene could never be examined.
        let request_bytes = serde_json::to_vec(&received) // disk boundary: the job dir's request.json, read back by the examiner and a legacy return
            .map_err(|e| FineTuningError::InvalidRequest(format!("the training request does not serialize: {e}")))?;
        match crate::genome::fine_tuning::job_dir::publish_job_file(&job_dir, "request.json", &request_bytes) {
            Ok(None) => {}
            Ok(Some(existing)) if existing == request_bytes => {}
            Ok(Some(_)) => {
                return Err(FineTuningError::InvalidRequest(format!(
                    "{} already holds another training request than job {id}'s",
                    job_dir.join("request.json").display()
                )))
            }
            Err(e) => return Err(FineTuningError::Transient(format!("job {id}'s training request could not be recorded before its run: {e}"))),
        }
        let mut corpus_chars = 0;
        let model_id = format!("{PROVIDER_ID}:{}:{id}", request.trait_kind);
        let epochs = schedule.epochs;
        // THE PARTITION (fork #51): only what the window can train is sent. The engine measures
        // every example at this window by the rule it trains by (never a core estimate: one kept
        // 62 of 311 where the engine kept 54); the ones it would skip are PARKED in the job's
        // directory, not sent and not consumed. A job whose examples cannot be measured, or
        // none of which fits, ends at once without a run, and records nothing returnable: a
        // dispatch error would be retried, and a return would refill her bucket with examples
        // that fit no window this node can plan today.
        // the partition at the window that will run: the served measurement already is it when
        // the window did not move
        let at_window = match at_served {
            Ok(rows) if window == served_sent => Ok(rows),
            // an example's full and smallest lengths do not depend on the window, so under
            // "middle" its fit at a smaller window is arithmetic, not another ~5 s render each
            // (Cormac on #4940); the run's skipped_indices stay the authority on what trained
            Ok(rows) if body.fit == Fit::Middle => Ok(refit_middle(rows, window)),
            Ok(_) => measure_examples(&http, &lane, &body.examples, body.fit, window).await,
            Err(e) => Err(e),
        };
        let unrunnable = match at_window {
            Err(e) => Some(format!("{e}: the examples were not measured at window {window}, so none is sent (an engine before fork #51 has no /train/measure; its next relaunch brings it)")),
            Ok(rows) => {
                let partition = partition_of(window, rows);
                let shortest_parked = partition
                    .rows
                    .iter()
                    .filter(|r| r.fit != ExampleFit::Kept)
                    .map(|r| r.tokens_full)
                    .min()
                    .unwrap_or(0); // probe field: 0 = nothing parked
                continuum_base::probe!(
                    class = "training.job.partitioned",
                    job = %id,
                    window = u64::from(window),
                    sent = partition.sent.len() as u64,
                    parked = partition.parked.len() as u64,
                    of = request.dataset.examples.len() as u64,
                    shortest_parked = shortest_parked,
                    "the examples this window can train are sent; the rest are parked in the job's directory until a window of at least shortest_parked tokens can be planned. \
                     A small window trains her SHORTEST turns only: the gene is biased toward short-context Kimi"
                );
                // HER LEARNING'S DEMAND ON SERVING (card ea157b1a): each parked example records
                // the window that trains it whole (the planner's own window_for_need, unclamped),
                // each sent one clears; the largest is priced by THIS shape's footprints at that
                // window (measured, a lower bound, or the fit's prediction, labeled; unknown when
                // none), and the serving requirement reads it as her target and reservation.
                use crate::genome::fine_tuning::training_demand::{Reservation, TrainingDemand};
                use crate::inference::planning::window_allocator::TrainingPrice;
                let demand = TrainingDemand::in_dir(self.demand_store.as_deref(), request.persona_id);
                let key = |i: usize| TrainingDemand::key_of(&request.dataset.examples[i]);
                let parked_needs: Vec<(String, u32)> = partition
                    .rows
                    .iter()
                    .filter(|r| r.fit != ExampleFit::Kept)
                    .map(|r| (key(r.index), window_for_need(std::slice::from_ref(r), u32::MAX)))
                    .collect();
                // priced only from THIS shape's footprints at that window, and bound to the model
                // and shape it was priced for (Codex on #5557); sent examples clear nothing here:
                // only a run that trains them does, at its finish
                let reservation = parked_needs.iter().map(|(_, w)| *w).max().map(|largest| {
                    let at = Shape { window: largest, chunk: TRAINING_CHUNK_MIN, ..family.clone() };
                    let priced = footprints.need(&at, model.map(|m| m.bytes(largest, TRAINING_CHUNK_MIN)));
                    let price = priced.map(|n| TrainingPrice { bytes: n.bytes, source: n.source, model: at.model.clone(), shape: at.key() });
                    Reservation { window: largest, price, job: id }
                });
                let reserve_receipt = reservation.as_ref().and_then(|r| r.price.as_ref()).map(|p| (p.bytes, p.source));
                if let Err(e) = demand.record(&parked_needs, &[], reservation) {
                    continuum_base::probe!(class = "training.demand.unrecorded", job = %id, error = %e,
                        "her parked examples' window demand could not be recorded; serving sizes her lane without it until the next partition");
                }
                if let Some(requested) = parked_needs.iter().map(|(_, w)| *w).max() {
                    continuum_base::probe!(
                        class = "training.demand.unresolved",
                        job = %id,
                        persona = %request.persona_id,
                        requested_window = u64::from(requested),
                        served_window = u64::from(served_sent),
                        planned_window = u64::from(window),
                        parked = parked_needs.len() as u64,
                        outstanding = demand.waiting() as u64,
                        reserve_bytes = reserve_receipt.map_or(0, |(bytes, _)| bytes), // probe field: 0 = unpriced, see reserve_source
                        reserve_source = ?reserve_receipt.map(|(_, source)| source),
                        "lived examples wait for a window her lane does not serve: the serving plan reads this as her target; it is unmet until a partition sends them (the serving plan's own receipt says why its window is smaller)"
                    );
                }
                let sent: Vec<EngineExample> = partition.sent.iter().map(|&i| body.examples[i].clone()).collect();
                let written = write_partition(&job_dir, &partition);
                match written {
                    Err(e) => Some(format!("the partition could not be recorded ({e}): no run, so no example is counted as trained that was not")),
                    Ok(()) if sent.is_empty() => Some(format!(
                        "none of the {} examples trains whole at window {window}: the shortest needs {shortest_parked} tokens whole; all are parked in {}",
                        request.dataset.examples.len(),
                        job_dir.join(PARTITION_FILE).display()
                    )),
                    Ok(()) => {
                        corpus_chars = partition.sent.iter().map(|&i| super::super::training_rate::example_chars(&request.dataset.examples[i])).sum();
                        body.examples = sent;
                        None
                    }
                }
            }
        };
        let spec = EngineRunSpec {
            adapter_path: train_dir.join(&out),
            job_dir,
            epochs,
            val_split: SplitPpm::from(val),
            model_id,
            shape: shape.clone(),
            corpus_chars,
            walk: Some(walk),
            trained_on: weights,
            demand_store: self.demand_store.clone(),
        };
        Ok(self.jobs.prepare(id, move |progress| async move {
            if let Some(why) = unrunnable {
                return Err(failure(why));
            }
            let consumer = format!("genome-train:{id}");
            let mut residency = None;
            let reservation = if governed {
                let daemon = crate::resources::ResourceDaemon::global()
                    .ok_or_else(|| failure("engine training requires the resource governor"))?;
                // A measured shape leases its number; an unmeasured one is a calibration run
                // that leases everything governed and free AT ADMISSION, so nothing grows into
                // it (`TrainingNeed::Calibrate`). `bytes` here is what is free now, for the plan
                // probe and the wait's report; the lease the engine runs under is what was granted.
                let need = match measured {
                    Some(bytes) => crate::inference::training_admission::TrainingNeed::Measured(bytes),
                    None => crate::inference::training_admission::TrainingNeed::Calibrate { at_least: calibration_floor },
                };
                let bytes = match measured {
                    Some(bytes) => bytes,
                    None => daemon.available_for(&consumer, memory),
                };
                continuum_base::probe!(
                    class = "training.job.planned",
                    job = %id,
                    base = shape.model.as_str(),
                    window = shape.window as u64,
                    depth = shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                    exact = shape.exact,
                    chunk = shape.chunk as u64,
                    measured = measured.is_some(),
                    memory_bytes = bytes,
                    "engine training: the measured footprint for this shape, or (unmeasured) all governed \
                     free VRAM for a calibration run"
                );
                let gate = crate::inference::serving_lifecycle::LifecycleGate::global()
                    .ok_or_else(|| failure("engine training requires the serving lifecycle gate"))?;
                // THE RUN BINDS TO ITS ENGINE INSIDE THE ADMISSION HOLD (SHARED-RESIDENT-LIFECYCLE.md
                // step 1): the lane chosen above must still be the one its source chose (the same
                // incarnation, endpoint and base: `LaneLease::still_bound`), or it went away or was
                // replaced while this job waited and admission is refused; then the binding is
                // recorded before any relaunch can take the gate.
                let store = residency_store
                    .clone()
                    .ok_or_else(|| failure("engine training requires a home for its engine binding"))?;
                let chosen = engine.ok_or_else(|| failure("the chosen lane has no engine incarnation to bind to"))?;
                let bound = crate::inference::engine_residency::ResidentWork {
                    job: id,
                    out: out.clone(),
                    engine: chosen,
                    base_model: base_model.clone(),
                    created_ms: continuum_base::time::EpochMs::now().millis(),
                    consumer: consumer.clone(),
                    reserved_bytes: bytes,
                    interrupted: None,
                    job_spec: Some(
                        crate::inference::engine_residency::OwnerSpec::of(&spec)
                            .map_err(|e| failure(&format!("engine training could not hold its re-attach spec: {e}")))?,
                    ),
                    memory: Some(memory),
                };
                let (bind_store, mut bind_work) = (store.clone(), bound.clone());
                let bind_job_dir = spec.job_dir.clone();
                let bind = move |granted: u64| -> Result<(), String> {
                    bind_work.reserved_bytes = granted;
                    match still_bound() {
                        Ok(()) => {}
                        Err(Unbound::Gone(error)) => return Err(error),
                        Err(Unbound::Replaced(error)) => {
                            // Nothing ran: the examples go back to her bucket and the next
                            // dispatch plans against the new lane (card 0b8de4da).
                            if let Err(e) = write_returnable(&bind_job_dir, &Returnable::LaneReplaced { error: error.clone() }) {
                                continuum_base::probe!(class = "training.job.returnable_unrecorded", error = %e, "the replaced lane could not be recorded; the examples wait for a manual return");
                            }
                            return Err(error);
                        }
                    }
                    crate::inference::engine_residency::record(&bind_store, bind_work)
                };
                let reservation = crate::inference::training_admission::wait_for_training_memory_bound(
                    daemon.clone(),
                    &gate,
                    &consumer,
                    memory,
                    need,
                    |available| progress.waiting_for_capacity(bytes, available),
                    bind,
                )
                .await
                .map_err(FineTuningError::Transient)?;
                let granted = reservation.bytes();
                residency = Some((store, crate::inference::engine_residency::ResidentWork { reserved_bytes: granted, ..bound }));
                // the engine refuses a graph over the lease it was GRANTED, before allocating it
                body.memory_budget_mib = Some(granted / (1024 * 1024));
                Some(reservation)
            } else {
                None
            };
            let last = Arc::new(Mutex::new(None));
            let run = EngineRun {
                http,
                lane,
                shape: spec.shape.clone(),
                footprints: job_footprints.clone(),
                post_wait: post_wait_for(render_per_example, body.examples.len(), post_wait_floor),
                start: Some(body),
                out: out.clone(),
                adapter_path: spec.adapter_path.clone(),
                job_dir: spec.job_dir.clone(),
                epochs,
                last: last.clone(),
                holds,
                job: id,
                hold_store,
                residency,
                _lane_hold: lane_hold,
                _lease: reservation,
            };
            Ok(PreparedJob {
                execution: Execution::InPlace(Box::new(run)),
                finish: finish_run(id, spec, job_footprints, last),
            })
        }))
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        self.jobs.poll(handle)
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        self.jobs.cancel(handle)
    }

    async fn reattach(&self, local_id: Uuid) -> Result<ReattachOutcome, FineTuningError> {
        self.reattach_job(local_id).await
    }
}

/// Her demand for the examples this finished run TRAINED is met (card ea157b1a): the partition as
/// amended with what the run skipped, against the request it was cut from. Only a finish clears,
/// so a run that fails or is cancelled after being sent leaves her demand as it was.
fn clear_trained_demand(job: Uuid, spec: &EngineRunSpec) {
    use crate::genome::fine_tuning::training_demand::TrainingDemand;
    let Some(store) = spec.demand_store.as_deref() else { return };
    let cleared = crate::genome::fine_tuning::job_dir::read_job_request(&spec.job_dir).and_then(|request| {
        let partition = read_partition(&spec.job_dir)?.ok_or("the job recorded no partition")?;
        let trained = TrainingDemand::trained(&request.dataset.examples, &partition);
        TrainingDemand::in_dir(Some(store), request.persona_id).record(&[], &trained, None).map_err(|e| e.to_string())?;
        Ok(trained.len())
    });
    match cleared {
        Ok(n) => continuum_base::probe!(class = "training.demand.trained", job = %job, cleared = n as u64, "the examples this run trained owe serving nothing more"),
        Err(e) => continuum_base::probe!(class = "training.demand.uncleared", job = %job, error = %e,
            "the trained examples' demand could not be cleared: her lane may hold a window for examples already trained until the next partition re-records her demand"),
    }
}

/// A run's finish: collect its adapter into the job directory, read the engine's receipts, and
/// file the footprint under the geometry that ran. ONE body for a run this core started and one
/// it re-attached to (step 3), because both finish from the same [`EngineRunSpec`].
pub(super) fn finish_run(
    job: Uuid,
    spec: EngineRunSpec,
    footprints: Footprints,
    last: Arc<Mutex<Option<TrainStatus>>>,
) -> Box<dyn FnOnce(u64) -> Result<TrainingArtifact, String> + Send> {
    Box::new(move |wall_clock_ms| {
        let adapter = move_adapter(&spec.adapter_path, &spec.job_dir)?;
        let status = last
            .lock()
            .ok()
            .and_then(|l| l.clone())
            .ok_or("the engine's final status was never read")?;
        let final_loss = status.epochs.last().map(|e| e.train_loss);
        let final_validation_loss = held_out_loss(&status, f32::from(spec.val_split));
        let trainable = status
            .trainable_tokens
            .ok_or("the engine reported no trainable-token count")?;
        if trainable == 0 || final_loss.is_none_or(|l| !l.is_finite()) {
            return Err("the engine produced no finite measured learning receipt".into());
        }
        // the footprint this shape needs: the training graph the engine measured before
        // allocating it (exact, where a VRAM sample could miss the peak)
        let grown = status.run_bytes();
        // recorded under the depth the engine ACTUALLY adapted: an engine that
        // ignored top_layers measured a full-depth graph, and that number must never
        // be leased for a reduced-depth run (Codex on #27)
        continuum_base::probe!(
            class = "training.job.examples_fit",
            job = %job,
            kept = status.examples.unwrap_or(0), // probe field: 0 = an engine that does not report it
            truncated = status.examples_truncated.unwrap_or(0), // probe field: as above
            skipped = status.examples_skipped.unwrap_or(0), // probe field: as above
            "how her examples met the window: kept whole, fitted by dropping their oldest history, or skipped"
        );
        // the run is the authority on what trained: an example IT skipped is parked, never
        // counted as trained (same rule as the measure, so normally none)
        if !status.skipped_indices.is_empty() {
            match read_partition(&spec.job_dir) {
                Ok(Some(partition)) => {
                    let amended = with_run_skipped(partition, &status.skipped_indices);
                    if let Err(e) = write_partition(&spec.job_dir, &amended) {
                        continuum_base::probe!(class = "training.job.partition_unamended", job = %job, error = %e, "the run's skipped examples could not be parked; they remain in the job's request");
                    }
                }
                Ok(None) => {}
                Err(e) => continuum_base::probe!(class = "training.job.partition_unamended", job = %job, error = %e, "the partition could not be read, so the run's skipped examples could not be parked; they remain in the job's request"),
            }
        }
        clear_trained_demand(job, &spec);
        let adapted = effective_depth(status.layers_adapted, status.n_layer);
        if adapted != spec.shape.depth {
            continuum_base::probe!(
                class = "training.job.depth_differs",
                job = %job,
                asked = spec.shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                adapted = adapted.map_or(0, u64::from), // probe field: 0 = every block
                "the engine adapted a different depth than was asked (an engine without top_layers adapts every block)"
            );
        }
        // The footprint is filed under the geometry that RAN (fork #30 sizes the graph
        // to the data, the served window is only its ceiling): a graph measured at a
        // 15k window must never be leased for a 61k one (Codex on #4498).
        let ran_window = status.window.filter(|w| *w > 0).unwrap_or(spec.shape.window);
        if ran_window != spec.shape.window {
            continuum_base::probe!(
                class = "training.job.window_used",
                job = %job,
                sent = u64::from(spec.shape.window),
                used = u64::from(ran_window),
                "the engine sized its training graph to the data: the footprint is filed under the window that ran"
            );
        }
        // …and under the chunk the engine ran at that window: the engine's rule applied to
        // the chunk the core asked for (`train_chunk_for`), never the asked number as sent.
        let ran_chunk = status.chunk.unwrap_or_else(|| train_chunk_for(ran_window, spec.shape.chunk)); // unwrap_or_else: an engine that reports no chunk ran the mirrored rule
        let measured_shape = Shape { depth: adapted, window: ran_window, chunk: ran_chunk, ..spec.shape.clone() };
        // A depth equal to the model's block count IS full depth (fork #27 refuses one
        // past it): filed under the asked key too, or that request would calibrate on
        // every run (Cormac on #4472).
        let asked_full = matches!((spec.shape.depth, status.n_layer), (Some(k), Some(n)) if k >= n);
        if grown > 0 {
            let store = footprints;
            if asked_full {
                // the asked depth, at the window that ran
                let asked_ran = Shape { window: ran_window, chunk: ran_chunk, ..spec.shape.clone() };
                let _ = store.record_with(&asked_ran, grown, status.context_mib.is_some(), status.n_layer, job); // best effort: the full-depth row below is the one that matters
            }
            if let Err(e) = store.record_with(&measured_shape, grown, status.context_mib.is_some(), status.n_layer, job) {
                continuum_base::probe!(
                    class = "training.job.footprint_unrecorded",
                    job = %job,
                    error = %e,
                    "the measured engine-training footprint could not be recorded; the next run of \
                     this shape calibrates again"
                );
            }
        }
        // the measured cost of this run: what the next dispatch of this base is sized by
        if let Some(rates) = super::super::training_rate::TrainingRates::in_home() {
            if let Err(e) = rates.record(&spec.shape.model, ran_chunk, spec.corpus_chars, spec.epochs, wall_clock_ms, job) {
                continuum_base::probe!(class = "training.job.rate_unrecorded", job = %job, error = %e, "the run's measured cost could not be recorded; the next dispatch of this base calibrates at the bucket's threshold");
            }
        }
        Ok(TrainingArtifact {
            model_id: spec.model_id.clone(),
            local_path: Some(adapter),
            format: ArtifactFormat::GgufLora,
            metrics: JobMetrics {
                walk: spec.walk.clone(),
                trained_tokens: trainable * status.epochs.len() as u64,
                final_loss,
                final_validation_loss,
                wall_clock_ms,
                layers_adapted: status.layers_adapted.or(status.n_layer),
                trained_on: spec.trained_on.clone(),
                ..Default::default()
            },
        })
    })
}

/// One example as `/train` reads it. A lived call goes as its served conversation: the
/// system prompt and every served message framed exactly as serving framed them
/// ([`crate::inference::request_body::wire_messages`]: her earlier tool calls kept, tool
/// results as `role: tool`), then her reply with its reasoning and tool calls. Earlier
/// assistant turns in that history are context (`"train": false`), not this lesson. The tools she was offered ride along so the rendered prompt has the tool
/// block she saw. Anything else goes as `{prompt, completion}`.
pub(super) fn engine_example(e: &TrainingExample) -> EngineExample {
    let Some(call) = e.lived.as_ref() else {
        return EngineExample::Pair {
            prompt: e.prompt.clone(),
            completion: e.completion.clone(),
        };
    };
    use crate::inference::request_body::{close_trailing_assistant, wire_messages, wire_tool_call};
    // The history exactly as serving framed it; images drop (the trainer is text-only,
    // and a text model was served the description bridge anyway).
    let mut messages = wire_messages(
        &call.request.messages,
        call.request.system_prompt.as_deref(),
        false,
        false,
        PROVIDER_ID,
    );
    // serving closes a history that ends in her own turn before she replies (a self-tick's
    // continuation); without it the trained render has two assistant turns in a row
    close_trailing_assistant(&mut messages);
    for m in messages.iter_mut().filter(|m| m["role"] == "assistant") {
        m["train"] = json!(false);
    }
    let r = &call.response;
    let calls: Vec<Value> = r
        .tool_calls
        .iter()
        .flatten()
        .map(|c| wire_tool_call(&c.id, &c.name, &c.input))
        .collect();
    let mut reply = json!({"role": "assistant", "content": r.text});
    if let Some(reasoning) = r.reasoning.as_deref().filter(|s| !s.is_empty()) {
        reply["reasoning_content"] = json!(reasoning);
    }
    if !calls.is_empty() {
        reply["tool_calls"] = json!(calls);
    }
    messages.push(reply);
    EngineExample::Conversation {
        messages,
        tools: call
            .request
            .tools
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|tools| json!(crate::inference::request_body::openai_tools(tools))),
    }
}
