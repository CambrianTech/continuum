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
use continuum_client::{ClientError, Connection, Transport};

use crate::commands::training_trigger::submit::SubmitOutcome;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// WHAT ACTUALLY SERVED the generation — not what the profile asked for (card 0d51573a).
///
/// `TextGenerationResponse` carries `model`, `provider` and `request_id`; both
/// producer call sites throw them away and pass `ctx.profile.model_id`, the
/// CONFIGURED model. Those are two different facts and the bucket key
/// `(persona_id, trait_kind, base_model)` wants the second one: any fallback,
/// adapter route or lane substitution files the example under a base that did not
/// generate it. **A curriculum bucketed by a base model that never produced its
/// examples is worse than no bucket** — the genome loop pages a gene against a
/// benchmark for weights it was not trained from. Same one-value-two-meanings
/// shape as the rest of this class; the fix is to carry the served fact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServedProvenance {
    /// `TextGenerationResponse::model` — what answered, not what was configured.
    pub model: String,
    /// `TextGenerationResponse::provider` — which adapter served it.
    pub provider: String,
    /// `TextGenerationResponse::request_id` — the join back to the generation.
    pub request_id: String,
}

/// Credit CAPTURED AT SELECTION, before any verdict exists (card 0d51573a).
///
/// Taken from the projected winning `WorkCard` at `act_question` selection, so
/// ownership is READ from the accepted claim rather than inferred from a time
/// window — no event-store rescan, no `persona_id == peer_id` invariant load-bearing
/// (that equality is prose at this module's line ~444, not a type).
///
/// **THERE IS DELIBERATELY NO OUTCOME FIELD, AND NO PLACEHOLDER FOR ONE.** The link
/// ("this turn belongs to card X") and the verdict ("card X worked") are two facts
/// at two different times, and collapsing them is exactly the defect this card
/// exists to prevent: an ungraded work-in-progress trace treated as outcome-verified
/// credit. Observed live 2026-09-08 — a citizen accumulating toward a training
/// threshold on a card still in `Claimed` with no settlement. Such a turn must stay
/// an ordinary UNSTAMPED example in the bare-domain bucket; capturing the link is
/// never license to stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedCredit {
    /// The card she was rooted at when this turn began. Its PRESENCE is what makes
    /// the turn card-linked, and card-linked is what makes it STAGE.
    pub card_id: Uuid,
    /// The accepted claim, captured at the same instant as `card_id` — what makes
    /// this attribution rather than a guess. `WorkCard::owner` and `claim_id` are
    /// both `Option`, so a rooted turn on a claimless card yields `None` here.
    ///
    /// **A `None` NEVER falls through to an immediate unlinked submission, and is
    /// NEVER filled in later from a global.** The turn is still card-linked (she
    /// was rooted at that checkout; her edits landed there), so it stages — it
    /// simply can never be stamped, and expires unsubmitted if nothing settles.
    /// A later settlement cannot retroactively invent a selection receipt that was
    /// not taken, and the type is shaped so it cannot try.
    pub claim: Option<ClaimReceipt>,
}

/// The claim receipt captured AT SELECTION — the evidence that this citizen was
/// accountable for this card at the moment the turn began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClaimReceipt {
    /// `WorkCard::claim_id` as accepted.
    pub claim_id: Uuid,
    /// `WorkCard::owner` as the work store spells it: a typed peer, not a bare Uuid.
    pub owner: airc_core::PeerId,
    /// Which hand this citizen had in the card. A claim yields `Owner`; `Reviewer`
    /// and `Finder` are NOT derivable from a claim alone and need their own seams.
    pub role: CreditRole,
}

