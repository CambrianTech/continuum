use super::*;

/// How many times an act's deadline may be extended while its generation advances: the
/// ceiling is `1 + ACT_EXTENSIONS` bounds, so even a moving act cannot hold the lane past
/// it (a slow clip still reaches every mind, card ef25bf6c).
// derived-or-floor: a ceiling — 3x the act bound total (Fable on 6f3218ed); it only ENDS a wait, never shortens one.
pub(super) const ACT_EXTENSIONS: u32 = 2;

/// Await one settle step under the act bound, where the bound is a CHECKPOINT while the
/// act's own generation advances (card 6f3218ed). IntelMac, 2026-09-27: five of seven
/// cancellations in an hour were generations dropped at 24.6 to 25.0 min, alive and queued
/// or prefilling, because the bound was pure elapsed time and the measured turn that
/// would raise it is what this tier rarely completes. At the deadline the persona's
/// pinned slot is read (`llama_server::in_flight_slot`): processing at the first
/// checkpoint, or the same task with its work moved at a later one, extends by one bound,
/// up to [`ACT_EXTENSIONS`]. No pinned slot, an idle or silent slot, or a new task on it
/// ends the act exactly as before (`Err(Elapsed)`), so a request lost inside a busy engine
/// still ends. Registered admission queue intervals are excluded from this
/// step's watchdog clock: they retain FIFO ownership without a pinned engine
/// slot while relevant grants or blocking holders advance at each act-bound
/// checkpoint. A stalled/unknown queue ends the act. On grant the unspent budget
/// resumes; no other await gets this exemption.
///
/// `Err` says whether the step's generation was already dispatched: a step ended after
/// that point dropped a generation in flight and must never be counted as a lane wait.
///
/// A condemned engine ends the step AT ONCE, not at its next checkpoint: the relaunch's
/// drain waits for this step's guard, so every second held here is the whole node dark
/// (card 4d2a64a2: 23 minutes on enrico, 2026-10-26). Condemned means the daemon decided to
/// replace the engine (`llama_server::condemn`), never a transient not-ready snapshot.
pub(super) async fn until_act_stalls<F: std::future::Future>(
    deadline: tokio::time::Instant,
    bound: std::time::Duration,
    persona: Option<uuid::Uuid>,
    step: F,
) -> Result<F::Output, Abandoned> {
    until_act_stalls_or(deadline, bound, persona, step, crate::inference::llama_server::until_condemned()).await
}

/// [`until_act_stalls`] with the condemnation it listens for given, so a test can raise it.
pub(super) async fn until_act_stalls_or<F: std::future::Future, C: std::future::Future<Output = ()>>(
    deadline: tokio::time::Instant,
    bound: std::time::Duration,
    persona: Option<uuid::Uuid>,
    step: F,
    condemned: C,
) -> Result<F::Output, Abandoned> {
    use crate::cognition::llm_deliberation_faculty::admission_clock;
    let (queue_tx, mut queue_rx) = tokio::sync::watch::channel(admission_clock::QueueTime::new(deadline, bound));
    let step = admission_clock::scope(queue_tx.clone(), step);
    tokio::pin!(step);
    tokio::pin!(condemned);
    let mut extensions = 0u32;
    let mut previous = None;
    loop {
        let state = *queue_rx.borrow_and_update();
        let due = if state.stalled { Some(tokio::time::Instant::now()) } else { state.deadline() };
        tokio::select! {
            biased;
            out = &mut step => return Ok(out),
            _ = queue_rx.changed() => continue,
            () = &mut condemned => {
                let abandoned = Abandoned::at(&queue_rx);
                continuum_base::probe!(
                    class = "settle.tick.condemned",
                    dispatched = abandoned.dispatched,
                    extensions = extensions as u64,
                    "the engine was condemned mid-act: ending the act now, so the relaunch's drain is not held to its next checkpoint"
                );
                return Err(abandoned);
            }
            _ = async {
                match due {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                if state.stalled { return Err(Abandoned::at(&queue_rx)); }
                let pinned = persona.and_then(crate::inference::llama_server::in_flight_slot);
                let now = match &pinned {
                    Some(p) => crate::inference::llama_server::slot_work(&p.root, p.slot, act_probe_client()).await,
                    None => None,
                };
                // A condemned engine is never extended on: its relaunch cuts this act anyway,
                // and holding on keeps that relaunch's drain (and the whole node) waiting.
                if extensions >= ACT_EXTENSIONS
                    || crate::inference::llama_server::engine_condemned()
                    || !crate::inference::llama_server::slot_advanced(previous, now)
                {
                    return Err(Abandoned::at(&queue_rx));
                }
                extensions += 1;
                previous = now;
                queue_tx.send_modify(|state| state.extend());
                continuum_base::probe!(
                    class = "settle.tick.extended",
                    slot = pinned.as_ref().map(|p| p.slot as u64).unwrap_or(0), // probe field: extension implies a pinned slot
                    extensions = extensions as u64,
                    bound_s = bound.as_secs(),
                    "the act reached its deadline with its generation still advancing on its pinned slot — busy, not dead; one more bound"
                );
            }
        }
    }
}

/// A step its deadline ended, and whether its generation had been dispatched by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Abandoned {
    pub(super) dispatched: bool,
}

