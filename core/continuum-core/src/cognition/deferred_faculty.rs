//! The deferred lane — taking a SLOW faculty off the cognition hot path.
//!
//! **The cbar lesson (slice 2 of the decoupling work; slice 1 = the `CycleId`
//! stamp in `workspace.rs`).** A deep analyzer — segmentation, a full-LLM
//! faculty — is too slow to run inside the per-tick loop. cbar never let one
//! block the 45fps pipeline: it ran on its own thread, and the loop READ its
//! last-good result (reprojected forward via feature-history). This is the
//! cognition analog, at the faculty layer.
//!
//! A [`DeferredFaculty`] wraps an inner [`Faculty`] so that the inner's
//! expensive work runs on its **own** `tokio` task, while the hot-path
//! `contribute()` is non-blocking:
//!
//! 1. `contribute()` publishes the current world at the background task through a
//!    `watch` channel (always-latest, never blocks, never drops the newest), and
//! 2. returns the inner faculty's **most recent finding** from another `watch`
//!    snapshot — already stamped with the (possibly older) [`CycleId`] it was
//!    computed against — or `None` until the first compute lands.
//!
//! So a slow faculty sits in the SAME `faculties` Vec as the fast ones and the
//! per-tick `join_all` barrier in [`WorkspaceCycle::run_in_room`] never waits on
//! it: the barrier stays, but nothing slow is ON it. The late finding lands a
//! tick or three later carrying its own cycle, and the hot path **reprojects it
//! forward** ([`reproject_to_now`], slice 3) — re-anchoring a turn-N finding onto
//! turn-N+3's world via current-relevance, the cbar reprojection. This is the
//! cheap synchronous "bring it up to speed" step that lets a 90%-async concern
//! behave as if it were synchronous: the expensive compute ran off-loop; the only
//! on-loop cost is the warp.
//!
//! This is the "scary-fast reflexes in a slow brain": the immediate lane answers
//! every tick; the slow lane lands late, honestly stamped, and is merged in
//! rather than blocking the spoken turn.
//!
//! Conforms to `docs/architecture/CONCURRENCY-STYLE-GUIDE.md`: own `tokio::spawn`
//! task, `catch_unwind` around the loop body, `watch` for state in BOTH
//! directions (no `Arc<Mutex>` across `await`), event-triggered (not a
//! sleep-loop), and a quarantine after repeated inner panics.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::FutureExt;
use tokio::sync::watch;
use uuid::Uuid;

use crate::cognition::workspace::{Contribution, CycleId, Faculty, FacultyId, Workspace};

/// The slice of world-state the background task recomputes against. Carried
/// through a `watch` so the hot path always overwrites with the latest (no
/// backlog, no dropped-newest) and the worker chews the freshest burst it can.
#[derive(Debug, Clone)]
struct DeferredInput {
    world_state: String,
    room_id: Uuid,
    /// The cycle this input belongs to — the worker stamps its finding with it,
    /// so a finding always carries the moment it reasoned about, not the moment
    /// it finished.
    cycle: CycleId,
}

impl DeferredInput {
    /// The initial `watch` value: a sentinel the worker skips (UNSTAMPED cycle =
    /// "no real burst yet"). `watch::Receiver::changed()` doesn't fire on the
    /// initial value, so this is never actually computed against — it just gives
    /// the channel something to hold before the first real tick.
    fn sentinel() -> Self {
        Self {
            world_state: String::new(),
            room_id: Uuid::nil(),
            cycle: CycleId::UNSTAMPED,
        }
    }
}

/// A finding the worker has computed, tagged with the room it reasoned about so
/// the hot path can refuse to serve it into a DIFFERENT room. This is the
/// "assuming it didn't context-switch to where it's irrelevant" guard: a recall
/// computed against room A's burst must not be injected into room B's turn just
/// because it happens to be the last thing the worker finished.
#[derive(Debug, Clone)]
struct StampedFinding {
    /// The room the inner faculty reasoned about.
    room_id: Uuid,
    /// The finding itself (already carrying the cycle it was computed against).
    contribution: Contribution,
}

