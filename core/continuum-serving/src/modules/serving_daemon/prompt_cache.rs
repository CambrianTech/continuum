//! Prompt cache ownership for the serving daemon.

use super::*;

/// Conservative host floor for the OS + every process we don't own: an eighth of
/// physical RAM, floored at 2 GiB. DERIVED from the box (never env-tunable, never a
/// flat constant that breaks on an 8GB laptop or a 512GB server): on the incident's
/// 63GB box it reserves ~7.9GB — the conservative side of the ~6GB her measurements
/// attribute to OS+other, and over-reserving only shrinks the cache (safe), while
/// under-reserving re-creates the pagefile thrash. Follow-up (named, not silent):
/// derive from measured non-serving usage once mmap attribution is solved — the
/// free-memory monitor can't be used here because mmap'd weight pages report
/// "available" while load-bearing (see host_cache_lease module doc).
/// The derived `--cache-ram` for a persona-population serve (restore-economy
/// 1.b): per-citizen MEASURED demand (WorkingSetRegistry, persisted across
/// restarts) × the model's own kv/token, clamped by what the host affords
/// after the serve's peak resident working set and the OS floor.
///
/// Cold start (no demand ever measured — fresh install) and a missing
/// footprint both return the declared prior [`lane_args::CACHE_RAM_MIB`]:
/// "cold start is a real state, not a missing measurement". Those prior paths
/// bypass affordability, and the existing hard floor can exceed a tiny grant.
/// The diagnostic record preserves these facts; it does not fix the sizing law.
pub(super) fn derived_prompt_cache_mib(
    model_id: &str,
    fp: Option<&crate::inference::planning::serving_plan::ModelFootprint>,
    served_ctx: u32,
    lanes: u32,
    physical_bytes: u64,
    available_bytes: u64,
    memory_mode: Option<crate::gpu::monitor::MemoryMode>,
) -> PromptCacheGrant {
    // LIVE citizens only. The WorkingSetRegistry persists every persona that
    // EVER recorded demand — measured 2026-08-28, first live fire: citizens=332
    // (weeks of rotated identities), a fictional want that only the
    // affordability clamp kept sane, and which double-booked host RAM in the
    // lease (16.8 GB reserved where the four RESIDENT minds honestly need ~9).
    // "Live" means resident NOW: intersect with the runtime roster — the same
    // liveness truth every other consumer uses, never a recency threshold we
    // would have to invent (Law 1).
    // WHO WILL RESUME, not only who is resident this instant.
    //
    // This filter was live-residents-only, and at the moment that matters most — the
    // FIRST launch of the engine — live residents are zero by construction: citizens
    // resume only after the engine exists. So the derivation fell to its 4096 MiB
    // prior, and every 4.5-5.5 GiB prompt state was refused by the engine for the life
    // of the process (`prompt state size 4498.997 MiB exceeds cache size limit
    // 4096.000 MiB, skipping`; Kimi at 317-359 s/turn, cached=0, hit=0.00, measured
    // 2026-09-16 on the 5090). The M5 escaped only because its lane count changes when
    // 16 residents arrive and forces a SECOND launch that re-derives; a `--parallel 1`
    // box never gets that second chance.
    //
    // The seeds on disk ARE the resume set, and the demand registry persists across
    // restarts for exactly this. Union them with the live roster: at first launch the
    // seeds carry the answer; mid-life the live roster does; a persona in both counts
    // once. `--cache-ram` is a launch argument, so getting this right at the first
    // launch is the only chance there is.
    let live: std::collections::HashSet<uuid::Uuid> =
        crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
            .map(|r| r.live_personas().into_iter().collect())
            .unwrap_or_default();
    let planned: Vec<uuid::Uuid> =
        crate::persona::seed::persona_ids_with_seeds_in(&crate::identity::citizen_path::citizens_kind_dir(
            &crate::paths::continuum_root(),
            crate::identity::IdentityKind::Persona,
        ));
    let resident: std::collections::HashSet<uuid::Uuid> = live
        .iter()
        .copied()
        .chain(planned.iter().copied())
        .collect();
    // THE CACHE HOLDS THE ACTIVE ROSTER'S STATES, at most one served window each — not
    // every seed on disk at its unclamped peak. Measured 2026-09-20 on the M5: 23 seeded
    // minds × `peak_tokens` of 225k–505k (the whole assembled context, not what a turn
    // sends) made `want` astronomically large, so every guard below only carved the
    // afford and the grant was ALWAYS "everything the box can spare": 14,396 MiB at
    // 2 lanes, 26,806 MiB for a 1 × 2,048 lane at boot, 26,985 MiB on the fresh engine.
    // The cache filled to it, the box went into swap, the plan shed a lane, and every
    // deploy went dark with no room for a warm build (cards 7b01ed65, b57b19fd). The
    // roster bound (#4244/#4245) already says who can take a turn: `lanes × 2`; a mind
    // beyond it is dormant and re-prefills when a slot wakes it. Sizing the want by that
    // law makes the grant demand-driven again; the affordability guards stay the net.
    let population: Vec<(u32, u32)> = crate::cognition::working_set::global()
        .all()
        .into_iter()
        .filter(|(id, _)| resident.contains(id))
        .map(|(_, d)| (d.sent_peak, d.peak_tokens))
        .collect();
    // HOW MANY STATES: the minds actually taking turns, floored at the warm-slot bound
    // (card 58ba9752, 5346e86a). lanes x 2 alone was the guard written after the M5 swap,
    // not a sizing law: on the IntelMac, 2026-10-10, it held 4 states while 7 minds
    // rotated through 2 lanes, so each evicted another and every turn re-prefilled
    // 15-31k cold (cached 0-1,554). Counting minds seen within the rotation window keeps
    // the M5's 23 dormant seeds out (the 7b01ed65 want stays demand-sized), and the bytes
    // remain capped by `affordable_bytes` below, which pressure still shrinks.
    let warm_floor = crate::persona::spawner_module::bounded_by_warm_slots(
        resident.len(),
        (lanes > 0).then_some(lanes),
    );
    let seen: Vec<(u64, u64)> = crate::cognition::working_set::global()
        .all()
        .into_iter()
        .filter(|(id, _)| resident.contains(id))
        .map(|(_, d)| (d.last_seen_ms, d.prev_seen_ms))
        .collect();
    let active =
        rotating_minds(&seen, EpochMs::now().millis(), rotation_window_ms(&seen)).max(warm_floor);
    // the served model's own page dir: the same key the spawn writes its pages under
    let measured_state = crate::inference::slots::measured_state_bytes(
        &crate::inference::llama_server::kv_page_dir(model_id),
    );
    let estimated = active_cache_states(&population, served_ctx, active);
    let demands = with_measured_state(estimated.clone(), fp.map(|f| f.kv_per_token), measured_state);
    let serve_host_bytes = serve_host_bytes(fp, served_ctx, lanes, memory_mode);
    let mut decision = prompt_cache_decision(
        fp,
        &demands,
        planned.len(),
        served_ctx,
        lanes,
        physical_bytes,
        serve_host_bytes,
        available_bytes,
    );
    decision.reason = sized_by(&estimated, &demands, decision.reason);
    // Deduplicate the decision, not just its MiB: a different model or a prior
    // becoming measured can yield the same limit and still needs a receipt.
    static LAST_SPOKEN: parking_lot::Mutex<Option<(String, PromptCacheDecision)>> =
        parking_lot::Mutex::new(None);
    let changed = {
        let mut last = LAST_SPOKEN.lock();
        if last
            .as_ref()
            .is_some_and(|(id, previous)| id == model_id && previous == &decision)
        {
            false
        } else {
            *last = Some((model_id.to_owned(), decision.clone()));
            true
        }
    };
    if changed {
        let afford_mib = decision.affordable_bytes.map(|bytes| bytes / (1024 * 1024));
        // Preserve the existing numeric field when known; Empty is not recorded
        // by tracing visitors, so a prior does not fabricate zero affordability.
        let afford_field: &dyn tracing::field::Value = match afford_mib.as_ref() {
            Some(value) => value,
            None => &tracing::field::Empty,
        };
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.prompt_cache.derived",
            model = model_id,
            reason = decision.reason,
            derived_mib = decision.desired_mib as u64,
            one_conversation_mib = decision.one_conversation_mib as u64,
            afford_mib = afford_field,
            desired_mib = decision.desired_mib as u64,
            applied_mib = "unobserved",
            served_ctx = served_ctx as u64,
            lanes = lanes as u64,
            citizens = demands.len() as u64,
            population = population.len() as u64,
            active_bound = active as u64,
            physical_bytes = physical_bytes,
            kv_per_token = ?decision.kv_per_token,
            estimated_kv_bytes = ?decision.estimated_kv_bytes,
            affordable_bytes = ?decision.affordable_bytes,
            serve_host_bytes = decision.serve_host_bytes,
            available_bytes = decision.available_bytes,
            serialized_state_bytes = ?measured_state,
            "prompt-cache sizing decision, not engine readback; KV estimate excludes draft state and checkpoints"
        );
    }
    PromptCacheGrant {
        mib: decision.desired_mib,
        one_conversation_mib: decision.one_conversation_mib,
    }
}