impl Abandoned {
    fn at(clock: &tokio::sync::watch::Receiver<crate::cognition::llm_deliberation_faculty::admission_clock::QueueTime>) -> Self {
        Self { dispatched: clock.borrow().dispatched }
    }
}

/// One client for the act deadline's slot reads, built once per process.
pub(super) fn act_probe_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

// The working-memory trail-head bound lives in `working_memory.rs` now (its home — WM owns
// its own truncation). Still used here for the settlement answer-head.
/// Drive the mind to SETTLEMENT: tick → if `Act`, run it + fold the observation
/// into the next perception → re-tick → until it `Speak`s/`Pass`es or the
/// external `max_acts` budget is spent.
///
/// `max_acts` is the EXTERNAL observer's stopwatch — the grader holds it because
/// the eval room has no heartbeat to pace re-perception. It is NOT a cap that
/// lives in the persona's head (the live path has no such bound; an "acts
/// forever" persona is a fitness gap to train away, never a substrate ceiling —
/// §4). When the budget runs out mid-action, the final un-driven `Act` is
/// returned and the grader scores it as unfinished — never a fabricated answer.
///
/// ## Why the lived-experience write lives HERE and not at the call sites
///
/// This function is the ONE place a `SettleOutcome` is produced, so it is the one
/// place "a turn was lived" can be recorded without the fact being re-derived per
/// caller. It was not always: the #319 producer was first wired into a SINGLE
/// service_loop call site (the directed-message path), which left the self-tick
/// path and the held-work path settling turns that no experience record ever
/// described. Three callers, one of them remembering — the missing-constraint
/// shape ([[the-same-bug-at-two-sites-is-a-missing-constraint]]), and the reason
/// zero `LivedTurn` records existed on disk while citizens were demonstrably
/// deliberating.
///
/// Recording once around the driver — rather than at each of its four return
/// paths — is deliberate for the same reason: a fifth return path added later
/// inherits the record instead of silently opting out of learning.
///
/// The write is gated on [`WorkspaceCycle::acting`] because that is where a
/// citizen's identity lives. A cycle with no `ActingBody` is pure cognition (a
/// faculty test, a replay) — it is nobody's lived experience, so there is no
/// stream it belongs in. That is a structural absence, not a skipped write.
/// Consecutive spoken plans a workspace-deliverable turn tolerates before it
/// settles without a deliverable (each is re-perceived, not ended).
pub(in crate::cognition::act_observe) const NARRATION_BUDGET: usize = 3;

/// WHAT A SETTLE ITERATION WAS — an act, or a wait wearing an act's clothes.
///
/// Measured on the M5 2026-09-20 ~21:55Z (build 0047d521b, 4 residents sharing 2 lanes
/// at 67,072): the hour's `persona.act.pace` rows carried act_secs 164, 249, 365, 866,
/// 920, 1411 and 1500, and TWO of them read `model_ms=0` with `residue_ms` equal to the
/// WHOLE act — 365,000 and 1,500,001. The same minds' `delib.gate.lane_wait` read
/// `lanes_available=0`. Those two iterations never reached the model: a mind burned 6
/// minutes and another 25, produced nothing, and the turn charged the time to her ACT
/// pace anyway, so the rolling mean climbed to 693–1018 s on a node whose acts were
/// mostly a queue. A number that only a wait can raise is not a measurement of work
/// ([[a-measurement-that-can-only-rise-is-not-a-measurement]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActClass {
    /// The model was reached — this iteration is an act of cognition.
    Act,
    /// It never reached the model: no generation dispatched, no metrics. She waited
    /// on a lane she did not get. It is NOT an act and must not be counted as one.
    LaneStarved,
}

/// PURE: classify one settle iteration. `model_ms` is the act's generation wall-time;
/// `generations` is how many generation receipts the step dispatched.
///
/// A FAULTED call still pushes a receipt (`GenerationReceipt::faulted`), so a model
/// that answered badly is an act that reached the model — never a starve. Only an
/// iteration that dispatched NOTHING is a wait.
pub(crate) fn classify_act(model_ms: u64, generations: usize) -> ActClass {
    if model_ms == 0 && generations == 0 {
        ActClass::LaneStarved
    } else {
        ActClass::Act
    }
}

/// A first-dispatch refusal can preserve the original input. Prior model faults
/// or completed actions cannot be replayed even if their provenance was absent.
pub(super) fn can_defer_input(generations: usize, acts: usize, recorded_acts: usize, retries: u32) -> bool {
    generations == 0 && acts == 0 && recorded_acts == 0 && retries == 0
}