/// A faculty whose inner work is taken off the cognition hot path (see module
/// docs). Drops the background task when dropped.
pub struct DeferredFaculty {
    id: FacultyId,
    /// Hot path reads the inner faculty's last-good finding here, lock-free.
    latest: watch::Receiver<Option<StampedFinding>>,
    /// Hot path pushes the current world here for the worker to recompute against.
    world_tx: watch::Sender<DeferredInput>,
    /// The wrapped faculty, retained for the DIRECTED-turn bypass: an addressed
    /// question runs the inner faculty SYNCHRONOUSLY on ITS OWN burst (the
    /// orienting response — acute stimulus interrupts), because the deferred
    /// lane structurally serves the PREVIOUS turn's finding, so an acute
    /// question could never benefit from its own recall (glass-boxed live
    /// 2026-07-10: the fact surfaced at z=5.5σ one turn too late; the rendered
    /// prompt had no [recall]; she confabulated "port 3001" and her peer
    /// absorbed it as shared truth). Ambient turns keep the cheap lane.
    inner: Arc<dyn Faculty>,
    /// The cold-start self-warm's publish handle: the FIRST tick's synchronous
    /// finding lands in `latest` through this, so subsequent ambient ticks
    /// serve it as last-good instead of re-paying the synchronous cost until
    /// the background worker's first publish.
    warm_tx: watch::Sender<Option<StampedFinding>>,
    /// Owns the worker task so it's aborted when this faculty is dropped — no
    /// orphaned compute outliving the mind it served.
    _worker: DropGuard,
}

/// Aborts the held task on drop. Keeps the worker's lifetime tied to the faculty.
struct DropGuard(tokio::task::JoinHandle<()>);
impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// After this many consecutive inner panics the worker stops recomputing and the
/// faculty just serves its last-good (or `None`) — a bad backend degrades the
/// lane to stale, it never crashes the mind. Matches the guide's quarantine
/// discipline (the monitor's 3-strike rule).
const QUARANTINE_AFTER: u32 = 3;

impl DeferredFaculty {
    /// Wrap `inner` and spawn its background worker. The inner faculty must be a
    /// **perception-tier** faculty (`reacts_to_broadcast() == false`): the worker
    /// reconstructs a minimal [`Workspace`] from the raw world-state, with an
    /// empty broadcast, so a deliberation-tier inner (which reads the assembled
    /// broadcast) would run blind. We force `reacts_to_broadcast() == false` on
    /// the wrapper to keep it in the perception phase regardless.
    pub fn spawn(inner: Arc<dyn Faculty>) -> Self {
        let id = inner.id();
        let (world_tx, mut world_rx) = watch::channel(DeferredInput::sentinel());
        let (latest_tx, latest) = watch::channel::<Option<StampedFinding>>(None);
        let warm_tx = latest_tx.clone();

        let inner_for_hot_path = Arc::clone(&inner);
        let worker = tokio::spawn(async move {
            let mut consecutive_panics: u32 = 0;
            // Event-triggered, not a sleep-loop: wake only when the hot path
            // publishes a new world. `changed()` skips the initial sentinel.
            while world_rx.changed().await.is_ok() {
                if consecutive_panics >= QUARANTINE_AFTER {
                    // Quarantined: stop recomputing, keep the last-good. Draining
                    // the receiver here would spin; just stop reacting.
                    break;
                }
                // Take the freshest burst (watch already coalesced to latest).
                let input = world_rx.borrow_and_update().clone();
                if input.cycle == CycleId::UNSTAMPED {
                    continue; // sentinel / not a real burst
                }
                let room_id = input.room_id;
                let ws = Workspace::in_room(input.world_state, room_id).with_cycle(input.cycle);

                // The inner faculty's contribute is async (real inference/IPC).
                // Catch a panic so a flawed backend degrades the lane to stale,
                // never kills the runtime (guide: catch_unwind around the body).
                let outcome = AssertUnwindSafeFut(inner.contribute(&ws))
                    .catch_unwind()
                    .await;

                match outcome {
                    Ok(finding) => {
                        consecutive_panics = 0;
                        // Stamp with the cycle it reasoned against AND the room it
                        // reasoned about — the late finding carries its own moment
                        // (cycle) and its own place (room), so the hot path can
                        // refuse to serve it into a different context.
                        let stamped = finding.map(|mut c| {
                            c.cycle = input.cycle;
                            StampedFinding {
                                room_id,
                                contribution: c,
                            }
                        });
                        // Ignore send error: no receivers left = faculty dropped.
                        let _ = latest_tx.send(stamped);
                    }
                    Err(_) => {
                        consecutive_panics += 1;
                    }
                }
            }
        });

        Self {
            id,
            warm_tx,
            latest,
            world_tx,
            inner: inner_for_hot_path,
            _worker: DropGuard(worker),
        }
    }
}