/// A REAL settlement verdict — the second fact, arriving later than [`CapturedCredit`].
///
/// **A TERMINAL CARD STATE IS NOT A VERDICT.** `is_terminal_card_state` accepts
/// `closed | done | merged`; `merged` implies success but `closed` covers abandonment,
/// so deriving the outcome from the state string would collapse "settled" into
/// "succeeded" — the same conflation, one layer down. This type therefore carries a
/// verdict a grader actually produced, and there is no constructor from a state name.
#[derive(Debug, Clone, PartialEq)]
pub struct SettlementVerdict {
    passed: bool,
    /// The judge's normalized score, kept for the staged record.
    score: f64,
    /// WHY. `activity::Verdict` calls this "the receipt a citizen and a human both
    /// read, and the text the curriculum keeps when the task failed" — so a
    /// settlement that discards it throws away the only part a failure leaves behind.
    reason: String,
}

impl SettlementVerdict {
    /// The ONLY constructor, and it takes an activity's ACTUAL judgment.
    ///
    /// Deliberately not `from_graded(bool)`: a bool is something a caller asserts,
    /// and "clean turn / Closed is not success proof" (Astra, 2026-09-08). A card
    /// reaching a terminal state is not evidence that the work succeeded —
    /// `is_terminal_card_state` accepts `closed | done | merged` and `closed` covers
    /// abandonment. Requiring a [`crate::cognition::activity::Verdict`] means the
    /// evidence is produced by an `ActivityAdapter::judge`, so "I settled it because
    /// the board said Closed" is not expressible here.
    pub fn from_activity(verdict: &crate::cognition::activity::Verdict) -> Self {
        Self {
            passed: verdict.passed,
            score: verdict.score,
            reason: verdict.reason.clone(),
        }
    }

    pub fn passed(&self) -> bool {
        self.passed
    }

