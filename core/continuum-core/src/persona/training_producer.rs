//! L2 — the producer that feeds the training trigger.
//!
//! Every completed live persona turn is unrealized training signal
//! ([[capability-is-driver-plus-genome]]): the `(stimulus → reply)` pair is a
//! `(context, completion)` example the genome loop can learn from. This module is
//! the SEAM that turns one completed live turn into one buffered training example
//! — scored for quality, classified into the right domain bucket, and submitted to
//! the standing `genome/training-trigger`, which auto-fires a `genome/job-create`
//! once a bucket fills ([`DEFAULT_MIN_EXAMPLES`] examples). It is the L2 layer of
//! the dev-task continuous-learning loop (`docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md`):
//! the orchestration BREAK 1 — `submit` had zero non-test callers; this is the
//! call.
//!
//! ## Where it hooks (and why NOT a capture sweep)
//!
//! [`produce`] is called synchronously from the live turn-completion path
//! ([`crate::persona::service_loop::serve_persona_loop`], the `Spoke` arm, after
//! the reply is published). That path is structurally distinct from the eval forks
//! ([`crate::cognition::persona_workspace::fork_eval_cycle`] /
//! `drive_to_settle`), which never run the service loop — so the producer can never
//! train on an eval SIMULATION (a metric cheat). A background sweep of the
//! prompt-capture dir was rejected for exactly that reason: eval forks rebuild a
//! cycle with the SAME `persona_id` and write the SAME `<id>.jsonl`, so sweeping
//! captures would fold measurement sims into the training set.
//!
//! ## Best-effort, never on the turn's critical path
//!
//! Production runs on a spawned task: a failure (or absence) of training capture
//! must never break or delay the persona's reply. This mirrors
//! [`crate::persona::recorder`] and the prompt-capture sink — side-channel
//! observability that degrades quietly, NOT a fallback on the answer path. The
//! command executor is late-bound (post-#224 [`LateBound`], installed at boot in
//! `ipc/mod.rs`); before install (tests, early boot) production is a logged no-op.
//!
//! ## The quality gate is a data filter, not a cognition heuristic
//!
//! [`MIN_TRAINING_QUALITY`] gates which turns enter the TRAINING corpus — the same
//! role as `dataset.rs` dropping empty/system-only captures. It does not read or
//! steer the persona's behavior or output ([[no-hardcoded-heuristics-to-steer-cognition]]
//! is about puppeting cognition; this is corpus hygiene).

use std::sync::{Arc, OnceLock};

use serde_json::json;
use uuid::Uuid;

use crate::genome::fine_tuning::types::TrainingExample;
use crate::persona::domain_classifier::{score_interaction_quality, DomainClassifier};
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

/// The substrate-wired [`CommandExecutor`] (the one `start_server` builds with the
/// `GridTrustAuthPolicy` + interceptors), installed once at boot via
/// [`install_executor`]. The producer dispatches `genome/training-trigger/submit`
/// through a per-persona [`Connection`] over this executor so the submit is gated
/// AS the persona (`LocalPersona` → `Trusted`, which may run the `Privileged`
/// submit but not Owner-gated ops). Late-bound because the service loop has no
/// executor in scope; absent before boot installs it (tests).
static EXECUTOR: LateBound<CommandExecutor> = LateBound::new("training_producer::executor");

/// One shared, stateless-for-classification [`DomainClassifier`]. `classify` is a
/// pure function of the text (the instance only holds keyword tables), so a single
/// shared instance is correct and avoids locking the per-persona
/// `PersonaCognition` on the producer path. Lazily built on first turn.
static CLASSIFIER: OnceLock<DomainClassifier> = OnceLock::new();

/// Quality bar below which a live turn is NOT worth training on.
///
/// On a live turn there is no human feedback or task outcome, so
/// `score_interaction_quality` reduces to `0.275 + 0.3 * substance` (+ a small
/// structural bonus for code/list formatting). Trivial one-liners ("ok", "thanks"
/// — substance `0.1` → score ≈ `0.305`) fall below this bar; substantive replies
/// (≥100 chars → substance `0.7` → ≈ `0.485`, or ≥500 chars → ≈ `0.545`) clear it.
/// `0.45` is the clean separator between "acknowledgement" and "real content."
const MIN_TRAINING_QUALITY: f32 = 0.45;