#[async_trait]
impl Faculty for DeferredFaculty {
    fn id(&self) -> FacultyId {
        self.id.clone()
    }

    /// AMBIENT turns: non-blocking — kick the worker with the current world and
    /// return the last-good finding, never awaiting the inner faculty (the whole
    /// point of the lane). DIRECTED turns: the ORIENTING RESPONSE — run the inner
    /// faculty synchronously on THIS burst, because the deferred lane
    /// structurally serves the PREVIOUS turn's finding and an addressed question
    /// must benefit from its OWN recall (the eval fork already runs perception
    /// synchronously for the same reason; this brings the live directed path to
    /// parity). Cost: one fresh perception pass on turns that were already going
    /// to run full deliberation — negligible next to decode.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        if ws.directed_at_self {
            // Orienting response: fresh inner perception on this burst. Probe the
            // outcome — glass-boxed 2026-07-10: a directed silver-harbor question
            // 30s after boot produced NO recall bid and the seam was dark; whether
            // the inner ran-and-found-nothing vs never-ran was unattributable.
            let found = self.inner.contribute(ws).await;
            if let Some(c) = &found {
                // A directed run's fresh finding seeds last-good too — the next
                // ambient tick serves it non-blocking instead of a cold miss.
                let _ = self.warm_tx.send(Some(StampedFinding {
                    room_id: ws.room_id,
                    contribution: c.clone(),
                }));
            }
            crate::probe!(
                class = "deferred.serve",
                faculty = %self.id.as_str(),
                mode = "sync-directed",
                found = found.is_some(),
                "orienting response: inner ran synchronously"
            );
            return found;
        }
        // Publish the current world for the worker (always-latest, never blocks).
        // Ignore send error: worker gone = serve whatever last-good we have.
        let _ = self.world_tx.send(DeferredInput {
            world_state: ws.world_state.clone(),
            room_id: ws.room_id,
            cycle: ws.cycle,
        });
        // Context guard: serve the last-good ONLY if it was computed for the room
        // the mind is in NOW. A finding from another room is "context-switched to
        // where it's irrelevant" — withhold it rather than inject a cross-context
        // memory into this turn. A same-room-but-stale finding is REPROJECTED
        // forward (slice 3, below); a different-room finding is simply not ours.
        let cold = {
            let guard = self.latest.borrow();
            match guard.as_ref() {
                Some(found) if found.room_id == ws.room_id => {
                    let served = Some(reproject_to_now(found, ws));
                    crate::probe!(
                        class = "deferred.serve",
                        faculty = %self.id.as_str(),
                        mode = "last-good",
                        found = true,
                        "deferred lane served"
                    );
                    return served;
                }
                Some(_) => {
                    crate::probe!(
                        class = "deferred.serve",
                        faculty = %self.id.as_str(),
                        mode = "other-room-withheld",
                        found = false,
                        "deferred lane served"
                    );
                    return None;
                }
                None => true,
            }
        };
        debug_assert!(cold);
        // COLD START: no last-good exists yet (first tick after boot). Serving
        // None here made every persona's first turns BLIND — three observed
        // post-reboot greeting rounds on 2026-07-10: no roster, no kanban, no
        // doctrine in the first prompt, so they re-introduced themselves to a
        // room they'd lived in all day. Self-warm instead: run the inner
        // faculty synchronously ONCE (the same move the orienting response
        // makes for directed turns); the bg worker takes over from tick 2.
        // Boot pays one slow first tick; the room keeps its continuity.
        let found = self.inner.contribute(ws).await;
        if let Some(c) = &found {
            // Publish the warm finding as last-good so the NEXT ambient tick
            // serves it non-blocking — the cold cost is paid exactly once.
            let _ = self.warm_tx.send(Some(StampedFinding {
                room_id: ws.room_id,
                contribution: c.clone(),
            }));
        }
        crate::probe!(
            class = "deferred.serve",
            faculty = %self.id.as_str(),
            mode = "cold-start-sync",
            found = found.is_some(),
            "deferred lane self-warmed on first tick"
        );
        found
    }

    /// A deferred faculty is perception-tier (see [`DeferredFaculty::spawn`]): it
    /// reconstructs from raw world-state, so it must run in phase 1, never read
    /// the assembled broadcast.
    fn reacts_to_broadcast(&self) -> bool {
        false
    }
}

