use super::*;

/// Decide how to serve persona inference on `host` given the `candidates`.
/// Returns `None` only when there are no candidates to choose from.
///
/// `demand_lanes` is how many minds actually need a concurrent lane (the
/// persona floor). Lanes come from DEMAND, never from maximum concurrency:
/// llama-server splits the KV budget evenly across slots, so every lane
/// nobody asked for divides every mind's window for nothing. Glass-boxed
/// 2026-07-10: under benchmark memory pressure the old
/// maximize-lanes-first shape served 2 personas through 4 slots at 3633
/// tokens each — framing alone ate the window, recall/board/roster never
/// rendered, and the room degenerated into a greeting loop. 2 slots would
/// have doubled every mind's window with zero lost concurrency.
///
/// The decision is pure classification on memory arithmetic — no model is
/// loaded, no inference is run.
pub fn plan_serving(
    host: &HostBudget,
    candidates: &[ModelFootprint],
    demand: &ServingDemand,
) -> Option<ServingPlan> {
    // The slot count is sized to every mind this seat SERVES: its residents plus the
    // minds leased in from other nodes. Counting only residents is how one seat carried
    // three nodes' coders on one slot (c84d885a). Saturating: a wild peak never wraps.
    // …and the knee bounds that TOTAL: a leased-in stream decodes on the same box as a
    // resident's, so it is one of the streams the knee was measured on (see `knee`).
    let demand_lanes = crate::inference::decode_knee::knee_lanes(
        demand.lanes.saturating_add(demand.leased_in),
        demand.knee,
    );
    if candidates.is_empty() {
        return None;
    }

    // GPU-viable = weights + at least one lane's KV fit the honest budget AT a
    // given per-slot window. WHICH window is the whole question (#438-class):
    //
    // This filter used to be hardcoded to `MIN_SERVE_CTX` (2048) — bare survival.
    // That made "viable" mean "can technically hold a 2k window", so selection
    // always crowned the most capable model that cleared a trivial bar and then
    // let the window collapse to it. Glass-boxed live 2026-08-17 on this box:
    // a 27B chosen at `usable_gb=5`, `served_window=2048` against a MEASURED
    // `demand_window=63817` — a mind handed 3% of the context its own turn needs.
    // Every act it took was against a window too small to hold the task statement.
    //
    // The inconsistency was INTERNAL to this function: the lane-count loop below
    // already refuses to add a slot that can't clear `BOOTSTRAP_WORKING_SET`
    // ("one full turn"), because 1 lane @ 30k beats 2 lanes @ 2k. But when even
    // ONE lane couldn't clear that floor it fell back to `.unwrap_or(1)` and
    // called the result "honest starvation, surfaced downstream" — while the
    // MODEL was never reconsidered. Shedding a lane and shedding capability are
    // the same move for the same reason; only the first was implemented. The
    // correct response to "one lane can't hold a turn" is a SMALLER MODEL that
    // can, not the bigger model blinded. A model that fits only at 2048 is not
    // more capable on this host — it is unusable on this host.
    //
    // So viability is now tested at the SAME "one full turn" standard the lane
    // floor uses, with a documented degrade: if NO candidate clears it, fall back
    // to the old `MIN_SERVE_CTX` bar so a genuinely tiny host still serves
    // something rather than nothing (honest starvation is then real, not a
    // selection artifact). Deliberately the STABLE bootstrap constant and NOT the
    // moving measured p95 — coupling model CHOICE to a jittering demand signal is
    // the 718-replan flap that wedged three benchmark runs (see the lane floor's
    // own note). The served window still refines with measurement below.
    let fits_one_lane_at = |m: &ModelFootprint, ctx: u32| {
        m.weights_bytes.saturating_add(m.kv_at(ctx)) <= host.usable_bytes
    };

    // How many of the DEMANDED lanes this candidate can serve at a one-turn
    // window (weights + per-lane KV + per-lane compute buffer, same terms the
    // lane loop below reserves). Capped at demand — extra capacity beyond
    // demand earns no preference.
    let achievable_lanes = |m: &ModelFootprint, ctx: u32| -> u64 {
        let per_lane = m.kv_at(ctx).saturating_add(m.compute_buffer_per_lane());
        let after_weights = host.usable_bytes.saturating_sub(m.weights_bytes);
        if m.weights_bytes > host.usable_bytes || per_lane == 0 {
            return 0;
        }
        (after_weights / per_lane).min(demand_lanes.max(1) as u64)
    };
    // The pathology gate, and ONLY the pathology gate: a model that can serve
    // exactly ONE lane cannot host a fleet. Any capacity ≥ 2 competes on
    // capability exactly as before — no demand-scaled re-ranking (a rule that
    // flips the base model between demand 2 and demand 6 is a relaunch flap
    // and a tier-down engine; the 718 law and "never tiering down for its own
    // sake" both forbid it).
    let can_parallelize = |m: &ModelFootprint, ctx: u32| achievable_lanes(m, ctx) >= 2;

    // Prefer the MOST CAPABLE model — excluding single-lane-only models while
    // the fleet demands parallelism.
    //
    // The lane-axis twin of the 2026-08-17 window fix above (glass-boxed
    // 2026-08-30): selection tested "fits ONE lane", so a higher-ranked model
    // that could only ever serve one slot (Flash-Next) was crowned over a model
    // serving every demanded lane (Ornith, 3–4) — and four minds spent hours
    // single-file through one slot while every per-act number looked healthy.
    // The gate is a THRESHOLD at the degenerate point, not a re-ranking: with
    // demand > 1, a one-slot model is eligible only when NO candidate can
    // parallelize (honest degrade — then the old capability order decides and
    // the lane loop surfaces the starvation it always did). Solo demand leaves
    // the capability order untouched.
    let most_capable_where = |ctx: u32, fleet_only: bool| {
        candidates
            .iter()
            .filter(|m| fits_one_lane_at(m, ctx))
            .filter(|m| !fleet_only || demand_lanes <= 1 || can_parallelize(m, ctx))
            .max_by(|a, b| {
                a.capability_rank
                    .cmp(&b.capability_rank)
                    .then(a.weights_bytes.cmp(&b.weights_bytes))
                    .then(b.model_id.cmp(&a.model_id))
            })
    };
    let best = most_capable_where(BOOTSTRAP_WORKING_SET, true)
        .or_else(|| most_capable_where(BOOTSTRAP_WORKING_SET, false))
        .or_else(|| most_capable_where(MIN_SERVE_CTX, false));

    let Some(model) = best else {
        // Nothing fits a lane on the GPU budget. Degrade honestly: name the
        // smallest candidate, single lane, fits_on_gpu = false. The caller
        // owns the CPU-exception / grid-routing choice; we do not silently
        // CPU-serve.
        let smallest = candidates
            .iter()
            .min_by(|a, b| {
                a.weights_bytes
                    .cmp(&b.weights_bytes)
                    .then(a.model_id.cmp(&b.model_id))
            })
            .expect("candidates non-empty checked above");
        return Some(ServingPlan {
            base_model: (*smallest).clone(),
            served_context_window: MIN_SERVE_CTX,
            lanes: 1,
            // Nothing fits the GPU budget → the WHOLE demand is off-box work (the
            // caller CPU-serves or routes every lane to a peer). fits_on_gpu = false
            // is the hard signal; grid_overflow_lanes quantifies "how much to place."
            grid_overflow_lanes: demand_lanes,
            resident_models: 1,
            fits_on_gpu: false,
            lanes_bound: LanesBound::NoFit,
            rationale: format!(
                "no candidate fits the {:.1}GB GPU budget; smallest is {} ({:.1}GB) — \
                 caller must CPU-serve (Intel-Mac exception) or route to a grid peer",
                bytes_gb(host.usable_bytes),
                smallest.model_id,
                bytes_gb(smallest.weights_bytes),
            ),
        });
    };

    // Lanes: DEMAND first (how many minds need a concurrent lane), then capped by
    // how many lanes' (minimum KV **+ transient prefill compute buffer**) fit the
    // budget left after weights, by perf cores, and by the MAX_LANES safety ceiling;
    // floored at 1. The per-lane floor now INCLUDES the compute buffer — the term the
    // KV arithmetic used to ignore, which let the plan size N windows to fill the
    // budget with KV and leave no room for the N concurrent compute buffers (the
    // 2026-07-14 OOM). With the buffer accounted, the lane count can follow persona
    // demand on a roomy host (each persona its OWN warm slot → no cross-persona
    // clobber → prefill caches) and still degrade to fewer lanes on a small one.
    // Effective budget = the budget as handed. The co-consumers are already out of it:
    // the governor's board subtracts every other holder's live residency, and the power
    // mode withholds its fraction (Comfort keeps 20% of the box free for a spike; Eco
    // 45%). A further 15% here was the same reservation a second time — on the M5 it was
    // the difference between one lane and two (see `CACHE_GRANT_HEADROOM`).
    let effective = host.usable_bytes;
    let window_for = |l: u64| -> u32 { model.window_within(host.usable_bytes, l as u32) };

    // THE DAEMON'S PURPOSE, made concrete (#213): serve as many concurrent minds as DEMAND
    // wants — but ONLY while each still gets a window big enough to THINK in. Concurrency is
    // worthless if it collapses every lane to the MIN_SERVE_CTX floor: a 2048-token mind
    // can't hold its own memory + the task, so 1 lane @ 30k beats 2 lanes @ 2k. The old
    // math capped lanes by what fit at the FLOOR window (`kv_at(MIN_SERVE_CTX)`), so it
    // admitted N lanes and let the window collapse — a lane-count target chased off a cliff.
    // Instead, pick the LARGEST lane count (≤ demand, ≤ cores, ≤ MAX_LANES) whose per-lane
    // window still clears the floor. Fall back to 1 only when the host is genuinely too small
    // for even one real window — honest starvation (surfaced downstream), never a silent
    // collapse. Same lanes-follow-demand-on-a-roomy-host / degrade-on-a-small-one behavior,
    // but the degrade path now SHEDS LANES (queue) instead of shrinking the mind to nothing
    // ([[never-thrash-sticky-hysteresis-on-every-lane]], [[situation-aware-focuser]]).
    let lane_cap = demand_lanes
        .max(1)
        .min(host.perf_cores.max(1))
        .min(MAX_LANES)
        .max(1);
    // Each RESIDENT persona wants its OWN warm slot so its prefilled KV survives across turns
    // (no LRU clobber → no ~10k cold re-prefill every turn — #266's whole latency story: 96%
    // prefill, two of four citizens at 0% cache reuse when 4 minds shared 2 slots). Grow the
    // lane count toward the resident population (`demand_lanes`, set from the persona floor),
    // but ONLY while each slot still clears the full-turn usable floor: adding a slot that
    // drops the per-slot window below one assembled turn trades a warm-but-blind mind for a
    // warm one that can't see — the exact 4-lanes-@-6k starvation the 2026-07-17 revert caught
    // (it clamped the CEILING; the real fix is gating the COUNT on this floor). So pick the
    // LARGEST lane count whose per-slot window ≥ the floor; if even one slot can't clear it (a
    // genuinely tiny host), fall back to 1 — honest starvation, surfaced downstream — never
    // below. The floor is `BOOTSTRAP_WORKING_SET` (≈ a ~9k prompt + generation headroom), the
    // SAME "one full turn" constant the demand cap uses (§`BOOTSTRAP_WORKING_SET`), deliberately
    // the STABLE bootstrap value and NOT the moving measured p95: coupling the lane COUNT to a
    // jittering demand signal is the 718-replan lane-flap that wedged three benchmark runs. The
    // served WINDOW still refines with measurement (below); the slot COUNT rests on a fixed
    // floor. THIS floor — not the `MAX_LANES` backstop — is the binding constraint (#266).
    // Per-lane floor: the stable bootstrap, RAISED by the typical prompt. Lanes
    // multiply only while EVERY lane still fits the floor — parallel to the max, starvation to none.
    // LANES FOLLOW THE ROSTER AT THE TYPICAL PROMPT (2026-09-14). The floor is the
    // residents' median sent prompt with headroom, never the ceiling: one 96k prompt
    // sized every lane to 120k and 16 minds ran on 3 lanes with 40 GB of KV idle.
    // The outlier is reconciled down to the served window; the roster gets its lanes.
    // The value and the term that set it are chosen TOGETHER (Codex on #4971), so a new term
    // can never fall silently into another's label. Ties go to the earlier term.
    let (per_lane_floor, floor_term) = [
        (BOOTSTRAP_WORKING_SET, FloorTerm::Bootstrap),
        // no sent prompt measured yet = no floor from it, the bootstrap prior stands
        (demand.typical_prompt_floor().unwrap_or(0), FloorTerm::TypicalPrompt), // JUSTIFIED unwrap_or: an absent median contributes no floor; 0 never wins the max
    ]
    .into_iter()
    .fold((0, FloorTerm::Bootstrap), |best, term| if term.0 > best.0 { term } else { best });
    // THE GOVERNED SIZE (card 2eec3977, Joel 2026-09-20: "context must go along with
    // required governed size"): when the roster's requirements are known, lanes × window
    // are the allocator's — every served mind at her requirement, minds shed before the
    // window, never a lane below one real turn. The median/bootstrap arithmetic below
    // is the legacy path for demands without requirements.
    // The allocator chooses over ALL candidates: the most minds at their requirement,
    // then the most capable — a warm slot for the roster outranks capability the
    // roster cannot use, so it may pick a smaller model than `most_capable_where` did.
    let (model, allocation) = if demand.requirements.is_empty() {
        (model, None)
    } else {
        match crate::inference::planning::window_allocator::allocate(&demand.requirements, candidates, host, demand_lanes.max(1)) {
            Ok(a) => {
                continuum_base::probe!(
                    class = "serving.training_growth",
                    model = %a.model_id,
                    window = a.window as u64,
                    lanes = a.lanes as u64,
                    budget_bytes = host.usable_bytes,
                    device_bytes = a.device_bytes,
                    decision = ?a.training_growth,
                    "shared-window growth reports unresolved training demand separately from conversational admission"
                );
                let chosen = candidates
                    .iter()
                    .find(|c| c.model_id == a.model_id)
                    .unwrap_or(model); // JUSTIFIED unwrap_or: the allocator returns one of `candidates` by construction; `model` only if that invariant ever broke
                (chosen, Some(a))
            }
            Err(why) => {
                // This model cannot hold ONE lane at the smallest requirement: it is not a
                // serving option here — the same honest degrade as "does not fit", never
                // a starved lane. The rationale carries the numbers.
                return Some(ServingPlan {
                    base_model: model.clone(),
                    served_context_window: 0,
                    lanes: 0,
                    grid_overflow_lanes: demand.lanes,
                    resident_models: 1,
                    fits_on_gpu: false,
                    lanes_bound: LanesBound::NoFit,
                    rationale: format!(
                        "no candidate holds one lane at the roster's smallest requirement within the budget: {why:?} — shed the model, never the window"
                    ),
                });
            }
        }
    };
    let (lanes, lanes_bound) = match &allocation {
        Some(a) => (
            a.lanes,
            LanesBound::Allocator {
                served: a.served.len() as u32,
                unserved: a.unserved.len() as u32,
                window: a.window,
                lane_cap: a.lane_cap,
                lane_cap_by: a.lane_cap_by,
                device_bytes: a.device_bytes,
                budget_bytes: host.usable_bytes,
            },
        ),
        None => {
            let lanes = (1..=lane_cap)
                .rev()
                .find(|&l| window_for(l as u64) >= per_lane_floor)
                .unwrap_or(1);
            let bound = if lanes < lane_cap {
                LanesBound::PerLaneFloor {
                    floor_tokens: per_lane_floor,
                    floor_term,
                    rejected_lanes: lanes + 1,
                    rejected_window: window_for(lanes as u64 + 1),
                }
            } else if lane_cap >= demand_lanes.max(1) {
                LanesBound::Demand
            } else if lane_cap == MAX_LANES && MAX_LANES < host.perf_cores.max(1) {
                LanesBound::MaxLanes
            } else {
                LanesBound::Cores
            };
            (lanes, bound)
        }
    };
    // Over-subscription: more resident personas than warm slots the window floor permits. The
    // remainder can't get a persistent slot — with N minds on M<N slots the llama.cpp per-slot
    // LRU eviction re-prefills a cold ~10k prefix every time an evicted mind speaks (#266). The
    // honest node ceiling is "how many minds fit warmly at a full-turn window", and exceeding it
    // is a real condition to make VISIBLE, never silently absorb: `grid_overflow_lanes` (below)
    // carries the same count to the governor for off-box placement, and the serving daemon's
    // adopted-plan probe names it so "0% cache reuse" can't go unseen for weeks.
    // NO probe here for oversubscription (#399): this is a PURE planning function
    // and `plan_serving_stable` calls it TWICE per tick with different budgets
    // (fresh + at-rest credit), so any per-call emission — even dedup'd through a
    // static — alternates between the two call sites and floods anyway (measured
    // live 2026-08-16: a single-slot dedup static still emitted 260 rows/3min
    // because the call sites' lane counts, 1 vs 2, defeated it every tick). The
    // condition already rides the RETURNED plan (`grid_overflow_lanes`, and
    // lanes < demand is recomputable from plan + demand); the serving daemon
    // probes it for the ADOPTED plan only, behind its emit-on-change gate.
    // DEMAND cap (M5+BigMama 2026-07-26): provision for what personas USE, not for
    // what RAM allows. `window_for` maximizes the window to fill the budget (94k on a
    // roomy host) → ~33GB pre-allocated KV × lanes → swap/wedge. Cap DOWN to demand.
    //
    // That demand is now MEASURED (`demand.window_tokens`, from
    // [`crate::cognition::working_set`]) rather than the `BOOTSTRAP_WORKING_SET`
    // constant it used to be — which is what this line's own comment promised
    // ("measured p95 later, #234") and never delivered. The constant was doing real
    // damage in the meantime: 16384 across 2 lanes gave each citizen **8192 tokens**
    // of a 128k-capable model on a host that could serve 94k, and measured 2026-08-06
    // that left a median context budget of 55 tokens after framing and conversation —
    // the work board (median 5,364 tokens) reached a prompt zero times in 495.
    //
    // The measurement is DEMAND, not usage, precisely so it can exceed what is
    // currently served; a usage-based signal would re-derive whatever cap produced it.
    // Ordering is unchanged and the safety envelope is identical: `window_for` is
    // already ≤ the model's trained ceiling AND ≤ what the host fits, so `.min` never
    // forces UP past either, and `.max(MIN_SERVE_CTX)` keeps the lane runnable. A
    // citizen who demands more than the machine has simply receives what fits.
    // STICKY + STEPPED (2026-09-13): the same host + model served 83,968 → 102,656 →
    // 126,464 → 138,240 across four boots as the measured demand moved, and every
    // boot re-keyed the KV page geometry and swept the previous pages. The window
    // now holds still: last serve's window while it still fits and still covers the
    // demand, else the demand. See `choose_served_window`.
    let served_context_window = match &allocation {
        // The allocator's window IS the requirement it serves; the sticky window may
        // only stand in when it still covers that requirement and fits.
        Some(a) => match demand.sticky_window {
            Some(s) if s >= a.window && s <= window_for(lanes as u64) => s,
            _ => a.window,
        },
        None => choose_served_window(window_for(lanes as u64), demand.window_target(), demand.sticky_window),
    };
    // The honest per-lane compute reserve AT the chosen window (floor + window-scaled),
    // reused by the packing math below AND reported to the board via
    // `peak_resident_bytes` — ONE formula (`prefill_compute_reserve`), never two that
    // could drift so resident accounting can't under-charge it.
    let compute_reserve = model.prefill_compute_reserve(served_context_window, lanes);

    // Resident models: pack the smallest other candidates (each at its minimum
    // runnable footprint) into whatever is left after the chosen base + its lanes'
    // KV at the served window + its lanes' concurrent compute buffers. "Keep as many
    // models alive, practically" — without overcommitting the budget (the compute
    // reserve is part of the chosen model's real cost, so packing can't claim it).
    let chosen_cost = model
        .weights_bytes
        .saturating_add(
            model
                .kv_at(served_context_window)
                .saturating_mul(lanes as u64),
        )
        .saturating_add(compute_reserve);
    let mut left = effective.saturating_sub(chosen_cost);
    let mut resident = 1u32;
    let mut others: Vec<&ModelFootprint> = candidates
        .iter()
        .filter(|m| m.model_id != model.model_id)
        .collect();
    others.sort_by(|a, b| {
        a.weights_bytes
            .cmp(&b.weights_bytes)
            .then(a.model_id.cmp(&b.model_id))
    });
    for m in others {
        let cost = m.weights_bytes.saturating_add(m.kv_at(MIN_SERVE_CTX));
        if cost <= left {
            left = left.saturating_sub(cost);
            resident += 1;
        }
    }

    Some(ServingPlan {
        base_model: model.clone(),
        served_context_window,
        lanes,
        // Demand the local lanes couldn't absorb (2 slots < N personas is the misfit
        // norm) → the governor must place these on a grid peer, NOT cram them into
        // `lanes` and pay the ~8s KV cache-swap round-trip per turn. 0 when demand fit.
        grid_overflow_lanes: demand_lanes.saturating_sub(lanes),
        resident_models: resident,
        fits_on_gpu: true,
        lanes_bound,
        rationale: format!(
            "most-capable model fitting {:.1}GB GPU budget: {} ({:.1}GB weights, rank {}), \
             {} lane(s) @ {} ctx, {} model(s) warm",
            bytes_gb(host.usable_bytes),
            model.model_id,
            bytes_gb(model.weights_bytes),
            model.capability_rank,
            lanes,
            served_context_window,
            resident,
        ),
    })
}