/// THE TURN'S OWN PACE CADENCE — the rolling mean each act's wall-clock is judged
/// against. Only an [`ActClass::Act`] is a sample: a lane wait is real elapsed time
/// but it is not a sample of how fast she THINKS, and folding it in is what took the
/// mean to 1018 s above. A starved iteration advances nothing here.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PaceCadence {
    pub(super) sum_secs: f64,
    pub(super) samples: u32,
}

impl PaceCadence {
    /// Record one iteration. Returns whether it advanced the cadence.
    pub(crate) fn record(&mut self, act_secs: f64, class: ActClass) -> bool {
        if class == ActClass::Act {
            self.sum_secs += act_secs;
            self.samples += 1;
            true
        } else {
            false
        }
    }
    pub(crate) fn mean_secs(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.sum_secs / self.samples as f64
        }
    }
    pub(crate) fn samples(&self) -> u32 {
        self.samples
    }
    /// "Slow" is relative to THIS turn's own pace, never a constant: twice the rolling
    /// mean, from the 4th sample on (the old `acts >= 3` written in the cadence's own
    /// unit, so a turn whose early iterations were waits still needs four real acts
    /// before anything is called slow).
    pub(crate) fn is_slow(&self, act_secs: f64) -> bool {
        self.samples >= 4 && act_secs > self.mean_secs() * 2.0
    }
}

pub fn drive_to_settle(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    max_acts: usize,
    framing: TurnFraming,
) -> impl std::future::Future<Output = SettleOutcome> + Send + '_ {
    drive_with_input(cycle, burst.into(), max_acts, framing, None, None)
}

/// The live driver leases its existing conversation between steps. No second
/// runner, cancellation of an in-flight tool, or replacement task is involved.
pub fn drive_to_settle_with_input<'a>(
    cycle: &'a WorkspaceCycle,
    burst: Burst,
    max_acts: usize,
    framing: TurnFraming,
    conversation: &'a mut dyn crate::cognition::conversation::PersonaConversation,
) -> impl std::future::Future<Output = SettleOutcome> + Send + 'a {
    drive_with_input(cycle, burst, max_acts, framing, Some(conversation), None)
}

/// A live card-linked turn uses the same driver with its own credit capture.
/// The optional owner never changes the activity's decision or assigns a verdict.
pub fn drive_to_settle_with_credit<'a>(
    cycle: &'a WorkspaceCycle,
    burst: Burst,
    max_acts: usize,
    framing: TurnFraming,
    conversation: &'a mut dyn crate::cognition::conversation::PersonaConversation,
    credit: Option<&'a mut dyn TurnCredit>,
) -> impl std::future::Future<Output = SettleOutcome> + Send + 'a {
    // Return the shared driver directly: an async forwarding wrapper needlessly
    // nests its state in every live/eval caller and inflates future layout depth.
    drive_with_input(cycle, burst, max_acts, framing, Some(conversation), credit)
}

pub(super) async fn drive_with_input<'c>(
    cycle: &WorkspaceCycle,
    burst: Burst,
    max_acts: usize,
    framing: TurnFraming,
    conversation: Option<&mut dyn crate::cognition::conversation::PersonaConversation>,
    mut credit: Option<&mut (dyn TurnCredit + 'c)>,
) -> SettleOutcome {
    let turn_room = burst.room.as_uuid();
    let started = std::time::Instant::now();
    let settled = settle_to_outcome(
        cycle,
        burst,
        max_acts,
        framing,
        conversation,
        credit.as_deref_mut(),
    )
    .await;
    // Her service ledger counts EVERY turn, refusals included, before any early return: a
    // mind refused before inference is the stall this exists to see (card 1c157668).
    if let Some(body) = cycle.acting() {
        let wall_ms = u64::try_from(started.elapsed().as_millis()).map_or(u64::MAX, |ms| ms); // past u64::MAX ms saturates; no turn reaches it
        crate::cognition::resident_service::note_settled(body.persona_id, &settled, wall_ms);
    }
    // Admission refusal is not a lived/completed turn or training credit.
    if settled.deferred_before_dispatch {
        return settled;
    }
    // A turn in her mind room is hers (PRIVACY-OF-THOUGHT.md §4, sink 9): it becomes neither
    // curriculum (experience.jsonl) nor training credit. Until her sealed private space exists
    // (build step 2) it is not written at all.
    let private = cycle
        .acting()
        .is_some_and(|body| crate::cognition::mind_room::is_private_room(body.persona_id, turn_room));
    if private {
        if let Some(body) = cycle.acting() {
            crate::cognition::mind_room::note_withheld(body.persona_id, "experience_and_credit");
        }
        return settled;
    }
    if let Some(body) = cycle.acting() {
        crate::cognition::extensions::lived_turn_settled(
            &crate::paths::continuum_root(),
            crate::identity::PeerId::from_uuid(body.persona_id),
            &settled,
        );
    }
    if let Some(capture) = credit {
        capture
            .record(
                &settled.turn_acts,
                &settled.generation_receipts,
                settled.spoken.as_deref(),
                true,
            )
            .await;
    }
    settled
}
