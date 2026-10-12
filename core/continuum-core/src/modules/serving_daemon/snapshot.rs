//! SNAPSHOT: the served window that sticks, the wedge-heal floor, and the serving snapshot published from an outcome.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

/// Map a reconcile [`EnsureOutcome`] (+ the real per-slot window read from
/// `/props`) to the published [`ServingSnapshot`]. Pure (no IO) so the mapping
/// #175 sticky per-slot window on relaunch. The startup LoRA-load cascade relaunches
/// the living lane once per persona (each genome-set change is a relaunch), and each
/// relaunch recomputes `-c` against now-lower free memory, ratcheting the per-slot
/// window DOWN — which strands the personas pinned to the earlier, larger slot (they
/// then overflow it → the poisoned-slot "Compute error"). Keep the incumbent window
/// when we relaunch at the SAME lane count: its KV is already resident and a same-lane
/// genome reload adds only a tiny LoRA delta, so preserving it is memory-safe. A
/// LANE-count change legitimately resizes per-slot KV (2 slots need half the window
/// each) — keeping the old window across a lane INCREASE would multiply resident KV
/// and risk OOM, so only stick when `live.lanes == plan_lanes`. A LARGER plan window
/// (memory freed) is never held down here — the "starved lane" grow check above owns
/// growing. Pure so the invariant is unit-tested without a live gateway.
pub(super) fn sticky_served_window(plan_window: u32, plan_lanes: u32, live: &ServingSnapshot) -> u32 {
    if live.ready && live.lanes == plan_lanes && live.served_context_window > plan_window {
        live.served_context_window
    } else {
        plan_window
    }
}

/// The #363 wedge-heal floor — [`sticky_served_window`]'s sibling for the case sticky
/// cannot see: the live snapshot was EMPTIED by a wedge declaration (or a cold boot after
/// a crash), so `live.ready` is false and sticky passes the plan through untouched. If a
/// healthy lane with the SAME lane count was observed before, the successor spawns at
/// least that big — the plan's momentary dip is the dying predecessor's memory not yet
/// freed, and that memory is precisely what the successor inherits. Applies ONLY when
/// live is not ready (when it is, sticky already owns the decision) and only same-lanes
/// (a lane-count change legitimately resizes KV).
pub(super) fn wedge_heal_floor(
    plan_window: u32,
    plan_lanes: u32,
    live_ready: bool,
    last_healthy_window: u32,
    last_healthy_lanes: u32,
) -> u32 {
    if !live_ready && last_healthy_lanes == plan_lanes && last_healthy_window > plan_window {
        last_healthy_window
    } else {
        plan_window
    }
}

/// is unit-tested directly. A live/spawned model is `ready` with the served base
/// url AND the real served window; a degraded reconcile — OR a ready server whose
/// window we could not read (`served_context_window == 0`) — publishes "nothing
/// live" rather than a half-true "ready but no/guessed window"
/// ([[fallbacks-are-illegal-fail-loud]]). The window-0 guard is what keeps a
/// persona from ever binding a broken prompt budget: it would rather see the gap
/// (and the next tick re-reads `/props` via `AlreadyServing` → self-heals) than
/// budget against a zero window.
/// The grant of a lane THIS core did not spawn — a past form of ourself, adopted across a
/// deploy, whose handle records no spawn fact. Read off the ENGINE'S OWN ARGV
/// (`--cache-ram N` on the canonical pid), the way its window and slots are read off
/// `/props`: the process's truth, never a re-derivation (Cormac's condition on the fix,
/// 2026-09-20: before it, the adopted case — every deploy — read the grant as 0 and the
/// measurement attributed the whole prompt cache as per-token bytes, the same corrupt
/// record the fix exists to stop, now on the deploy path). An argv never changes for the
/// life of a pid, so the read is cached by pid — one single-pid `sysinfo` refresh per
/// adopted engine, not one per tick. `None` = no live pid, or no flag on its line —
/// unknown stays unknown.
pub(super) fn adopted_prompt_cache_mib() -> Option<u32> {
    static BY_PID: parking_lot::Mutex<Option<(u32, Option<u32>)>> = parking_lot::Mutex::new(None);
    let pid = crate::inference::lane_pidfile::read()?;
    let mut cached = BY_PID.lock();
    if let Some((p, mib)) = *cached {
        if p == pid {
            return mib;
        }
    }
    let mib = crate::inference::lane_process::command_args(pid)
        .as_deref()
        .and_then(crate::inference::lane_process::cache_ram_mib_in);
    *cached = Some((pid, mib));
    mib
}

