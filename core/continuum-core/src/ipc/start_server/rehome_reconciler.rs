//! Boot phase 11d: the served-model re-home reconciler.

use super::*;

// ── Served-model re-home reconciler ─────────────────────────────────────
// The base-model sibling of LoRA/genome paging. The serving snapshot is the ONE
// owner of "the binding a local persona deliberates through": its active model
// AND its served context window. This task owns the serving-SNAPSHOT watch and,
// on every ready snapshot, binds every local resident to exactly that pair —
// WITHOUT losing the genome, working memory, or admission she accumulated. That
// covers a `serving/pin`, a re-plan under memory pressure, a grid failover, AND a
// resident materialized before serving was ready: she was bound to her
// provisional window (2048), and the first ready snapshot corrects it (the M5,
// 2026-10-29: all four residents sat at ctx_len=2048 for the whole deploy because
// this task adopted the first ready model as a baseline and keyed only on the
// model name, so working memory discarded every result body past ~4k chars).
//
// Distinct from the boot spawn loop above (which reacts to the serving PLAN to
// FIRST-host personas): this reacts to the serving SNAPSHOT to re-home
// ALREADY-hosted personas. Both are the canonical concurrent-concern shape —
// own task, `watch` receiver, no lock across await, log-and-continue, exit on
// watch close. The comparison is per persona inside the workspace owner, so a
// steady-state tick touches nobody; a shared adapter is built only when some
// resident needs the new MODEL (one HTTP init, shared by every lane); a
// window-only correction keeps each resident's adapter.
// See [[seamless-persona-failover-model-and-genome]].
pub(super) fn spawn_rehome_reconciler(
    rt_handle: &tokio::runtime::Handle,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
) {
    let mut serving_rx = serving_daemon.subscribe_serving();
    rt_handle.spawn(async move {
        // LEVEL-TRIGGERED retry (#368): edge-only waking is how 4 citizens
        // stayed stranded on a torn-down model for 47 minutes — the adapter
        // build failed ONCE during a wedge window and the snapshot never
        // republished. Each tick re-reads the CURRENT snapshot; a resident
        // already bound to it costs nothing.
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
            if !snap.ready || snap.served_context_window == 0 {
                continue; // a ready snapshot always carries its window; anything else is not yet served.
            }
            let Some(active) = snap.active_model.clone() else {
                continue;
            };
            let window = snap.served_context_window;
            let workspaces = crate::cognition::persona_workspace::global();
            let Some(reason) = workspaces.rehome_reason(&active, window) else {
                continue; // every local resident is already bound to (model, window).
            };
            // A new model needs the ONE shared served-model adapter. Fail LOUD into
            // a log — a failed build leaves residents on their prior (still-live)
            // binding rather than a silent wrong-brain, and the next tick retries
            // ([[fallbacks-are-illegal-fail-loud]]).
            let adapter = match reason {
                crate::cognition::persona_workspace::RehomeReason::WindowChanged => None,
                crate::cognition::persona_workspace::RehomeReason::ModelChanged => {
                    match crate::persona::supervisor::build_served_adapter(&snap).await {
                        Ok(a) => Some(a),
                        Err(e) => {
                            tracing::warn!(
                                model = %active,
                                error = %e,
                                "served-model re-home: adapter build failed — personas stay \
                                 on their prior binding; the next tick retries"
                            );
                            continue;
                        }
                    }
                }
            };
            let n = workspaces.re_home_all(adapter, &active, window);
            tracing::info!(
                model = %active,
                context_window = window,
                personas = n,
                ?reason,
                "re-homed local residents onto the served binding"
            );
        }
    });
}