/// PURE: does this tick owe a receipt, given what was last said?
///
/// `None` = the state is unchanged, stay silent. `Some(previous)` = speak, and `previous`
/// is what was said before (itself `None` when nothing had been said yet). Extracted from
/// the divergence cell so the edge behaviour is pinned by a test rather than only by the
/// live ledger: a probe that sits on the 5-second ready path and fires on PERSISTENCE
/// rather than CHANGE turns a three-hour condition into ~2,160 identical lines — a rising
/// count that is not rising evidence (Cormac on #4298).
pub(super) fn say_on_change<T: PartialEq>(
    last: &mut Option<T>,
    now: Option<T>,
) -> Option<Option<T>> {
    if *last == now {
        None
    } else {
        Some(std::mem::replace(last, now))
    }
}

/// Diagnostic record of the existing sizing law. This deliberately does not
/// change its priors or affordability policy; neither is an engine measurement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PromptCacheDecision {
    pub(super) reason: &'static str,
    pub(super) desired_mib: u32,
    pub(super) served_ctx: u32,
    pub(super) lanes: u32,
    pub(super) physical_bytes: u64,
    pub(super) citizens: usize,
    pub(super) kv_per_token: Option<u64>,
    pub(super) estimated_kv_bytes: Option<u64>,
    pub(super) affordable_bytes: Option<u64>,
    /// ONE CONVERSATION in MiB — the largest single prefix this cache must hold without
    /// thrashing, from `lane_args::one_conversation_bytes` (the same function the sizing
    /// floor uses). Below it, prefix reuse is not poor but IMPOSSIBLE: every context
    /// switch is a guaranteed full re-prefill. Carried on the decision so the running
    /// engine can be judged by the SAME predicate the spawn was sized by — Cormac on
    /// #4298: extracting the function for a second consumer and then not calling it from
    /// that consumer is the one-value-two-meanings shape this PR exists to expose.
    /// 0 = nothing measured (no conversation to hold), never a guessed size.
    pub(super) one_conversation_mib: u32,
    /// What the serve's own working set costs HOST RAM — the peak on-device
    /// residency on unified memory, ZERO on a discrete GPU (weights and KV live in
    /// VRAM). This is the term that took the 5090's cache to the 256 MiB floor.
    pub(super) serve_host_bytes: u64,
    /// Host RAM actually available at derivation (the system monitor's reading; 0 =
    /// unknown). The cache may never promise more than the box can give right now:
    /// the second ceiling on `afford`, minus the OS floor. NOT the commit charge —
    /// on a CUDA host commit is chronically above physical (measured 2026-09-17:
    /// 75.5 GB committed, 43 GB free, 63 GB physical — reserved-but-untouched and
    /// pagefile-backed allocations count), so `physical − commit` is always the hard
    /// floor there; it is the right clamp only where commit ≈ resident, the MoE
    /// mmap'd-expert case the host-cache lease was written for.
    pub(super) available_bytes: u64,
}