/// **Reconcile-forward (slice 3) — the cheap synchronous "bring it up to speed"
/// step that lets a 90%-async concern behave AS IF it were synchronous.**
///
/// The cbar lesson stated as a rule: almost nothing needs to run synchronously in
/// the hot loop; the only synchronous cost is warping a stale last-good finding to
/// approximately where it belongs in the NOW, using history. For geometry that
/// warp is a pose transform (`getWorldTransform(frameIndex)`); for RAG it is
/// **re-anchoring the finding against the current burst** — the text analog of
/// "warp to where it's now, and if it's no longer in view it falls out."
///
/// Why relevance and not blind age-decay: reproject semantics are faculty-
/// dependent. A *memory* doesn't become false with conversation age — only less
/// *relevant* if the topic moved on; a *world-model prediction* about an old state
/// genuinely goes stale. Re-anchoring against the current burst captures both with
/// one rule: a still-on-topic memory keeps its salience, an off-topic stale
/// finding decays toward zero. Age rides along only as audit metadata.
///
/// This NEVER hard-withholds a same-room finding — eviction stays the arbiter's
/// single job (a near-zero bid simply loses the capacity competition). The faculty
/// reports an honest, reprojected salience; it does not decide what survives.
///
/// v1 is the **algorithmic-first** reprojector: a cheap lexical overlap, faculty-
/// agnostic (a `DeferredFaculty` wraps ANY perception faculty, so it can't assume
/// an embedder). The ladder, same shape as `SalienceArbiter → LlmFocusArbiter`:
/// v2 = a `Faculty::reproject` hook so a faculty re-anchors with its OWN model
/// (recall via its cached neural embedder); endgame = a learned reprojector that
/// folds the full turn-history, not just the current burst. This is input-side
/// attention (re-weighting a faculty's own bid by honest current-relevance), never
/// output-puppeteering — it does not read the deliberator's generated words.
pub(crate) fn reproject_to_now(found: &StampedFinding, now: &Workspace) -> Contribution {
    let mut c = found.contribution.clone();

    // STANDING FRAMING IS NOT A MEMORY — DO NOT RE-ANCHOR IT.
    //
    // The rule above is stated for findings whose truth is topic-relative ("a
    // still-on-topic memory keeps its salience, an off-topic stale finding decays").
    // A `stable` contribution is the opposite kind of thing: session-stable structural
    // context (the work board, the room roster, the workspace map) whose entire
    // contract is to be present REGARDLESS of what this turn happens to be about.
    // `STANDING_FRAMING_SALIENCE` says so in as many words — "high enough that the
    // top-k arbiter never truncates it under attention pressure".
    //
    // Multiplying that floor by a lexical ratio silently repealed the contract, and
    // the ratio is length-biased against exactly the sources that need it most: the
    // denominator is the FINDING'S OWN token count, so the bigger a block is, the
    // lower its ceiling. Measured live 2026-08-07 — room-kanban (median offer 5,364
    // tokens) bid 0.9 x 0.133 = **0.12** and lost to recall at 0.77, so a citizen
    // holding a live, renewing card could not see the board she held it on. Roster
    // (small) survived at 0.62; workspace-map bid its full 0.90 for the sole reason
    // that it is not `defer_tolerant` and therefore never passed through here. Three
    // sources, three numbers, one cause.
    //
    // Deferrability is documented as ORTHOGONAL to salience policy
    // ([`crate::cognition::persona_workspace`]). This is what made it not so.
    // Ambient staleness still shows in the reasoning; the BID is left alone.
    if c.stable {
        let age = now.cycle.0.saturating_sub(c.cycle.0);
        c.reasoning = format!(
            "{} [reprojected: {age} cycles stale, standing framing — salience {:.2} held \
             (topic-independent by contract)]",
            c.reasoning, c.salience
        );
        return c;
    }

    let relevance = lexical_relevance(&c.content, &now.world_state);
    let age = now.cycle.0.saturating_sub(c.cycle.0);
    let original = c.salience;
    c.salience = (original * relevance).clamp(0.0, 1.0);
    // Keep the cycle stamp (it carries the moment it reasoned about) and append the
    // reprojection to the reasoning so the warp is observable in audit/replay.
    c.reasoning = format!(
        "{} [reprojected: {age} cycles stale, relevance {relevance:.2}, salience {original:.2}→{:.2}]",
        c.reasoning, c.salience
    );
    c
}

