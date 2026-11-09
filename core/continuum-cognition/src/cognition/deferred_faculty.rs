//! The deferred lane — taking a SLOW faculty off the cognition hot path.
//!
//! **The cbar lesson (slice 2 of the decoupling work; slice 1 = the `CycleId`
//! stamp in `workspace.rs`).** A deep analyzer — segmentation, a full-LLM
//! faculty — is too slow to run inside the per-tick loop. cbar never let one
//! block the 45fps pipeline: it ran on its own thread, and the loop READ its
//! last-good result (reprojected forward via feature-history). This is the
//! cognition analog, at the faculty layer.
//!
//! A [`DeferredFaculty`] wraps an inner [`Faculty`] as a stage on the kernel's stage runner
//! (`continuum_kernel::runtime::stage`, the M0 stage contract): the inner's expensive work runs
//! on the stage's **own** task, while the hot-path `contribute()` is non-blocking:
//!
//! 1. `contribute()` submits the current world to the stage (latest wins, never blocks,
//!    never drops the newest), and
//! 2. returns the stage's **most recent published finding** — stamped with the (possibly
//!    older) [`CycleId`] it was computed against, published under the stage contract's rule
//!    that an output from older input never replaces a newer one — or `None` until the first
//!    run lands.
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
//! The concurrency discipline (own task, `catch_unwind` around each run, `watch` in both
//! directions, event-triggered, quarantine after repeated panics) is the stage runner's, once,
//! for every stage.

use std::sync::Arc;

use async_trait::async_trait;
use continuum_kernel::runtime::stage::{
    Cost, ExecutionCost, InputRevision, MindId, Provenance, Ran, ResidencyCost, RunOutcome, Stage,
    StageContext, StageId, StageIncarnation, StageRunner, TopicName, Triggers,
};
use uuid::Uuid;

use crate::cognition::workspace::{Contribution, CycleId, Faculty, FacultyId, Workspace};

/// The topic a deferred faculty reads: the world as of a cycle (a replaceable snapshot; only the
/// newest matters, and its revision is the cycle).
const WORLD: TopicName = TopicName("cognition.world");

/// A finding tagged with both wake and resolved activity rooms so
/// the hot path can refuse to serve it into a DIFFERENT room. This is the
/// "assuming it didn't context-switch to where it's irrelevant" guard: a recall
/// computed against room A's burst must not be injected into room B's turn just
/// because it happens to be the last thing the worker finished.
#[derive(Debug, Clone)]
pub(crate) struct StampedFinding {
    /// Wake provenance and resolved subject scope both identify this finding.
    room_id: Uuid,
    activity_room_id: Uuid,
    /// The finding itself (already carrying the cycle it was computed against).
    contribution: Contribution,
}

/// A perception faculty as a stage: its input is the world as of a cycle, its output the finding
/// stamped with the cycle and rooms it reasoned about.
struct FacultyStage {
    inner: Arc<dyn Faculty>,
    id: StageId,
}

#[async_trait]
impl Stage for FacultyStage {
    type Input = Workspace;
    type Output = Option<StampedFinding>;

    fn id(&self) -> StageId {
        self.id
    }

    fn triggers(&self) -> Triggers {
        Triggers {
            inputs: vec![WORLD],
            ..Triggers::default()
        }
    }

    fn cost(&self, _revisions: &[InputRevision]) -> Cost {
        Cost {
            execution: ExecutionCost::Cpu { measured: None },
            residency: ResidencyCost::default(),
        }
    }

    async fn run(
        &self,
        ctx: &StageContext,
        ws: &Workspace,
        revisions: &[InputRevision],
    ) -> RunOutcome<Option<StampedFinding>> {
        let finding = self.inner.contribute(ws).await.map(|mut c| {
            // Stamp with the cycle it reasoned against AND the room it reasoned about: the late
            // finding carries its own moment and its own place, so the hot path can refuse to
            // serve it into a different context.
            c.cycle = ws.cycle;
            StampedFinding {
                room_id: ws.room_id,
                activity_room_id: ws.activity_room_id,
                contribution: c,
            }
        });
        RunOutcome::Produced(vec![(
            finding,
            Provenance {
                by: ctx.run,
                from: revisions.to_vec(),
            },
        )])
    }
}

