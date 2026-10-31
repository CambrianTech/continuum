#![cfg(test)]

// The DreamConsolidationRegion — the organism's rest-state servicer. Nested
// here per the one-tests-mod-per-file rule; these drive `tick`/`consolidate`
// directly against a stub source (no governor, no real backend), the
// ship-it-dark unit-test contract for slice 3.

use super::*;
use crate::runtime::brain_region::RegionContext;

// ce37fbfb, a31e72a4: the idle pass runs while she works (no wait for her idle); a reviewed
// boundary delivered while it is in flight is finished by a later tick, never swallowed by the
// in-flight flag; repeated delivery and concurrent ticks never double inference.
#[tokio::test]
async fn a_reviewed_boundary_during_an_idle_pass_is_finished_once_without_doubling_inference() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let persona = Uuid::new_v4();
    let seeds: Vec<_> = (0..3)
        .map(|n| {
            episodic(
                Uuid::new_v4(),
                &format!("reviewed work observation {n}"),
                &["project"],
            )
        })
        .collect();
    let admission = seeded_admission(&seeds);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut region = DreamConsolidationRegion::new(Arc::new(StubReflectionSource {
        boundary_store: None,
        credit_unavailable: false,
        persona_id: persona,
        admission: admission.clone(),
        adapter: Arc::new(
            HeuristicInferenceAdapter::new().with_generate_observer(calls.clone()),
        ),
    }));
    let park = Arc::new(tokio::sync::Notify::new());
    region.idle_wait = Some(park.clone());
    region.consolidate(persona).await;
    tokio::task::yield_now().await;
    assert!(region.dreaming(), "the idle pass is in flight");
    let review = Uuid::new_v4();
    assert!(region.request_reviewed_boundary(persona, review));
    assert!(!region.request_reviewed_boundary(persona, review), "repeated delivery queues once");
    park.notify_one();
    drain(&region).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the idle pass ran to the end while she works: one consolidator and one historian request"
    );
    tokio::join!(region.consolidate(persona), region.consolidate(persona));
    drain(&region).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the boundary finds its episodes already folded: finished, with no second inference"
    );
    assert_eq!(
        admission
            .recall_recent(16)
            .iter()
            .filter(|e| e.kind == EngramKind::Semantic)
            .count(),
        1
    );
    assert!(!region.request_reviewed_boundary(persona, review), "a finished boundary is not queued again");
    region.consolidate(persona).await;
    drain(&region).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        !region.request_reviewed_boundary(Uuid::new_v4(), review),
        "unregistered recipients are not queued"
    );
}

