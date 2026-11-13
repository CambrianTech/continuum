//! Boot phase 11c: the persona hosting task. Waits for the wired executor,
//! pulls the first-boot model when none is local, then stays resident as the
//! hosting reconciler driven by the serving daemon's plan edges.

use super::*;

pub(super) async fn run_persona_host(
    executor_ready_rx: tokio::sync::oneshot::Receiver<Arc<crate::runtime::CommandExecutor>>,
    continuum_root_for_boot: std::path::PathBuf,
    persona_floor: usize,
    supervisor: crate::persona::host::PersonaSpawnSupervisor,
    mut serving_plan_rx: tokio::sync::watch::Receiver<
        Option<crate::inference::planning::serving_plan::ServingPlan>,
    >,
) {
    // Wait for the IPC thread to deliver the WIRED executor (this both
    // gates ordering AND hands us the executor the personas' hands ride).
    // If the sender is dropped without sending (substrate shutdown
    // mid-boot), exit cleanly without bootstrapping personas against an
    // uninitialized executor.
    let tool_executor = match executor_ready_rx.await {
        Ok(ex) => ex,
        Err(_) => {
            tracing::warn!(
                "persona supervisor task aborting: executor-ready signal dropped \
                         before fire (substrate shutdown mid-boot? init_executor never \
                         reached?). No personas bootstrapped this run."
            );
            return;
        }
    };
    pull_first_boot_model_if_no_local_gguf(&tool_executor);

    use crate::persona::resume_or_mint_provider::ResumeOrMintProvider;
    // `persona_floor` (read once above) floors the provider's mint count
    // to the SAME value that sized the spawner's plan via with_population
    // — so plan-slots and minted identities stay 1:1.
    let provider = match ResumeOrMintProvider::new(&continuum_root_for_boot, persona_floor).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "ResumeOrMintProvider construction failed — server up, no \
                 citizens online. Resolve continuum_root permissions + \
                 restart, or fire `persona/spawn` manually."
            );
            return;
        }
    };
    // #432: size the roster plan to what the provider WILL yield —
    // every resumed citizen, floored by the mint floor. Before this,
    // plan slots == floor while the provider held all resumed seeds,
    // so with 5 citizens on disk and floor=1 only the alphabetically
    // first came online; the other four sat unhosted forever. The
    // floor remains a MINT floor only ([[benchmarks-use-our-citizens-never-spawn-disposable-solvers]]:
    // the durable population is the asset — leaving members unhosted
    // on disk breaks the contract).
    // THE ONE-ROSTER RULE (card b3b922c0): the provider consults the grid before
    // minting — a node that arrives empty into a grid already seating a team
    // mints none and offers lanes. Read lazily at each draw, so a team heard
    // after boot still counts, and remembered across the first computer's reboots.
    let mut provider = provider.with_grid_view({
        let home = continuum_root_for_boot.clone();
        std::sync::Arc::new(move || {
            crate::persona::grid_roster_memory::team_elsewhere_at(
                &home,
                crate::persona::operator_peer::operator_airc().map(|a| a.peer_id().as_uuid()),
                continuum_base::time::EpochMs::now().millis(),
            )
        })
    });
    let mut supervisor = supervisor;
    let population = provider.identities_available();
    if population > persona_floor {
        tracing::info!(
            resumed_plus_floor = population,
            persona_floor,
            "resumed citizens exceed the mint floor — plan re-sized so \
                     EVERY citizen on disk comes online (#432)"
        );
    }
    supervisor.set_population(population);
    // Event-driven spawn: the persona host reacts to the serving
    // daemon's published plan (its watch channel) instead of probing
    // the gateway once at boot. The serving daemon ticks every 5s; the
    // local llama-server may still be loading the model when boot first
    // reaches here, so the adapter factory's live `/v1/models` probe
    // can fail on the first attempt — the "0 citizens hosted" race.
    //
    // Rather than leave the citizen permanently mute (the bug: a
    // one-shot probe that lost a race), reconcile on each plan edge:
    // retry `spawn_all` until the persona materializes. When the
    // gateway finishes warming, the next tick's plan edge drives a
    // successful spawn — no restart, self-healing like the rest of the
    // substrate. (Single-persona reality today — P2. Idempotent
    // multi-slot reconcile + respawn-on-death is the slice-14
    // follow-on; we break once a citizen is hosted to avoid
    // re-bootstrapping a live persona's airc identity.)
    let mut attempt = 0u32;
    // #429: once boot composition succeeds, this task does NOT end —
    // it becomes the standing HOSTING RECONCILER. `persona/spawn`
    // births end at `registry.register` (identity only); hosting
    // (adapter + cognition loop) is the supervisor's job, and before
    // this flag existed it ran exactly once, so a command-born
    // citizen was on airc, carded — and MUTE. Post-boot, each
    // serving-plan edge drives `host_unattended`: registered
    // citizens with no service loop get the same materialize +
    // attach pipeline boot slots got.
    let mut booted = false;
    // GLASS BOX (#412 residency stall): tracing lines from this task did not survive
    // the concurrency for diagnosis; a probe does. This pins whether the reconciler is
    // even alive after a reboot-with-adopted-lane, how many passes it makes, and what
    // it saw each pass — the exact seam three indirect hypotheses could not resolve.
    continuum_base::probe!(
        class = "persona.host.reconciler_started",
        population = provider.identities_available(),
        "hosting reconciler task alive — entering the plan-edge loop"
    );
    let mut pass = 0u32;
    // Card 08ef6101: the reconciler runs at ~1 Hz and both of its rows said
    // the same thing every second (2 × 3,600 rows/hour on a healthy node,
    // the ledger's largest class). A row now means a CHANGE — or one
    // heartbeat per RECONCILER_HEARTBEAT_PASSES so "still running, still
    // the same" is never silent.
    const RECONCILER_HEARTBEAT_PASSES: u32 = 60;
    let mut last_pass_row: Option<(bool, bool, bool, bool, u32)> = None;
    let mut last_ready_row: Option<(bool, bool, u32)> = None;
    // The grid allocation's publishes wake the reconciler too (card 10bba591):
    // a node joining or a plan changing re-seats the grid, and this node's open
    // seats are drawn on that edge, not on the next plan edge.
    let mut alloc_rx = crate::modules::grid_allocator::subscribe();
    let mut alloc_closed = false;
    loop {
        pass += 1;
        let plan_ready = serving_plan_rx
            .borrow()
            .as_ref()
            .map(|p| p.fits_on_gpu)
            .unwrap_or(false);
        // Lane-source-agnostic hosting (misfit / grid design): when the
        // operator pinned an EXTERNAL OpenAI-compatible endpoint, there is
        // no local lane to "fit on GPU" — enter the ready-check regardless
        // of the local plan. `await_ready_serving` short-circuits to a
        // direct decode-probe of the pinned endpoint (K3, a grid peer).
        let external_lane = crate::inference::llama_server::external_serving_pin().is_some();
        // The roster follows the LIVE plan's lane count (a pin or tier swap
        // must never leave more minds than warm lanes — 2026-09-06).
        let live_plan = serving_plan_rx.borrow().clone();
        supervisor.refresh_serving(live_plan.as_ref());
        // This node's seats as the grid allocation counts them (seated + open),
        // once the daemon has published; `None` keeps the warm-slot prior.
        let roster_here = alloc_rx.borrow().as_ref().and_then(|p| p.roster_here());
        supervisor.refresh_grid_roster(roster_here);
        let snapshot_live = crate::inference::llama_server::current_serving().is_live();
        let pass_state = (plan_ready, external_lane, booted, snapshot_live);
        let pass_row_due = match last_pass_row {
            Some((a, b, c, d, at)) => {
                (a, b, c, d) != pass_state || pass.wrapping_sub(at) >= RECONCILER_HEARTBEAT_PASSES
            }
            None => true,
        };
        if pass_row_due {
            continuum_base::probe!(
                        class = "persona.host.reconciler_pass",
                        pass = pass,
                        plan_ready = plan_ready,
                        external_lane = external_lane,
                        booted = booted,
                        snapshot_live = snapshot_live,
                        changed = last_pass_row.map(|(a, b, c, d, _)| (a, b, c, d) != pass_state).unwrap_or(true),  // unwrap_or: first row of the boot = a change by definition
                        "hosting reconciler pass — will enter host body iff (plan_ready || external_lane); a row = a change or one heartbeat per 60 passes"
                    );
            last_pass_row = Some((plan_ready, external_lane, booted, snapshot_live, pass));
        }
        if plan_ready || external_lane {
            // `fits_on_gpu` is a RESOURCE decision (the model fits VRAM) — it does
            // NOT prove the lane can DECODE. A lane can fit yet fail EVERY
            // generation with `500 "Compute error."` while `/health` still answers
            // 200 — e.g. spawned with a flag that forces embedding/non-causal mode
            // (the live-2026-07-03 `--embeddings` outage), a bad LoRA, or a wedged
            // Metal context. Hosting personas onto such a lane makes every turn
            // 500 SILENTLY. So gate the spawn on the DECODE-VERIFIED serving
            // snapshot: `ServingSnapshot.ready` is published only on a serve/adopt
            // outcome that passed the 1-token decode smoke-probe. `await_ready_serving`
            // parks on the serving snapshot until the lane proves it can think, or
            // the deadline lapses. On timeout we do NOT spawn (a can't-decode lane
            // is a loud, novel failure — never a silent per-turn 500); we park and
            // retry on the next serving edge, self-healing once the lane recovers.
            let decode_ready = crate::inference::llama_server::await_ready_serving(
                std::time::Duration::from_secs(120),
            )
            .await
            .is_some();
            let ready_state = (decode_ready, booted);
            let ready_row_due = match last_ready_row {
                Some((a, b, at)) => {
                    (a, b) != ready_state || pass.wrapping_sub(at) >= RECONCILER_HEARTBEAT_PASSES
                }
                None => true,
            };
            if ready_row_due {
                continuum_base::probe!(
                            class = "persona.host.await_ready",
                            pass = pass,
                            decode_ready = decode_ready,
                            booted = booted,
                            changed = last_ready_row.map(|(a, b, _)| (a, b) != ready_state).unwrap_or(true),  // unwrap_or: first row of the boot = a change by definition
                            "await_ready_serving returned — decode_ready gates hosting; a row = a change or one heartbeat per 60 passes"
                        );
                last_ready_row = Some((decode_ready, booted, pass));
            }
            if !decode_ready {
                tracing::warn!(
                    "serving plan fits on GPU but the lane is NOT decode-ready \
                             within 120s — /health may be 200 while every generation 500s \
                             (a broken spawn flag / wedged compute context). NOT hosting \
                             personas onto a lane that cannot generate; retrying on the \
                             next serving edge."
                );
            } else if !booted {
                attempt += 1;
                if attempt > 1 {
                    // Every attempt draws from the ONE provider built at boot; a
                    // failed attempt leaves its cursor at the end. Nothing was hosted
                    // (booted is false), so start the draw over.
                    provider.rewind();
                    continuum_base::probe!(
                                class = "persona.host.provider_rewound",
                                attempt,
                                population = provider.identities_available(),
                                "hosting retry: the identity draw starts over — nothing was hosted last time"
                            );
                }
                let summary = supervisor
                    .spawn_all(&mut provider, tool_executor.clone())
                    .await;
                if summary.hosted > 0 {
                    tracing::info!(
                        hosted = summary.hosted,
                        failed = summary.failed(),
                        attempts = attempt,
                        "🌐 Substrate boot composition complete (slice 13.5) — \
                                 citizen(s) hosted; supervisor stays resident as the \
                                 hosting reconciler (#429)"
                    );
                    booted = true;
                } else {
                    tracing::warn!(
                        failed = summary.failed(),
                        attempt,
                        "persona spawn found a decode-ready serving lane but no citizen \
                                 materialized — will retry on the next serving-plan edge"
                    );
                }
            } else {
                // THE ROSTER FOLLOWS THE LANES UP (2026-09-06): the boot plan is
                // often the small under-memory one, so the first composition
                // seats one or two; when the pin's plan opens more lanes, draw
                // the missing seats from the provider on this edge. `spawn_all`
                // draws only `seats - hosted` identities, so live citizens are
                // never re-bootstrapped and a full roster is a no-op.
                let grown = supervisor
                    .spawn_all(&mut provider, tool_executor.clone())
                    .await;
                if grown.hosted > 0 {
                    tracing::info!(
                                hosted = grown.hosted,
                                failed = grown.failed(),
                                "🌐 hosting reconciler: the plan opened more seats — {} more citizen(s) hosted",
                                grown.hosted
                            );
                }
                // Post-boot reconcile (#429): a `persona/spawn` birth ends at
                // `registry.register`; the hosting half (adapter + cognition
                // loop) is ours. Each serving-plan edge, host any registered
                // citizen with no attached service loop.
                let summary = supervisor
                    .host_unattended(tool_executor.clone())
                    .await;
                if summary.hosted > 0 {
                    tracing::info!(
                        hosted = summary.hosted,
                        failed = summary.failed(),
                        "🌐 hosting reconciler: {} newborn citizen(s) hosted \
                                 (persona/spawn → live mind)",
                        summary.hosted
                    );
                }
                for failure in &summary.failures {
                    tracing::warn!(
                        persona_id = ?failure.persona_id,
                        reason = %failure.reason,
                        "hosting reconciler: slot failed — retried after its backoff \
                         (persona.host.slot_backoff names the wait)"
                    );
                }
            }
        }
        // Park until the serving daemon republishes (every tick, or
        // sooner on a pressure edge) — or the grid allocation does. The plan
        // watch's `changed()` errs only if the daemon is gone — then there is
        // nothing left to react to.
        let plan_edge = tokio::select! {
            changed = serving_plan_rx.changed() => Some(changed),
            changed = alloc_rx.changed(), if !alloc_closed => {
                if changed.is_err() {
                    alloc_closed = true; // never in practice: the channel is process-global
                }
                None
            }
        };
        if matches!(plan_edge, Some(Err(_))) {
            if booted {
                tracing::warn!(
                    "serving-daemon watch closed — hosting reconciler exiting; \
                             citizens already hosted stay live, but later persona/spawn \
                             births will not be hosted this run (#429)"
                );
            } else {
                tracing::warn!(
                    "serving-daemon watch closed before any persona materialized \
                             — no citizens online this run"
                );
            }
            break;
        }
    }
}

