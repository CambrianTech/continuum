#![cfg(test)]

// Her dream part: its passes driven through `step` and `pass` against a stub source (no Core, no
// bus), its need from what waits, and one test of it started on her bus, woken by what her mind
// says there. Nested here per the one-tests-mod-per-file rule.

use super::*;
use continuum_sage::component::{start, Bus, Component, Presence, Urgency, PRESENCE};
use continuum_sage::dataflow::OwnerId;

/// Her dream over `admission` with an adapter that counts its generations.
fn counted(persona: Uuid, admission: &Arc<AdmissionState>, adapter: HeuristicInferenceAdapter, store: Option<Arc<dyn ReviewBoundaryLedger>>) -> (DreamPart, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut part = DreamPart::new(
        persona,
        Arc::new(StubReflectionSource {
            persona_id: persona,
            admission: Arc::clone(admission),
            adapter: Arc::new(adapter.with_generate_observer(Arc::clone(&calls))),
            boundary_store: store,
            credit_unavailable: false,
        }),
    );
    part.her.lived_from = Some(0); // her store is seeded before her dream reads it
    (part, calls)
}

fn semantic_facts(admission: &AdmissionState) -> usize {
    admission.recall_recent(16).iter().filter(|e| e.kind == EngramKind::Semantic).count()
}

// ce37fbfb, a31e72a4: a reviewed boundary that arrives after her idle pass folded its episodes is
// finished once with no second inference, and repeated delivery queues once (a processed one is
// never asked for again: her credit store records it, core's reviewed-credit test). One pass of hers at a time is her part's worker: never two dreams at once.
#[tokio::test]
async fn a_reviewed_boundary_after_an_idle_pass_is_finished_once_without_a_second_inference() {
    use std::sync::atomic::Ordering;
    let persona = measured(Uuid::new_v4());
    let seeds: Vec<_> = (0..3).map(|n| episodic(Uuid::new_v4(), &format!("reviewed work observation {n}"), &["project"])).collect();
    let admission = seeded_admission(&seeds);
    let (mut dream, calls) = counted(persona, &admission, HeuristicInferenceAdapter::new(), None);
    dream.step(DreamInput::Experienced).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "the idle pass: one consolidator and one historian request");
    let review = Uuid::new_v4();
    assert!(dream.her.queue_boundary(persona, review));
    assert!(!dream.her.queue_boundary(persona, review), "repeated delivery queues once");
    dream.step(DreamInput::Drain).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "the boundary finds its episodes already folded: finished, with no second inference");
    assert!(!dream.review_boundary_pending(review));
    assert_eq!(semantic_facts(&admission), 1);
}

// ce37fbfb: failed inference leaves the same accepted receipt pending; her next pass retries it
// without another review or a new id.
#[tokio::test]
async fn reviewed_boundary_retries_failed_distillation_on_her_next_pass() {
    use std::sync::atomic::Ordering;
    let persona = measured(Uuid::new_v4());
    let seeds: Vec<_> = (0..3).map(|n| episodic(Uuid::new_v4(), &format!("project observation {n}"), &["retry"])).collect();
    let admission = seeded_admission(&seeds);
    let good = HeuristicInferenceAdapter::new()
        .generate_text(TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "a reusable observation")],
            // A distillation answers in the lesson's shape: the canned reply is one.
            response_format: Some(super::super::lesson::DistilledLesson::format(0).expect("the lesson schema serializes")),
            ..Default::default()
        }, crate::ai::adapter::Dispatched::fixture())
        .await
        .unwrap();
    let mut empty = good.clone();
    empty.text.clear();
    let (mut dream, calls) = counted(persona, &admission, HeuristicInferenceAdapter::new().with_responses(vec![empty.clone(), good, empty]), None);
    let review = Uuid::new_v4();
    dream.step(DreamInput::Asked(Ask::Reviewed { acceptance: review })).await;
    assert!(dream.review_boundary_pending(review), "a failed distillation keeps its boundary");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    dream.step(DreamInput::Drain).await;
    assert!(!dream.review_boundary_pending(review), "her next pass finished it");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(semantic_facts(&admission), 1);
    assert_eq!(
        admission.recall_recent(16).iter().filter(|e| e.kind == EngramKind::SelfReflection).count(),
        0,
        "optional historian failure must not be reported as an admitted thought"
    );
}