/// Cheap, faculty-agnostic current-relevance for the v1 reprojector: the fraction
/// of the finding's tokens that still appear in the current burst. Asymmetric on
/// purpose — "how much of this finding is still on-topic," not symmetric overlap.
///
/// Returns `1.0` (no decay) when either side has no tokens: an empty finding has
/// nothing to fade, and an empty/contentless current burst gives no signal to
/// reproject against, so we must not penalize the finding for our own blindness
/// (fail toward serving last-good, not toward silently dropping it).
fn lexical_relevance(finding: &str, current: &str) -> f32 {
    fn tokens(s: &str) -> std::collections::HashSet<String> {
        s.split(|ch: char| !ch.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect()
    }
    let f = tokens(finding);
    let c = tokens(current);
    if f.is_empty() || c.is_empty() {
        return 1.0;
    }
    let hits = f.iter().filter(|t| c.contains(*t)).count();
    hits as f32 / f.len() as f32
}

/// `Future` adapter that asserts unwind-safety so an async body can be
/// `catch_unwind`'d. The captured `&Workspace` and `Arc<dyn Faculty>` are only
/// read across the await, so this is sound for our use.
struct AssertUnwindSafeFut<F>(F);
impl<F: std::future::Future> std::future::Future for AssertUnwindSafeFut<F> {
    type Output = F::Output;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: structural pin projection of the single field; we never move it.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}
impl<F> std::panic::UnwindSafe for AssertUnwindSafeFut<F> {}
impl<F> std::panic::RefUnwindSafe for AssertUnwindSafeFut<F> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::Contribution;

    /// A deliberately slow perception faculty: it waits, then surfaces a finding.
    /// Models a full-LLM / deep-analyzer faculty whose latency would blow the
    /// per-tick budget if run inline.
    struct SlowRecall;
    #[async_trait]
    impl Faculty for SlowRecall {
        fn id(&self) -> FacultyId {
            FacultyId::Recall
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            Some(Contribution::context(
                FacultyId::Recall,
                "a slow, late recall finding",
                0.8,
                "deep analyzer, off the hot path",
            ))
        }
    }

    // what this catches: a slow faculty wrapped as Deferred never blocks the hot
    // path — contribute() returns immediately with the last-good (None until the
    // first compute lands), and when the late finding arrives it carries the
    // OLDER cycle it was computed against, not the current tick. This is the
    // immediate-vs-deferred split: the per-tick join_all barrier stays harmless
    // because nothing slow is ON it, and reproject (slice 3) folds the late
    // finding forward precisely because it knows its own moment.
    #[tokio::test]
    async fn deferred_faculty_never_blocks_and_lands_late_with_its_own_cycle() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));

        // Tick 1 (cycle 1): COLD START — the lane self-warms by running the
        // inner synchronously (the 2026-07-10 fix for post-boot blind turns:
        // three observed greeting rounds because first prompts carried no
        // grounding). The first tick pays the inner cost and returns the FRESH
        // finding; it also publishes it as last-good.
        let ws1 = Workspace::in_room("burst one", Uuid::nil()).with_cycle(CycleId(1));
        let r1 = deferred.contribute(&ws1).await;
        assert!(
            r1.is_some(),
            "cold start self-warms: the first tick carries grounding"
        );

        // Tick 2 immediately after (before the worker publishes anything): the
        // warm finding serves as last-good, NON-BLOCKING — the cold cost is
        // paid exactly once, and the steady-state hot path never waits on the
        // slow inner (it sleeps 40ms; we demand <15ms).
        let ws2 = Workspace::in_room("burst two", Uuid::nil()).with_cycle(CycleId(2));
        let t = tokio::time::Instant::now();
        let r2 = deferred.contribute(&ws2).await;
        assert!(
            t.elapsed() < std::time::Duration::from_millis(15),
            "steady-state hot path waited on the slow inner ({:?}) — the lane isn't deferred",
            t.elapsed()
        );
        assert!(r2.is_some(), "the warm finding serves as last-good");

        // Give the worker time to compute against cycle 1 and publish.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // A later tick (cycle 5): now the last-good is available, and it's stamped
        // with cycle 1 (when it was computed), NOT 5 (now).
        let ws5 = Workspace::in_room("burst five", Uuid::nil()).with_cycle(CycleId(5));
        let r5 = deferred.contribute(&ws5).await;
        let late = r5.expect("the slow finding has landed by now");
        assert!(
            late.cycle.0 < 5 && late.cycle != CycleId::UNSTAMPED,
            "the late finding must carry the (older) cycle it reasoned against,              not the current tick; got {:?}",
            late.cycle
        );
        assert_eq!(late.faculty, FacultyId::Recall);
    }

    // what this catches: the ORIENTING RESPONSE — a DIRECTED turn (addressed
    // question) runs the inner faculty synchronously on ITS OWN burst instead of
    // serving the previous turn's last-good. Without this, an acute question can
    // never benefit from its own recall (glass-boxed live 2026-07-10: the taught
    // fact surfaced at z=5.5σ one turn too late, the prompt had no [recall], and
    // the persona confabulated "port 3001" — which her peer then absorbed as
    // shared truth). Ambient turns keep the non-blocking deferred lane.
    #[tokio::test]
    async fn directed_turn_gets_fresh_perception_not_last_good() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));

        // Directed turn, cycle 1, NOTHING computed yet: the ambient lane would
        // return None — the orienting response must instead await the inner
        // faculty and return the FRESH finding for THIS burst.
        let ws = Workspace::in_room("which port is the staging gateway on?", Uuid::nil())
            .with_cycle(CycleId(1))
            .directed(true);
        let r = deferred.contribute(&ws).await;
        let fresh = r.expect("a directed turn must get fresh perception, not stale None");
        assert_eq!(fresh.faculty, FacultyId::Recall);
        assert!(
            fresh.content.contains("slow, late recall finding"),
            "the finding must be the inner faculty's own output"
        );

        // The same tick UNDIRECTED still behaves as the deferred lane (fast —
        // the directed run warm-published, so this serves last-good) — ambience
        // never pays the synchronous cost.
        let ws_ambient = Workspace::in_room("idle chatter", Uuid::nil()).with_cycle(CycleId(2));
        let t = tokio::time::Instant::now();
        let _ = deferred.contribute(&ws_ambient).await;
        assert!(
            t.elapsed() < std::time::Duration::from_millis(15),
            "ambient turns must stay non-blocking; waited {:?}",
            t.elapsed()
        );
    }

    // what this catches: the context guard — a finding computed against room A
    // must NOT be served into room B's turn. This is Joel's "assuming it didn't
    // context-switch to something where it was irrelevant": a deferred recall
    // that lands a minute late is only useful if the mind is still in the room it
    // was reasoned for; in a different room it's withheld, never injected as a
    // cross-context memory. (Same-room-but-stale is slice-3 reproject; this test
    // covers different-room = simply not ours to serve.)
    #[tokio::test]
    async fn deferred_finding_is_withheld_when_the_mind_changed_rooms() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));
        let room_a = Uuid::new_v4();
        let room_b = Uuid::new_v4();

        // Tick against room A — kicks the worker to compute for room A.
        let ws_a = Workspace::in_room("burst in A", room_a).with_cycle(CycleId(1));
        let _ = deferred.contribute(&ws_a).await;

        // Let the worker finish computing against room A.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // Now the mind is in room B. The room-A finding must be withheld.
        let ws_b = Workspace::in_room("burst in B", room_b).with_cycle(CycleId(2));
        let in_b = deferred.contribute(&ws_b).await;
        assert!(
            in_b.is_none(),
            "a room-A finding must not be injected into room B's turn"
        );

        // Back in room A: the finding is relevant again and IS served.
        let ws_a2 = Workspace::in_room("back in A", room_a).with_cycle(CycleId(3));
        let in_a = deferred.contribute(&ws_a2).await;
        let found = in_a.expect("the room-A finding is ours to serve back in room A");
        assert_eq!(
            found.cycle,
            CycleId(1),
            "still stamped with its original cycle"
        );
    }

    // what this catches: reproject-to-now (slice 3) — the cheap synchronous "bring
    // it up to speed" warp that lets a 90%-async concern behave as if synchronous.
    // A same-room stale finding is re-anchored against the CURRENT burst: when the
    // burst is still on-topic the finding keeps its salience; when the topic has
    // moved on the same finding decays toward zero — WITHOUT being withheld (the
    // cycle stamp survives; eviction stays the arbiter's job, a near-zero bid just
    // loses the capacity competition). Faculty-dependence is handled by one rule:
    // a memory only fades when it stops being RELEVANT, never merely because it
    // aged. If this regresses to serve-verbatim, async-by-default serves stale
    // garbage (the skeptic's win) instead of behaving synchronous.
    #[tokio::test]
    async fn reproject_reanchors_stale_finding_against_the_current_burst() {
        // SlowRecall surfaces "a slow, late recall finding" at salience 0.8.
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));
        let room = Uuid::new_v4();

        // Kick the worker and let it land its finding (stamped cycle 1).
        let ws1 = Workspace::in_room("kickoff", room).with_cycle(CycleId(1));
        let _ = deferred.contribute(&ws1).await;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // On-topic burst (shares "slow late recall finding") → high relevance, the
        // finding keeps most of its salience and is served, cycle preserved.
        let on_topic = Workspace::in_room("the slow late recall finding is relevant", room)
            .with_cycle(CycleId(4));
        let kept = deferred
            .contribute(&on_topic)
            .await
            .expect("same-room finding is served");
        assert_eq!(
            kept.cycle,
            CycleId(1),
            "reproject preserves the original cycle stamp"
        );
        assert!(
            kept.salience > 0.4,
            "on-topic reproject keeps salience high, got {}",
            kept.salience
        );

        // Off-topic burst (no shared tokens) → relevance ~0, the SAME finding
        // decays toward zero but is still served (not withheld) — the arbiter, not
        // the faculty, decides it loses the competition.
        let off_topic =
            Workspace::in_room("tomorrow's weather forecast outlook", room).with_cycle(CycleId(5));
        let faded = deferred
            .contribute(&off_topic)
            .await
            .expect("a decayed finding is still served, not withheld");
        assert_eq!(faded.cycle, CycleId(1), "still its own moment");
        assert!(
            faded.salience < 0.05,
            "off-topic reproject decays salience toward zero, got {}",
            faded.salience
        );
        assert!(
            kept.salience > faded.salience,
            "on-topic must out-bid off-topic for the same stale finding"
        );
    }
}
