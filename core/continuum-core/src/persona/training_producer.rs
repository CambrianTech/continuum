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

/// WHICH HAND a citizen had in the card the turn is credited to (card cc34ac0f).
///
/// The three are not interchangeable and must not collapse into one "contributor"
/// bucket: writing the patch, judging someone else's patch, and naming the defect
/// in the first place are different skills, and a curriculum that mixes them
/// teaches none of them. The role is half the bucket key for exactly that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditRole {
    /// Authored the submission — the submission's `publisher`.
    Owner,
    /// Moved the review card: judged another citizen's work.
    Reviewer,
    /// Named the defect the card exists for.
    Finder,
}

impl CreditRole {
    /// Stable wire/bucket spelling. Lowercase because it is concatenated into a
    /// `traitKind` and read back by humans in probe rows.
    pub fn as_str(self) -> &'static str {
        match self {
            CreditRole::Owner => "owner",
            CreditRole::Reviewer => "reviewer",
            CreditRole::Finder => "finder",
        }
    }
}

/// A VERIFIED outcome attached to a turn: this turn helped produce card X, in role
/// R, and the card's verdict was `passed`.
///
/// The card's own title is the argument for its existence — "a persona's curriculum
/// carries verified outcomes instead of self-assessed noteworthiness". Without a
/// stamp, [`score_interaction_quality`] is handed `task_outcome: None` and falls
/// back to a NEUTRAL 0.5 `task_success`, so a turn that shipped a merged fix and a
/// turn that produced nothing score identically on the factor that should separate
/// them most. The stamp is what turns "the citizen wrote something substantive"
/// into "the citizen wrote something that WORKED".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeStamp {
    /// The card this turn is credited to.
    pub card_id: Uuid,
    /// Which hand the citizen had in it.
    pub role: CreditRole,
    /// The card's settled verdict. `true` = the grade passed.
    pub outcome: bool,
}

/// A scored, gated, classified training example ready to submit. The pure product
/// of [`plan`] — it separates the JUDGMENT (is this turn worth training on? which
/// domain bucket?) from the EFFECT (dispatch to the trigger), so the judgment is
/// unit-testable with no executor.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitPlan {
    /// The domain bucket — `DomainClassifier::classify(...).domain`. Becomes the
    /// `traitKind` of the `(persona_id, trait_kind, base_model)` bucket key when
    /// UNSTAMPED; a stamped plan keys on `domain × role × outcome` (see
    /// [`SubmitPlan::bucket_key`]).
    pub trait_kind: String,
    /// The stimulus (the triggering message text).
    pub prompt: String,
    /// The persona's reply.
    pub completion: String,
    /// The interaction-quality score that cleared [`MIN_TRAINING_QUALITY`]
    /// (carried into example metadata as training provenance).
    pub quality: f32,
    /// The verified outcome, when this turn is credited to a settled card.
    /// `None` for an ordinary live turn — the pre-cc34ac0f behaviour, unchanged.
    pub stamp: Option<OutcomeStamp>,
}

impl SubmitPlan {
    /// The bucket this example trains. UNSTAMPED turns keep the bare domain, so
    /// every existing bucket keeps its name and its history; a stamped turn gets
    /// `domain/role`, so a citizen's "code she wrote that passed" is a different
    /// lesson from "review she did that passed". There is NO outcome segment:
    /// a failed turn is refused by [`plan`] before it can be filed anywhere.
    ///
    /// Deliberately NOT a new field on the params: the bucket key is already
    /// `traitKind`, and widening the key rather than adding a parallel dimension
    /// keeps one bucket concept instead of two ([[logic-deepest-level-thin-on-the-way-out]]).
    pub fn bucket_key(&self) -> String {
        match self.stamp {
            None => self.trait_kind.clone(),
            // No `/passed` | `/failed` segment: a FAILED turn never reaches a
            // bucket at all (see the evidence floor in [`plan`]), so every plan
            // that gets this far already carries a passing verdict. Encoding an
            // outcome that is now constant would advertise a distinction the
            // data no longer has — and would invite a `/failed` sibling to be
            // reintroduced as "just another bucket".
            Some(s) => format!("{}/{}", self.trait_kind, s.role.as_str()),
        }
    }
}