/// A scored, gated, classified training example ready to submit. The pure product
/// of [`plan`] — it separates the JUDGMENT (is this turn worth training on? which
/// domain bucket?) from the EFFECT (dispatch to the trigger), so the judgment is
/// unit-testable with no executor.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitPlan {
    /// The domain bucket — `DomainClassifier::classify(...).domain`. Becomes the
    /// `traitKind` of the `(persona_id, trait_kind, base_model)` bucket key.
    pub trait_kind: String,
    /// The stimulus (the triggering message text).
    pub prompt: String,
    /// The persona's reply.
    pub completion: String,
    /// The interaction-quality score that cleared [`MIN_TRAINING_QUALITY`]
    /// (carried into example metadata as training provenance).
    pub quality: f32,
}

/// Score the turn, gate on quality, classify its domain → `Some(SubmitPlan)`;
/// `None` if gated out (trivial / low-quality). Pure: no I/O, no executor, no
/// spawn — this is the testable judgment half of the producer.
///
/// `classifier` is borrowed (the shared [`DomainClassifier`]). Quality uses
/// `score_interaction_quality` with no feedback/outcome — a live turn carries
/// neither. Classification runs on the full `(prompt, completion)` pair so both
/// the ask and the answer inform the bucket.
pub fn plan(classifier: &DomainClassifier, prompt: &str, completion: &str) -> Option<SubmitPlan> {
    let quality = score_interaction_quality(prompt, completion, None, None);
    if quality.score < MIN_TRAINING_QUALITY {
        return None;
    }
    let domain = classifier
        .classify(&format!("{prompt}\n{completion}"))
        .domain;
    Some(SubmitPlan {
        trait_kind: domain,
        prompt: prompt.to_string(),
        completion: completion.to_string(),
        quality: quality.score,
    })
}

/// Install the substrate-wired [`CommandExecutor`] so the producer can dispatch
/// the trigger. Called once at boot (`ipc/mod.rs`, right after
/// `install_executor_on_all`). Second install is a silent no-op ([`LateBound`]
/// semantics) — the boot executor wins.
pub fn install_executor(executor: Arc<CommandExecutor>) {
    EXECUTOR.install(executor);
}

/// Turn one completed live turn into one buffered training example.
///
/// Best-effort and non-blocking: it spawns the scoring/classify/submit work so the
/// caller's turn latency is untouched, and quietly does nothing if the executor
/// isn't installed yet (tests / early boot) or the turn is gated out. NEVER call
/// this from an eval/measurement path — only the live `Spoke` completion, so the
/// training set stays uncontaminated by simulations.
pub fn produce(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    prompt: String,
    completion: String,
) {
    let Some(executor) = EXECUTOR.cloned() else {
        // Expected during tests / before boot installs the executor. Named, not
        // silent — but debug, because a turn before install is normal at startup
        // and this is a side channel, not the answer path.
        tracing::debug!(
            persona = %persona_id,
            "training_producer: executor not installed yet — skipping training capture for this turn"
        );
        return;
    };

    tokio::spawn(async move {
        let classifier = CLASSIFIER.get_or_init(DomainClassifier::new);
        let Some(plan) = plan(classifier, &prompt, &completion) else {
            crate::probe!(
                class = "training.example.skipped",
                persona = %persona_name,
                prompt_chars = prompt.len() as u64,
                completion_chars = completion.len() as u64,
                "live turn below the training-quality floor — not buffered"
            );
            return;
        };
        crate::probe!(
            class = "training.example.produced",
            persona = %persona_name,
            domain = %plan.trait_kind,
            quality = plan.quality as f64,
            "live turn buffered as a training example (L2)"
        );

        // One submit path, N experience sources — the live turn is the "live-turn"
        // provenance into the shared flywheel entry.
        submit_plan(
            persona_id,
            persona_name,
            base_model,
            executor,
            plan,
            "live-turn",
        )
        .await;
    });
}

/// Plan a RECEIVED lesson (one another agent taught via `memory/share` #2025) for
/// the SAME training flywheel — the efferent wiring for the being-loop's third axis
/// ([[lived-and-eval-experience-are-one-stream-one-being]]). Pure: classify + package,
/// no I/O.
///
/// Unlike [`plan`], there is **no `MIN_TRAINING_QUALITY` gate**: a received lesson's
/// salience is its PROVENANCE — someone deliberately chose to teach it (`ReceivedSalience`,
/// where provenance IS the signal), not a chat-interaction score designed for live
/// `(stimulus → reply)` turns. A terse-but-important lesson must not be filtered by a
/// heuristic built for a different shape. We DO classify the domain, so the lesson buckets
/// into the gym that MEASURES its trait — the sentinel still refuses to page in a gene the
/// benchmark-lift gate (#59) didn't clear, so unmeasurable lessons train but never adopt
/// ([[fallbacks-are-illegal-fail-loud]]). `quality = 1.0`: the deliberate act of sharing.
pub fn plan_received(classifier: &DomainClassifier, topic: &str, lesson: &str) -> SubmitPlan {
    let domain = classifier.classify(&format!("{topic}\n{lesson}")).domain;
    SubmitPlan {
        trait_kind: domain,
        prompt: topic.to_string(),
        completion: lesson.to_string(),
        quality: 1.0,
    }
}

