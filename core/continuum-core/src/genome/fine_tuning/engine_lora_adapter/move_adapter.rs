use super::*;

/// The adapter leaves the lanes' train dir for the job's own directory. Runs in `finish`, i.e.
/// BEFORE the job goes terminal: the engine-train sweep deletes every file whose job is not live,
/// so a finished gene left there would be deleted.
pub(super) fn move_adapter(from: &Path, job_dir: &Path) -> Result<PathBuf, String> {
    let size = std::fs::metadata(from).map_err(|e| format!("the engine wrote no adapter at {}: {e}", from.display()))?.len();
    if size == 0 {
        return Err(format!("the engine's adapter at {} is empty", from.display()));
    }
    let adapters = job_dir.join("adapters");
    std::fs::create_dir_all(&adapters).map_err(|e| format!("{}: {e}", adapters.display()))?;
    let to = adapters.join("adapter.gguf");
    std::fs::rename(from, &to).map_err(|e| format!("moving {} -> {}: {e}", from.display(), to.display()))?;
    Ok(to)
}

/// What re-attaching decides from what it observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Disposition {
    Attach,
    Gone(String),
    Uncertain(String),
}

/// PURE: the step-3 decision, from what was observed in order: the bound incarnation's liveness
/// BEFORE asking, the engine's answer, and its liveness AFTER (the answer is this engine's only
/// if the process did not change under the round trip). Only positive evidence decides:
/// the same live incarnation answering with THIS run attaches, and a verifiably dead one is
/// gone. An engine that answers with another run, or not at all, is not proof this run ended
/// (Codex on the step-3 plan): that is uncertain, and uncertain never resumes.
pub(super) fn reattach_disposition(
    before: crate::inference::engine_residency::Liveness,
    has_spec: bool,
    answer: Option<Result<TrainStatus, String>>,
    after: crate::inference::engine_residency::Liveness,
    out: &str,
) -> Disposition {
    use crate::inference::engine_residency::Liveness;
    match before {
        Liveness::Dead => return Disposition::Gone("its bound engine incarnation is verifiably gone".into()),
        Liveness::Unknown => return Disposition::Uncertain("its bound engine's liveness cannot be read".into()),
        Liveness::Alive => {}
    }
    if !has_spec {
        return Disposition::Uncertain("the binding predates job_spec, so the run cannot be watched again here".into());
    }
    match answer {
        None => return Disposition::Uncertain("the engine was not asked".into()),
        Some(Err(e)) => return Disposition::Uncertain(format!("the engine did not answer: {e}")),
        Some(Ok(s)) if s.out.as_deref() != Some(out) => {
            return Disposition::Uncertain(format!("the engine reports run {:?}, not this one: not proof this run ended", s.out))
        }
        Some(Ok(_)) => {}
    }
    match after {
        Liveness::Alive => Disposition::Attach,
        Liveness::Dead => Disposition::Gone("its engine died while it was being asked".into()),
        Liveness::Unknown => Disposition::Uncertain("its engine's liveness could not be re-read after the answer".into()),
    }
}

impl EngineLoraFineTuner {
    /// BOOT, before anything can admit (Cormac on the step-3 plan): re-take the governed lease
    /// of every run a previous core bound to an engine that may still be running it. The lease
    /// lived in the dead core's memory; the engine's allocation did not, so until this runs a
    /// second job could be admitted into memory a surviving run is using. Called once, right
    /// after the adapter is built and before any module ticks; a binding whose engine is
    /// verifiably dead takes no lease (re-attach releases it).
    pub fn reclaim_resident_leases(&self) {
        if let Some(daemon) = crate::resources::ResourceDaemon::global() {
            self.reclaim_resident_leases_in(&daemon);
        }
    }

    /// [`Self::reclaim_resident_leases`] against `daemon`. Each lease is re-taken in the pool
    /// its binding recorded (`ResidentWork::bound_memory`), never the pool the current
    /// configuration would choose: the run survived in the pool it was granted.
    pub(super) fn reclaim_resident_leases_in(&self, daemon: &std::sync::Arc<crate::resources::ResourceDaemon>) {
        use crate::inference::engine_residency::Liveness;
        let Some(store) = self.residency_store.as_ref() else {
            return;
        };
        let bound = match crate::inference::engine_residency::all(store) {
            Ok(bound) => bound,
            Err(error) => {
                continuum_base::probe!(
                    class = "training.reattach.store_unreadable",
                    error = %error,
                    "engine bindings cannot be read at boot: ownership is unknown, so serving stays off every engine it guards"
                );
                return;
            }
        };
        for work in bound {
            if work.engine.liveness() == Liveness::Dead || work.reserved_bytes == 0 {
                continue;
            }
            let consumer = if work.consumer.is_empty() { format!("genome-train:{}", work.job) } else { work.consumer.clone() };
            match daemon.acquire_guarded(&crate::forge::training_admission::request(&consumer, work.bound_memory(), work.reserved_bytes)) {
                Ok(guard) => {
                    if let Ok(mut leases) = self.resident_leases.lock() {
                        leases.insert(work.job, guard);
                    }
                    continuum_base::probe!(
                        class = "training.reattach.lease_reclaimed",
                        job = %work.job,
                        bytes = work.reserved_bytes,
                        "a run bound to a live engine by a previous core: its lease is re-taken before anything can admit"
                    );
                }
                Err(error) => continuum_base::probe!(
                    class = "training.reattach.lease_refused",
                    job = %work.job,
                    bytes = work.reserved_bytes,
                    error = %format!("{error:?}"),
                    "a surviving run's lease could not be re-taken: its memory is in use but unaccounted"
                ),
            }
        }
    }