    pub fn score(&self) -> f64 {
        self.score
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl CapturedCredit {
    /// The ONLY way to obtain an [`OutcomeStamp`]. Consuming a [`SettlementVerdict`]
    /// is what makes "stamp a card that has not settled" not a bug you can write but
    /// a value you cannot produce — the same move as making an illegal state
    /// unrepresentable rather than guarding against it at every call site.
    ///
    /// A FAILED verdict still produces a stamp: the evidence floor in [`plan`] is
    /// what refuses it for SFT, and failure experience persists through the ordinary
    /// experience/engram path. Refusing to stamp a failure here would hide the
    /// failure instead of merely declining to train on it.
    ///
    /// `None` when no [`ClaimReceipt`] was captured at selection. **This is the
    /// only place that decision can be made, which is the point:** the receipt is
    /// an input to construction, so no settlement — however well-verified — can
    /// supply one after the fact. A staged turn with no receipt expires
    /// unsubmitted rather than being credited on a later guess.
    pub fn settle(self, verdict: SettlementVerdict) -> Option<OutcomeStamp> {
        let claim = self.claim?;
        Some(OutcomeStamp {
            card_id: self.card_id,
            role: claim.role,
            outcome: verdict.passed(),
        })
    }

    /// Card-linked turns STAGE; only truly unlinked conversation submits at
    /// completion. Presence of the card — not of the claim — is the test, because
    /// she was rooted at that checkout and her edits landed there regardless of
    /// whether a claim receipt was recorded.
    pub fn is_card_linked(&self) -> bool {
        // Constructing this value at all means a card was captured at selection.
        // Kept as a named predicate so the staging call site reads as a decision
        // rather than as `Option::is_some` on something whose meaning is elsewhere.
        !self.card_id.is_nil()
    }
}

/// ONE card-linked turn, held until its card settles (card 0d51573a).
///
/// A card-linked turn must NOT submit at completion. An unstamped example is an
/// immediate POSITIVE SFT target and submission is one-way — `TrainingExample` is
/// `{prompt, completion}` with no preference schema anywhere in the tree — so a turn
/// submitted before its card settles cannot be withdrawn when that card later fails.
/// Observed live 2026-09-08: a citizen accumulating toward a training threshold on a
/// card still in `Claimed` with no settlement.
///
/// Lives in the citizen's own [`crate::persona::home::PersonaHome`], alongside
/// `engrams.sqlite` — persistence inside its existing owner, per-citizen, NOT a new
/// global queue ([[source-drain-is-the-universal-pattern]]: the drain is settlement).
///
/// ## Two write-once halves, at two different times
///
/// **SELECTION IS IMMUTABLE.** `card_id`, `claim_id`, `owner`, `role` and
/// `submitted_request_id` are written when the turn begins and never rewritten. A
/// MISSING CLAIM REMAINS MISSING: `claim_id` is `None` for a turn rooted at a card
/// whose `WorkCard::owner`/`claim_id` were absent, and no later settlement may fill
/// it in. That turn still stages (it was card-linked) and can never be stamped.
///
/// **SERVED PROVENANCE IS ATTACHED LATER**, once the generation returns, and only
/// ever added — never overwriting a selection field.
///
/// ## Why the id IS the submission id
///
/// Per the #278 contract the submit gains an optional `submissionId` minted ONCE and
/// reused on every retry, so the destination can recognise a replay. Making it this
/// row's primary key means there is no second identifier to keep in sync, and a retry
/// necessarily reuses it because it is reading the same row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "staged_credit")]
pub struct StagedCredit {
    /// The `submissionId` carried to the training-trigger, minted once at stage time
    /// and reused on every retry. Primary key, so the derive pulls in the BaseEntity
    /// columns and a retry cannot mint a second one for the same batch.
    #[entity(primary_key)]
    pub id: Uuid,

    /// The card this turn is credited to. Indexed: settlement arrives per-card and
    /// asks "what did this citizen stage against it".
    #[entity(indexed)]
    pub card_id: Uuid,

    /// The accepted claim captured at SELECTION. `None` means no receipt was taken
    /// and none may ever be supplied — this row can stage but never stamp.
    #[entity(indexed)]
    pub claim_id: Option<Uuid>,

    /// The claim's owner, as the work store spells it. `None` exactly when
    /// `claim_id` is `None`; the two travel together or not at all.
    pub owner: Option<Uuid>,

    /// Which hand the citizen had in the card. JSON because it is a tagged enum.
    #[entity(json)]
    pub role: Option<CreditRole>,

    /// **THE JOIN KEY**: `TextGenerationRequest.request_id` as SUBMITTED, assigned or
    /// retained once before submit and shared with the Mind capture's `CycleId`. The
    /// adapter's response id, when it differs, lives in `served` and NEVER replaces
    /// this — otherwise the correlation key is silently swapped for one the capture
    /// never saw, and playback breaks precisely on a lane substitution.
    #[entity(indexed)]
    pub submitted_request_id: String,

    /// What ACTUALLY served the generation, attached once the response returns.
    /// `None` until then. Never overwrites a selection field.
    #[entity(json)]
    pub served: Option<ServedProvenance>,

    /// The stimulus, held verbatim so the staged example is the turn she was
    /// actually handed rather than a re-derived approximation.
    pub prompt: String,

    /// Her reply, likewise verbatim.
    pub completion: String,

    /// When this turn was staged (epoch ms). Not a settlement clock — a staged row
    /// whose card never settles is never submitted, and that is correct, not a leak.
    pub staged_at_ms: u64,
}

/// A scored, gated, classified training example ready to submit. The pure product
/// of [`plan`] — it separates the JUDGMENT (is this turn worth training on? which
/// domain bucket?) from the EFFECT (dispatch to the trigger), so the judgment is
/// unit-testable with no executor.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitPlan {
    /// The domain bucket — `DomainClassifier::classify(...).domain`. Becomes the
    /// `traitKind` of the `(persona_id, trait_kind, base_model)` bucket key when
    /// UNSTAMPED; a stamped plan keys on `domain × role` (see
    /// [`SubmitPlan::bucket_key`]). There is no outcome segment — a failed turn
    /// never reaches a bucket at all.
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
    // failure can do is keep quiet.
    //
    // AND KEEPING QUIET IS ALL IT DOES — a refused turn leaves NO TRACE. There is
    // no plan, so no metadata, no bucket, and no probe. An earlier version of this
    // comment claimed the stamp "still rides on the plan's metadata, so a failure
    // remains ACCOUNTED FOR"; that was false, because there is no plan to carry it.
    // Whether a refused turn SHOULD leave durable provenance is an open question
    // and belongs at the caller that supplies the stamp, not in this pure function
    // (astra-s6-review: a probe here would pollute pure planning, and a probe is
    // glass-box evidence rather than durable accounting — it can be disabled).
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
        // A LIVE turn carries no verdict — nothing has settled yet, so `None` here
        // is the pre-cc34ac0f path, byte-identical.
        //
        // THIS IS THE ONLY NON-TEST CALL SITE, and it always passes `None`. There is
        // no `produce_stamped` and nothing carries a settled card's verdict into this
        // module, so the stamped path — including the evidence floor in [`plan`] — is
        // UNREACHABLE in production today. An earlier version of this comment named
        // `produce_stamped` as if it existed; it does not. Card 0d51573a owns minting
        // the link (which turns produced which card, in which role) that a stamped
        // caller would need before it could truthfully stamp anything.
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
    // Gym lookup keys on the DOMAIN, not the widened bucket: `code/owner` has no
    // gym of its own, and the trait it measures is still `code`. Passing the bucket
    // here would silently drop the eval set for every stamped example, which is the
    // [[no-fallbacks-ever]] shape — a capability quietly lost, not refused. The test
    // asserts this positively (`gym_for_trait(bucket_key()) == None`) rather than by
    // comparing two `is_some()`s, which is how it managed to guard nothing for so long.
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
    match submit_training(&conn, params).await {
        Ok(receipt) if receipt.success => {
            crate::probe!(
                class = "training.example.produced",
                persona = %persona_name,
                domain = %plan.trait_kind,
                quality = plan.quality as f64,
                provenance = provenance,
                outcome = %receipt.outcome.as_deref().unwrap_or("unspecified"),
                "training trigger accepted the example (L2)"
            );
        }
        Ok(receipt) => {
            crate::probe!(
                class = "training.example.rejected",
                persona = %persona_name,
                domain = %plan.trait_kind,
                provenance = provenance,
                error_kind = %receipt.error_kind.as_deref().unwrap_or("unspecified"), // Missing diagnostic stays explicit after the receipt's success:false refusal.
                error = %receipt.error.as_deref().unwrap_or("submit returned success:false"), // Display-only absence label; the typed receipt remains unchanged.
                "training trigger did not report successful submission"
            );
            tracing::warn!(
                persona = %persona_id,
                error_kind = ?receipt.error_kind,
                error = ?receipt.error,
                "training_producer: submit refused (best-effort, turn unaffected)"
            );
        }
        Err(error) => {
            crate::probe!(
                class = "training.example.failed",
                persona = %persona_name,
                domain = %plan.trait_kind,
                provenance = provenance,
                error = %error,
                "training submission has no valid receipt"
            );
            tracing::warn!(
                persona = %persona_id,
                error = %error,
                "training_producer: submit failed (best-effort, turn unaffected)"
            );
        }
    }
}

/// Decode the command's outcome once at the typed boundary. Transport success is
/// not acceptance: InconsistentBucket and DispatchFailed deliberately return Ok
/// with success:false. All producers, including consolidation, inspect this receipt.
pub(crate) async fn submit_training<T: Transport>(
    conn: &Connection<T>,
    params: serde_json::Value,
) -> Result<SubmitOutcome, ClientError> {
    let receipt = conn
        .commands()
        .execute_value("genome/training-trigger/submit", params)
        .await?;
    Ok(serde_json::from_value(receipt)?) // Decode the Value-native command response once; producers share this typed receipt.
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: card 3eaabbc6 — a successful transport can carry a
    // refused submit; absent/malformed receipts must never look like acceptance.
    #[tokio::test]
    async fn training_submission_preserves_refusal_and_requires_a_receipt() {
        use continuum_client::mock::MockTransport;
        let transport = MockTransport::new();
        for value in [
            json!({"success": true, "outcome": "BatchAppended", "currentCount": 1, "threshold": 16}),
            json!({"success": false, "errorKind": "InconsistentBucket", "error": "different policy"}),
            json!({"success": false, "errorKind": "DispatchFailed", "error": "provider unavailable"}),
            json!({"outcome": "BatchAppended"}),
        ] {
            transport.respond_to("genome/training-trigger/submit", move |_| Ok(value.clone()));
        }
        let conn = Connection::new(transport);
        let accepted = submit_training(&conn, json!({}))
            .await
            .expect("accepted receipt");
        assert!(accepted.success);
        assert_eq!(accepted.current_count, Some(1));
        for kind in ["InconsistentBucket", "DispatchFailed"] {
            let refused = submit_training(&conn, json!({}))
                .await
                .expect("refusal is data");
            assert!(!refused.success);
            assert_eq!(refused.error_kind.as_deref(), Some(kind));
            assert!(refused.error.is_some());
        }
        assert!(
            submit_training(&conn, json!({})).await.is_err(),
            "missing success is malformed"
        );
        assert!(
            submit_training(&conn, json!({})).await.is_err(),
            "transport failure stays a failure"
        );
    }

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
    //
    // what this catches: capture-time credit being treated as outcome-verified, and
    // `settle` ignoring the verdict it was handed. Card 0d51573a. Observed live
    // 2026-09-08 — a citizen accumulating toward a training threshold on a card still
    // in `Claimed` with no settlement, which must stay unstamped.
    //
    // THE FIXTURE MUST CLEAR THE QUALITY GATE ON ITS OWN (the cc34ac0f lesson below):
    // a FAILED outcome scores `0.20 + 0.3*substance`, so only the ≥500-char substance
    // step reaches 0.47 and clears MIN_TRAINING_QUALITY 0.45. With a shorter fixture
    // the gate refuses the turn, `plan` returns `None` for the wrong reason, and the
    // failed-case assertion passes while proving nothing.
    // what this catches: a dropped `#[entity(indexed)]` on the staging keys. Field
    // presence alone is not the claim — settlement arrives PER CARD and asks "what
    // did this citizen stage against it", and playback joins on the SUBMITTED
    // request id. Without those indexes both become table scans on a store that
    // grows one row per card-linked turn, and nothing anywhere reports it: the
    // queries still return the right answers, just slower forever. Card 0d51573a.
    #[test]
    fn staged_credit_indexes_the_three_keys_settlement_and_playback_join_on() {
        use crate::orm::OrmEntity;
        let schema = StagedCredit::collection_schema();
        assert_eq!(schema.collection, "staged_credit");

        let indexed: std::collections::BTreeSet<&str> = schema
            .fields
            .iter()
            .filter(|f| f.indexed)
            .map(|f| f.name.as_str())
            .collect();

        for key in ["cardId", "claimId", "submittedRequestId"] {
            assert!(
                indexed.contains(key),
                "staged_credit must INDEX {key:?} — settlement looks up by card and \
                 playback joins on the submitted request id; indexed fields are {indexed:?}"
            );
        }

        // POSITIVE CONTROL: a field I deliberately did NOT index must not be
        // indexed, or this test would pass with `indexed` blanket-set on everything
        // and would be asserting nothing about my attributes.
        assert!(
            !indexed.contains("prompt"),
            "prompt must not be indexed — if it is, this test cannot distinguish my \
             three deliberate indexes from every field being indexed by default"
        );
    }

    // what this catches: the staging collection colliding with an existing one.
    // `OrmEntity::COLLECTION` must be unique across BOTH the Rust registry and
    // entity_schemas.json, and a collision is a registration-time hard error — i.e.
    // it fails at BOOT, on a citizen's machine, not here, unless this test exists.
    #[test]
    fn staged_credit_registers_without_colliding_with_an_existing_collection() {
        use crate::orm::entity::OrmEntityRegistry;
        let registry = OrmEntityRegistry::new();
        registry
            .register::<StagedCredit>()
            .expect("StagedCredit must register cleanly");
        let resolved = registry
            .resolve("staged_credit")
            .expect("staged_credit collection must resolve");
        assert_eq!(resolved.collection, "staged_credit");
    }

    #[test]
    fn a_verdict_is_required_to_stamp_and_settle_carries_the_verdict_it_was_given() {
        let classifier = DomainClassifier::new();
        let substantial = "Pool them behind one supervised task and hand out permits; a websocket \
             per request will exhaust file descriptors long before it exhausts memory, and the \
             reconnect storm is worse than the original load. Bound the pool and queue the \
             overflow, then shed load at the queue rather than at accept() — a refused connection \
             the caller can retry is cheaper than a half-open socket nobody owns. Size the permit \
             count from the descriptor ceiling, not from a guess, and make the queue depth the \
             knob you actually tune under pressure.";
        assert!(
            substantial.len() >= 500,
            "fixture must clear the substance ladder's 500-char step, or the failed \
             case returns None from the quality gate and the floor is never exercised"
        );

        let captured = CapturedCredit {
            card_id: Uuid::from_u128(0x0d51573a),
            claim: Some(ClaimReceipt {
                claim_id: Uuid::from_u128(0x457b0d7d),
                owner: airc_core::PeerId::from_u128(0xe2f0e022),
                role: CreditRole::Owner,
            }),
        };

        // A PASSING verdict stamps and buckets on domain x role.
        let passed = captured
            .clone()
            .settle(SettlementVerdict::from_activity(&crate::cognition::activity::Verdict::pass("tests pass on the staged checkout")))
            .expect("test: a captured claim receipt can be stamped");
        assert!(passed.outcome, "settle must carry a PASSING verdict through");
        let planned = plan(&classifier, "How do I pool websockets?", substantial, Some(passed))
            .expect("test: a passing stamped turn clears the gate and plans a bucket");
        assert_eq!(
            planned.bucket_key(),
            format!("{}/owner", planned.trait_kind),
            "a settled passing turn keys on domain x role"
        );

        // A FAILING verdict must still STAMP (hiding the failure is not the job) and
        // must then be refused by the evidence floor. If `settle` hardcoded `true`,
        // this plans a bucket and the assertion below fails — that is the mutation.
        let failed = captured
            .settle(SettlementVerdict::from_activity(&crate::cognition::activity::Verdict::fail("the patch did not apply")))
            .expect("test: a receipt stamps regardless of which way the verdict went");
        assert!(
            !failed.outcome,
            "settle must carry a FAILING verdict through rather than defaulting to success"
        );
        assert!(
            plan(&classifier, "How do I pool websockets?", substantial, Some(failed)).is_none(),
            "a turn credited to a FAILED card submits nothing — same text plans fine \
             when the verdict passes, so this None can only come from the evidence floor"
        );

        // what this catches: a settlement inventing the selection receipt it never
        // got. A turn rooted at a card whose `owner`/`claim_id` were absent is still
        // CARD-LINKED (so it stages, and must never fall through to immediate
        // unlinked SFT), but it can never be stamped — not by a passing verdict,
        // not by any verdict. If `settle` ever defaulted the missing receipt to an
        // Owner role, both assertions below flip.
        let claimless = CapturedCredit {
            card_id: Uuid::from_u128(0x0d51573a),
            claim: None,
        };
        assert!(
            claimless.is_card_linked(),
            "a rooted turn on a claimless card is still card-linked — it stages, \
             because her edits landed in that checkout either way"
        );
        assert!(
            claimless.settle(SettlementVerdict::from_activity(&crate::cognition::activity::Verdict::pass("tests pass on the staged checkout"))).is_none(),
            "no receipt captured at selection means no stamp is constructible later — \
             a settlement cannot retroactively invent accountability"
        );
    }

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
        //
        // THE FIXTURE MUST CLEAR THE QUALITY GATE ON ITS OWN, or this test proves
        // nothing. `score_interaction_quality` weights `task_success` at 0.25 and
        // scores a failed outcome 0.2, so a failed turn scores `0.20 + 0.3*substance`
        // — and `long` above (237 chars → substance 0.7) lands at 0.41, BELOW
        // MIN_TRAINING_QUALITY 0.45. With that fixture the gate refuses the turn and
        // the evidence floor is never reached: deleting the floor entirely left this
        // assertion green. Found by mutation, not by reading.
        //
        // `substantial` is ≥500 chars → substance 0.9 → 0.47, which CLEARS the gate.
        // The positive control below proves it: same text, outcome `true`, plans a
        // bucket. So `None` for the failed case can only come from the floor.
        let substantial = "Pool them behind one supervised task and hand out permits; a websocket \
             per request will exhaust file descriptors long before it exhausts memory, and the \
             reconnect storm is worse than the original load. Bound the pool and queue the \
             overflow, then shed load at the queue rather than at accept() — a refused connection \
             the caller can retry is cheaper than a half-open socket nobody owns. Size the permit \
             count from the descriptor ceiling, not from a guess, and make the queue depth the \
             knob you actually tune under pressure.";
        assert!(
            substantial.len() >= 500,
            "fixture must clear the substance ladder's 500-char step, or the quality \
             gate refuses the turn and this test cannot see the floor at all"
        );

        // POSITIVE CONTROL: the same text with a PASSING verdict must still plan.
        // Without this, a fixture that happened to be gated out would make the
        // failed-case assertion vacuous — which is exactly the bug this block had.
        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
                Some(OutcomeStamp {
                    outcome: true,
                    role: CreditRole::Reviewer,
                    ..stamp
                }),
            )
            .is_some(),
            "control: this fixture clears the quality gate when the verdict PASSES, \
             so a None below is the evidence floor and not the gate"
        );

        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
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
        // The regression that would be invisible: the gym lookup keys on the BARE
        // domain. If it ever reads the widened bucket, every credited example loses
        // its eval set silently.
        //
        // THE OLD VERSION OF THIS CHECK NEVER GUARDED ANYTHING, and it took Astra's
        // review of 17e8c2f37 plus a precondition assert to prove it. It compared
        // `gym_for_trait(trait_kind).is_some()` against `params["evalSet"].is_some()`
        // — but `build_submit_params` inserts `evalSet` IFF that same lookup is Some,
        // and the fixture above classifies as `conversation`, WHICH HAS NO GYM. Both
        // sides were `false`, so it passed as `false == false`, identically whether
        // the production lookup used the bare domain or the widened bucket. The exact
        // regression it was written for could not have failed it.
        //
        // So this is re-anchored on a fixture that MAPS TO A REAL GYM. `code` is the
        // one trait with a committed eval set, and the path is asserted literally
        // rather than as `is_some()`: "there is some gym" is the weaker claim that let
        // the tautology hide.
        let code_reply = "Here is the fix: the bug is a null deref in the cargo build \
            script. Add an `if let Some(x) = opt` guard before the `.unwrap()`, return \
            an `Err` with the missing-field name, and the async function compiles and \
            the test passes against the typescript interface.";
        let code_stamped = plan(
            &classifier,
            "Why does my Rust function panic?",
            code_reply,
            Some(stamp),
        )
        .expect("a substantive code reply with a PASSING verdict must plan");
        assert_eq!(
            code_stamped.trait_kind, "code",
            "fixture must classify as `code`, or the eval-path assertions below go \
             vacuous the way the `conversation` fixture did"
        );
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&code_stamped.trait_kind),
            Some("docs/genome/coder-eval.jsonl"),
            "the BARE domain must resolve to the coder gym — the exact path, not merely some path"
        );
        // The regression, stated positively: the WIDENED bucket has no gym at all, so
        // a lookup that keyed on it would drop the eval set. This is the assertion the
        // old `false == false` comparison was supposed to be making.
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&code_stamped.bucket_key()),
            None,
            "the widened bucket ({}) must NOT resolve to a gym — which is exactly why \
             the lookup has to use the bare domain",
            code_stamped.bucket_key()
        );
        let code_params =
            build_submit_params(Uuid::from_u128(7), "Cass", "qwen", &code_stamped, "live-turn");
        assert_eq!(
            code_params["evalSet"],
            json!("docs/genome/coder-eval.jsonl"),
            "the dispatched example must carry the BARE domain's eval set, so the L3 \
             sentinel can A/B the gene it produces"
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