/// PURE: the prompt STATES the host cache is sized to hold — the law behind the grant.
/// One state per ACTIVE mind (the roster bound: `lanes × MINDS_PER_LANE_STARVED_ABOVE`,
/// handed in as `active`), each the largest prompt that mind actually SENT (`sent_peak`;
/// the unclamped `peak_tokens` only when nothing was ever sent), and never more than the
/// served window — a cached state cannot exceed the slot it was prefilled in. The largest
/// states are kept (the worst case must fit); the rest are dormant minds whose states the
/// cache does not carry. `active == 0` (no bound known) keeps the FLOOR — the smallest
/// roster that can serve (`MINDLESS_RESIDENT_FLOOR` states) — never the whole population:
/// an absence is not a number in either direction, and "everything" is exactly the
/// astronomical want this law exists to delete (Cormac's condition on #4253).
/// PURE: the roster's rotation window, measured: the p90 of residents' return intervals
/// (the gap between each one's last two turns). A state is worth holding while its mind
/// may come back for it, and how long that takes is a property of the seat, not a
/// constant: the IntelMac, 2026-10-10, returned residents every 4-6 hours, and a declared
/// 1h window counted too few to clear the warm-slot floor and changed nothing (Fable on #4898: "a measured window
/// sizes; a constant guesses"). `None` when no resident has two turns on record: then
/// nothing counts as rotating and the warm-slot floor alone sizes the cache.
pub fn rotation_window_ms(seen: &[(u64, u64)]) -> Option<u64> {
    let mut gaps: Vec<u64> = seen
        .iter()
        .filter(|(last, prev)| *prev > 0 && last > prev)
        .map(|(last, prev)| last - prev)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_unstable();
    Some(gaps[(gaps.len() * 9).div_ceil(10) - 1])
}

