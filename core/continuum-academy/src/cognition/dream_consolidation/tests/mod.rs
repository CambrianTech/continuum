#![cfg(test)]

use super::*;
pub(crate) use super::test_support::*;
use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
use crate::cognition::engram::{Engram, EngramKind, EngramOrigin};

/// The tests' observation budget: room for a few short engrams. The size is the test's
/// choice; production derives it from the served window (`passes.rs`).
const OBSERVATION_CHARS: usize = 4_096;

// what this catches (#175 budget-at-assembly): the observation block is composed
// WITHIN the char budget by dropping WHOLE trailing engrams — never truncating an
// engram's text (which would malform the prompt). This is what stops a big cluster
// from overflowing the served slot → 500 "Compute error" → poisoned lane.
#[test]
fn observations_block_budgets_by_dropping_whole_trailing_engrams() {
    let big = "x".repeat(100);
    let sources: Vec<Engram> = (0..10)
        .map(|i| episodic(Uuid::from_u128(i + 1), &big, &["k"]))
        .collect();
    let (block, kept) = SemanticDistiller::observations_block(&sources, 350);
    assert!(
        kept >= 1 && kept < 10,
        "dropped the tail to fit; kept {kept}"
    );
    // The LAST kept engram is present whole (not truncated mid-content).
    assert!(block.contains(&format!("{}. {}", kept, big)));
}

// what this catches: the autonomic consolidation trickle gate. The FIRST pass (no
// prior timestamp) is always allowed; after a pass, another is refused until the
// cooldown elapses — training is expensive, so background self-consolidation must
// trickle, never storm the serving lane (same discipline as the belief-review gate).
#[test]
fn consolidate_cooldown_gates_the_autonomic_trickle() {
    let now = 1_000_000_000u64;
    // Never run before → always allowed.
    assert!(consolidate_cooldown_elapsed(None, now));
    // Just ran → refused until the cooldown passes.
    assert!(!consolidate_cooldown_elapsed(Some(now), now));
    assert!(!consolidate_cooldown_elapsed(
        Some(now - (CONSOLIDATE_RECEIVED_COOLDOWN_MS - 1)),
        now
    ));
    // Cooldown elapsed → allowed again.
    assert!(consolidate_cooldown_elapsed(
        Some(now - CONSOLIDATE_RECEIVED_COOLDOWN_MS),
        now
    ));
}

// what this catches: a single engram larger than the whole budget still distills
// (kept=1) — the distiller never drops to zero; the adapter's overflow backstop is
// the last-resort net for that degenerate case, not this composer.
#[test]
fn observations_block_always_keeps_at_least_one() {
    let huge = "y".repeat(10_000);
    let sources = vec![episodic(Uuid::from_u128(1), &huge, &["k"])];
    let (block, kept) = SemanticDistiller::observations_block(&sources, 100);
    assert_eq!(kept, 1);
    assert!(
        block.contains(&huge),
        "the one engram is included whole, never sliced"
    );
}

// what this catches: the distiller actually invokes the inference adapter,
// captures its output as the fact content, and preserves full source
// provenance (ids in input order + the union of recall keys as tags).
// Regression guard for the dream silently dropping provenance or never
// calling the model.
#[tokio::test]
async fn distill_invokes_adapter_and_preserves_provenance() {
    let id1 = Uuid::from_u128(1);
    let id2 = Uuid::from_u128(2);
    let id3 = Uuid::from_u128(3);
    let sources = vec![
        episodic(id1, "Operator prefers Rust for the core", &["rust", "core"]),
        episodic(id2, "Node is only the shell", &["core", "node"]),
        episodic(id3, "Headless core, many clients", &["node", "clients"]),
    ];

    let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()), OBSERVATION_CHARS);
    let fact = distiller
        .distill(Some(Uuid::from_u128(99)), &sources)
        .await
        .expect("distillation succeeds against the heuristic adapter");

    // The heuristic adapter echoes the prompt deterministically — its
    // signature in the content proves the model was really called and its
    // output captured (not fabricated locally).
    assert!(
        fact.content.contains("[heuristic:"),
        "fact content should be the adapter's output, got: {}",
        fact.content
    );
    // Source ids preserved, in input order.
    assert_eq!(fact.source_ids, vec![id1, id2, id3]);
    // tags = union of recall keys, first-seen order, deduped.
    assert_eq!(
        fact.tags,
        vec![
            "rust".to_string(),
            "core".to_string(),
            "node".to_string(),
            "clients".to_string(),
        ]
    );
}