/// The input revision a workspace is: the world as of its cycle.
fn world_revision(ws: &Workspace) -> Vec<InputRevision> {
    vec![InputRevision {
        topic: WORLD,
        through: ws.cycle.0,
    }]
}

/// A faculty whose inner work is taken off the cognition hot path (see module docs): the inner
/// faculty runs as a stage on the kernel's stage runner (own task, latest-wins input, the
/// publication rule, panic quarantine), and the hot path reads its last published finding.
pub struct DeferredFaculty {
    id: FacultyId,
    runner: StageRunner<FacultyStage>,
}

impl DeferredFaculty {
    /// Wrap `inner` and start its stage. The inner faculty must be a **perception-tier**
    /// faculty (`reacts_to_broadcast() == false`): background runs see a minimal [`Workspace`]
    /// rebuilt from the raw world-state, with an empty broadcast, so a deliberation-tier inner
    /// (which reads the assembled broadcast) would run blind. The wrapper reports
    /// `reacts_to_broadcast() == false` to keep it in the perception phase regardless.
    pub fn spawn(inner: Arc<dyn Faculty>) -> Self {
        let id = inner.id();
        let stage = FacultyStage {
            id: StageId::named(&format!("deferred.{}", id.as_str())),
            inner,
        };
        let incarnation = StageIncarnation {
            stage: stage.id,
            mind: MindId(Uuid::nil()),
            epoch: 0,
        };
        Self {
            id,
            runner: StageRunner::spawn(Arc::new(stage), incarnation),
        }
    }