/// PURE: how many residents took a turn within `window_ms` of `now_ms`, from each one's
/// `(last_seen_ms, prev_seen_ms)`. The prompt cache holds this many states (never fewer
/// than the warm-slot floor), so the minds that rotate keep their prefixes.
pub fn rotating_minds(seen: &[(u64, u64)], now_ms: u64, window_ms: Option<u64>) -> usize {
    let Some(window) = window_ms else { return 0 };
    seen.iter()
        .filter(|(last, _)| *last > 0 && now_ms.saturating_sub(*last) <= window)
        .count()
}

/// PURE: each mind's cache demand in TOKEN-EQUIVALENTS of the measured saved state, so the
/// sizing (`lane_args::host_prompt_cache_mib`, `one_conversation_bytes`), which prices a
/// state as `kv_per_token × tokens`, prices what a state really costs. A hybrid model's
/// state also holds its recurrent state and context checkpoints: the 5090's 27B saved
/// 4.3-5.8 GB states that the KV estimate sized at 4,288 MiB. Each demand is raised to at
/// least the measured state; nothing measured, or no KV rate, leaves the demands as they are.
pub fn with_measured_state(
    demands: Vec<u32>,
    kv_per_token: Option<u64>,
    measured_state: Option<u64>,
) -> Vec<u32> {
    let (Some(kv), Some(state)) = (kv_per_token.filter(|k| *k > 0), measured_state) else {
        return demands;
    };
    let state_tokens = state.div_ceil(kv).min(u32::MAX as u64) as u32;
    demands.into_iter().map(|t| t.max(state_tokens)).collect()
}

/// PURE: the lane count the admission gate sizes by: the running engine's own lanes when it
/// is serving, the plan's only before an engine is up (a cold boot has nothing else to go
/// on). A plan the reconcile has not taken changes nothing the engine serves.
pub fn admitted_lanes(live_ready: bool, live_lanes: u32, plan_lanes: u32) -> u32 {
    if live_ready && live_lanes > 0 {
        live_lanes
    } else {
        plan_lanes
    }
}