// what this catches: distilling an empty cluster fails LOUD with NoSources
// rather than silently returning an empty/fabricated fact
// ([[fallbacks-are-illegal-fail-loud]]).
#[tokio::test]
async fn distill_empty_cluster_fails_loud() {
    let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()), OBSERVATION_CHARS);
    let err = distiller
        .distill(None, &[])
        .await
        .expect_err("empty cluster must error, not return a fact");
    assert!(matches!(err, DistillError::NoSources));
}

// what this catches: the #145 anti-psychosis invariant at its synthesis
// point — a tagged lens's output ALWAYS carries `[thought:<lens>]` in the
// content itself, and the consolidator's durable facts stay untagged. If
// the prefix ever moves out of `distill_with`, an admit path could write
// unlabeled inner speech that recall would render as perception.
#[tokio::test]
async fn historian_output_is_provenance_tagged_and_consolidator_is_not() {
    let sources = vec![
        episodic(Uuid::from_u128(1), "tried the fence idiom again", &["act"]),
        episodic(Uuid::from_u128(2), "the fence parsed this time", &["act"]),
    ];
    let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()), OBSERVATION_CHARS);

    let thought = distiller
        .distill_with(LENS_HISTORIAN, None, &sources)
        .await
        .expect("historian distills");
    assert!(
        thought.content.starts_with("[thought:historian] "),
        "wanderer output must carry its provenance tag in-content, got: {}",
        thought.content
    );

    let fact = distiller
        .distill_with(LENS_CONSOLIDATOR, None, &sources)
        .await
        .expect("consolidator distills");
    assert!(
        fact.content.starts_with("[heuristic:"),
        "consolidated durable facts stay untagged, got: {}",
        fact.content
    );
}

// what this catches: each lens's system prompt is actually threaded into
// the request. The heuristic adapter hashes (model, messages,
// system_prompt), so two lenses over the SAME sources must produce
// different signatures; if the prompt were dropped or shared, the hashes
// would collide and this fails.
#[tokio::test]
async fn lens_system_prompt_reaches_the_adapter() {
    let sources = vec![episodic(Uuid::from_u128(7), "one memory", &["k"])];
    let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()), OBSERVATION_CHARS);

    let historian = distiller
        .distill_with(LENS_HISTORIAN, None, &sources)
        .await
        .expect("historian distills");
    let consolidator = distiller
        .distill_with(LENS_CONSOLIDATOR, None, &sources)
        .await
        .expect("consolidator distills");

    let sig = |s: &str| {
        let start = s.find("[heuristic:").expect("adapter signature present");
        s[start..start + 20].to_string()
    };
    assert_ne!(
        sig(&historian.content),
        sig(&consolidator.content),
        "different lens prompts must reach the model as different requests"
    );
}