// what this catches: the source/drain regression that started this whole arc: `apply_decay_sweep`
// had ZERO live callers, so engram salience never decayed in a running persona (glass-boxed
// 2026-07-22). Every pass runs the decay drain, before any consolidation gate, and a storage
// failure neither acknowledges pending work nor skips it. regression for #221.
#[tokio::test]
async fn every_pass_runs_the_decay_drain() {
    use crate::cognition::recall_metadata::RecallMetadata;
    let persona = measured(Uuid::from_u128(7));
    // A single episodic is no pattern, so the consolidation rests; decay runs before that.
    let admission = seeded_admission(&[episodic(Uuid::from_u128(1), "one memory", &["k"])]);
    let decayable = Uuid::from_u128(99);
    admission.recall_metadata().admit(decayable, RecallMetadata { salience: 0.8, last_decayed_ms: 0, protected_until_ms: 0, ..Default::default() });
    let mut dream = DreamPart::new(
        persona,
        Arc::new(StubReflectionSource {
            persona_id: persona,
            admission: admission.clone(),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            boundary_store: None,
            credit_unavailable: true,
        }),
    );
    dream.her.lived_from = Some(0);
    let retained = Uuid::new_v4();
    dream.step(DreamInput::Asked(Ask::Reviewed { acceptance: retained })).await;
    assert!(dream.review_boundary_pending(retained), "storage failure cannot acknowledge pending work or disable ordinary decay");
    let after = admission.recall_metadata().get(decayable).expect("engram still tracked");
    assert!(after.last_decayed_ms > 0, "the pass must have run the decay sweep");
    assert!(after.salience < 0.8, "salience must have decayed, got {}", after.salience);
}

// what this catches: the whole consolidation arc: her fresh episodics sharing a recall key are
// distilled through the adapter into one durable Semantic fact recall surfaces, with ONE
// historian thought beside it (#145), each carrying its typed provenance.
#[tokio::test]
async fn her_fresh_cluster_is_distilled_into_a_semantic_fact() {
    let persona = measured(Uuid::from_u128(7));
    let seeds = vec![
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
        episodic(Uuid::from_u128(3), "Headless core, many clients", &["rust"]),
    ];
    let admission = seeded_admission(&seeds);
    let mut dream = part_over(persona, admission.clone());
    assert_eq!(dream.pass(DreamTrigger::Idle).await, Passed::Ran { more: false });
    let semantic: Vec<Engram> = admission.recall_recent(16).into_iter().filter(|e| e.kind == EngramKind::Semantic).collect();
    assert_eq!(semantic.len(), 1, "exactly one durable fact admitted");
    assert!(semantic[0].content.contains("[heuristic:"), "fact is the adapter's distillation, got: {}", semantic[0].content);
    assert!(matches!(semantic[0].origin, EngramOrigin::SelfReflection { .. }));
    let thoughts: Vec<Engram> = admission.recall_recent(16).into_iter().filter(|e| e.kind == EngramKind::SelfReflection).collect();
    assert_eq!(thoughts.len(), 1, "exactly one historian thought admitted");
    assert!(thoughts[0].content.starts_with("[thought:historian] "), "inner speech carries its provenance tag, got: {}", thoughts[0].content);
    assert!(thoughts[0].recall_keys.iter().any(|k| k == "thought:historian"), "the lens stream is queryable by recall key");
}

// what this catches: her dream rests instead of running on a clock: once her material is
// consolidated, the next pass finds nothing fresh, does no inference and rests, and a lone episode
// is not yet a pattern to distill.
#[tokio::test]
async fn her_dream_rests_when_nothing_waits() {
    let persona = measured(Uuid::from_u128(7));
    let admission = seeded_admission(&[
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
        episodic(Uuid::from_u128(3), "a lone observation", &["solo"]),
    ]);
    let mut dream = part_over(persona, admission.clone());
    dream.step(DreamInput::Experienced).await;
    let admitted = admission.recall_recent(32).len();
    assert_eq!(dream.pass(DreamTrigger::Idle).await, Passed::Rested, "nothing fresh but a singleton: she rests");
    assert_eq!(admission.recall_recent(32).len(), admitted, "no new engrams from a resting pass");
    assert_eq!(semantic_facts(&admission), 1, "still exactly one fact, no duplicate");
}

