use super::*;

/// The slow clip's yield: each chosen seat's checkpoint is flushed and the seat yields, and she
/// goes to the back of the draw (no resting record) to come round in turn.
pub(super) async fn rest_seats(chosen: Vec<(uuid::Uuid, MindHour)>, h: &CitizenHealth) -> Vec<String> {
    if chosen.is_empty() {
        return Vec::new();
    }
    let Some(registry) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() else {
        return Vec::new();
    };
    // Her checkpoint first — the flush is the explicit, repeatable save, on a
    // blocking worker like every other checkpoint pass (modules/cognition.rs).
    let flushed = tokio::task::spawn_blocking(|| {
        crate::cognition::persona_workspace::global().flush_volatile_all()
    })
    .await
    .unwrap_or_default(); // unwrap_or_default: a panicked flush task = no checkpoint receipts; the probe below then says checkpoint_saved=false
    let mut out = Vec::new();
    for (persona, m) in chosen {
        let saved = flushed.iter().any(|(id, r)| *id == persona && r.is_ok());
        let Some(runtime) = registry.shutdown_slot(persona).await else {
            continue; // already gone this tick
        };
        let agent_name = runtime.agent_name().to_string();
        // Not paged out for cause: no resting record, which would keep her out of
        // the draw. She goes to the back of the draw order and comes round in turn.
        crate::persona::resting_seat::requeue(&agent_name);
        continuum_base::probe!(
            class = "persona.rotation.rested",
            persona = %agent_name,
            persona_id = %persona,
            resident = h.resident,
            lanes = h.lanes,
            lane_grants = m.lane_grants,
            checkpoint_saved = saved,
            "the slow clip: a dormant mind waits and this seat's hour held no act — she yields the seat with her checkpoint and returns in turn"
        );
        out.push(agent_name);
    }
    out
}

pub struct CitizenHealthModule;

impl CitizenHealthModule {
    /// The slow clip's actor: every resident on this node (a mind with no activity this
    /// hour has no row, and she is the quietest of all), the pure choice, then the first
    /// candidate who holds no live claim yields her seat. A claim read that fails counts
    /// as holding one: a mind is never rotated off work on a guess.
    pub(super) async fn rotate_one(&self, minds: &[(uuid::Uuid, MindHour)], h: &CitizenHealth) -> Vec<String> {
        // Someone in THIS node's own draw order waits; a mind dormant elsewhere is that
        // node's to rotate in, and a rotation here would redraw the same resident.
        let dormant_waiting = crate::persona::resting_seat::waiting() > 0;
        let Some(registry) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() else {
            return Vec::new();
        };
        let residents: Vec<(uuid::Uuid, MindHour)> = registry
            .ids()
            .into_iter()
            .filter_map(|id| {
                let rt = registry.get(id)?;
                let hour = minds.iter().find(|(m, _)| *m == id).map(|(_, m)| m.clone());
                Some((id, hour.unwrap_or_else(|| MindHour { agent_name: rt.agent_name().to_string(), ..MindHour::default() }))) // unwrap_or_else: no row this hour = no verdict, act, write or grant: the truth
            })
            .collect();
        let now_ms = continuum_base::time::EpochMs::now().millis();
        let candidates = rotation_candidates(
            dormant_waiting,
            &residents,
            crate::persona::resting_seat::seated_since,
            now_ms,
        );
        for (id, hour) in candidates {
            let Some(rt) = registry.get(id) else { continue };
            let holds_work = crate::persona::held_work::AircWorkReader::active_claims(rt.as_ref())
                .await
                .map_or(true, |cards| !cards.is_empty());
            if holds_work {
                continue;
            }
            return rest_seats(vec![(id, hour)], h).await;
        }
        Vec::new()
    }