/// Submit ONE received lesson into the live training flywheel — the being-loop going
/// live for the received axis. Best-effort and non-blocking (mirrors [`produce`]): a
/// lesson one agent learned becomes another's trained-in capability, gated per-consolidation
/// by whole-being benchmark lift (the trigger's gym-`evalSet` → the L3 sentinel). Quietly a
/// no-op before the executor is installed (tests / early boot). `topic` frames the lesson
/// (its scope); `lesson` is the deliberately-authored knowledge (the completion).
pub fn produce_received(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    topic: String,
    lesson: String,
) {
    let Some(executor) = EXECUTOR.cloned() else {
        tracing::debug!(
            persona = %persona_id,
            "training_producer: executor not installed yet — skipping received-lesson consolidation"
        );
        return;
    };
    tokio::spawn(async move {
        let classifier = CLASSIFIER.get_or_init(DomainClassifier::new);
        let plan = plan_received(classifier, &topic, &lesson);
        submit_plan(
            persona_id,
            persona_name,
            base_model,
            executor,
            plan,
            "received-lesson",
        )
        .await;
    });
}

/// Build the wire params for `genome/training-trigger/submit` from a plan — the ONE
/// payload contract, pure + `pub` so a synchronous caller (`memory/consolidate`
/// dispatching received lessons on demand) produces byte-identical params to the
/// fire-and-forget producers. `source: raw` = unfiltered capture; minExamples omitted →
/// DEFAULT_MIN_EXAMPLES (the trigger auto-fires job-create at the threshold). `evalSet`
/// rides the {trait → gym} edge: the committed gym that MEASURES `plan.trait_kind`. When
/// the trait HAS a gym it is declared so the L3 sentinel can A/B and adopt the gene; when
/// it has NONE the field is OMITTED and the sentinel REFUSES to adopt as unmeasurable
/// ([[fallbacks-are-illegal-fail-loud]]) — never paged into a live persona on a gym that
/// doesn't measure its trait. `provenance` is metadata only (live-turn vs received-lesson).
pub fn build_submit_params(
    persona_id: Uuid,
    persona_name: &str,
    base_model: &str,
    plan: &SubmitPlan,
    provenance: &str,
) -> serde_json::Value {
    let example = TrainingExample {
        prompt: plan.prompt.clone(),
        completion: plan.completion.clone(),
        metadata: Some(json!({
            "source": provenance,
            "quality": plan.quality,
            "domain": plan.trait_kind,
        })),
    };
    let mut params = json!({
        "personaId": persona_id,
        "personaName": persona_name,
        "baseModel": base_model,
        "traitKind": plan.trait_kind,
        "examples": [example],
        "source": "raw",
    });
    if let Some(eval_set) = crate::cognition::gym::gym_for_trait(&plan.trait_kind) {
        if let serde_json::Value::Object(ref mut map) = params {
            map.insert(
                "evalSet".into(),
                serde_json::Value::String(eval_set.to_string()),
            );
        }
    }
    params
}