/// Hysteresis wrapper around [`plan_serving`]: stops model THRASH from live-
/// budget jitter. Keeps the `incumbent` model as long as it still fits the
/// budget — switching DOWN only when the incumbent no longer fits (forced
/// eviction) and UP only when a strictly more capable model fits with
/// [`SWITCH_UP_HEADROOM`] to spare.
///
/// Lanes and window are sized against the AT-REST budget (the live budget with the
/// incumbent's own resident weights credited back) for as long as the incumbent keeps
/// serving — so a model's own KV can never convince the planner to shed the lane that model
/// is currently running. That self-eviction was a real lane flap, not a theoretical one; see
/// the note in the body. No incumbent (or it's gone / no longer fits) → plain
/// [`plan_serving`]. Use this for the ONGOING serving loop; boot uses
/// `plan_serving` directly (no incumbent yet).
pub fn plan_serving_stable(
    host: &HostBudget,
    candidates: &[ModelFootprint],
    incumbent: Option<&str>,
    demand: &ServingDemand,
) -> Option<ServingPlan> {
    // The credit is applied HERE, for a caller holding the LIVE budget (the
    // incumbent's weights read as "used"). A caller whose budget is already at rest
    // — the daemon, whose `host_budget()` is the ledger's `budget_for_replacing`
    // (own footprint added back, capped at the device) — calls
    // [`plan_serving_at_rest`] and must NOT come through here: crediting twice
    // planned a 27B at 2 × 70,093 on a card whose honest fit was 2 × 47,098, and the
    // re-home law relaunched the lane three times mid-solve chasing a window the
    // launch bound kept turning back into the live one (5090, 2026-09-17).
    let at_rest = match incumbent.and_then(|id| candidates.iter().find(|m| m.model_id == id)) {
        Some(inc) => HostBudget {
            usable_bytes: host.usable_bytes.saturating_add(inc.weights_bytes),
            perf_cores: host.perf_cores,
        },
        None => *host,
    };
    plan_serving_at_rest(&at_rest, candidates, incumbent, demand)
}