    pub fn new() -> Self {
        WINDOW_OPENED_MS.store(EpochMs::now().millis(), Ordering::Relaxed);
        Self
    }
    /// Close the hour: its counters, reset, as the tick reads them. The window is what the
    /// counters actually cover (from the last close), and the closed hour is kept for readers.
    pub(super) fn read(&self) -> CitizenHealth {
        let hour = assemble(Read::Close);
        WINDOW_OPENED_MS.store(EpochMs::now().millis(), Ordering::Relaxed);
        *LAST_HOUR.lock() = Some(hour.clone());
        hour
    }
}

/// When the current window opened: the last close, or the module's start. A window's length
/// is this measurement, never the configured hour (2026-11-02: read just after a close, the
/// command reported "last 60 min: acts 0 ... IDLE" while 66 acts had landed that hour).
static WINDOW_OPENED_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The last closed hour, for a reader asking between ticks.
static LAST_HOUR: parking_lot::Mutex<Option<CitizenHealth>> = parking_lot::Mutex::new(None);

/// The one builder of a health reading: the ledger's counters read one way (a close resets,
/// a peek does not), plus what is served now.
fn assemble(read: Read) -> CitizenHealth {
    let now_ms = EpochMs::now().millis();
    let window_secs = now_ms.saturating_sub(WINDOW_OPENED_MS.load(Ordering::Relaxed)) / 1000;
    let c = hour(read);
    let resident = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
        .map(|r| r.live_personas().len() as u64)
        .unwrap_or(0); // JUSTIFIED unwrap_or: no registry = no residents, and the verdict says so
    let serving = crate::inference::llama_server::current_serving();
    let lanes_now = serving.lanes as u64;
    // The window's lanes: its max folded with this sample; a close starts the next window
    // from what is served now.
    let lanes = match read {
        Read::Close => LEDGER.lanes_max.swap(lanes_now, Ordering::Relaxed),
        Read::Peek => LEDGER.lanes_max.load(Ordering::Relaxed),
    }
    .max(lanes_now);
    // The peak stands when the window had too few lane asks to have a shape — a thin
    // window is not evidence of a shortage.
    let lanes_sustained = lanes_sustained_of(&c.lane_buckets).unwrap_or(lanes); // JUSTIFIED unwrap_or: too few distinct buckets = the window has no shape, and the peak is the read that rests nobody
    let (directed_wait_p50_ms, directed_wait_p90_ms, directed_waits) =
        crate::inference::resource_admission::directed_lane_wait_ms();
    CitizenHealth {
        window_secs,
        resident,
        lanes,
        lanes_now,
        lanes_sustained,
        directed_wait_p50_ms,
        directed_wait_p90_ms,
        directed_waits: directed_waits as u64,
        served_window: serving.served_context_window as u64,
        acts: c.acts,
        writes: c.writes,
        lanes_granted: c.lanes_granted,
        learning: c.learning,
        // Measured from the boards by the async readers (tick, command), never by this sync fold.
        review_queue: None,
        stops: c.stops,
        generations: c.generations,
        generations_dropped: c.generations_dropped,
        knee: knee_of(serving.active_model.as_deref()),
        prompt_cached_tokens: c.prompt_cached,
        prompt_prefill_tokens: c.prompt_prefilled,
        lane_starved: c.lane_starved,
        moves_opportunity: c.moves_opportunity,
        moves_failure: c.moves_failure,
        oldest_dormant_turn_age_ms: oldest_dormant_turn_age_ms(),
        held_for: serving.held_for.clone(),
    }
}

/// The served model's measured decode knee, if any — the fact that turns "owes lanes"
/// into "at the knee".
pub(super) fn knee_of(model: Option<&str>) -> Option<u64> {
    model.and_then(crate::inference::decode_knee::knee_for).map(u64::from)
}

