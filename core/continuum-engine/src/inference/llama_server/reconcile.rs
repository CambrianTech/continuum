//! Reconcile responsibility of the llama-server process owner.
use super::*;
/// A same-model/same-genome relaunch is required when the target per-slot window
/// exceeds the running server's served window by MORE than this. llama.cpp has no
/// hot-resize API, so a genuine window GROW can only be honored by a relaunch —
/// exactly like an adapter-set change. The margin absorbs llama.cpp's internal
/// 256-multiple padding of the launch `-c/--parallel` window (served ≈ round-up-256
/// of the launched per-slot value), so a padded steady-state window never reads as
/// a spurious grow and re-triggers a relaunch every tick. Comfortably above one
/// 256-pad; the daemon only sends a grow target when it is ≥ 2× the served window
/// (its `starved` gate), so this margin never masks a real grow.
// context-budget-exempt: a HYSTERESIS band: how far the served window may drift before a relaunch is worth it. Tolerance, never a cap — it doesn't limit the window, it stops us thrashing it
pub(super) const WINDOW_RELAUNCH_TOLERANCE: u32 = 512;


/// Would relaunching toward `(target_lanes, target_window)` leave every citizen a
/// NARROWER lane than `(served_lanes, served_window)` serves, in exchange for more
/// lanes? The grow ladder's one geometry question (card d9d747b2): a lane grow that
/// costs window is a trade the ladder refuses; a pure lane grow, a window grow (at
/// any lane count), and an unreadable served side (0) are not. "Narrower" uses the
/// same tolerance the window check does — a 256-pad or the boot plan's drift under a
/// served window is not a trade.
pub(crate) fn lane_grow_would_narrow_the_window(
    target_lanes: u32,
    target_window: u32,
    served_lanes: u32,
    served_window: u32,
) -> bool {
    if served_lanes == 0 || served_window == 0 || target_lanes <= served_lanes {
        return false;
    }
    let tolerance = WINDOW_RELAUNCH_TOLERANCE.max(served_window / 8);
    target_window.saturating_add(tolerance) < served_window
}


/// Is this relaunch a GROW that the closed memory gate forbids? The gate closes at
/// sustained Critical pressure and its contract is that subsystems refuse new
/// allocations (`is_memory_gate_closed`) — and a relaunch toward more lanes or a wider
/// window is the largest elective allocation this node makes. Eval and Whisper read the
/// gate; the grow ladder did not. Measured on the M5 2026-09-23: the engine settled at
/// 8 lanes × 35,328 while `memory.pressure` read Critical for 39 consecutive samples,
/// 36.8 GB resident on a 69 GB box. And fseventsd has twice taken over a quarter of the
/// machine (26.6 GB 2026-09-08, 14 GB 2026-09-24) — growing into THAT is how an
/// unattended node goes down.
///
/// Only the lane and window axes are refused. A KV-type or sight relaunch is a
/// DECISION change, not necessarily a larger footprint, and stays allowed. Pure, so the
/// test does not touch the process-global gate.
pub(crate) fn grow_refused_by_memory_gate(gate_closed: bool, window_ok: bool, lanes_ok: bool, cache_ok: bool) -> bool {
    gate_closed && (!window_ok || !lanes_ok || !cache_ok)
}


/// Pure reconcile decision: bring the running server in line with `desired`.
/// Split from the process impl so the branch logic is unit-tested against a
/// fake. `Unreachable` from the probe means "nothing up" → serve; any other
/// probe error degrades rather than blindly relaunching over a sick server.
pub async fn ensure_model_serving<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
    // Force a decode SMOKE-PROBE even on a child we own, bypassing the "trusted
    // thereafter" short-circuit. The serving daemon's liveness heartbeat sets this after
    // it has already seen the live lane fail the decode probe on a slow cadence
    // (#175): a Metal-OOM-poisoned backend answers `/v1/models` 200 while every decode
    // 500s, so without re-proving decode the owned-child trust would re-adopt the wedged
    // lane forever. `false` is the steady state (no per-tick decode load).
    force_probe: bool,
) -> EnsureOutcome {
    ensure_model_serving_if_current(ctrl, target, force_probe, &|| true).await
}