    /// Run the inner faculty on this workspace now, under the stage's publication rule.
    async fn run_now(&self, ws: &Workspace) -> Option<Contribution> {
        match self.runner.run_inline(ws, &world_revision(ws)).await {
            Ran::Produced { accepted, .. } => accepted
                .into_iter()
                .flatten()
                .next()
                .map(|found| found.contribution),
            _ => None,
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
    /// point of the lane). Priority turns (addressed or human opportunity): run the inner
    /// faculty synchronously on THIS burst, because the deferred lane
    /// structurally serves the PREVIOUS turn's finding and an addressed question
    /// must benefit from its OWN recall (the eval fork already runs perception
    /// synchronously for the same reason; this brings the live directed path to
    /// parity). Cost: one fresh perception pass on turns that were already going
    /// to run full deliberation — negligible next to decode.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        if ws.attention.requires_priority() {
            // Orienting response: fresh inner perception on this burst. Probe the
            // outcome — glass-boxed 2026-07-10: a directed silver-harbor question
            // 30s after boot produced NO recall bid and the seam was dark; whether
            // the inner ran-and-found-nothing vs never-ran was unattributable.
            // A directed run's fresh finding is published as last-good too: the next ambient
            // tick serves it non-blocking instead of a cold miss.
            let found = self.run_now(ws).await;
            continuum_base::probe!(
                class = "deferred.serve",
                faculty = %self.id.as_str(),
                mode = "sync-directed",
                found = found.is_some(),
                "orienting response: inner ran synchronously"
            );
            return found;
        }
        // Hand the current world to the stage's background run (latest wins, never blocks). A
        // real burst always carries a real room and a stamped cycle (#425); anything else is
        // skipped here, said, and never submitted: this lane degrades, it never panics.
        match crate::identity::ActivityRoom::from_uuid(ws.room_id) {
            Ok(room) if ws.cycle != CycleId::UNSTAMPED => {
                let mut minimal = Workspace::from_burst(
                    crate::cognition::workspace::Burst::raw_in(room, ws.world_state.clone()),
                )
                .with_cycle(ws.cycle);
                minimal.activity_room_id = ws.activity_room_id;
                self.runner.submit(minimal, world_revision(ws));
            }
            _ => continuum_base::probe!(
                class = "cognition.deferred.roomless_input_skipped",
                "deferred faculty received an unstamped or nil-room world — not submitted (#425)",
            ),
        }
        // Context guard: serve the last-good ONLY if it was computed for the room
        // the mind is in NOW, including its resolved activity. A different scope is
        // irrelevant — withhold it rather than inject a cross-context
        // memory into this turn. A same-room-but-stale finding is REPROJECTED
        // forward (slice 3, below); a different-room finding is simply not ours.
        let cold = {
            // The outer Option is "has a run published?"; the inner is "did it find anything?".
            // A published empty finding is WARM (Codex at d627b8af): it serves nothing, and never
            // re-pays the synchronous cold start on every tick after an empty recall.
            let published = self.runner.latest().map(|published| published.output);
            match published.as_ref().map(Option::as_ref) {
                Some(None) => {
                    continuum_base::probe!(
                        class = "deferred.serve",
                        faculty = %self.id.as_str(),
                        mode = "last-good-empty",
                        found = false,
                        "deferred lane served"
                    );
                    return None;
                }
                Some(Some(found))
                    if found.room_id == ws.room_id
                        && found.activity_room_id == ws.activity_room_id =>
                {
                    let served = Some(reproject_to_now(found, ws));
                    continuum_base::probe!(
                        class = "deferred.serve",
                        faculty = %self.id.as_str(),
                        mode = "last-good",
                        found = true,
                        "deferred lane served"
                    );
                    return served;
                }
                Some(Some(_)) => {
                    continuum_base::probe!(
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
        // The warm finding is published as last-good, so the NEXT ambient tick serves it
        // non-blocking: the cold cost is paid exactly once.
        let found = self.run_now(ws).await;
        continuum_base::probe!(
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
    pub fn tokens(s: &str) -> std::collections::HashSet<String> {
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
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let mut finding = Contribution::context(
                FacultyId::Recall,
                "a slow, late recall finding",
                0.8,
                "deep analyzer, off the hot path",
            );
            finding.cycle = ws.cycle;
            Some(finding)
        }
    }

    /// A perception faculty that finds nothing, counting how often it ran.
    struct EmptyRecall(std::sync::atomic::AtomicUsize);
    #[async_trait]
    impl Faculty for EmptyRecall {
        fn id(&self) -> FacultyId {
            FacultyId::Recall
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            // Counted at the END of a run that takes a while: an inline run has counted before
            // contribute() returns; a background run cannot have finished by then.
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            None
        }
    }

    // what this catches: a published EMPTY finding read as "never ran", so every ambient tick
    // after an empty recall re-pays the synchronous cold start (Codex at d627b8af). The first
    // tick runs inline once; later ambient ticks serve the warm empty without an inline run.
    #[tokio::test]
    async fn an_empty_recall_is_warm_and_never_repeats_the_cold_start() {
        let inner = Arc::new(EmptyRecall(std::sync::atomic::AtomicUsize::new(0)));
        let deferred = DeferredFaculty::spawn(Arc::clone(&inner) as Arc<dyn Faculty>);
        let room = Uuid::new_v4();
        assert!(deferred
            .contribute(&Workspace::in_room("first", room).with_cycle(CycleId(1)))
            .await
            .is_none());
        assert_eq!(
            inner.0.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "the cold start ran inline once"
        );
        let mut published = deferred.runner.subscribe();
        for cycle in 2..5 {
            let before = inner.0.load(std::sync::atomic::Ordering::Relaxed);
            assert!(deferred
                .contribute(&Workspace::in_room("again", room).with_cycle(CycleId(cycle)))
                .await
                .is_none());
            // Each ambient tick submits to the background run and returns at once: the inner
            // count rises only when the background run lands, never inside contribute().
            assert_eq!(
                inner.0.load(std::sync::atomic::Ordering::Relaxed),
                before,
                "no inline cold start on tick {cycle}"
            );
            tokio::time::timeout(std::time::Duration::from_secs(2), published.changed())
                .await
                .expect("test: the background run publishes") // test: the assertion failing loudly
                .expect("test: the runner stays alive"); // test: the assertion failing loudly
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
        // A real room: the worker (correctly, #425) skips nil-room inputs, so a
        // nil-room test never exercises the deferred lane at all.
        let room = Uuid::new_v4();

        // Tick 1 (cycle 1): COLD START — the lane self-warms by running the
        // inner synchronously (the 2026-07-10 fix for post-boot blind turns:
        // three observed greeting rounds because first prompts carried no
        // grounding). The first tick pays the inner cost and returns the FRESH
        // finding; it also publishes it as last-good.
        let ws1 = Workspace::in_room("burst one", room).with_cycle(CycleId(1));
        let r1 = deferred.contribute(&ws1).await;
        assert!(
            r1.is_some(),
            "cold start self-warms: the first tick carries grounding"
        );

        // Tick 2 immediately after (before the worker publishes anything): the
        // warm finding serves as last-good, NON-BLOCKING — the cold cost is
        // paid exactly once, and the steady-state hot path never waits on the
        // slow inner (it sleeps 40ms; we demand <15ms).
        let ws2 = Workspace::in_room("burst two", room).with_cycle(CycleId(2));
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
        let ws5 = Workspace::in_room("burst five", room).with_cycle(CycleId(5));
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

        // Regression for card157c095d: separating literal addressing must not
        // demote an unmentioned human question to the previous turn's recall.
        let human = Workspace::in_room("which port is the new gateway on?", Uuid::nil())
            .with_cycle(CycleId(2))
            .with_attention(crate::cognition::workspace::TurnAttention::HumanOpportunity);
        let human_finding = deferred
            .contribute(&human)
            .await
            .expect("fresh human recall");
        assert_eq!(
            human_finding.cycle,
            CycleId(2),
            "human opportunity must perceive this burst"
        );

        // The same tick UNDIRECTED still behaves as the deferred lane (fast —
        // the directed run warm-published, so this serves last-good) — ambience
        // never pays the synchronous cost.
        let ws_ambient = Workspace::in_room("idle chatter", Uuid::nil()).with_cycle(CycleId(3));
        let t = tokio::time::Instant::now();
        let _ = deferred.contribute(&ws_ambient).await;
        assert!(
            t.elapsed() < std::time::Duration::from_millis(15),
            "ambient turns must stay non-blocking; waited {:?}",
            t.elapsed()
        );
    }

    // what this catches: a late result must retain both resolved activity and
    // wake provenance. A task switch within one wake room invalidates it too.
    #[tokio::test]
    async fn deferred_finding_is_withheld_when_the_mind_changed_rooms() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));
        let wake = Uuid::new_v4();
        let activity = Uuid::new_v4();
        let mut original = Workspace::in_room("original activity", wake).with_cycle(CycleId(1));
        original.activity_room_id = activity;
        let mut completed = deferred.runner.subscribe();
        // Exercise the actual background run without a synchronous cold-start seed.
        deferred
            .runner
            .submit(original.clone(), world_revision(&original));
        let mut switched =
            Workspace::in_room("new activity, same wake", wake).with_cycle(CycleId(2));
        switched.activity_room_id = Uuid::new_v4();
        // Completion is observed, not inferred from a sleep duration.
        tokio::time::timeout(std::time::Duration::from_secs(5), completed.changed())
            .await
            .expect("deferred worker completion")
            .expect("worker remains owned");
        assert!(
            deferred.contribute(&switched).await.is_none(),
            "old activity cannot leak into the new task"
        );
        let mut other_wake =
            Workspace::in_room("same activity, other wake", Uuid::new_v4()).with_cycle(CycleId(3));
        other_wake.activity_room_id = activity;
        assert!(
            deferred.contribute(&other_wake).await.is_none(),
            "wake provenance remains part of the scope"
        );
        let found = deferred
            .contribute(&original)
            .await
            .expect("original scope remains eligible");
        assert_eq!(found.cycle, CycleId(1));
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
