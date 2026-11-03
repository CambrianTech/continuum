//! Reconcile responsibility of the llama-server process owner.
use super::*;
// ONE LAUNCH (2026-11-03, Joel: "Slot counts should never require shutdown"). The engine
// starts once on a shared KV pool (`--kv-unified`); which minds hold how much of it is
// decided at runtime. A different window, lane count or cache grant is never a relaunch
// here: only a different model, genome the engine cannot take in place, sight, KV type,
// engine binary, an engine not yet on the shared pool, or a dead engine. The window,
// lane and cache grow ladders that lived here (WINDOW_RELAUNCH_TOLERANCE, the
// narrowing-trade guard, the memory-gate refusal) relaunched the M5's engine 40 times.

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
            // SIGHT (2026-09-18). Today's decision may put
            // the projector on this lane (the only eyes on the node) while the running
            // server was spawned by an earlier core or policy without one: /props says
            // vision=false, and adopting it would keep the node blind until an
            // unrelated relaunch. A lane whose sight is below target relaunches for
            // that alone. A build
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
            // THE SHARED POOL, once: an engine launched with fixed slots (every node before
            // 2026-11-03) is moved to the shared pool by one relaunch; after that no window,
            // lane or cache change ever relaunches it. Unknown (no argv, a fake) is not a
            // mismatch.
            let pool_ok = match ctrl.launched_on_shared_pool() {
                Some(on_pool) => on_pool,
                None => {
                    // Unknown is not a mismatch (no spurious relaunch), but it is said, so a
                    // node left on fixed slots is findable (Cormac on #5720).
                    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.pool.unknown",
                        model = target.model_id(),
                        "the running engine's argv is unreadable: whether it is on the shared pool is unknown, so it is kept as it is"
                    );
                    true
                }
            };
            // THE ENGINE ITSELF — the fourth operand (card 7c5f139d). A deploy installs the
            // engine the pin names (install-llama-server.sh, stamp-gated, atomic), but every
            // deploy leaves the lane up for the next core, so an adopted lane keeps running
            // the binary it was launched from: the M5 and the IntelMac served 965d38a90 (no
            // /train) for a day while the pin moved to 9733aca6c, and the M5 could not dream.
            // Asked of the lane like the other three: its `/props` build against the
            // installed stamp. Unknown on either side (a probe error, no field, an operator
            // override, no stamp) is "engine OK": never a spurious relaunch. Not a resize, so
            // neither the trade guard nor the memory gate applies: the relaunch is at the
            // same size, onto the binary already on disk.
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
            let engine_ok = engine_current || {
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
            };
            *ENGINE_STALE.lock() = (!engine_current).then(|| pair.clone());
            if !engine_ok {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.converged",
                    model = target.model_id(),
                    from = served_engine.as_deref().unwrap_or(""), // probe field: engine_ok=false implies both are known
                    to = installed_engine.as_deref().unwrap_or(""), // probe field: as above
                    "the running lane is on an engine older than the installed one: relaunching \
                     onto the installed binary"
                );
            }
            if !engine_ok || !sight_ok || !kv_ok || !pool_ok {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.relaunch",
                    model = target.model_id(),
                    engine_ok,
                    sight_ok,
                    kv_ok,
                    pool_ok,
                    planned_kv = %planned_kv.cache_type,
                    planned_flash_attn = planned_kv.flash_attn,
                    running_kv = running_kv.as_ref().map(|r| r.cache_type_or_default().to_string()).unwrap_or_default(), // unwrap_or_default: "" = the running decision is unknown (no live argv); kv_ok is then true
                    running_flash_attn = running_kv.as_ref().map(|r| r.flash_attn).unwrap_or(false), // unwrap_or: unknown reads false alongside the empty type above
                    "the running engine is not the one a launch would start now (its binary, its \
                     sight, its KV decision, or a cache not yet on the shared pool) — relaunching \
                     once; a window or lane change never relaunches",
                );
                // fall through to relaunch on the installed binary, with eyes, on the decided
                // cache type, on the shared pool.
            } else {
                // The launch matches. Is the COMPUTE path alive? A child we spawned
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

/// The refusal last reported by [`keep_incumbent`], so an unchanged one is said once, not on
/// every reconcile tick while the replacement keeps being wanted.
static REPLACEMENT_REFUSED: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// A replacement was refused before anything was signalled (card 931cfd22; enrico 2026-10-25:
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
                "the incumbent engine could not be retired safely and was not signalled: it \
                 keeps serving (the target model, the target genes, and it decodes); the \
                 replacement waits until its custody can be satisfied"
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
        reason: format!("retiring the incumbent was refused ({why}) and it cannot serve the target: {unfit}"),
    }
}