pub async fn ensure_model_serving_if_current<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
    force_probe: bool,
    current: &ServingLifecycleCheck<'_>,
) -> EnsureOutcome {
    let outcome = serve_or_adopt(ctrl, target, force_probe, current).await;
    // Every adoption, whichever rail took it (an owned child, a decode-proven orphan, a busy
    // one), records the lane's placement when its record lacks one (card 22aa3379).
    // Every adoption also follows the engine's log relay: a no-op for one already followed.
    if matches!(outcome, EnsureOutcome::AlreadyServing) {
        ctrl.record_observed_placement().await;
        ctrl.follow_engine_log();
    }
    outcome
}

/// Engine replacement alone is not permission to apply a different geometry.
/// Geometry changes are reconsidered separately after convergence, under the trade guards.
pub(super) fn engine_convergence_target(
    planned: &ServingTarget,
    served_window: u32,
    served_lanes: u32,
) -> Result<ServingTarget, String> {
    let mut selected = planned.clone();
    {
        if served_window == 0 || served_lanes == 0 {
            return Err("engine convergence requires the incumbent's observed window and lane count; geometry is unknown".to_string());
        }
        selected.context_window = served_window;
        selected.lanes = served_lanes;
    }
    Ok(selected)
}

async fn serve_or_adopt<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
    // Force a decode SMOKE-PROBE even on a child we own, bypassing the "trusted
    // thereafter" short-circuit. The serving daemon's liveness heartbeat sets this after
    // it has already seen the live lane fail the decode probe on a slow cadence
    // (#175): a Metal-OOM-poisoned backend answers `/v1/models` 200 while every decode
    // 500s, so without re-proving decode the owned-child trust would re-adopt the wedged
    // lane forever. `false` is the steady state (no per-tick decode load).
    force_probe: bool,
    current: &ServingLifecycleCheck<'_>,
) -> EnsureOutcome {
    let mut convergence_pair = None;
    let mut convergence_target = None;
    let active = match ctrl.active_model().await {
        Ok(active) => active,
        Err(LlamaServerError::Unreachable(_)) => None,
        Err(other) => {
            return EnsureOutcome::Degraded {
                reason: format!("probe failed: {other}"),
            }
        }
    };

    if active.as_deref() == Some(target.model_id()) {
        // Same base model live. Now check the loaded genome SET: a new gene (or a
        // retired one) is a set change that llama.cpp can only honor by relaunch
        // (no hot-load API). An unreadable adapter probe is treated as "unknown
        // set" → relaunch to be safe, never a silent serve of a stale genome.
        let desired = target.adapter_paths();
        // The comment above states the policy: an unreadable probe is an UNKNOWN set →
        // relaunch. `.unwrap_or_default()` did the opposite whenever `desired` is also
        // empty (the common no-genome case): the error became an empty Vec that COMPARED
        // EQUAL to the target, short-circuiting to AlreadyServing — the exact silent
        // stale-serve the comment promises never happens. Honor the stated policy.
        let genome_matches = match ctrl.active_adapters().await {
            Ok(active) if active == desired => true,
            Ok(active) => {
                // A KNOWN set that differs: the engine takes the change in place (fork #26:
                // /lora-adapters/load and /unload) instead of a relaunch that kills every
                // turn in flight and re-prefills every slot (charter S1, card 49b5e806).
                // Only an engine that cannot falls through to the relaunch.
                match ctrl.adopt_genome_in_place(target).await {
                    Ok(GenomeInPlace::Adopted) => {
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.genome.adopted_in_place",
                            model = target.model_id(),
                            from = active.len() as u64,
                            to = desired.len() as u64,
                            "the gene set changed and the running engine took it: no relaunch"
                        );
                        true
                    }
                    Ok(GenomeInPlace::Busy) => {
                        // The set stands this tick, and the window, lane and engine checks
                        // below still run: a busy retire must not hide a starved lane.
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.genome.in_place_busy",
                            model = target.model_id(),
                            "a retiring gene is active in a turn in flight: the change waits a tick"
                        );
                        true
                    }
                    Ok(GenomeInPlace::Unsupported) => {
                        // The costliest fall-through this function has (every turn in flight
                        // killed, every slot re-prefilled), so it is named like its siblings:
                        // an engine without the in-place routes, or a gene with no staged name.
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.genome.in_place_unsupported",
                            model = target.model_id(),
                            from = active.len() as u64,
                            to = desired.len() as u64,
                            "the gene set changed and this engine cannot take it in place: relaunching onto the target set"
                        );
                        false
                    }
                    Err(e) => {
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.genome.in_place_failed",
                            model = target.model_id(),
                            error = %e,
                            "the in-place gene change failed: relaunching onto the target set"
                        );
                        false
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "continuum_core::inference::llama_server", error = %e,
                    "could not read the served genome set — treating it as UNKNOWN and \
                     falling through to relaunch rather than risk serving a stale genome"
                );
                false
            }
        };
        if genome_matches {
            // Right model, right genome — but does the running server's per-slot
            // WINDOW match the target? llama.cpp cannot hot-resize, so a target
            // window that meaningfully EXCEEDS the served window is a real mismatch
            // only a relaunch can honor — exactly like the adapter-set change below.
            // Without this, a daemon-decided starved grow-back (2048→31k, same model
            // + same genome) short-circuits to AlreadyServing and the lane stays
            // frozen at the boot floor FOREVER, while the daemon re-decides "starved"
            // and logs "re-homing" every tick — a relaunch that never happens
            // (glass-boxed 2026-07-20: window pinned 2048 vs a 31k plan, benchmarks
            // blocked). Grow-only: a down-plan is kept by the daemon's sticky window
            // and never reaches here; a `/props` read failure (served 0/Err) is
            // treated as "window OK" so a probe error never spuriously relaunches.
            let served_window_probe = ctrl.served_context_window().await;
            let window_ok = match &served_window_probe {
                Ok(served) => {
                    let served = *served;
                    // Tolerance is a PERCENTAGE of the served window, floored at the
                    // flat minimum — never a bare flat count. A flat 512 is 0.35% of a
                    // 147k lane, so the boot plan's normal drift (it re-computes against
                    // live memory, which climbs ~7% once the old core frees its share:
                    // measured 2026-09-03, 147712 → 158107) tripped a RELAUNCH — a full
                    // ~7-10 min 35B reload on EVERY reboot, the dominant "resume is slow"
                    // cost. A warm lane at 88% of the ideal window NOW beats a cold ideal
                    // one in 10 minutes; the runtime re-home (streak-debounced) still
                    // grows it later if a bigger window's gain actually persists. A
                    // genuinely starved lane (2048 vs a 31k plan) still relaunches:
                    // 31k ≫ 2048 × 1.125. served 0 / probe-err stays "OK" (no spurious
                    // relaunch), unchanged.
                    let tolerance = WINDOW_RELAUNCH_TOLERANCE.max(served / 8);
                    served == 0 || target.context_window <= served.saturating_add(tolerance)
                }
                Err(e) => {
                    // Deliberate: a probe error must not spuriously relaunch a healthy
                    // lane (stated above). But the error is still evidence — if windows
                    // stop growing, this line is the reason, and it must be findable.
                    tracing::warn!(target: "continuum_core::inference::llama_server", error = %e,
                        "could not read the served window — treating the window as OK \
                         (no spurious relaunch); a starved lane will not grow while this \
                         probe keeps failing"
                    );
                    true
                }
            };
            // LANE grow-back — the exact sibling of the window grow-back above. After a
            // transient RAM/VRAM squeeze relaunched the lane at fewer parallel slots
            // (n_seq_max 4→1), a later plan that wants more lanes would otherwise
            // short-circuit to AlreadyServing on `genome_matches && window_ok` and stay
            // FROZEN AT ONE LANE FOREVER — starving every citizen to one-at-a-time while
            // the daemon re-decides the same plan each tick (glass-boxed 2026-08-11:
            // ~11GB free, lanes stuck at 1, a benchmark solve infra-failed). llama.cpp
            // cannot hot-resize n_seq_max any more than the window, so a lane increase
            // needs a relaunch. Grow-only + discrete: served 0 = nothing to compare;
            // target ≤ served is fine (a down-plan is the daemon's sticky choice); only
            // target > served relaunches.
            //
            // ASK THE LANE, NOT OUR MEMORY OF IT (fixed 2026-08-19). This read used to
            // be `current_serving().lanes` — the process-local snapshot — while the
            // window operand three lines up probed the running server. That asymmetry
            // is invisible for a lane we spawned (snapshot == process) and wrong for a
            // PAST FORM OF OURSELF: a lane that survived a crash on the canonical port,
            // named in our own custody record, has no entry in a fresh core's snapshot.
            // Measured cost of the old read: a healthy 4-lane 27B declined, reaped and
            // rebuilt over ~110s, returning at 3 lanes and a SMALLER window than the
            // lane that was already running. Both operands now ask the process.
            let served_lanes_probe = ctrl.served_lanes().await;
            let lanes_ok = match &served_lanes_probe {
                Ok(served) => *served == 0 || target.lanes <= *served,
                Err(e) => {
                    // Deliberate, and identical to the window's policy above: a probe
                    // error must not spuriously relaunch a healthy lane. The error is
                    // still evidence — if lanes stop growing, this line is the reason.
                    tracing::warn!(target: "continuum_core::inference::llama_server", error = %e,
                        "could not read the served lane count — treating lanes as OK (no \
                         spurious relaunch); a starved lane will not grow while this probe \
                         keeps failing"
                    );
                    true
                }
            };
            // Report what the LANE said, and say so when it could not be asked — a
            // probe that silently substitutes a number is how the old bug hid.
            let served_lanes = match &served_lanes_probe {
                Ok(n) => n.to_string(),
                Err(e) => format!("unreadable ({e})"),
            };
            // SIGHT grow-back — the third sibling (2026-09-18). Today's decision may put
            // the projector on this lane (the only eyes on the node) while the running
            // server was spawned by an earlier core or policy without one: /props says
            // vision=false, and adopting it would keep the node blind until an
            // unrelated relaunch. A lane whose sight is below target relaunches for
            // that alone, exactly like a window or lane count below target. A build
            // that publishes no modalities block is unverifiable and is NOT relaunched
            // in a loop for it (the daemon reports that case as unverified sight).
            let wants_sight =
                main_lane_mmproj_decision_for(&target.model, target.vision_sidecar).wears();
            let sight_ok = if !wants_sight {
                true
            } else {
                match ctrl.multimodal_support().await {
                    Ok(Some(m)) => m.vision,
                    Ok(None) => true,
                    Err(e) => {
                        tracing::warn!(target: "continuum_core::inference::llama_server", error = %e,
                            "could not read the served modalities — treating sight as OK (no \
                             spurious relaunch); a blind lane will not grow while this probe \
                             keeps failing"
                        );
                        true
                    }
                }
            };
            // THE KV DECISION IS LAUNCH GEOMETRY TOO (card 977842fd). The cache type and
            // flash-attention are applied at spawn; an engine now survives a deploy
            // (#4284) and is adopted across a window drift on purpose. Measured on the
            // 5090, 2026-09-21 00:33Z: the new core read the q8_0 decision (#4285),
            // found its healthy f16 engine at 26,880, adopted it, and the decision never
            // reached the process it was written to change — three minds fell home from
            // that seat within seven minutes. Adopt against the geometry a spawn would
            // produce NOW: a different decision is a stale lane and relaunches once, the
            // probe naming both. Unknown (no argv, a fake) is not a mismatch.
            let planned_kv = crate::inference::kv_cache_plan::resolve();
            let running_kv = ctrl.launch_kv();
            let kv_ok = running_kv.as_ref().is_none_or(|r| {
                r.cache_type_or_default().eq_ignore_ascii_case(&planned_kv.cache_type)
                    && r.flash_attn == planned_kv.flash_attn
            });
            // A LANE GROW MUST NOT NARROW THE WINDOW (card d9d747b2, the M5 2026-09-21
            // 09:29-09:36Z). The four checks are per-axis and grow-only, but a relaunch is
            // a GEOMETRY change: the plan asked for 5 lanes and, to fit them, sized each at
            // 36,810 over a lane serving 3 × 78,592. `lanes_ok` fired, the engine relaunched
            // at 5 × 36,810, the next plan found the window starved and relaunched back to
            // 3 × 78,592 — every ~3 min, 35 generations reaped in 20, a prompt cache that
            // could never warm. The window is the requirement and lanes are throughput
            // (card 18ba6a1b: the window first, then lanes), so a lane-driven relaunch that
            // would leave every citizen a NARROWER lane than she has is a trade, not a
            // grow-back, and the ladder refuses it. The opposite trade — a wider window at
            // fewer lanes — stays allowed: that is the starved-window grow-back this ladder
            // exists for. A plan that keeps asking is now a probe line, not a relaunch.
            let served_window_n = served_window_probe.as_ref().ok().copied().unwrap_or(0); // unwrap_or: unreadable = 0 = no served window to compare against, same as window_ok's policy
            let served_lanes_n = served_lanes_probe.as_ref().ok().copied().unwrap_or(0); // unwrap_or: unreadable = 0 = no served lanes to compare against, same as lanes_ok's policy
            // THE CACHE GRANT IS LAUNCH GEOMETRY TOO (card d7294f0b). `--cache-ram` is fixed
            // at spawn. Kimi's node, 2026-10-15 07:37-07:39Z: the daemon found her adopted
            // lane's 3,248 MiB grant held no measured state and decided "re-homing" twice,
            // but this rail compared only model, genes, window, lanes, sight and the KV type,
            // answered AlreadyServing, and every switch between minds stayed a full re-prefill.
            // The daemon's own rule decides here too: relaunch only when the running grant
            // holds no state and the target's would. Unknown (no argv) or drift is never one.
            let page_dir = kv_page_dir(target.model_id());
            crate::inference::slots::seed_state_samples(&page_dir).await;
            let cache_ok = !crate::inference::lane_args::cache_relaunch_owed(
                running_kv.as_ref().and_then(|r| r.cache_ram_mib),
                target.host_prompt_cache_mib,
                crate::inference::slots::measured_state_bytes(&page_dir),
            );
            let refused_trade = !lanes_ok
                && window_ok
                && sight_ok
                && kv_ok
                && lane_grow_would_narrow_the_window(
                    target.lanes,
                    target.context_window,
                    served_lanes_n,
                    served_window_n,
                );
            if refused_trade {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.grow_refused_trade",
                    model = target.model_id(),
                    target_lanes = target.lanes,
                    target_window = target.context_window,
                    served_lanes = served_lanes_n,
                    served_window = served_window_n,
                    "the plan wants more lanes at a NARROWER window than the lane serves — a \
                     trade, not a grow-back; refused (card d9d747b2: following it relaunched the \
                     engine every ~3 min between 5 × 36k and 3 × 78k). The lane keeps its window; \
                     more lanes come when they fit at it",
                );
            }
            let gate_closed = crate::system_resources::is_memory_gate_closed();
            let refused_pressure = !refused_trade && grow_refused_by_memory_gate(gate_closed, window_ok, lanes_ok, cache_ok);
            if refused_pressure {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.grow_refused_memory_gate",
                    model = target.model_id(),
                    target_lanes = target.lanes,
                    target_window = target.context_window,
                    served_lanes = served_lanes_n,
                    served_window = served_window_n,
                    window_ok,
                    lanes_ok,
                    "the plan wants a larger engine but the memory gate is CLOSED (sustained \
                     Critical) — refused; the lane keeps its geometry. Growing into a machine \
                     that is already short is how an unattended node goes down. The grow is \
                     re-asked every tick and proceeds once the gate reopens",
                );
            }
            // THE ENGINE ITSELF — the fourth operand (card 7c5f139d). A deploy installs the
            // engine the pin names (install-llama-server.sh, stamp-gated, atomic), but every
            // deploy leaves the lane up for the next core, so an adopted lane keeps running
            // the binary it was launched from: the M5 and the IntelMac served 965d38a90 (no
            // /train) for a day while the pin moved to 9733aca6c, and the M5 could not dream.
            // Asked of the lane like the other three: its `/props` build against the
            // installed stamp. Unknown on either side (a probe error, no field, an operator
            // override, no stamp) is "engine OK": never a spurious relaunch. Not a resize, so
            // neither the trade guard nor the memory gate applies: the relaunch is at the
            // same size, onto the binary already on disk. No slot holding the pin (`Err`):
            // a relaunch would be refused before the transition, so the convergence is
            // reported blocked and the incumbent keeps serving.
            let pin_hold = super::engine_pin::hold_current_engine_to_pin();
            let installed_engine = installed_engine_commit();
            let served_engine_probe = ctrl.served_engine_build().await;
            // The same reading decides what this engine process can take (`tail_tools`); an
            // error keeps the evidence only while the same incarnation serves (Codex on #5491).
            observe_served_engine(ctrl.served_incarnation(), served_engine_probe.as_ref().map(Option::as_deref).map_err(|_| ()));
            let served_engine = match served_engine_probe {
                Ok(build) => build,
                Err(_) => None, // a probe error is "engine OK", as for the window and lanes
            };
            let engine_current = match (&served_engine, &installed_engine) {
                (Some(lane), Some(installed)) => lane_runs_installed_engine(lane, installed),
                _ => true,
            };
            // ONCE PER from->to (Fable on #4464). If the relaunched lane still reports the
            // old build (its build_info not regenerated, a local merge, a gitdir-file
            // submodule), relaunching again would put the lane dark on a loop onto the
            // same binary. The first disagreement relaunches; the same pair seen again only
            // says so, and the health line keeps `engine_stale` for a human to read.
            let pair = format!(
                "{}->{}",
                served_engine.as_deref().unwrap_or(""), // unwrap_or: only read when !engine_current, where both are known
                installed_engine.as_deref().unwrap_or("") // unwrap_or: as above
            );
            let engine_ok = engine_current
                || match &pin_hold {
                    Err(refusal) => {
                        convergence_blocked(target, &pair, refusal);
                        true
                    }
                    Ok(_) => {
                        let tried = ENGINE_CONVERGE_TRIED.lock();
                        if tried.as_deref() == Some(pair.as_str()) {
                            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.converge_failed",
                                model = target.model_id(),
                                pair = pair.as_str(),
                                "the lane still reports an older engine after one relaunch onto the \
                                 installed binary: not relaunching again (a loop would keep it dark); \
                                 engine_stale stays on the health line"
                            );
                            true
                        } else {
                            convergence_pair = Some(pair.clone());
                            false
                        }
                    }
                };
            *ENGINE_STALE.lock() = (!engine_current).then(|| pair.clone());
            if !engine_ok {
                let resize_admitted = false; // Engine convergence and geometry resize are separate lifecycle decisions.
                match engine_convergence_target(target, served_window_n, served_lanes_n) {
                    Ok(selected) => convergence_target = Some(selected),
                    Err(reason) => return EnsureOutcome::Degraded { reason },
                }
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.converged",
                    model = target.model_id(),
                    from = served_engine.as_deref().unwrap_or(""), // probe field: engine_ok=false implies both are known
                    to = installed_engine.as_deref().unwrap_or(""), // probe field: as above
                    resize_admitted,
                    incumbent_lanes = served_lanes_n,
                    incumbent_window = served_window_n,
                    plan_lanes = target.lanes,
                    plan_window = target.context_window,
                    "the running lane is on an engine older than the installed one: relaunching \
                     onto the installed binary; preserve incumbent geometry unless a separate resize was admitted"
                );
            }
            if !engine_ok
                || ((!window_ok || !lanes_ok || !sight_ok || !kv_ok || !cache_ok) && !refused_trade && !refused_pressure)
            {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.grow",
                    model = target.model_id(),
                    target_window = target.context_window,
                    target_lanes = target.lanes,
                    served_lanes = served_lanes.as_str(),
                    window_ok,
                    lanes_ok,
                    sight_ok,
                    kv_ok,
                    cache_ok,
                    planned_kv = %planned_kv.cache_type,
                    planned_flash_attn = planned_kv.flash_attn,
                    running_kv = running_kv.as_ref().map(|r| r.cache_type_or_default().to_string()).unwrap_or_default(), // unwrap_or_default: "" = the running decision is unknown (no live argv); kv_ok is then true
                    running_flash_attn = running_kv.as_ref().map(|r| r.flash_attn).unwrap_or(false), // unwrap_or: unknown reads false alongside the empty type above
                    "served geometry differs from the target (window, lanes, sight and/or the KV \
                     decision) — relaunching (llama.cpp has no hot-resize; a genome-set match \
                     alone must not strand a starved lane at the boot floor, a blind one without \
                     its eyes, or an adopted lane on last deploy's cache type)",
                );
                // fall through to relaunch at the larger window / more lanes / with eyes /
                // on the decided cache type.
            } else {
                // Window matches. Is the COMPUTE path alive? A child we spawned
                // ourselves was decode-verified at `wait_ready` and is trusted
                // thereafter (no per-tick decode load). A server we did NOT spawn (an
                // orphan reclaimed from a dead core) can answer `/v1/models` while
                // every `llama_decode` 500s — so prove decode before adopting it. A
                // wedged orphan fails the probe and falls through to `serve()`, which
                // reaps it and spawns fresh. Trust an owned child without a per-tick
                // decode probe — UNLESS `force_probe` is set, in which case the
                // liveness heartbeat flagged it wedged and we must re-prove decode
                // before re-adopting (#175). A non-owned orphan is always probed.
                if ctrl.owns_child() && !force_probe {
                    return EnsureOutcome::AlreadyServing;
                }
                // BUSY IS NOT DEAD, at adoption too (card 59052747). The heartbeat already
                // reads a missed probe against the server's own `/slots` account (L11,
                // `judge_smoke_miss`); the adopt rail did not, so ONE missed probe reaped
                // the warm engine — measured 2026-09-20 20:44Z on the IntelMac: the adoptee
                // answered `/v1/models`, its decode probe missed while the box was on the
                // deploy's link step, and "compute-wedged" reaped a lane whose slots were
                // moving. The fingerprint is taken before and after the probe: a lane that
                // advanced during the probe's own window is alive by its own account and
                // is adopted; the heartbeat keeps verifying it from there. A FROZEN or
                // unreadable fingerprint is the wedge signature and is reaped as before.
                let before = ctrl.slots_activity_fingerprint().await;
                if ctrl.decode_smoke_ok().await {
                    return EnsureOutcome::AlreadyServing;
                }
                let after = ctrl.slots_activity_fingerprint().await;
                if judge_smoke_miss(before, after) == SmokeMissVerdict::AliveViaSlotProgress {
                    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.adopt_busy_alive",
                        model = target.model_id(),
                        owned = ctrl.owns_child(),
                        force_probe,
                        "lane missed the decode smoke-probe but its slots advanced during it — \
                         busy, not wedged; adopted, the heartbeat verifies from here",
                    );
                    return EnsureOutcome::AlreadyServing;
                }
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.adopt_rejected",
                    model = target.model_id(),
                    owned = ctrl.owns_child(),
                    force_probe,
                    slots_moved = false,
                    "lane answers /v1/models, fails the decode smoke-probe AND its slots did \
                     not move during it (compute-wedged — a poisoned Metal backend / OOM, \
                     #175); reaping + respawning a fresh lane",
                );
                // fall through to relaunch.
            }
        }
        // else: genome set differs → fall through to relaunch.
    }

    // Drop also runs on cancellation: only an actual launch-stage advance consumes
    // this pair. A refused checkpoint must remain eligible on the next tick.
    let target = convergence_target.as_ref().unwrap_or(target); // unwrap_or: absent convergence retains the ordinary planned launch
    let _attempt = super::convergence_attempt::ConvergenceAttempt::new(ctrl, convergence_pair);
    match ctrl.serve_if_current(target, current).await {
        Ok(()) => {
            // A fresh lane starts with a CLEAN failure record. `serve` kills the old
            // child, which murders its in-flight turns; their deaths stamp
            // `note_real_decode_failure` DURING the load window — after the reset
            // `declare_lane_wedged` did — so without this the corpses of the turns OUR
            // kill created condemn the replacement on its first heartbeat and the
            // relaunch loops (measured 2026-08-16: wedge 10:19:04 → kill → 4 stamped
            // fails by 10:19:49 → wedged again; 23 min of self-fighting). The reset
            // comment on `reset_real_decode_failures` always promised "the freshly
            // relaunched lane starts clean" — this is the site that makes it true.
            reset_real_decode_failures();
            mark_engine_proven(ctrl.owned_serving_target().map(|t| t.engine_program));
            observe_launched_engine(ctrl).await;
            EnsureOutcome::Spawned {
                model: target.model_id().to_string(),
            }
        }
        Err(LlamaServerError::Superseded) => EnsureOutcome::Superseded,
        // The incumbent could not be retired safely and was not signalled (card 931cfd22):
        // keep serving on it when it is the target model and still proves it decodes; never
        // condemn a working engine because a replacement was refused.
        Err(LlamaServerError::RetirementRefused(why)) => {
            keep_incumbent(ctrl, target, active.as_deref() == Some(target.model_id()), &why).await
        }
        // No slot holding the pin is current: the launch refused in `prepare`, before any
        // transition, so the incumbent was never signalled and keeps serving when it fits.
        Err(LlamaServerError::EngineUnpinned(refusal)) => {
            keep_incumbent(ctrl, target, active.as_deref() == Some(target.model_id()), &refusal.to_string()).await
        }
        // One relaunch onto the restored engine; the engine is resolved per launch, so this
        // `serve` takes the slot the rollback made current.
        Err(reason) if roll_back_engine_after_failed_launch(&reason) => {
            match ctrl.serve_if_current(target, current).await {
                Ok(()) => {
                    reset_real_decode_failures();
                    mark_engine_proven(ctrl.owned_serving_target().map(|t| t.engine_program));
                    observe_launched_engine(ctrl).await;
                    EnsureOutcome::Spawned {
                        model: target.model_id().to_string(),
                    }
                }
                Err(LlamaServerError::Superseded) => EnsureOutcome::Superseded,
                Err(again) => EnsureOutcome::Degraded {
                    reason: format!("{reason}; after rolling the engine back: {again}"),
                },
            }
        }
        Err(reason) => EnsureOutcome::Degraded {
            reason: reason.to_string(),
        },
    }
}