/// PURE: the lane count a ready snapshot publishes — the engine's `/props total_slots`
/// when it names one, else what was asked of it. An engine adopted with MORE slots than
/// the plan wants (a down-plan is the daemon's sticky choice; only growth relaunches)
/// still serves every slot it has: that is the residency the board charges and the
/// ambient budget the citizens get.
pub fn published_lanes(served: Option<u32>, target_lanes: u32) -> u32 {
    served.filter(|n| *n > 0).unwrap_or(target_lanes) // unwrap_or: an engine that names no slot count serves what was asked of it (see the doc)
}

pub(super) fn snapshot_from_outcome(
    outcome: &EnsureOutcome,
    desired: &str,
    adapters: &[String],
    served_context_window: u32,
    lanes: u32,
    // The `--cache-ram` the running engine was launched with (a fact off the control;
    // 0 = unknown). Rides the snapshot so the measurement and the board charge the
    // engine's grant, never the plan's latest derivation.
    host_prompt_cache_mib: u32,
    // The VERIFIED vision endpoint on this node, if any (#106): the main lane
    // itself when its model sees, or the sidecar lane. `Some` ⇒ its `/props`
    // confirmed sight; the snapshot's `vision_ready`/`vision_base_url`/
    // `vision_model` are all projected from this ONE value, so an address can
    // never be published without the verified flag (or vice versa).
    vision: Option<crate::inference::vision_sidecar::SidecarLane>,
) -> ServingSnapshot {
    match outcome {
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. }
            if served_context_window > 0 =>
        {
            ServingSnapshot {
                // A lane with a verified window is serving, not loading.
                loading_model: None,
                // The claim carries the age of its evidence. Stamped HERE, at the
                // moment the reconcile confirmed readiness — not at read time, which
                // is what let `ready:true` survive a SIGKILLed process for as long as
                // nobody republished. A reader compares this against now and decides
                // whether the claim is still worth trusting.
                //
                // `.ok()`, NOT `.unwrap_or(0)`: if the clock is before the epoch we
                // cannot attest to WHEN this was verified, and `Some(0)` would be a
                // fabricated attestation (verified in 1970) rather than an absent one.
                // `None` already means "never confirmed" in this field's contract, so
                // the honest answer is available — take it. Inventing a plausible
                // number is the worse half of swallowing an error: it doesn't just
                // hide the failure, it feeds fiction to every downstream reader.
                ready_verified_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .ok(),
                active_model: Some(desired.to_string()),
                ready: true,
                base_url: serving_v1_url(),
                // The genome set now live — feeds the next reconcile's relaunch
                // guard and gives readers the active genome without probing.
                adapters: adapters.to_vec(),
                // The real per-slot window the process serves — every persona
                // budgets its prompt to THIS (task #50, the drift fix).
                served_context_window,
                // The `--parallel` slot count — lets a reader (the resource
                // authority's footprint(), a grid allocator) charge total resident
                // KV as `lanes × kv_at(served_context_window)` (#79).
                lanes,
                host_prompt_cache_mib,
                degraded_reason: None,
                // The reconcile's verified multimodal verdict (#106): the ONE
                // `vision` value projects all three fields, so readiness and
                // the routing address can never disagree. The observe path
                // gates on THIS, never on a row's declaration alone.
                vision_ready: vision.is_some(),
                vision_base_url: vision.as_ref().map(|v| v.base_url.clone()),
                vision_model: vision.map(|v| v.model_id),
            }
        }
        // Ready outcome but the served window was unreadable (0) → do NOT publish
        // a ready snapshot with a zero window; that would poison every binding
        // persona's budget. Publish "nothing live"; the server stays up and the
        // next reconcile re-reads /props.
        // LOADING. The lane is up (spawned or already serving) but `/props` has not yet
        // given us a real window, so it is not READY. This arm used to return
        // `ServingSnapshot::empty()` and DISCARD `desired` — throwing away the only
        // knowledge of what is being brought up, for the whole load window. Measured on a
        // cold boot 2026-08-19: physical climbed 29.90 → 36.88 GB while serving attributed
        // 0.00 GB, because the consumer had nothing to name. Those bytes read as unowned,
        // and a plan computed in that window sizes against a machine that looks two thirds
        // full of someone else's memory.
        //
        // The bytes exist from spawn, not from readiness. Name the model so the consumer
        // can charge them.
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => ServingSnapshot {
            loading_model: Some(desired.to_string()),
            ..ServingSnapshot::empty()
        },
        // A Degraded reconcile PUBLISHES its reason — the spawn/probe failure
        // (e.g. a missing llama-server binary, its path in the text) reaches
        // `serving/status` instead of dying as an anonymous empty snapshot
        // (live repro 2026-07-24: Windows spawn failed every tick, status
        // showed null/false with no why).
        EnsureOutcome::Degraded { reason } => ServingSnapshot::degraded(reason.clone()),
        EnsureOutcome::Superseded => ServingSnapshot::degraded("serving intent superseded".into()),
    }
}