// ce37fbfb: failed inference leaves the same accepted receipt pending;
// the existing governor retries it without another review or new ID.
#[tokio::test]
async fn reviewed_boundary_retries_failed_distillation_with_bounded_receipt_state() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let persona = Uuid::new_v4();
    let seeds: Vec<_> = (0..3)
        .map(|n| {
            episodic(
                Uuid::new_v4(),
                &format!("project observation {n}"),
                &["retry"],
            )
        })
        .collect();
    let admission = seeded_admission(&seeds);
    let good = HeuristicInferenceAdapter::new()
        .generate_text(TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "a reusable observation")],
            ..Default::default()
        })
        .await
        .unwrap();
    let mut empty = good.clone();
    empty.text.clear();
    let calls = Arc::new(AtomicUsize::new(0));
    let region = DreamConsolidationRegion::new(Arc::new(StubReflectionSource {
        boundary_store: None,
        credit_unavailable: false,
        persona_id: persona,
        admission: admission.clone(),
        adapter: Arc::new(
            HeuristicInferenceAdapter::new()
                .with_responses(vec![empty.clone(), good, empty])
                .with_generate_observer(calls.clone()),
        ),
    }));
    let review = Uuid::new_v4();
    assert!(region.request_reviewed_boundary(persona, review));
    region.consolidate(persona).await;
    drain(&region).await;
    assert!(region.review_boundaries.lock().unwrap()[&persona]
        .pending
        .contains(&review));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    region.consolidate(persona).await;
    drain(&region).await;
    assert!(region.review_boundaries.lock().unwrap()[&persona]
        .completed
        .contains(&review));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        admission
            .recall_recent(16)
            .iter()
            .filter(|e| e.kind == EngramKind::Semantic)
            .count(),
        1
    );
    assert_eq!(
        admission
            .recall_recent(16)
            .iter()
            .filter(|e| e.kind == EngramKind::SelfReflection)
            .count(),
        0,
        "optional historian failure must not be reported as an admitted thought"
    );
    for _ in 0..REVIEW_BOUNDARY_CAPACITY {
        assert!(region.request_reviewed_boundary(persona, Uuid::new_v4()));
    }
    let overflow = Uuid::new_v4();
    assert!(!region.request_reviewed_boundary(persona, overflow));
    region.consolidate(persona).await; // All eligible episodes already processed; no new inference.
    assert!(region.request_reviewed_boundary(persona, overflow));
    let all = region.review_boundaries.lock().unwrap();
    assert_eq!(all[&persona].completed.len(), REVIEW_BOUNDARY_CAPACITY);
    assert_eq!(all[&persona].pending.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

// what this catches: the dream reads a persona's fresh episodics,
// clusters those sharing a recall key, distills the cluster via the
// adapter, and admits the result as a durable Semantic engram that
// recall then surfaces — the whole consolidation arc end-to-end.
// what this catches: the source/drain regression that started this whole arc —
// `apply_decay_sweep` had ZERO live callers, so engram salience NEVER decayed in a
// running persona and she got "set in her ways" (glass-boxed 2026-07-22). The dream tick
// must now run the decay drain: a decayable engram in the hippocampus decays after ONE
// tick, so nothing accumulates forever. regression for the plastic-memory fix (#221).
#[tokio::test]
async fn dream_tick_runs_the_decay_drain() {
    use crate::cognition::recall_metadata::RecallMetadata;
    let persona = Uuid::from_u128(7);
    // A single episodic so there's a live reflective surface (below min_cluster → the
    // consolidation itself sleeps, which is the point: decay runs BEFORE that gate).
    let admission = seeded_admission(&[episodic(Uuid::from_u128(1), "one memory", &["k"])]);
    // An unprotected, decayable engram sitting in the hippocampus (salience 0.8, never
    // decayed, no novelty-protection window).
    let decayable = Uuid::from_u128(99);
    admission.recall_metadata().admit(
        decayable,
        RecallMetadata {
            salience: 0.8,
            last_decayed_ms: 0,
            protected_until_ms: 0,
            ..Default::default()
        },
    );
    let region = DreamConsolidationRegion::new(Arc::new(StubReflectionSource {
        persona_id: persona,
        admission: admission.clone(),
        adapter: Arc::new(HeuristicInferenceAdapter::new()),
        boundary_store: None,
        credit_unavailable: true,
    }));
    let retained = Uuid::new_v4();
    assert!(region.request_reviewed_boundary(persona, retained));
    region.tick(&RegionContext::for_persona(0, persona)).await;
    assert!(
        region.review_boundary_pending(persona, retained),
        "storage failure cannot acknowledge pending work or disable ordinary decay"
    );

    let after = admission
        .recall_metadata()
        .get(decayable)
        .expect("engram still tracked");
    assert!(
        after.last_decayed_ms > 0,
        "the dream tick must have run the decay sweep"
    );
    assert!(
        after.salience < 0.8,
        "salience must have decayed, got {}",
        after.salience
    );
}

#[tokio::test]
async fn dream_distills_fresh_cluster_into_semantic_fact() {
    let persona = Uuid::from_u128(7);
    let seeds = vec![
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
        episodic(Uuid::from_u128(3), "Headless core, many clients", &["rust"]),
    ];
    let admission = seeded_admission(&seeds);
    let region = region_over(persona, admission.clone());

    let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;

    // The tick is the gate + launcher ONLY (the governor isolates ticks
    // behind a hard timeout no 24B inference can fit — the pre-fix
    // inline shape timed out every pass on first live boot). Work is in
    // flight; the tick itself publishes nothing.
    assert_eq!(outcome.published, 0, "tick launches; the pass publishes");
    assert!(region.dreaming(), "dream pass spawned on its own task");
    drain(&region).await;
    // The new fact is a Semantic engram carrying the adapter's output
    // (its signature proves the model was really called, not fabricated).
    let semantic: Vec<Engram> = admission
        .recall_recent(16)
        .into_iter()
        .filter(|e| e.kind == EngramKind::Semantic)
        .collect();
    assert_eq!(semantic.len(), 1, "exactly one durable fact admitted");
    assert!(
        semantic[0].content.contains("[heuristic:"),
        "fact is the adapter's distillation, got: {}",
        semantic[0].content
    );
    // Provenance: SelfReflection parent = the cluster's first source.
    assert!(matches!(
        semantic[0].origin,
        EngramOrigin::SelfReflection { .. }
    ));
    // The wander pass (#145): ONE historian thought admitted alongside
    // the fact — SelfReflection KIND, provenance tag in-content, lens
    // queryable by recall key. what this catches: the dream going live
    // without its wanderer, or the thought losing its typed provenance.
    let thoughts: Vec<Engram> = admission
        .recall_recent(16)
        .into_iter()
        .filter(|e| e.kind == EngramKind::SelfReflection)
        .collect();
    assert_eq!(thoughts.len(), 1, "exactly one historian thought admitted");
    assert!(
        thoughts[0].content.starts_with("[thought:historian] "),
        "inner speech must carry its provenance tag, got: {}",
        thoughts[0].content
    );
    assert!(
        thoughts[0]
            .recall_keys
            .iter()
            .any(|k| k == "thought:historian"),
        "the lens stream must be queryable by recall key"
    );
}

// what this catches: the organism rests instead of running on a clock —
// once material is consolidated, a second dream finds nothing fresh,
// does NO inference (publishes nothing), and asks to sleep. Regression
// guard against the rejected `tick_number % N` automaton design.
#[tokio::test]
async fn dream_rests_when_no_fresh_experience() {
    let persona = Uuid::from_u128(7);
    let seeds = vec![
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
    ];
    let admission = seeded_admission(&seeds);
    let region = region_over(persona, admission.clone());

    // First dream consolidates the cluster (launch + drain).
    region.tick(&RegionContext::for_persona(0, persona)).await;
    drain(&region).await;
    let admitted_after_first = admission.recall_recent(32).len();

    // Second dream: the episodics are already consolidated, so nothing
    // fresh remains — it rests, spawns nothing, asks to sleep.
    let second = region.tick(&RegionContext::for_persona(1, persona)).await;
    assert_eq!(
        second.published, 0,
        "no re-distillation of consolidated material"
    );
    assert!(!region.dreaming(), "nothing fresh → no pass spawned");
    assert_eq!(second.cadence_hint, Some(CadenceHint::Sleep));
    assert_eq!(
        admission.recall_recent(32).len(),
        admitted_after_first,
        "no new engrams from the resting tick"
    );
    let semantic_count = admission
        .recall_recent(16)
        .into_iter()
        .filter(|e| e.kind == EngramKind::Semantic)
        .count();
    assert_eq!(semantic_count, 1, "still exactly one fact — no duplicate");
}

// what this catches: REVIEW-ONLY dreams (#221 slice 2c) — on a QUIET
// day (zero fresh episodics) the dream still works: it launches a
// review pass over her oldest age-eligible beliefs, marks them
// reviewed so the queue drains, and once drained it truly sleeps.
// Without this, a repeat-work day (identical lessons, correctly
// deduped) starved the review entirely — glass-boxed live 2026-07-22:
// six ludicrous cycles, zero supersession probes, because reviewing
// old beliefs required new experience as a vehicle.
#[tokio::test]
async fn quiet_day_dream_reviews_old_beliefs_then_sleeps() {
    let persona = Uuid::from_u128(9);
    let mut old_belief = episodic(
        Uuid::from_u128(41),
        "You can work with specific Rust files (main.rs, life.rs, wordstats.rs)",
        &["rust"],
    );
    old_belief.kind = EngramKind::Semantic; // an ancient conclusion (admitted_at_ms = 1_000)
    let admission = seeded_admission(&[old_belief]);
    let region = region_over(persona, admission.clone());

    // Quiet day: no fresh episodics at all — yet the dream launches a
    // review pass instead of sleeping.
    region.tick(&RegionContext::for_persona(0, persona)).await;
    assert!(
        region.dreaming(),
        "review-only pass launched with zero fresh material"
    );
    drain(&region).await;

    // Queue drained (the belief is marked reviewed; the refreshed
    // restatement the heuristic adapter produced is too YOUNG for the
    // age gate) — the next quiet tick truly rests.
    let second = region.tick(&RegionContext::for_persona(1, persona)).await;
    assert!(!region.dreaming(), "review queue drained — no second pass");
    assert_eq!(second.cadence_hint, Some(CadenceHint::Sleep));

    // what this catches (card 07188783; the 5090, three review passes in 14 minutes):
    // the cooldown read had no writer. Another age-eligible belief arrives within the
    // cooldown: the rest gate holds, and no second review launches.
    let mut another = episodic(Uuid::from_u128(42), "Cargo workspaces share one target directory", &["rust"]);
    another.kind = EngramKind::Semantic;
    admission.admit_reflection(another).expect("test: an old belief");
    region.tick(&RegionContext::for_persona(2, persona)).await;
    assert!(!region.dreaming(), "within the review cooldown, no second review pass");
}

// what this catches: a lone episode is not yet a pattern — below
// min_cluster the dream waits (sleeps, no inference) rather than
// distilling a singleton into a spurious "fact".
#[tokio::test]
async fn dream_waits_below_min_cluster() {
    let persona = Uuid::from_u128(7);
    let seeds = vec![episodic(
        Uuid::from_u128(1),
        "a lone observation",
        &["solo"],
    )];
    let admission = seeded_admission(&seeds);
    let region = region_over(persona, admission.clone());

    let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;

    assert_eq!(
        outcome.published, 0,
        "a singleton is not a pattern to distill"
    );
    assert_eq!(outcome.cadence_hint, Some(CadenceHint::Sleep));
}

// what this catches: a global (non-persona-scoped) tick has nothing to
// consolidate and sleeps — the dream is per-persona, like the digest.
#[tokio::test]
async fn dream_global_tick_is_noop() {
    let persona = Uuid::from_u128(7);
    let admission = seeded_admission(&[
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
    ]);
    let region = region_over(persona, admission);

    let outcome = region.tick(&RegionContext::global(0)).await;

    assert_eq!(outcome.published, 0);
    assert_eq!(outcome.cadence_hint, Some(CadenceHint::Sleep));
}

// what this catches: consolidation is declared SelfDirected interiority,
// NOT the default Reactive. The governor's orientation budget floors the
// SelfDirected share, so a reverter who drops this back to Reactive would
// silently let memory-consolidation compete with (and preempt) the
// foreground responding budget — exactly the latency regression the
// off-foreground design exists to prevent.
#[tokio::test]
async fn consolidation_draws_from_the_interiority_budget() {
    let region = region_over(Uuid::from_u128(7), seeded_admission(&[]));
    assert_eq!(
        region.orientation(),
        Orientation::SelfDirected,
        "consolidation is the being's own inner work, never reactive stimulus-response",
    );
}