/// A lane this reconcile launched answered ready: read its own build now, not at the next
/// reconcile that happens to pass the convergence check, so her tool head is decided by what the
/// running engine says before her acts reach it.
async fn observe_launched_engine<C: LlamaServerControl + ?Sized>(ctrl: &C) {
    let probe = ctrl.served_engine_build().await;
    observe_served_engine(ctrl.served_incarnation(), probe.as_ref().map(Option::as_deref).map_err(|_| ()));
}

/// The blocked convergence last reported, so an unchanged one is said once per tick-run.
static CONVERGENCE_BLOCKED: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// The lane runs another engine than the installed one, but no slot holding this core's pin is
/// current, so a relaunch would launch an engine the source does not pin. The incumbent keeps its
/// custody and state; the blocked convergence is said once per change.
fn convergence_blocked(target: &ServingTarget, pair: &str, refusal: &super::engine_pin::EngineUnpinned) {
    let why = refusal.to_string();
    let said = format!("{pair}: {why}");
    if CONVERGENCE_BLOCKED.lock().as_deref() == Some(said.as_str()) {
        return;
    }
    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.convergence_blocked",
        model = target.model_id(),
        pair,
        why = why.as_str(),
        "the lane runs an engine other than the installed one, but no slot holding this core's pin \
         is current: no relaunch (it would launch an unpinned engine); the incumbent keeps serving"
    );
    *CONVERGENCE_BLOCKED.lock() = Some(said);
}