/// The shared submit tail used by BOTH producers ([`produce`], [`produce_received`]):
/// build the persona's identity-bearing `Connection`, package the plan via
/// [`build_submit_params`], and dispatch `genome/training-trigger/submit`. Identical
/// flywheel entry for a live turn and a received lesson — only the `provenance` label
/// differs. One submit path, N experience sources.
async fn submit_plan(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    executor: Arc<CommandExecutor>,
    plan: SubmitPlan,
    provenance: &'static str,
) {
    // The persona's own hands: a Connection carrying its LocalPersona identity through
    // the wired executor, so the Privileged submit is gated AS the persona
    // ([[persona-is-a-client]]). `persona_id` IS the airc `peer_id` (equal by invariant).
    let conn = Connection::new(InProcessTransport::new(
        executor,
        Some(CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(persona_id),
        )),
    ));
    let params = build_submit_params(persona_id, &persona_name, &base_model, &plan, provenance);
    if let Err(e) = conn
        .commands()
        .execute_value("genome/training-trigger/submit", params)
        .await
    {
        tracing::warn!(
            persona = %persona_id,
            error = %e,
            "training_producer: submit failed (best-effort, turn unaffected)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the quality gate. A substantive turn produces a plan
    // bucketed by its classified domain; a trivial one-liner ("ok") is gated out
    // (None) so the training corpus is never polluted with acknowledgements. This
    // is the L2 contract: "N graded turns fill the right bucket; low-quality turn
    // gated out" (DEV-TASK-LOOP-CLOSURE-PLAN.md).
    #[test]
    fn substantive_turn_plans_a_bucket_trivial_turn_is_gated() {
        let classifier = DomainClassifier::new();

        // A trivial reply: below MIN_TRAINING_QUALITY → no plan.
        assert!(
            plan(&classifier, "thanks!", "ok").is_none(),
            "a one-word acknowledgement must be gated out of the training corpus"
        );

        // A substantive reply: clears the bar → a plan into a non-empty bucket.
        let long = "Here is how you implement connection pooling for websockets: \
            keep a fixed-size pool of live sockets, hand them out on acquire, return \
            them on release, and health-check idle ones on a timer so a dead socket \
            is replaced before a caller ever sees it. Cap the pool and queue waiters \
            so a burst cannot exhaust file descriptors.";
        let p = plan(&classifier, "How do I pool websockets?", long)
            .expect("a substantive reply must clear the quality gate");
        assert!(!p.trait_kind.is_empty(), "must be bucketed by a domain");
        assert_eq!(p.completion, long, "the reply is the training completion");
        assert!(
            p.quality >= MIN_TRAINING_QUALITY,
            "the carried quality must be the score that cleared the gate: {}",
            p.quality
        );
    }

    // what this catches: the recipe's {trait → gym} edge that lets the AUTOMATIC
    // loop close. A code turn classifies as the `code` trait, which resolves to a
    // committed gym — so the gene the producer dispatches carries an eval_set the
    // L3 sentinel can A/B and adopt. Before this map the producer omitted eval_set
    // and every auto-produced gene was unadoptable (only hand-dispatched jobs
    // closed the loop). The wiring inside `produce` stamps this same lookup onto
    // the submit params.
    #[test]
    fn a_code_turn_auto_produces_a_measurable_gene() {
        let classifier = DomainClassifier::new();
        let code_reply = "Here is the fix: the bug is a null deref in the cargo build \
            script. Add an `if let Some(x) = opt` guard before the `.unwrap()`, return \
            an `Err` with the missing-field name, and the async function compiles and \
            the test passes against the typescript interface.";
        let p = plan(&classifier, "Why does my Rust function panic?", code_reply)
            .expect("a substantive code reply must clear the quality gate");
        assert_eq!(
            p.trait_kind, "code",
            "a code turn must bucket as the code trait"
        );
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&p.trait_kind),
            Some("docs/genome/coder-eval.jsonl"),
            "the code trait must resolve to a measuring gym so the gene is adoptable"
        );
    }

    // what this catches: the being-loop's THIRD axis reaching the live flywheel.
    // plan_received returns a plan UNCONDITIONALLY — no chat-quality gate, because a
    // deliberately-shared lesson's salience is its PROVENANCE (ReceivedSalience), not a
    // score built for live (stimulus→reply) turns. It still classifies a domain (bucket
    // non-empty) so the lesson lands in a gym that MEASURES it — the sentinel then adopts
    // the auto-produced gene only if it lifts the benchmark (#59). This is telepathy
    // reaching weights: a lesson one agent learns becomes another's trained capability.
    #[test]
    fn received_lesson_skips_chat_quality_gate_but_stays_classified() {
        let classifier = DomainClassifier::new();

        // A terse lesson — the kind the live-turn quality gate is designed to drop —
        // still produces a plan, because plan_received returns SubmitPlan, not Option.
        let p = plan_received(&classifier, "airc", "the call room IS the airc room");
        assert_eq!(p.prompt, "airc", "the topic frames the lesson");
        assert_eq!(
            p.completion, "the call room IS the airc room",
            "the lesson is the trained-in completion"
        );
        assert_eq!(
            p.quality, 1.0,
            "provenance IS the quality signal — a deliberately shared lesson is not gated"
        );
        assert!(
            !p.trait_kind.is_empty(),
            "still classified into a bucket so it maps to a measuring gym"
        );
    }
}