// what this catches: the supersession verdict parse (#221 slice 2) — the
// model's trailing `SUPERSEDES: n[,m]` line maps 1-based indices onto the
// PRESENTED prior beliefs and is stripped from the fact text (a belief must
// read as knowledge, not a grading transcript). Tolerant: no line → no
// supersessions; junk/out-of-range indices ignored; the word "supersedes"
// in the fact BODY never triggers.
#[test]
fn parse_supersedes_line_maps_indices_and_strips() {
    let mk = |content: &str| Engram {
        id: Uuid::new_v4(),
        context_id: None,
        kind: crate::cognition::engram::EngramKind::Semantic,
        content: content.to_string(),
        origin: crate::cognition::engram::EngramOrigin::SelfReflection {
            parent_engram_id: Uuid::nil(),
        },
        recall_keys: vec![],
        admitted_at_ms: 1,
        trust_state_at_admission: crate::cognition::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    let priors = vec![mk("old belief one"), mk("old belief two"), mk("old three")];

    let (body, ids) = parse_supersedes_line(
        "The project is python; mathlib.py is the target.\nSUPERSEDES: 1,3",
        &priors,
    );
    assert_eq!(body, "The project is python; mathlib.py is the target.");
    assert_eq!(ids, vec![priors[0].id, priors[2].id]);

    // No verdict line → untouched, nothing superseded.
    let (body, ids) = parse_supersedes_line("A plain fact, nothing replaced.", &priors);
    assert_eq!(body, "A plain fact, nothing replaced.");
    assert!(ids.is_empty());

    // 'supersedes' in the BODY (not last line) never triggers.
    let (body, ids) = parse_supersedes_line(
        "This supersedes: nothing, really.\nJust a two-line fact.",
        &priors,
    );
    assert!(body.contains("supersedes"));
    assert!(ids.is_empty());

    // Out-of-range + junk indices are ignored, valid ones kept.
    let (_body, ids) = parse_supersedes_line("fact\nSUPERSEDES: 0, 2, 9, banana", &priors);
    assert_eq!(ids, vec![priors[1].id], "only the in-range index maps");
}

// what this catches: a DECORATED verdict line silently discarding the
// model's ruling. Glass-boxed live 2026-08-13 — probe dream.review.raw_tail
// carried `**SUPERSEDES: 3,6**`, which matched none of the three exact
// prefixes, so the else-branch returned "no supersessions" AND left the
// marker baked into the stored belief. Both halves failed silently: the
// judgment was lost (#221's review no-ops) and the belief was contaminated.
// Goes RED if the normalization is ever narrowed back to exact prefixes.
#[test]
fn a_decorated_verdict_line_is_still_the_models_judgment() {
    let mk = |content: &str| Engram {
        id: Uuid::new_v4(),
        context_id: None,
        kind: crate::cognition::engram::EngramKind::Semantic,
        content: content.to_string(),
        origin: crate::cognition::engram::EngramOrigin::SelfReflection {
            parent_engram_id: Uuid::nil(),
        },
        recall_keys: vec![],
        admitted_at_ms: 1,
        trust_state_at_admission: crate::cognition::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    let priors = vec![mk("one"), mk("two"), mk("three")];

    // The exact live shape: bold-wrapped on BOTH ends, so the trailing `**`
    // also has to come off or `3**` fails to parse as an index.
    let (body, ids) =
        parse_supersedes_line("A consolidated fact.\n**SUPERSEDES: 1,3**", &priors);
    assert_eq!(
        body, "A consolidated fact.",
        "the marker must never survive into the belief"
    );
    assert_eq!(
        ids,
        vec![priors[0].id, priors[2].id],
        "the ruling must be honored"
    );

    // Other decorations models reach for, and case variance.
    for line in [
        "*Supersedes: 2*",
        "`SUPERSEDES: 2`",
        "__supersedes: 2__",
        "## SUPERSEDES: 2",
        "**SUPERSEDES: 2**",
    ] {
        let (body, ids) = parse_supersedes_line(&format!("Fact.\n{line}"), &priors);
        assert_eq!(body, "Fact.", "marker leaked into the body for {line:?}");
        assert_eq!(ids, vec![priors[1].id], "ruling dropped for {line:?}");
    }

    // The explicit no-op still strips and supersedes nothing — the shape
    // that reached #general as speech.
    let (body, ids) = parse_supersedes_line("Fact.\n**SUPERSEDES: none**", &priors);
    assert_eq!(body, "Fact.");
    assert!(ids.is_empty(), "'none' is a decision to keep every prior");
}

pub(crate) mod region;