/// [`plan_serving_stable`] for a budget that is ALREADY at rest — the incumbent's own
/// residency already credited back by the caller (the ledger's replace-myself
/// budget). Same hysteresis, no second credit.
pub fn plan_serving_at_rest(
    at_rest: &HostBudget,
    candidates: &[ModelFootprint],
    incumbent: Option<&str>,
    demand: &ServingDemand,
) -> Option<ServingPlan> {
    let host = at_rest;
    // NB: do NOT `?`-bail here. A deep transient dip can leave `plan_serving`
    // with nothing fitting the depressed budget (`fresh` = None) while a model
    // is STILL resident and serving fine — its memory is its own. Tearing that
    // down to "nothing" is the exact harm we're guarding against, so `fresh` is
    // an Option we fall back to only when the incumbent genuinely can't hold.
    let Some(inc_id) = incumbent else {
        return plan_serving(&host, candidates, &demand);
    };
    // NOTE (2026-08-04): there used to be an early return here — "fresh already chose the
    // incumbent → nothing to stabilize" — and it was the lane-flap bug. `fresh` is computed
    // against the LIVE budget, which the incumbent's own weights + KV depress while it serves.
    // On the same-model path that meant the planner re-derived lane count from a budget the
    // incumbent itself had eaten, decided it could no longer afford the lane it was already
    // running, dropped to 1, then added it back when the KV freed. Glass-boxed on an IDLE host:
    // 718 replans in one solve, `usable_gb` swinging 26→6, `lanes` oscillating 1↔2, and every
    // flip resizing the live admission semaphore (`set_served_lane_count`) and the prefill
    // throttle under in-flight requests — which is the `no response headers for 300s` lane wedge
    // that killed three benchmark runs.
    //
    // The at-rest credit below already existed for exactly this reason ("so a model's OWN
    // load/residency can never flap it out"); it was simply never applied to the case where the
    // incumbent KEEPS serving. So the paths are unified: whenever a living incumbent is still
    // servable, its plan is sized against the at-rest budget. Same-model is no longer a
    // shortcut — it is the main path. [[never-thrash-sticky-hysteresis-on-every-lane]]
    //
    // Incumbent dropped off disk entirely → honour whatever `fresh` chose
    // (possibly None = nothing servable).
    let Some(inc) = candidates.iter().find(|m| m.model_id == inc_id) else {
        return plan_serving(&host, candidates, &demand);
    };
    // The incumbent is ALREADY resident: its own weights read as "used" in live
    // free memory, which is exactly what depresses `usable_bytes` while it loads.
    // Measure it against the AT-REST budget — credit its committed weights back —
    // so a model's OWN load/residency can never flap it out for a smaller model.
    // A genuine EXTERNAL squeeze (another consumer grabbing VRAM) is NOT credited
    // back, so a real over-commit still forces the down-switch to `fresh`.
    let at_rest = host;
    // `fresh` is computed against the POST-EVICTION budget, and that is the whole fix.
    //
    // It used to use `host` — the live budget WITH the incumbent still resident — so the
    // planner asked "does the better model fit in the memory left over beside the one already
    // loaded?". That question is wrong, because a swap is never co-resident: `serve()` kills
    // the incumbent llama-server child and THEN launches the candidate, exactly as
    // `pin_fit_decision` documents ("the candidate only needs to fit AFTER the incumbent's
    // VRAM is reclaimed"). So the eviction credit-back was written, proven, and wired to the
    // PIN path only, while the autonomic planner kept asking the co-resident question.
    //
    // MEASURED on this M5 (2026-08-19): `serving.plan` chose Devstral-24B at `usable_gb: 18`
    // on a 64 GB box, because Devstral + the vision lane + embeddings were already resident.
    // Qwen3.8-27B (18.97 GB of weights) cannot fit a lane inside 18 GB, so it never entered
    // the plan. `serving/pin` — same models, same instant, same machine — computed
    // `budget_gb: 34.09` via this credit and the 27B fit with 15 GB to spare. Two answers to
    // one question, and only the operator-driven path could reach the better model.
    //
    // This is the third sighting of ONE defect: the planner could not reason about a budget
    // the incumbent was occupying. #214 fixed it for the WINDOW (grow-back), the pin path had
    // it for the MODEL, and #266's warm slots starve for the same reason — lanes are sized
    // from a budget the resident model has already eaten, so 3 of 4 citizens got no warm slot
    // and re-prefilled cold every turn. One credit, three cards.
    //
    // SAFE because it is still gated: the switch-up below additionally requires strictly
    // greater `capability_rank` AND `SWITCH_UP_HEADROOM` of margin, so a bigger model is
    // adopted only when it is genuinely better AND genuinely fits post-eviction. A transient
    // budget bump cannot flap it. An EXTERNAL squeeze is still not credited back — only the
    // incumbent's own committed weights are — so a real over-commit still forces the
    // down-switch. [[never-thrash-sticky-hysteresis-on-every-lane]]
    let fresh = plan_serving(&at_rest, candidates, &demand);
    // Even crediting its own residency, can the incumbent still hold a lane? If
    // not, a real squeeze has genuinely evicted it → take `fresh`.
    if inc.weights_bytes.saturating_add(inc.kv_at(MIN_SERVE_CTX)) > at_rest.usable_bytes {
        return fresh;
    }
    // Switch UP to `fresh` ONLY if it is strictly more capable AND fits the POST-EVICTION
    // budget with headroom. This used to test against `host` on the reasoning that "loading a
    // bigger NEW model needs actual free memory" — true for a co-resident load, false for the
    // swap `serve()` actually performs (kill incumbent, then launch). Testing against `host`
    // made the gate unreachable for any model larger than the free remainder, which on a
    // healthy box IS every upgrade worth making. `SWITCH_UP_HEADROOM` still supplies the
    // anti-flap margin; it is now margin on the right budget.
    let headroom_budget = (at_rest.usable_bytes as f64 * (1.0 - SWITCH_UP_HEADROOM)) as u64;
    // The plan HOLDS its model, so this is a field read, not a search. It used to be
    // `candidates.iter().find(|c| c.model_id == f.base_model_id)` — a name lookup that
    // returned None whenever the planned name wasn't in the candidate list, silently
    // making `is_some_and` false and suppressing a legitimate switch-up.
    // A SWITCH-UP NEVER MAKES MORE MINDS COLD. Measured 2026-09-06 01:0xZ on the M5:
    // the planner switched the base from a 5-lane MoE to a 3-lane dense model on
    // measured capability alone, with 12 minds resident — every HUD read ACT 0 /
    // QUE 12, one act in 30 minutes, and Joel called it what it was: a major
    // regression. Capability is the rank; warm slots are the roster's oxygen. A
    // candidate that would serve FEWER lanes than the incumbent while still fewer
    // than the roster needs leaves minds cold that were warm — not an upgrade,
    // whatever it scores. (BigMama, card a374c014: this compares FOOTPRINTS, so a
    // stale weights_bytes on a long-running core lies to it — fix the input there.)
    let inc_lanes = plan_serving(&at_rest, std::slice::from_ref(inc), &demand)
        .map(|p| p.lanes as u32)
        .unwrap_or(0); // unwrap_or: an incumbent that cannot plan at all has no lanes to protect
    let upgrade_worth_it = fresh.as_ref().is_some_and(|fresh_plan| {
        let f = &fresh_plan.base_model;
        let fresh_lanes = fresh_plan.lanes as u32;
        let starves_the_roster = fresh_lanes < inc_lanes && fresh_lanes < demand.lanes;
        f.capability_rank > inc.capability_rank
            && f.weights_bytes.saturating_add(f.kv_at(MIN_SERVE_CTX)) <= headroom_budget
            && !starves_the_roster
    });
    if upgrade_worth_it {
        return fresh;
    }
    // Keep the incumbent: re-rank it to the top so `plan_serving` selects it and
    // recomputes lanes + resident against the AT-REST budget (so its own residency
    // doesn't shrink its own window/lanes) — reusing all the fit/lane/pack logic
    // instead of duplicating it here.
    let mut promoted: Vec<ModelFootprint> = candidates.to_vec();
    if let Some(m) = promoted.iter_mut().find(|m| m.model_id == inc_id) {
        m.capability_rank = u8::MAX;
    }
    plan_serving(&at_rest, &promoted, &demand)
}

pub(super) fn bytes_gb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000_000.0
}