impl Default for CitizenHealthModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ServiceModule for CitizenHealthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "citizen_health",
            priority: ModulePriority::Normal,
            command_prefixes: &["citizen/health"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(HEALTH_WINDOW),
        }
    }
    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }
    async fn tick(&self) -> Result<(), String> {
        let mut h = self.read();
        h.review_queue = super::review_queue::read().await;
        if let Some(hour) = LAST_HOUR.lock().as_mut() {
            hour.review_queue = h.review_queue;
        }
        let v = verdict(&h);
        // No verdict pages a mind out (Joel, 2026-11-02: the mindless page-out is deleted; a
        // free mind is never judged out of her seat, and seats follow demand). THE SLOW CLIP
        // remains: a dormant mind waits, so one quiet resident yields her seat in turn.
        let minds = snapshot_minds_and_reset();
        let rotated = self.rotate_one(&minds, &h).await;
        continuum_base::probe!(
            class = "citizen.health.hour",
            resident = h.resident,
            // `from->to` while the serving lane runs an older engine than the installed
            // one (card 7c5f139d); "" once converged. A node that cannot dream says so here.
            engine_stale = crate::inference::llama_server::engine_stale().unwrap_or_default(), // unwrap_or_default: "" = current, or not knowable
            lanes = h.lanes,
            served_window = h.served_window,
            acts = h.acts,
            writes = h.writes,
            lane_grants = h.lanes_granted,
            stops_landed = h.stops.landed,
            stops_empty = h.stops.empty,
            stops_think_ended = h.stops.think_ended,
            stops_cut_answer = h.stops.cut_answer,
            stops_cut_think = h.stops.cut_think,
            stops_think_budget = h.stops.think_budget,
            lane_starved = h.lane_starved,
            generations = h.generations,
            generations_dropped = h.generations_dropped,
            dropped_pct = dropped_pct(&h).unwrap_or(0), // unwrap_or: 0 = no generation ended this hour; `generations` says so
            submissions_staged = h.learning.submissions_staged,
            credit_accepted = h.learning.outcomes.of(CreditOutcome::Accepted),
            credit_failed_review = h.learning.outcomes.of(CreditOutcome::FailedReview),
            credit_unknown_review = h.learning.outcomes.of(CreditOutcome::UnknownReview),
            credit_awaiting_acceptance = h.learning.outcomes.of(CreditOutcome::AwaitingAcceptance),
            credit_independent_review_required = h.learning.outcomes.of(CreditOutcome::IndependentReviewRequired),
            credit_held = h.learning.outcomes.of(CreditOutcome::Held),
            credit_ineligible = h.learning.outcomes.of(CreditOutcome::Ineligible),
            credit_undecided = h.learning.outcomes.of(CreditOutcome::Undecided),
            boundaries_processed = h.learning.boundaries_processed,
            awaiting_reviewer = h.review_queue.map(|q| q.awaiting_reviewer),
            unsubmitted_pull_requests = h.review_queue.map(|q| q.unsubmitted_pull_requests),
            rotated = rotated.len() as u64,
            moves_opportunity = h.moves_opportunity,
            moves_failure = h.moves_failure,
            oldest_dormant_turn_age_ms = h.oldest_dormant_turn_age_ms.unwrap_or(0), // unwrap_or: 0 = nobody dormant
            verdict = v.as_str(),
            "the hour's citizen health — the substrate's own read"
        );
        // Each resident's own hour (card 1c157668): a mind asked for work and given none is
        // named on the line and on the beacon, never found by someone asking.
        let stuck = super::resident_service::judge_residents(EpochMs::now().millis()).await;
        let mut said = line(&h, &v);
        if !stuck.is_empty() {
            let named: Vec<String> = stuck.iter().map(|a| a.line()).collect();
            said.push_str(&format!(" · residents stuck: {}", named.join("; ")));
        }
        said.push_str(&continuum_cli_lifecycle::deploy_health::read(
            &crate::paths::continuum_root(),
            env!("CONTINUUM_BUILD_GIT_SHA"),
        ).line(EpochMs::now().millis()));
        crate::modules::grid::say_in_org_room(&said).await;
        Ok(())
    }
    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        match command {
            // A read WITHOUT reset: the hour's running counters as they stand.
            "citizen/health" => {
                let mut h = assemble(Read::Peek);
                h.review_queue = super::review_queue::read().await;
                let v = verdict(&h);
                let deploy = continuum_cli_lifecycle::deploy_health::read(
                    &crate::paths::continuum_root(),
                    env!("CONTINUUM_BUILD_GIT_SHA"),
                );
                CommandResult::json(&serde_json::json!({
                    // BOTH NUMBERS, because the gap between them is the diagnosis and a
                    // machine reader must not be left with the one the verdict did NOT use.
                    // `lanes` stays the hour's PEAK (unchanged meaning for every existing
                    // consumer); `lanes_sustained` is what the verdict and the rest divided by.
                    "resident": h.resident, "lanes": h.lanes,
                    "lanes_sustained": h.lanes_sustained, "lanes_now": h.lanes_now,
                    "served_window": h.served_window,
                    "acts": h.acts, "writes": h.writes, "lane_grants": h.lanes_granted,
                    "learning": {
                        "submissions_staged": h.learning.submissions_staged,
                        "credit": CreditOutcome::ALL.iter().map(|o| (o.as_str().to_owned(), serde_json::json!(h.learning.outcomes.of(*o)))).collect::<serde_json::Map<_, _>>(),
                        "boundaries_processed": h.learning.boundaries_processed,
                        "awaiting_reviewer": h.review_queue.map(|q| q.awaiting_reviewer),
                        "unsubmitted_pull_requests": h.review_queue.map(|q| q.unsubmitted_pull_requests),
                    },
                    "stops": {
                        "landed": h.stops.landed, "empty": h.stops.empty, "think_ended": h.stops.think_ended,
                        "cut_answer": h.stops.cut_answer, "cut_think": h.stops.cut_think,
                        "think_budget": h.stops.think_budget,
                    },
                    "lane_starved": h.lane_starved,
                    "generations": h.generations, "generations_dropped": h.generations_dropped,
                    "dropped_pct": dropped_pct(&h),
                    "prompt_cached_tokens": h.prompt_cached_tokens, "prompt_prefill_tokens": h.prompt_prefill_tokens,
                    "prefix_reuse_pct": prefix_reuse_pct(&h),
                    "moves_opportunity": h.moves_opportunity, "moves_failure": h.moves_failure,
                    "oldest_dormant_turn_age_ms": h.oldest_dormant_turn_age_ms,
                    "window_secs": h.window_secs,
                    "verdict": v.as_str(), "line": format!("{}{}", line(&h, &v), deploy.line(EpochMs::now().millis())),
                    // The last closed hour, so a read just after a close still sees a full hour.
                    "last_hour": LAST_HOUR.lock().as_ref().map(|hour| line(hour, &verdict(hour))),
                    "deploy": deploy,
                    "note": "counters since the last hourly tick (not reset by this read)"
                }))
            }
            other => Err(format!("citizen_health: unknown command '{other}' — try 'citizen/health'")),
        }
    }
    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![CommandSchema {
            name: "citizen/health",
            description: "The hour's citizen health as the substrate reads it: residents, lanes, acts, lane-starved waits, think-only turns, dropped generations, writes, lane grants, learning (credit outcomes by type, submissions with staged credit, review boundaries, the review queue), prefix reuse, and the verdict (healthy / starved / dropping / reading / idle)",
            params: vec![],
        }]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
pub(super) fn open_window_at_for_test(ms: u64) {
    WINDOW_OPENED_MS.store(ms, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn peek_for_test() -> CitizenHealth {
    assemble(Read::Peek)
}

#[cfg(test)]
pub(super) fn last_hour_for_test() -> Option<CitizenHealth> {
    LAST_HOUR.lock().clone()
}