/// The refusal last reported by [`keep_incumbent`], so an unchanged one is said once, not on
/// every reconcile tick while the replacement keeps being wanted.
static REPLACEMENT_REFUSED: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// A replacement was refused before anything was signalled (card 931cfd22: retirement custody;
/// or no pinned engine to launch, `EngineUnpinned`; enrico 2026-10-25:
/// "adopted checkpoint requires a known pool" left a decoding engine unserved for 5 hours).
/// The incumbent keeps serving only when it is exactly what the target asks for: the target
/// model, the KNOWN loaded adapter set equal to the target's (an unreadable or different set
/// never passes, so target genes are never published as loaded), and the decode proof
/// adoption uses, on every refusal (`decode_smoke_ok`: its seat is the scratch slot, an empty
/// engine, or the lane's own `/slots` account when every slot holds a mind, so it never decodes
/// over a mind's KV, #5405). Anything else stays degraded: a condemned engine is never
/// advertised ready. The probe below says each refusal once.
async fn keep_incumbent<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
    serves_target: bool,
    why: &str,
) -> EnsureOutcome {
    let genome_is_target = matches!(ctrl.active_adapters().await, Ok(active) if active == target.adapter_paths());
    let decodes = serves_target && genome_is_target && ctrl.decode_smoke_ok().await;
    if decodes {
        if REPLACEMENT_REFUSED.lock().as_deref() != Some(why) {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.replace.refused_kept_incumbent",
                model = target.model_id(),
                why,
                "a replacement was refused before the incumbent engine was signalled: it \
                 keeps serving (the target model, the target genes, and it decodes); the \
                 replacement waits until its refusal clears"
            );
            *REPLACEMENT_REFUSED.lock() = Some(why.to_string());
        }
        return EnsureOutcome::AlreadyServing;
    }
    *REPLACEMENT_REFUSED.lock() = None;
    let unfit = if !serves_target {
        "it serves another model"
    } else if !genome_is_target {
        "its loaded genes are not known to be the target's"
    } else {
        "it failed its decode proof"
    };
    EnsureOutcome::Degraded {
        reason: format!("a replacement was refused ({why}) and the incumbent cannot serve the target: {unfit}"),
    }
}