// FIRST-BOOT MODEL ACQUISITION (2026-08-24, Joel: "this repo isn't for
// ME — a new repo user without an agent and a totally different
// machine"). A fresh clone has ZERO local GGUFs: the planner finds no
// viable candidate, citizens boot mute, and only a log line an agent
// would read names the fix. Boot now owns its engine like it owns its
// transport: when NO catalog model resolves a local gguf, pull the
// modest first-boot default (our own forged 4B coder, ~small download,
// tool-trained) in the background — the serving reconcile adopts it on
// its own edge when the pull lands. Escape hatch:
// CONTINUUM_NO_AUTO_PULL=1 restores the old fail-loud-only behavior.
fn pull_first_boot_model_if_no_local_gguf(tool_executor: &Arc<crate::runtime::CommandExecutor>) {
    let none_local = crate::model_registry::catalog::models()
        .iter()
        .all(|m| crate::model_registry::artifacts::resolve_gguf_for_model(m).is_none());
    let opted_out = continuum_base::config_env::read("CONTINUUM_NO_AUTO_PULL").is_some_and(|v| v == "1");
    if none_local && !opted_out {
        const FIRST_BOOT_MODEL: &str = "continuum-ai/qwen3.5-4b-code-forged-GGUF";
        continuum_base::probe!(
                    class = "boot.first_model_pull",
                    model = FIRST_BOOT_MODEL,
                    "fresh install: no local GGUF anywhere — boot pulls the first-boot                      default so citizens are never mute on a stranger's machine"
                );
        tracing::info!(
                    model = FIRST_BOOT_MODEL,
                    "🧲 first boot: no local models — downloading the default engine                      (serving adopts it automatically when the pull completes;                      CONTINUUM_NO_AUTO_PULL=1 to opt out)"
                );
        let exec = tool_executor.clone();
        tokio::spawn(async move {
            let outcome = exec
                .execute(
                    "models/pull",
                    serde_json::json!({ "model_id": FIRST_BOOT_MODEL }),
                )
                .await;
            match outcome {
                Ok(_) => tracing::info!(
                    model = FIRST_BOOT_MODEL,
                    "🧲 first-boot model pull COMPLETE — serving reconcile will adopt it"
                ),
                Err(e) => tracing::error!(
                    model = FIRST_BOOT_MODEL,
                    error = %e,
                    "first-boot model pull FAILED — citizens stay mute; manual fix:                              continuum models/pull --model_id {FIRST_BOOT_MODEL}"
                ),
            }
        });
    }
}