/// Score the turn, gate on quality, classify its domain → `Some(SubmitPlan)`;
/// `None` if gated out (trivial / low-quality). Pure: no I/O, no executor, no
/// spawn — this is the testable judgment half of the producer.
///
/// `classifier` is borrowed (the shared [`DomainClassifier`]). Classification runs
/// on the full `(prompt, completion)` pair so both the ask and the answer inform
/// the bucket.
///
/// `stamp` carries a VERIFIED outcome when the turn is credited to a settled card
/// (cc34ac0f). It feeds `score_interaction_quality`'s `task_outcome` — the argument
/// that has always existed and was always `None` on this path. A live, uncredited
/// turn still passes `None` and scores exactly as it did before.
pub fn plan(
    classifier: &DomainClassifier,
    prompt: &str,
    completion: &str,
    stamp: Option<OutcomeStamp>,
) -> Option<SubmitPlan> {
    // THE EVIDENCE FLOOR. A turn credited to a card that FAILED submits NOTHING.
    //
    // This is deliberately not "file it under `/failed` and let training sort it
    // out", which is what an earlier draft of this change did. The reason is the
    // shape of the corpus, not squeamishness: `TrainingExample` is
    // `{prompt, completion}` (genome::fine_tuning::types) and the writers render
    // exactly those two fields — there is NO `chosen`/`rejected` schema anywhere
    // in the tree. So every example in every bucket is an SFT TARGET. A "failed"
    // bucket does not teach "avoid this"; it teaches the failure, under a name
    // that makes it look handled.
    //
    // Until a preference-pair schema exists, the only honest thing a verified
    // failure can do is keep quiet. The stamp still rides on the plan's metadata
    // for attribution, so a failure remains ACCOUNTED FOR even though it is not
    // TRAINED ON — which is the distinction the `/failed` bucket blurred.
    if stamp.is_some_and(|s| !s.outcome) {
        return None;
    }
    let quality = score_interaction_quality(prompt, completion, None, stamp.map(|s| s.outcome));
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
        stamp,
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
        // A LIVE turn carries no verdict — nothing has settled yet. `None` here is
        // the pre-cc34ac0f path, byte-identical. The stamped path is
        // [`produce_stamped`], driven from settle, not from the turn.
        let Some(plan) = plan(classifier, &prompt, &completion, None) else {
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
        // A RECEIVED lesson is somebody else's settled experience arriving as
        // teaching. It has no card in THIS citizen's history to credit, and
        // borrowing the teacher's verdict would credit the student for an outcome
        // she did not produce — the confabulation this card exists to end.
        stamp: None,
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
    // `domain` stays the BARE domain in metadata even when the bucket key widens:
    // the domain is what the example is ABOUT, the key is where it is filed. Losing
    // the bare domain here would make a stamped example unqueryable alongside its
    // unstamped siblings.
    let mut metadata = json!({
        "source": provenance,
        "quality": plan.quality,
        "domain": plan.trait_kind,
    });
    if let (Some(stamp), serde_json::Value::Object(map)) = (plan.stamp, &mut metadata) {
        map.insert("cardId".into(), json!(stamp.card_id));
        map.insert("role".into(), json!(stamp.role.as_str()));
        map.insert("outcome".into(), json!(stamp.outcome));
    }
    let example = TrainingExample {
        prompt: plan.prompt.clone(),
        completion: plan.completion.clone(),
        metadata: Some(metadata),
    };
    let bucket = plan.bucket_key();
    let mut params = json!({
        "personaId": persona_id,
        "personaName": persona_name,
        "baseModel": base_model,
        "traitKind": bucket,
        "examples": [example],
        "source": "raw",
    });
    // Gym lookup keys on the DOMAIN, not the widened bucket: `code/owner/passed`
    // has no gym of its own, and the trait it measures is still `code`. Passing the
    // bucket here would silently drop the eval set for every stamped example, which
    // is the [[no-fallbacks-ever]] shape — a capability quietly lost, not refused.
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
    // what this catches (card cc34ac0f): a settled card must label the turns that
    // produced it, and the label must reach BOTH the bucket key and the example
    // metadata — otherwise the curriculum keeps grading itself on self-assessed
    // noteworthiness. Also pins the two regressions this change could cause:
    // an UNSTAMPED turn must key exactly as before, and the gym lookup must keep
    // using the BARE domain (a widened key has no gym, so keying on it would
    // silently drop the eval set from every credited example).
    #[test]
    fn a_stamped_turn_keys_on_domain_role_a_failed_one_submits_nothing_unstamped_is_unchanged() {
        let classifier = DomainClassifier::new();
        let long = "Pool them behind one supervised task and hand out permits; \
                    a websocket per request will exhaust file descriptors long \
                    before it exhausts memory, and the reconnect storm is worse \
                    than the original load. Bound the pool and queue the overflow.";

        let unstamped = plan(&classifier, "How do I pool websockets?", long, None)
            .expect("test: a substantive turn plans a bucket");
        assert_eq!(
            unstamped.bucket_key(),
            unstamped.trait_kind,
            "an unstamped live turn must key on the bare domain exactly as before cc34ac0f"
        );

        let card = Uuid::from_u128(0xcc34ac0f);
        let stamp = OutcomeStamp {
            card_id: card,
            role: CreditRole::Owner,
            outcome: true,
        };
        let stamped = plan(&classifier, "How do I pool websockets?", long, Some(stamp))
            .expect("test: a stamped turn still clears the quality gate");
        assert_eq!(
            stamped.bucket_key(),
            format!("{}/owner", stamped.trait_kind),
            "a stamped turn keys on domain x role — there is no outcome segment, \
             because a failed turn never reaches a bucket at all"
        );

        // what this catches: the `/failed` bucket coming back. An earlier draft of
        // cc34ac0f filed a failed outcome under `domain/role/failed`, reasoning that
        // "learning what did not work is the other half of the curriculum". That is
        // true of a PREFERENCE corpus and false of this one: `TrainingExample` is
        // `{prompt, completion}` with no chosen/rejected anywhere in the tree, so
        // every bucket is an SFT target and a `/failed` bucket TRAINS THE FAILURE.
        // Until a preference-pair schema exists, a verified failure submits nothing.
        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                long,
                Some(OutcomeStamp {
                    outcome: false,
                    role: CreditRole::Reviewer,
                    ..stamp
                }),
            )
            .is_none(),
            "a turn credited to a FAILED card must produce no training example — \
             not a differently-named bucket"
        );

        // The verdict must reach the example itself, not just the key.
        let params = build_submit_params(Uuid::from_u128(7), "Cass", "qwen", &stamped, "live-turn");
        let meta = &params["examples"][0]["metadata"];
        assert_eq!(meta["cardId"], json!(card));
        assert_eq!(meta["role"], json!("owner"));
        assert_eq!(meta["outcome"], json!(true));
        assert_eq!(
            meta["domain"], json!(stamped.trait_kind),
            "metadata keeps the BARE domain so a stamped example stays queryable \
             alongside its unstamped siblings"
        );
        assert_eq!(
            params["traitKind"],
            json!(stamped.bucket_key()),
            "the widened bucket is what the example is filed under"
        );
        // The regression that would be invisible: gym lookup keys on the domain.
        // If this ever reads the widened bucket, every credited example loses its
        // eval set silently.
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&stamped.trait_kind).is_some(),
            params.get("evalSet").is_some(),
            "evalSet presence must follow the BARE domain's gym, not the widened bucket"
        );
    }

    #[test]
    fn substantive_turn_plans_a_bucket_trivial_turn_is_gated() {
        let classifier = DomainClassifier::new();

        // A trivial reply: below MIN_TRAINING_QUALITY → no plan.
        assert!(
            plan(&classifier, "thanks!", "ok", None).is_none(),
            "a one-word acknowledgement must be gated out of the training corpus"
        );

        // A substantive reply: clears the bar → a plan into a non-empty bucket.
        let long = "Here is how you implement connection pooling for websockets: \
            keep a fixed-size pool of live sockets, hand them out on acquire, return \
            them on release, and health-check idle ones on a timer so a dead socket \
            is replaced before a caller ever sees it. Cap the pool and queue waiters \
            so a burst cannot exhaust file descriptors.";
        let p = plan(&classifier, "How do I pool websockets?", long, None)
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
        let p = plan(&classifier, "Why does my Rust function panic?", code_reply, None)
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
