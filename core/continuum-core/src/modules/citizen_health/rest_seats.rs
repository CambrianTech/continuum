use super::*;

pub(super) async fn rest_seats(chosen: Vec<(uuid::Uuid, MindHour)>, cause: RestCause, h: &CitizenHealth) -> Vec<String> {
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
        let reason = match cause {
            RestCause::Rotation => {
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
                continue;
            }
            RestCause::Mindless => format!(
                "{} of {} speak verdicts this hour were the gate refusing a recital or an envelope; {} acts, 0 writes",
                m.gate_refused, m.verdicts, m.acts
            ),
        };
        let since_ms = EpochMs::now().millis(); // pre-1970 reads 0: a clock before the epoch is not a real host; 0 reads as "unknown"
        let recorded = crate::persona::resting_seat::rest(crate::persona::resting_seat::RestingSeat {
            agent_name: agent_name.clone(),
            persona_id: persona,
            reason: reason.clone(),
            since_ms,
            build: crate::persona::resting_seat::current_build().to_string(),
        });
        match cause {
            RestCause::Rotation => {} // handled above: no resting record
            RestCause::Mindless => continuum_base::probe!(
                class = "persona.mindless.paged_out",
                persona = %agent_name,
                persona_id = %persona,
                verdicts = m.verdicts,
                gate_refused = m.gate_refused,
                acts = m.acts,
                checkpoint_saved = saved,
                recorded = recorded.is_ok(),
                reason = %reason,
                "a mindless seat was paged out with her checkpoint — she returns on a change, not a clock"
            ),
        }
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
    pub(super) async fn rotate_one(&self, minds: &[(uuid::Uuid, MindHour)], freed_this_pass: usize, h: &CitizenHealth) -> Vec<String> {
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
            freed_this_pass,
            &residents,
            crate::persona::resting_seat::seated_since,
            now_ms,
        );
        for (id, hour) in candidates {
            let Some(rt) = registry.get(id) else { continue };
            let holds_work = crate::persona::active_work_source::AircWorkReader::active_claims(rt.as_ref())
                .await
                .map_or(true, |cards| !cards.is_empty());
            if holds_work {
                continue;
            }
            return rest_seats(vec![(id, hour)], RestCause::Rotation, h).await;
        }
        Vec::new()
    }

    pub fn new() -> Self {
        Self
    }
    pub(super) fn read(&self) -> CitizenHealth {
        let (acts, writes, lanes_granted, settles, credits_staged, credits_settled) = snapshot_and_reset();
        let stops = snapshot_stops_and_reset();
        let resident = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
            .map(|r| r.live_personas().len() as u64)
            .unwrap_or(0); // JUSTIFIED unwrap_or: no registry = no residents, and the verdict says so
        let serving = crate::inference::llama_server::current_serving();
        let lanes_now = serving.lanes as u64;
        // The hour's lanes: the window's max folded with this tick's sample, then the
        // window starts again from what is served now.
        let lanes = LEDGER.lanes_max.swap(lanes_now, Ordering::Relaxed).max(lanes_now);
        // The peak stands when the window had too few lane asks to have a shape — a thin
        // hour is not evidence of a shortage.
        let lanes_sustained = lanes_sustained_of(&snapshot_lane_buckets_and_reset()).unwrap_or(lanes); // JUSTIFIED unwrap_or: too few distinct buckets = the hour has no shape, and the peak is the read that rests nobody
        let (directed_wait_p50_ms, directed_wait_p90_ms, directed_waits) =
            crate::inference::resource_admission::directed_lane_wait_ms();
        let (prompt_cached_tokens, prompt_prefill_tokens) = snapshot_prompt_and_reset();
        let lane_starved = snapshot_lane_starved_and_reset();
        let (generations, generations_dropped) = snapshot_generations_and_reset();
        let (moves_opportunity, moves_failure) = snapshot_moves_and_reset();
        CitizenHealth {
            window_secs: HEALTH_WINDOW.as_secs(),
            resident,
            lanes,
            lanes_now,
            lanes_sustained,
            directed_wait_p50_ms,
            directed_wait_p90_ms,
            directed_waits: directed_waits as u64,
            served_window: serving.served_context_window as u64,
            acts,
            writes,
            lanes_granted,
            settles,
            credits_staged,
            credits_settled,
            stops,
            generations,
            generations_dropped,
            knee: knee_of(serving.active_model.as_deref()),
            prompt_cached_tokens,
            prompt_prefill_tokens,
            lane_starved,
            moves_opportunity,
            moves_failure,
            oldest_dormant_turn_age_ms: oldest_dormant_turn_age_ms(),
            held_for: serving.held_for.clone(),
        }
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
        let h = self.read();
        let v = verdict(&h);
        // THE OWNER ACTS (card aed15611): a mind whose hour was recitals and gate-passes
        // is paged out WITH her checkpoint — her lane goes to a working mind, and she
        // returns on a change (a deploy, a trained gene, the operator's word), never
        // on a clock. The lake and the rabbits: her working memory is flushed first.
        let minds = snapshot_minds_and_reset();
        let paged_out = rest_seats(mindless_seats(&minds, h.resident), RestCause::Mindless, &h).await;
        let remaining: Vec<(uuid::Uuid, MindHour)> =
            minds.iter().filter(|(_, m)| !paged_out.contains(&m.agent_name)).cloned().collect();
        let after_mindless = CitizenHealth { resident: h.resident.saturating_sub(paged_out.len() as u64), ..h.clone() };
        // THE SLOW CLIP: a dormant mind waits, so one quiet resident yields her seat.
        let rotated = self.rotate_one(&remaining, paged_out.len(), &after_mindless).await;
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
            settles = h.settles,
            credits_staged = h.credits_staged,
            credits_settled = h.credits_settled,
            mindless_paged_out = paged_out.len() as u64,
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
        if !paged_out.is_empty() {
            said.push_str(&format!(" · mindless {} paged out ({})", paged_out.len(), paged_out.join(", ")));
        }
        crate::modules::grid::say_in_org_room(&said).await;
        Ok(())
    }
    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        match command {
            // A read WITHOUT reset: the hour's running counters as they stand.
            "citizen/health" => {
                let h = CitizenHealth {
                    window_secs: HEALTH_WINDOW.as_secs(),
                    resident: crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                        .map(|r| r.live_personas().len() as u64)
                        .unwrap_or(0), // JUSTIFIED unwrap_or: no registry = no residents
                    lanes: LEDGER.lanes_max.load(Ordering::Relaxed).max(crate::inference::llama_server::current_serving().lanes as u64),
                    lanes_now: crate::inference::llama_server::current_serving().lanes as u64,
                    // A read WITHOUT reset here too — the hour's shape as it stands.
                    lanes_sustained: lanes_sustained_of(&read_lane_buckets()).unwrap_or_else(|| { // JUSTIFIED unwrap_or_else: same thin-window fallback as the tick's read — the peak, which rests nobody
                        LEDGER.lanes_max.load(Ordering::Relaxed).max(crate::inference::llama_server::current_serving().lanes as u64)
                    }),
                    directed_wait_p50_ms: crate::inference::resource_admission::directed_lane_wait_ms().0,
                    directed_wait_p90_ms: crate::inference::resource_admission::directed_lane_wait_ms().1,
                    directed_waits: crate::inference::resource_admission::directed_lane_wait_ms().2 as u64,
                    served_window: crate::inference::llama_server::current_serving().served_context_window as u64,
                    acts: LEDGER.acts.load(Ordering::Relaxed),
                    writes: LEDGER.writes.load(Ordering::Relaxed),
                    lanes_granted: LEDGER.lanes_granted.load(Ordering::Relaxed),
                    settles: LEDGER.settles.load(Ordering::Relaxed),
                    credits_staged: LEDGER.credits_staged.load(Ordering::Relaxed),
                    credits_settled: LEDGER.credits_settled.load(Ordering::Relaxed),
                    stops: read_stops(),
                    lane_starved: LEDGER.lane_starved.load(Ordering::Relaxed),
                    generations: LEDGER.generations.load(Ordering::Relaxed),
                    generations_dropped: LEDGER.generations_dropped.load(Ordering::Relaxed),
                    knee: knee_of(crate::inference::llama_server::current_serving().active_model.as_deref()),
                    prompt_cached_tokens: LEDGER.prompt_cached.load(Ordering::Relaxed),
                    prompt_prefill_tokens: LEDGER.prompt_prefilled.load(Ordering::Relaxed),
                    moves_opportunity: LEDGER.moves_opportunity.load(Ordering::Relaxed),
                    moves_failure: LEDGER.moves_failure.load(Ordering::Relaxed),
                    oldest_dormant_turn_age_ms: oldest_dormant_turn_age_ms(),
                    held_for: crate::inference::llama_server::current_serving().held_for,
                };
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
                    "acts": h.acts, "writes": h.writes, "lane_grants": h.lanes_granted, "settles": h.settles,
                    "credits_staged": h.credits_staged, "credits_settled": h.credits_settled,
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
                    "verdict": v.as_str(), "line": format!("{}{}", line(&h, &v), deploy.line(EpochMs::now().millis())),
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
            description: "The hour's citizen health as the substrate reads it: residents, lanes, acts, lane-starved waits, think-only turns, dropped generations, writes, lane grants, settles, prefix reuse, and the verdict (healthy / starved / dropping / reading / idle)",
            params: vec![],
        }]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