pub fn active_cache_states(population: &[(u32, u32)], served_ctx: u32, active: usize) -> Vec<u32> {
    let mut states: Vec<u32> = population
        .iter()
        .map(|&(sent_peak, peak_tokens)| {
            let tokens = if sent_peak > 0 {
                sent_peak
            } else {
                peak_tokens
            };
            if served_ctx > 0 {
                tokens.min(served_ctx)
            } else {
                tokens
            }
        })
        .filter(|t| *t > 0)
        .collect();
    states.sort_unstable_by(|a, b| b.cmp(a));
    let keep = if active > 0 {
        active
    } else {
        crate::cognition::health_ledger::MINDLESS_RESIDENT_FLOOR as usize
    };
    states.truncate(keep);
    states
}

/// `planned` is how many personas have a seed on disk — the resume set — independent
/// of whether the registry holds demand for them. It exists so the EMPTY-demand case
/// can be named honestly: "no seeds on disk" (a genuinely cold node) and "seeds exist
/// but none has recorded demand yet" (their very first boot) are different states, and
/// the old single reason `prior_no_resident_demand` read benign for both — and for a
/// third state it was never meant to cover, "residents not yet up at launch", which is
/// the one that cost 5 minutes a turn. That third state no longer produces an empty
/// `demands` at all, because the caller now sizes from the resume set.
pub(super) fn prompt_cache_decision(
    fp: Option<&crate::inference::planning::serving_plan::ModelFootprint>,
    demands: &[u32],
    planned: usize,
    served_ctx: u32,
    lanes: u32,
    physical_bytes: u64,
    serve_host_bytes: u64,
    available_bytes: u64,
) -> PromptCacheDecision {
    let mut decision = PromptCacheDecision {
        reason: "prior_missing_footprint",
        desired_mib: crate::inference::lane_args::CACHE_RAM_MIB,
        served_ctx,
        lanes,
        physical_bytes,
        citizens: demands.len(),
        kv_per_token: fp.map(|f| f.kv_per_token),
        estimated_kv_bytes: fp.map(|f| {
            demands.iter().fold(0u64, |n, t| {
                n.saturating_add(f.kv_per_token.saturating_mul(*t as u64))
            })
        }),
        affordable_bytes: None,
        one_conversation_mib: fp
            .map(|f| {
                (crate::inference::lane_args::one_conversation_bytes(demands, f.kv_per_token)
                    / (1024 * 1024)) as u32
            })
            .unwrap_or(0), // unwrap_or: no footprint = the model's geometry is unknown, so one conversation has no size yet
        serve_host_bytes,
        available_bytes,
    };
    let Some(fp) = fp else {
        return decision;
    };
    // Every prior that KNOWS the model's geometry holds at least ONE full prompt state
    // (`kv_per_token × served window`): a persona's first-ever boot has seeds on disk
    // but no recorded demand yet, and shipping the bare constant there reproduced
    // "prompt state size … exceeds cache size limit 4096.000 MiB, skipping" for the
    // life of the process on a `--parallel 1` box (Cormac's review of #4128, the 5090
    // at -c 172436 = 4,499 MiB per state). Only the missing-footprint prior above is
    // honestly unable to compute it.
    // Ceil, not floor: the engine skips a save when `state_size > limit_size` in BYTES
    // (server-task.cpp), so a cache 1 MiB under a full state still refuses it — the
    // 5090's 4,498.997 MiB state against a floored 4,498 MiB limit is that refusal.
    let one_full_state_mib = fp
        .kv_per_token
        .saturating_mul(served_ctx as u64)
        .div_ceil(1024 * 1024)
        .min(u32::MAX as u64) as u32;
    decision.desired_mib = decision.desired_mib.max(one_full_state_mib);
    if demands.is_empty() {
        // Two different facts, two different names. A reader must never be able to
        // take "the node is cold" for "the residents exist and this sizing missed them".
        decision.reason = if planned == 0 {
            "prior_no_seeds_on_disk"
        } else {
            "prior_seeds_without_recorded_demand"
        };
    } else if physical_bytes == 0 {
        decision.reason = "prior_unknown_physical_memory";
    } else {
        // Two ceilings, the lower wins: what is left of PHYSICAL after the serve's host
        // working set and the OS floor, and what is AVAILABLE right now less the same
        // floor. On the 5090 (2026-09-17) the first said 55 GB while 31 GB of host RAM
        // was free: a `--cache-ram` that large fills to a pagefile thrash (the
        // 2026-08-01 shape) once two 115k-token states are saved. The available ceiling
        // holds it to what the box can actually give. (Not the commit charge — see the
        // field doc: on a CUDA host that reading is always past physical.)
        let floor = host_os_floor_bytes(physical_bytes);
        let headroom = physical_bytes
            .saturating_sub(serve_host_bytes)
            .saturating_sub(floor);
        let afford = if available_bytes > 0 {
            headroom.min(available_bytes.saturating_sub(floor))
        } else {
            headroom
        };
        // A cache that fills INTO the pressure band throttles its own planner. The live
        // budget steps the serving mode to Eco (budget × 0.55) when available RAM drops
        // under ECO_ENTER_BYTES and only leaves it above ECO_EXIT_BYTES. Measured
        // 2026-09-19 on the M5: a 17,021 MiB grant from 29.6 GB available filled, the
        // box crossed Comfort ↔ Eco five times in a morning (available 7 ↔ 12 GiB), and
        // at 12:19Z the lane RELAUNCHED at `--parallel 1` — five coders on one lane, a
        // seat that had read HEALTHY at three lanes two hours earlier — while the same
        // 17 GB grant was handed out again. The grant leaves the Eco EXIT band free
        // after it fills, so a full cache can never step the mode down by itself.
        let afford = if available_bytes > 0 {
            afford.min(
                available_bytes.saturating_sub(crate::capacity::power_mode::ECO_EXIT_BYTES),
            )
        } else {
            afford
        };
        // A cache FILLS to its grant by design, so "everything above the OS floor" is a
        // grant to end the serve at the floor with nothing left for the co-consumers
        // (the core's own growth, a sidecar, the eye). Measured 2026-09-19 on the M5: a
        // 19,519 MiB grant from a 20,468 MiB afford, and the box sat 6 GB into swap with
        // 25 GB in the compressor an hour later. The cache keeps this fraction back; the
        // planner's window no longer does (the mode's fraction and the board's live
        // subtraction are its headroom — `CACHE_GRANT_HEADROOM`).
        let afford =
            (afford as f64 * (1.0 - crate::inference::planning::serving_plan::CACHE_GRANT_HEADROOM)) as u64;
        decision.reason = "resident_demand_kv_estimate";
        decision.affordable_bytes = Some(afford);
        decision.desired_mib =
            crate::inference::lane_args::host_prompt_cache_mib(demands, fp.kv_per_token, afford);
    }
    decision
}