    /// Step 3: re-attach to `job` if a previous core bound it to an engine that is still running
    /// it, under the SAME id and without POSTing it again. See [`ReattachOutcome`].
    pub(super) async fn reattach_job(&self, job: Uuid) -> Result<ReattachOutcome, FineTuningError> {
        let Some(store) = self.residency_store.clone() else {
            return Ok(ReattachOutcome::NotResident);
        };
        let lookup = store.clone();
        let found = tokio::task::spawn_blocking(move || crate::inference::engine_residency::find(&lookup, job))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        let work = match found {
            Ok(Some(work)) => work,
            Ok(None) => return Ok(ReattachOutcome::NotResident),
            // an unreadable store is unknown ownership: never a reason to start the job again
            Err(error) => return Ok(ReattachOutcome::Uncertain { reason: format!("engine bindings cannot be read: {error}") }),
        };
        let liveness = |engine: crate::inference::engine_residency::EngineIncarnation| async move {
            tokio::task::spawn_blocking(move || engine.liveness())
                .await
                .unwrap_or(crate::inference::engine_residency::Liveness::Unknown) // unwrap_or: a probe that could not run proves nothing
        };
        let lane = work.engine.root_url();
        let before = liveness(work.engine).await;
        let answer = if before == crate::inference::engine_residency::Liveness::Alive && work.job_spec.is_some() {
            let probe = EngineRun::status_at(&self.http, &lane).await;
            Some(probe)
        } else {
            None
        };
        let after = liveness(work.engine).await;
        match reattach_disposition(before, work.job_spec.is_some(), answer, after, &work.out) {
            Disposition::Gone(reason) => {
                let (release_store, bound) = (store.clone(), work.clone());
                let released = tokio::task::spawn_blocking(move || crate::inference::engine_residency::release(&release_store, &bound)).await;
                if let Ok(mut leases) = self.resident_leases.lock() {
                    leases.remove(&job);
                }
                continuum_base::probe!(
                    class = "training.reattach.engine_gone",
                    job = %job,
                    released = matches!(released, Ok(Ok(true))),
                    reason = reason.as_str(),
                    "a previous core's run: its engine is verifiably gone, so its binding is released and the job may resume from its input"
                );
                Ok(ReattachOutcome::EngineGone { reason })
            }
            Disposition::Uncertain(reason) => {
                continuum_base::probe!(
                    class = "training.reattach.uncertain",
                    job = %job,
                    pid = work.engine.pid as u64,
                    reason = reason.as_str(),
                    "a previous core's run may still be training: its binding and lease stay, serving stays off the engine, and it is NOT started again"
                );
                Ok(ReattachOutcome::Uncertain { reason })
            }
            Disposition::Attach => {
                let Some(held) = work.job_spec.as_ref() else {
                    return Ok(ReattachOutcome::Uncertain { reason: "the binding carries no job_spec".into() });
                };
                let spec: EngineRunSpec = match held.read() {
                    Ok(spec) => spec,
                    Err(e) => return Ok(ReattachOutcome::Uncertain { reason: format!("the binding's job_spec is not this build's run spec: {e}") }),
                };
                let lease = self.resident_leases.lock().ok().and_then(|mut leases| leases.remove(&job));
                if lease.is_none() {
                    continuum_base::probe!(
                        class = "training.reattach.unleased",
                        job = %job,
                        "re-attached without a reclaimed lease: the run's memory is in use but not on the board"
                    );
                }
                let last = Arc::new(Mutex::new(None));
                let run = EngineRun {
                    shape: spec.shape.clone(),
                    footprints: self.footprints.clone(), // a re-attached run's engine build is not known: its rows are filed without one
                    http: self.http.clone(),
                    lane,
                    start: None,
                    post_wait: POST_WAIT_FLOOR, // a re-attached run is never POSTed
                    out: work.out.clone(),
                    adapter_path: spec.adapter_path.clone(),
                    job_dir: spec.job_dir.clone(),
                    epochs: spec.epochs,
                    last: last.clone(),
                    holds: self.holds.clone(),
                    job,
                    hold_store: self.hold_store.clone(),
                    residency: Some((store, work)),
                    _lane_hold: None, // a reattached run trains in the lane that still holds it
                    _lease: lease,
                };
                let finish = finish_run(job, spec, self.footprints.clone(), last);
                let handle = self.jobs.prepare(job, move |_progress| async move {
                    Ok(PreparedJob { execution: Execution::InPlace(Box::new(run)), finish })
                });
                continuum_base::probe!(
                    class = "training.reattach.attached",
                    job = %job,
                    "a run a previous core started is still this job's in its engine: watched again under the same id, never POSTed again"
                );
                Ok(ReattachOutcome::Attached { handle })
            }
        }
    }
}
