//! Boot phase 11d: the served-model re-home reconciler.

use super::*;

// ── Served-model re-home reconciler ─────────────────────────────────────
// The base-model sibling of LoRA/genome paging: when the serving daemon
// publishes a NEW active model — a `serving/pin`, a re-plan under memory
// pressure, or a grid failover — every ALREADY-hosted persona's deliberation
// must rebind to it (new shared adapter + new served context window) WITHOUT
// losing the genome, working memory, or admission it accumulated. This task
// owns the serving-SNAPSHOT watch and drives `re_home_all` on an ACTUAL model
// change — the seam that turns `serving/pin <model>` into a no-reboot,
// coherent model-sweep lever AND delivers portable-self live re-home.
//
// Distinct from the boot spawn loop above (which reacts to the serving PLAN to
// FIRST-host personas): this reacts to the serving SNAPSHOT to re-home
// ALREADY-hosted personas. Both are the canonical concurrent-concern shape —
// own task, `watch` receiver, no lock across await, log-and-continue, exit on
// watch close. The swap itself is wait-free (`ArcSwap` store under the cycles
// lock); the only cost per model edge is ONE shared adapter HTTP-init, reused
// by every persona lane. See [[seamless-persona-failover-model-and-genome]].
pub(super) fn spawn_rehome_reconciler(
    rt_handle: &tokio::runtime::Handle,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
) {
    let mut serving_rx = serving_daemon.subscribe_serving();
    rt_handle.spawn(async move {
        // The model live personas are currently bound to. `None` until the
        // first ready snapshot: boot binds personas via the upstart factory,
        // so the FIRST ready model is NOT a re-home (they spawn already bound
        // to it). We adopt it as the baseline without re-homing, and re-home
        // only on a SUBSEQUENT change — avoiding a wasteful boot-time adapter
        // rebuild + redundant swap.
        let mut bound: Option<String> = None;
        // LEVEL-TRIGGERED retry (#368): edge-only waking is how 4 citizens
        // stayed stranded on a torn-down model for 47 minutes — the adapter
        // build failed ONCE during a wedge window, the "retry on the next
        // snapshot edge" never came (the snapshot had already settled and
        // never republished), and `bound != active` sat unreconciled forever.
        // The interval turns that residual mismatch into a retried one: each
        // tick re-reads the CURRENT snapshot, and the `bound == active` fast
        // path below makes the steady-state tick free.
        let mut retry = tokio::time::interval(std::time::Duration::from_secs(30));
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            // Wake on a snapshot edge OR the retry tick — never park solely
            // on an edge that may already have passed.
            tokio::select! {
                changed = serving_rx.changed() => {
                    if changed.is_err() {
                        tracing::info!(
                            "serving-snapshot watch closed — served-model re-home \
                             reconciler exiting (substrate shutdown)"
                        );
                        break;
                    }
                }
                _ = retry.tick() => {}
            }
            let snap = serving_rx.borrow_and_update().clone();
            if !snap.ready {
                continue;
            }
            let Some(active) = snap.active_model.clone() else {
                continue;
            };
            if bound.as_deref() == Some(active.as_str()) {
                continue; // same model already bound — nothing to re-home.
            }
            if bound.is_none() {
                // The boot upstart already bound (or will bind) personas to
                // this first served model; adopt it as the baseline.
                bound = Some(active);
                continue;
            }
            // Build the ONE shared served-model adapter for this edge (HTTP
            // init once, shared by every persona lane). Fail LOUD into a log —
            // a failed re-home leaves personas on their prior (still-live)
            // binding rather than a silent wrong-brain, and retries on the
            // next snapshot edge ([[fallbacks-are-illegal-fail-loud]]).
            let adapter = match crate::persona::supervisor::build_served_adapter(&snap).await {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(
                        model = %active,
                        error = %e,
                        "served-model re-home: adapter build failed — personas stay \
                         on their prior binding; will retry on the next snapshot edge"
                    );
                    continue;
                }
            };
            let n = crate::cognition::persona_workspace::global().re_home_all(
                adapter,
                Some(active.clone()),
                snap.served_context_window,
            );
            continuum_base::probe!(
                class = "persona.rehome",
                model = %active,
                context_window = snap.served_context_window,
                personas = n,
                "served model changed — re-homed live personas onto the new binding"
            );
            bound = Some(active);
        }
    });
}