/// PURE: the reason a sizing reports. A measured saved state that raised any demand above
/// its KV estimate made it a measured sizing; the estimate's reason would read the same both
/// ways, and a fresh core's first sizing could not be told from the defect card 37eb1925
/// fixed (Kimi's node, 8eaed76f3: one_conversation 4,816 MiB from a 5,050,734,260-byte page,
/// under reason resident_demand_kv_estimate). Every other reason (a prior, no demand) stands.
fn sized_by(estimated: &[u32], demands: &[u32], reason: &'static str) -> &'static str {
    if reason == "resident_demand_kv_estimate" && demands != estimated {
        "resident_demand_measured_state"
    } else {
        reason
    }
}

#[cfg(test)]
mod tests {
    use super::{sized_by, with_measured_state};

    // what this catches (card 37eb1925, Kimi's node 8eaed76f3): a sizing raised by a measured
    // saved state reported the estimate's reason, so the probe could not show the fix working.
    #[test]
    fn a_measured_state_sizing_says_so_and_a_prior_is_never_relabeled() {
        let raised = with_measured_state(vec![1_000, 2_000], Some(32_768), Some(5_050_734_260));
        assert_eq!(sized_by(&[1_000, 2_000], &raised, "resident_demand_kv_estimate"), "resident_demand_measured_state");
        assert_eq!(sized_by(&[1_000, 2_000], &[1_000, 2_000], "resident_demand_kv_estimate"), "resident_demand_kv_estimate", "no measured state, no claim");
        assert_eq!(sized_by(&[], &raised, "prior_missing_footprint"), "prior_missing_footprint", "a prior is never relabeled");
    }
}