// what this catches: REVIEW-ONLY dreams (#221 slice 2c): with no fresh experience her dream still
// reviews her oldest age-eligible beliefs, marks them reviewed so the queue drains, and once drained
// it rests (glass-boxed 2026-07-22: reviewing old beliefs once required new experience as a vehicle).
// A belief that ages in later is reviewed by her next pass: no cooldown holds it (her need, Content
// outside a rest, keeps the review in the Core's slack).
#[tokio::test]
async fn a_quiet_dream_reviews_her_old_beliefs_then_rests() {
    let persona = measured(Uuid::from_u128(9));
    let mut old_belief = episodic(Uuid::from_u128(41), "You can work with specific Rust files (main.rs, life.rs, wordstats.rs)", &["rust"]);
    old_belief.kind = EngramKind::Semantic; // an ancient conclusion (admitted_at_ms = 1_000)
    let admission = seeded_admission(&[old_belief]);
    let mut dream = part_over(persona, admission.clone());
    assert_eq!(dream.pass(DreamTrigger::Idle).await, Passed::Ran { more: false }, "a review pass with zero fresh material, its queue drained");
    assert_eq!(dream.pass(DreamTrigger::Idle).await, Passed::Rested, "the restatement is too young to judge: she rests");
    let mut another = episodic(Uuid::from_u128(42), "Cargo workspaces share one target directory", &["rust"]);
    another.kind = EngramKind::Semantic;
    admission.admit_reflection(another).expect("test: an old belief");
    assert_eq!(dream.pass(DreamTrigger::Idle).await, Passed::Ran { more: false }, "a newly eligible belief is reviewed at once");
}

// what this catches: her dream's need from a clock or a constant instead of what waits for it. An
// accepted review waiting is Urgent; she at rest with fresh experience or a taught lesson due is
// Want; while she acts, or with only belief review left, it is Content (the Core's slack).
#[tokio::test]
async fn her_dream_needs_what_waits_for_it() {
    let persona = measured(Uuid::from_u128(11));
    let admission = seeded_admission(&[
        episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
        episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
    ]);
    let mut dream = part_over(persona, admission);
    assert_eq!(dream.urgency(), Urgency::Want, "at rest with a fresh cluster");
    dream.take(DreamInput::Acting(true));
    assert_eq!(dream.urgency(), Urgency::Content, "while she acts her dream takes only the Core's slack");
    dream.her.queue_boundary(persona, Uuid::new_v4());
    assert_eq!(dream.urgency(), Urgency::Urgent, "an accepted review waits to become what she knows");
    dream.her.boundaries.pending.clear();
    dream.step(DreamInput::Experienced).await; // consolidates the cluster; the taught lesson is taken
    dream.take(DreamInput::Acting(false));
    assert_eq!(dream.taught, Taught::Owed, "coming to rest owes her a taught-lesson pass");
    assert_eq!(dream.urgency(), Urgency::Want);
    dream.step(DreamInput::Drain).await;
    assert_eq!(dream.urgency(), Urgency::Content, "nothing fresh and nothing taught: only review is left");
}

// what this catches: her dream not started from what her mind says on her bus: her experience
// landing (an Episodic admit on her store) wakes it and it consolidates, with no tick anywhere.
#[tokio::test]
async fn her_experience_on_her_bus_wakes_her_dream() {
    let persona = measured(Uuid::from_u128(13));
    let admission = seeded_admission(&[]);
    let bus = Bus::new();
    admission.say_experience_on(&bus).expect("test: her experience has one writer");
    crate::cognition::turn_ledger::presence::say_on(persona, &bus).expect("test: her presence has one writer");
    let mut presence = bus.reader::<Presence>(PRESENCE, continuum_sage::component::presence_key(DreamPart::NAME));
    let _running = start(part_over(persona, admission.clone()), &bus, OwnerId(persona), 1);
    // Her presence at start is its first input: handled (Ready, then Acting and Ready again) before
    // anything lands, so only her experience can wake the pass that consolidates what lands next.
    handled(&mut presence, 1).await;
    for n in 0..2 {
        admission.admit_reflection(episodic(Uuid::new_v4(), &format!("lived moment {n}"), &["moment"])).expect("test: an episode");
    }
    // Nothing but her experience is said after her start: her two episodes wake her once (coalesced)
    // or twice (the first alone is no pattern, and she rests until the second).
    let mut inputs = 2;
    while semantic_facts(&admission) == 0 {
        handled(&mut presence, inputs).await;
        inputs += 1;
    }
    assert_eq!(semantic_facts(&admission), 1);
    crate::cognition::turn_ledger::presence::forget(persona);
}

/// Wait until her dream has handled `count` inputs in all: each handled input publishes two
/// presence frames (Acting, then Ready) after the Ready it starts with.
async fn handled(presence: &mut continuum_sage::dataflow::Reader<Presence>, count: u64) {
    loop {
        if let Some(frame) = presence.latest() {
            if frame.revision.through >= 1 + 2 * count {
                return;
            }
        }
        if presence.changed().await.is_err() {
            return;
        }
    }
}
