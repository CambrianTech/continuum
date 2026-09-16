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
//! ## Capture and submission have different ownership
//!
//! Unlinked speech submission runs on a spawned task. Card-linked live work awaits
//! its periodic and final atomic snapshots so revision order follows the turn;
//! storage failure is reported without changing the persona's decision. The
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
use crate::orm::types::{BatchOperation, BatchOperationType};
// `COLLECTION` is an associated const on the OrmEntity trait; the derive puts it
// there, so the trait must be in scope to name it.
use crate::orm::OrmEntity;

pub mod reviewed;

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

    /// Derive the selection capture from the card a turn was rooted onto.
    ///
    /// **Pure, so it is testable with no cycle, no hands and no executor** — the same
    /// judgment/effect split [`plan`] uses. The EFFECT (attaching this to the turn
    /// before the rooting await in `root_at_held_card`) lands with the consumer that
    /// reads it; this is the judgment half.
    ///
    /// `WorkCard::owner` and `claim_id` are both `Option` and travel together or not
    /// at all: a card the work store never claimed yields `claim: None`, and NOTHING
    /// may substitute a default — not the acting persona, not a nil claim. Such a
    /// turn is still card-linked, so it stages; it simply can never be stamped, and
    /// no later settlement may supply the receipt it never had.
    pub fn from_selected_card(card: &airc_work::WorkCard) -> Self {
        let claim = match (card.owner, card.claim_id) {
            (Some(owner), Some(claim_id)) => Some(ClaimReceipt {
                claim_id: claim_id.as_uuid(),
                owner,
                // A claim yields OWNER. Reviewer and Finder are different hands on
                // the same card and are not derivable from a claim alone.
                role: CreditRole::Owner,
            }),
            // Either half missing is no receipt. An owner without a claim is not an
            // accepted claim, and a claim without an owner names nobody.
            _ => None,
        };
        Self {
            card_id: card.card_id.as_uuid(),
            claim,
        }
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

/// ONE dispatched generation, as a correlation ROW — the index that
/// [`StagedCredit::receipts`] cannot carry (card 0d51573a).
///
/// The ORM's `FieldType` set has no collection type, so the parent's receipts are an
/// opaque JSON column and nothing inside them is indexable. Playback's correlation
/// runs FROM a generation TO the credit, so that direction needs a real index; these
/// rows are it. They hold REFERENCES only — the response payload stays in the parent
/// and is not duplicated here.
///
/// **`submitted_request_id` IS NOT ASSUMED GLOBALLY UNIQUE** (Astra, 2026-09-08).
/// Nothing in the producer contract proves a request id is unique across every staged
/// credit ever written, and a unique index on it alone would reject a legitimate write
/// the first time that assumption failed — silently turning a provenance record into a
/// dropped one. Uniqueness is asserted on the PAIR, which is the thing that genuinely
/// cannot repeat: one turn does not dispatch the same request id twice.
///
/// The row is a child in the truest sense: `on_delete = "cascade"`, so it cannot
/// outlive the credit it describes and there is no orphan class to reap.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "staged_credit_generation")]
#[entity(index(
    name = "idx_staged_credit_generation_pair",
    fields = ["stagedCreditId", "submittedRequestId"],
    unique = true
))]
pub struct StagedCreditGeneration {
    #[entity(primary_key)]
    pub id: Uuid,

    /// The staged credit this generation belongs to.
    #[entity(indexed)]
    #[entity(foreign_key("staged_credit.id", on_delete = "cascade"))]
    pub staged_credit_id: Uuid,

    /// The SUBMITTED request id — the join key playback holds. Indexed on its own so
    /// the generation-to-credit lookup is a real index hit, not a scan.
    #[entity(indexed)]
    pub submitted_request_id: String,
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

    /// EVERY generation this turn dispatched, in dispatch order, faults included.
    ///
    /// A cycle makes multiple calls and there is no canonical singular request, so
    /// this replaces the single `submitted_request_id` that used to sit here — a
    /// one-to-one field for a one-to-many domain. Stored as an opaque JSON payload
    /// because the ORM has no collection `FieldType`, which is exactly why the
    /// correlation index cannot live in here and gets its own child rows in
    /// [`StagedCreditGeneration`].
    ///
    /// The FULL receipts stay here; the child rows are references, not copies.
    #[entity(json)]
    pub receipts: Vec<crate::cognition::provenance::GenerationReceipt>,

    /// Representative served request when all usable generations agree on their
    /// model and provider. Mixed/all-faulted turns have no scalar summary; the
    /// complete ordered receipts above preserve every lane and fault.
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

/// Persist a continuing work turn at the existing four-act boundary.
pub const STAGE_EVERY_ACTS: usize = 4;

pub fn is_stage_point(acts: usize) -> bool {
    acts > 0 && acts % STAGE_EVERY_ACTS == 0
}

/// The selected card and staged revisions belong to THIS turn, not a persona
/// global. The shared driver borrows this owner while it runs. Writes are awaited
/// serially through the existing async data command; cancelling the driver cannot
/// leave a callback attached to another room or eval fork.
/// An in-flight transaction may finish, but it can only replace this turn's rows.
pub(crate) struct TurnCreditCapture {
    conn: Connection<InProcessTransport>,
    persona_name: String,
    prompt: String,
    credit: CapturedCredit,
    snapshot: Option<(Uuid, [u8; 32])>,
    superseded: Vec<Uuid>,
}

impl TurnCreditCapture {
    pub(crate) fn for_turn(
        persona_id: Uuid,
        persona_name: &str,
        prompt: &str,
        credit: Option<&CapturedCredit>,
    ) -> Option<Self> {
        let credit = credit.filter(|credit| credit.is_card_linked())?;
        let Some(executor) = EXECUTOR.cloned() else {
            crate::probe!(
                class = "training.credit.stage_unavailable",
                persona = %persona_id,
                card = %credit.card_id,
                "card credit capture unavailable: command executor is not installed"
            );
            return None;
        };
        Some(Self::with_executor(
            executor,
            persona_id,
            persona_name.to_owned(),
            prompt.to_owned(),
            credit.clone(),
        ))
    }

    fn with_executor(
        executor: Arc<CommandExecutor>,
        persona_id: Uuid,
        persona_name: String,
        prompt: String,
        credit: CapturedCredit,
    ) -> Self {
        Self {
            conn: Connection::new(InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(persona_id),
                )),
            )),
            persona_name,
            prompt,
            credit,
            snapshot: None,
            superseded: Vec::new(),
        }
    }

    /// Snapshot only at a completed-act boundary, or once at the shared driver's
    /// final return. A faulted final generation still belongs in the receipt list.
    /// A failed replacement leaves the previous whole snapshot intact; the next
    /// boundary retries identical content with the SAME identity. Changed content
    /// gets a new revision so a stale settlement cannot delete its successor.
    pub(crate) async fn record(
        &mut self,
        acts: &[(String, Vec<crate::ai::types::ToolCall>)],
        receipts: &[crate::cognition::provenance::GenerationReceipt],
        report: Option<&str>,
        final_snapshot: bool,
    ) -> bool {
        if (!final_snapshot && !is_stage_point(acts.len()))
            || (acts.is_empty() && report.is_none() && receipts.is_empty())
        {
            return false;
        }
        let started = std::time::Instant::now();
        let mut completion = acted_chain(acts);
        if let Some(report) = report {
            completion.push_str(report);
        }
        let digest = snapshot_digest(&completion, receipts);
        let submission_id = match self.snapshot {
            Some((id, prior)) if prior == digest => id,
            previous => {
                if let Some((id, _)) = previous {
                    self.superseded.push(id);
                }
                let id = Uuid::new_v4();
                // Record the attempt before awaiting: an acknowledgement can be
                // lost after commit. The next revision removes every possible
                // predecessor in its own atomic write, never a successor.
                self.snapshot = Some((id, digest));
                id
            }
        };
        let result = stage_credit(
            &self.conn,
            &self.persona_name,
            &self.credit,
            receipts.to_vec(),
            self.prompt.clone(),
            completion,
            submission_id,
            &self.superseded,
        )
        .await;
        match result {
            Ok(_) => {
                self.superseded.clear();
                crate::modules::citizen_health::note_credit_staged();
                crate::probe!(
                    class = "training.credit.staged",
                    persona = %self.persona_name,
                    card = %self.credit.card_id,
                    submission = %submission_id,
                    acts = acts.len() as u64,
                    generations = receipts.len() as u64,
                    final_snapshot,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "card-linked turn snapshot persisted; outcome remains unverified"
                );
                true
            }
            Err(error) => {
                crate::probe!(
                    class = "training.credit.stage_failed",
                    persona = %self.persona_name,
                    card = %self.credit.card_id,
                    submission = %submission_id,
                    error = %error,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "turn snapshot refused; prior staged snapshot retained for retry"
                );
                false
            }
        }
    }
}

/// Compare immutable snapshot payloads without retaining another full copy or
/// serializing them. Prompt/card are fixed on the turn owner; these are its only
/// changing fields. Tags and length framing distinguish optional/adjacent strings.
fn snapshot_digest(
    completion: &str,
    receipts: &[crate::cognition::provenance::GenerationReceipt],
) -> [u8; 32] {
    use crate::cognition::provenance::GenerationOutcome;
    use sha2::{Digest, Sha256};
    fn field(hash: &mut Sha256, value: Option<&str>) {
        hash.update([u8::from(value.is_some())]);
        if let Some(value) = value {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
    }
    let mut hash = Sha256::new();
    field(&mut hash, Some(completion));
    for receipt in receipts {
        field(&mut hash, Some(&receipt.submitted_request_id));
        match &receipt.outcome {
            GenerationOutcome::Served {
                model,
                provider,
                provider_request_id,
            } => {
                hash.update([0]);
                field(&mut hash, Some(model));
                field(&mut hash, Some(provider));
                field(&mut hash, provider_request_id.as_deref());
            }
            GenerationOutcome::Faulted {
                detail,
                model,
                provider,
            } => {
                hash.update([1]);
                field(&mut hash, Some(detail));
                field(&mut hash, model.as_deref());
                field(&mut hash, provider.as_deref());
            }
        }
    }
    hash.finalize().into()
}

/// Turn a completed live turn into a buffered training example. Unlinked speech
/// is scored and submitted by the existing best-effort background operation;
/// linked work is retained for a later verified outcome. Never call from eval.
pub fn produce(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    prompt: String,
    completion: String,
    credit: Option<CapturedCredit>,
    generation_receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
) {
    produce_with_id(
        persona_id,
        persona_name,
        base_model,
        prompt,
        completion,
        credit,
        generation_receipts,
        Uuid::new_v4(),
        None,
    );
}

/// [`produce`] with the submission id chosen by the caller and, for a re-stage of the
/// same turn, the partial submission it REPLACES (deleted in the same batch).
#[allow(clippy::too_many_arguments)]
pub fn produce_with_id(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    prompt: String,
    completion: String,
    // The card this turn was rooted at, captured AT SELECTION on her hands.
    // `Some` routes the turn to STAGING; `None` is ordinary conversation and keeps
    // the pre-existing immediate-submit behaviour byte-identical.
    credit: Option<CapturedCredit>,
    // Every generation this turn dispatched, in order, faults included.
    generation_receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
    submission_id: Uuid,
    replaces: Option<Uuid>,
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
        // CARD-LINKED TURNS STAGE. The quality floor and the domain bucket are NOT
        // decided here for them: `plan` applies the evidence floor and it needs a
        // verdict, which does not exist until the card settles. Deciding now would
        // bake in a judgment made without the evidence — the exact conflation this
        // card exists to remove. Unlinked conversation still takes the immediate
        // path below, byte-identical.
        if let Some(credit) = credit.filter(|c| c.is_card_linked()) {
            // The SAME identity-bearing connection `submit_plan` builds — not a bare
            // `Connection::new(executor)`, which does not even satisfy `Transport`.
            // The identity is load-bearing here and not merely for gating: the
            // `@persona:{name}` handle these writes target resolves AS this persona,
            // so a connection without her `LocalPersona` would stage her credit
            // somewhere other than her own store.
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(persona_id),
                )),
            ));
            match stage_credit(
                &conn,
                &persona_name,
                &credit,
                generation_receipts,
                prompt,
                completion,
                submission_id,
                replaces.as_slice(),
            )
            .await
            {
                Ok(submission_id) => crate::probe!(
                    class = "training.credit.staged",
                    replaced_partial = replaces.is_some(),
                    persona = %persona_name,
                    card = %credit.card_id,
                    submission = %submission_id,
                    stampable = credit.claim.is_some(),
                    "card-linked turn staged — it submits only if this card settles PASS"
                ),
                // Best-effort like the submit path below: a staging failure must
                // never touch the turn that already happened. NAMED, not swallowed
                // — a turn that failed to stage is credit that silently vanished.
                Err(e) => crate::probe!(
                    class = "training.credit.stage_failed",
                    persona = %persona_name,
                    card = %credit.card_id,
                    error = %e,
                    "card-linked turn could NOT be staged — this turn's credit is lost"
                ),
            }
            return;
        }

        let classifier = CLASSIFIER.get_or_init(DomainClassifier::new);
        crate::modules::citizen_health::note_credit_staged();
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
                // THE JOIN KEY, not a count. Without it this probe says an example
                // was accepted but not WHICH submission it became, so a room turn
                // cannot be joined to `training_trigger_submissions` or to dispatch
                // state — and a count can never establish a causal link, only a
                // correlation someone will read as one (Astra, 2026-09-08).
                //
                // Emitted as the destination's OWN id, never minted here: an id we
                // invented would join to nothing and would look exactly like one
                // that did.
                // `Option` rather than a "none" String: an absent acceptance OMITS
                // the field instead of emitting a sentinel that reads like a value.
                // "none" in an id column is exactly the absence-as-data shape this
                // card exists to remove — and it allocated per probe. (Astra
                // verified tracing-core implements Value for Option<T>.)
                submission = receipt
                    .acceptance
                    .as_ref()
                    .map(|a| tracing::field::display(&a.submission_id)),
                // A replay is an acceptance the destination has SEEN BEFORE. Without
                // this, a retry and a first submission are the same row.
                replayed = receipt.acceptance.as_ref().is_some_and(|a| a.replayed),
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
/// Persist a card-linked turn as a STAGED credit — the parent row plus one child
/// row per generation — instead of submitting it as training data now.
///
/// Card 0d51573a. A `TrainingExample` is `{prompt, completion}` with no preference
/// schema anywhere in the tree, so submission is ONE-WAY: a turn submitted before
/// its card settles cannot be withdrawn when that card later fails. Staging is what
/// makes the evidence floor reachable — the example waits for a verdict that may
/// never arrive, and never arriving is the correct outcome rather than a leak.
///
/// **Schemas are ensured through the registry, never inferred.** `data/ensure-schema`
/// resolves a collection by NAME from the boot registry, which is why both are
/// registered in `persona::register_substrate_orm_entities`. Skipping it would not
/// fail loudly: sqlite would build both tables from the data's SHAPE and silently
/// omit every declared index, including the unique one that stops a single
/// generation being credited to a staged turn twice.
///
/// The whole write is ONE `data/batch`, which is atomic on both adapters — a parent
/// surviving its rolled-back children would be a credit record asserting provenance
/// it cannot produce.
#[allow(clippy::too_many_arguments)]
async fn stage_credit<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    credit: &CapturedCredit,
    generation_receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
    prompt: String,
    completion: String,
    submission_id: Uuid,
    replaces: &[Uuid],
) -> Result<Uuid, ClientError> {
    // Her OWN store, not the shared main DB: a citizen's staged credit is her
    // record of her own turns.
    let handle = format!("@persona:{persona_name}");
    // `submission_id` identifies immutable payload, not mutable turn content. A
    // retry replaces only that identical revision; a new revision removes known
    // and uncertain predecessors. Children cascade in the same transaction.

    for collection in [StagedCredit::COLLECTION, StagedCreditGeneration::COLLECTION] {
        let result = conn
            .commands()
            .execute_value(
                "data/ensure-schema",
                // `dbPath`, NOT `handle`. `CommandRequest<P>` flattens the params into the
                // SAME JSON object as its own `handle: Option<HandleRef>`, so a
                // top-level "handle" is claimed by the ENVELOPE and never reaches the
                // params' `handle: Option<String>` — the request fails to deserialize
                // with `invalid type: string, expected struct HandleRef`. The params
                // declare `#[serde(alias = "dbPath")]` precisely as the escape hatch
                // from that collision. One word, two meanings, one object.
                json!({ "collection": collection, "dbPath": handle }),
            )
            .await?;
        storage_ok(&result, "data/ensure-schema", collection)?;
    }

    // The ordered receipts remain authoritative. A single scalar is meaningful
    // only if all usable generations agree on the model AND provider.
    let served = served_provenance(&generation_receipts);
    let parent = StagedCredit {
        id: submission_id,
        card_id: credit.card_id,
        claim_id: credit.claim.as_ref().map(|c| c.claim_id),
        owner: credit.claim.as_ref().map(|c| c.owner.as_uuid()),
        role: credit.claim.as_ref().map(|c| c.role),
        receipts: generation_receipts,
        served,
        prompt,
        completion,
        // Same clock the rest of this file already uses. `now_unix_ms` exists as a
        // PRIVATE helper in three other modules and none is importable — copying it
        // a fourth time would be the duplication the compression principle forbids.
        staged_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
    };

    let mut operations = Vec::new();
    for prev in replaces
        .iter()
        .filter(|id| **id != submission_id)
        .chain(std::iter::once(&submission_id))
    {
        operations.push(BatchOperation {
            operation_type: BatchOperationType::Delete,
            collection: StagedCredit::COLLECTION.to_string(),
            id: Some(prev.to_string()),
            data: None,
        });
    }
    operations.push(BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: StagedCredit::COLLECTION.to_string(),
        id: Some(submission_id.to_string()),
        data: Some(serde_json::to_value(&parent)?), // ORM batch boundary: BatchOperation.data is Value by contract
    });
    // One correlation row per generation, faults included: the receipts themselves
    // ride on the parent as an opaque JSON column (the ORM has no collection
    // FieldType), so these child rows ARE the index that column cannot carry.
    for receipt in &parent.receipts {
        let child = StagedCreditGeneration {
            id: Uuid::new_v4(),
            staged_credit_id: submission_id,
            submitted_request_id: receipt.submitted_request_id.clone(),
        };
        operations.push(BatchOperation {
            operation_type: BatchOperationType::Create,
            collection: StagedCreditGeneration::COLLECTION.to_string(),
            id: Some(child.id.to_string()),
            data: Some(serde_json::to_value(&child)?), // ORM batch boundary: BatchOperation.data is Value by contract
        });
    }

    let result = conn
        .commands()
        .execute_value(
            "data/batch",
            // `dbPath` for the same reason as above: a top-level "handle" is eaten by
            // the envelope's HandleRef field.
            json!({ "operations": operations, "dbPath": handle }),
        )
        .await?;
    storage_ok(&result, "data/batch", StagedCredit::COLLECTION)?;
    Ok(submission_id)
}

/// Summarize a homogeneous served lane using a real representative request. Mixed
/// models/providers and all-faulted turns retain their full receipts without a
/// fabricated scalar base model. Fault receipts never become successful output.
fn served_provenance(
    receipts: &[crate::cognition::provenance::GenerationReceipt],
) -> Option<ServedProvenance> {
    use crate::cognition::provenance::GenerationOutcome;
    let mut first: Option<(&str, &str, &str)> = None;
    for receipt in receipts {
        if let GenerationOutcome::Served {
            model,
            provider,
            provider_request_id,
        } = &receipt.outcome
        {
            if first.is_some_and(|(m, p, _)| m != model || p != provider) {
                return None;
            }
            first.get_or_insert((
                model,
                provider,
                provider_request_id
                    .as_deref()
                    .unwrap_or(&receipt.submitted_request_id), // Equal provider/submitted IDs are stored once in the receipt.
            ));
        }
    }
    first.map(|(model, provider, request_id)| ServedProvenance {
        model: model.to_owned(),
        provider: provider.to_owned(),
        request_id: request_id.to_owned(),
    })
}

/// Fail on a command that SUCCEEDED AS A CALL and FAILED AS AN OPERATION.
///
/// `execute_value` returns `Ok` for anything that made the round trip, and the data
/// commands report adapter failure INSIDE the payload as
/// `StorageResult { success: false, error }` — which `InProcessTransport` forwards
/// unchanged. So `.await?` catches a broken transport and nothing else: a child
/// constraint violation rolls the whole batch back, the call still returns `Ok`, and
/// without this the caller emitted `training.credit.staged` with a submission id for
/// rows that do not exist (Astra/S6 on 799b8fe9). Transport success is not operation
/// success, and a receipt that cannot report failure is not a receipt.
fn storage_ok(
    value: &serde_json::Value,
    command: &str,
    collection: &str,
) -> Result<(), ClientError> {
    // Decoded as the TYPED result the data layer actually returns, not by poking at a
    // "success" key: a shape change should break the build here rather than silently
    // read as false and start rejecting every staging attempt.
    let decoded: crate::orm::types::StorageResult<serde_json::Value> =
        serde_json::from_value(value.clone())?; // ORM boundary: decode the data layer's typed StorageResult
    if decoded.success {
        return Ok(());
    }
    // `Refused` is the variant whose own doc says "substrate accepted the command but
    // returned an error" — which is precisely this: the call arrived, the operation
    // did not happen.
    Err(ClientError::Refused {
        command: command.to_string(),
        reason: format!(
            "`{collection}`: {}",
            decoded
                .error
                .as_deref()
                // An unsuccessful result with no message is still a failure. Saying so
                // beats inventing a reason or, worse, reading the silence as success.
                .unwrap_or("reported success=false with no error message"),
        ),
    })
}

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

/// The COMPLETION a work turn that ended in acts is lifted as: the intent she
/// stated plus the exact calls she made, rendered deterministically. This is the
/// ACTED chain the learning loop never saw (2026-09-13: every work turn ends
/// `Acted`; only the rare spoken report reached the producer, so a day of five
/// coders produced 14 conversation examples and zero code ones). A card that later
/// settles PASS turns each of these into a code example in her curriculum.
pub fn acted_completion(intent: &str, calls: &[crate::ai::types::ToolCall]) -> String {
    let mut out = String::new();
    let intent = intent.trim();
    if !intent.is_empty() {
        out.push_str(intent);
        out.push('\n');
    }
    for c in calls {
        out.push_str(&format!(
            "{}({})\n",
            c.name,
            serde_json::to_string(&c.input).unwrap_or_default() // JUSTIFIED unwrap_or_default: an unserializable arg renders empty; the call NAME still trains the shape
        ));
    }
    out
}

/// The whole turn's act chain, rendered in order — one `acted_completion` per act.
pub fn acted_chain(turn_acts: &[(String, Vec<crate::ai::types::ToolCall>)]) -> String {
    turn_acts
        .iter()
        .map(|(intent, calls)| acted_completion(intent, calls))
        .collect::<Vec<_>>()
        .join("")
}

/// Settle a card's staged credit: eligible claimed turns on `card_id` may transfer
/// after a passing verdict, through the same quality gate as live turns. Failed,
/// unknown and incompatible evidence remains staged. Reads each
/// resident's own staged store, the mirror of `stage_credit`. Best-effort and
/// receipted: a store that cannot be read names itself; nothing here can fail the
/// verdict that triggered it.
pub async fn settle_card_credit(card_id: Uuid, passed: bool) {
    let Some(executor) = EXECUTOR.cloned() else {
        return;
    };
    let Some(registry) = crate::persona::PersonaAircRuntimeRegistry::try_global() else {
        return;
    };
    for (persona_name, persona_id) in registry.roster_snapshot() {
        let handle = format!("@persona:{persona_name}");
        let conn = Connection::new(InProcessTransport::new(
            executor.clone(),
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona_id),
            )),
        ));
        let listed = match conn
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection": StagedCredit::COLLECTION,
                    "dbPath": handle,
                    "filter": { "cardId": card_id.to_string() },
                }),
            )
            .await
        {
            Ok(v) => staged_credit_from_list(v),
            Err(e) => Err(e),
        };
        let rows = match listed {
            Ok(rows) => rows,
            Err(e) => {
                crate::probe!(
                    class = "training.credit.settle_unreadable",
                    persona = %persona_name,
                    card = %card_id,
                    error = %e,
                    "a citizen's staged-credit store could not be read at settle — her credit on this card stands unsettled"
                );
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }
        let mut submitted = 0usize;
        for row in &rows {
            match settle_staged_row(&conn, persona_id, &persona_name, row, passed).await {
                Ok(true) => submitted += 1,
                Ok(false) => {}
                Err(error) => crate::probe!(
                    class = "training.credit.settle_failed",
                    persona = %persona_name,
                    card = %card_id,
                    submission = %row.id,
                    error = %error,
                    "staged revision retained; settlement has no completed ownership transfer"
                ),
            }
        }
        // The card is a boundary for its maker: one bounded consolidation pass, now.
        if let Some(region) = crate::cognition::dream_consolidation::global() {
            let pid = persona_id;
            tokio::spawn(async move {
                let _ = region.consolidate_at_card_boundary(pid).await;
            });
        }
        crate::probe!(
            class = "training.credit.settled",
            persona = %persona_name,
            card = %card_id,
            passed,
            staged = rows.len() as u64,
            submitted = submitted as u64,
            "card verdict checked staged revisions; only acknowledged transfers were removed"
        );
        crate::modules::citizen_health::note_credit_settled();
    }
}

/// `data/list` returns DataRecord envelopes, not bare entities. Validate the
/// complete response before transferring any row; malformed evidence stays put.
fn staged_credit_from_list(listed: serde_json::Value) -> Result<Vec<StagedCredit>, ClientError> {
    let listed: crate::modules::data::DataListResult = serde_json::from_value(listed)?; // Decode the Value-native data/list response at its command boundary.
    if listed.items.len() != listed.total as usize {
        return Err(ClientError::Transport(format!(
            "staged-credit list is incomplete: {} of {} rows",
            listed.items.len(),
            listed.total,
        )));
    }
    listed
        .items
        .into_iter()
        .map(|item| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(item)?; // Decode the persisted ORM envelope; move its entity payload without cloning it.
            Ok(serde_json::from_value(record.data)?) // Decode one staged-credit entity from its typed storage envelope.
        })
        .collect()
}

/// Transfer one immutable revision using the destination's existing idempotency
/// key. The accepted receipt proves ownership independently of training dispatch;
/// a refusal, failed outcome, missing claim or incompatible lane keeps evidence.
/// Generation reservations also cover ordinary reviewed submissions, so a later
/// cumulative successor cannot repeat the same acts under an earlier verdict.
fn staged_submission_params(
    persona_id: Uuid,
    persona_name: &str,
    row: &StagedCredit,
    passed: bool,
) -> Option<serde_json::Value> {
    let eligible_role = match (passed, row.claim_id, row.owner, row.role) {
        (true, Some(_), Some(owner), Some(role)) if owner == persona_id => role,
        _ => return None,
    };
    // The full receipts are authoritative, including records written before the
    // scalar served summary existed. Never borrow another row's model.
    let served = served_provenance(&row.receipts)
        .filter(|served| !served.model.trim().is_empty() && !served.provider.trim().is_empty())?;
    let stamp = OutcomeStamp {
        card_id: row.card_id,
        role: eligible_role,
        outcome: passed,
    };
    let plan = plan(
        CLASSIFIER.get_or_init(DomainClassifier::new),
        &row.prompt,
        &row.completion,
        Some(stamp),
    )?;
    let mut params = build_submit_params(
        persona_id,
        persona_name,
        &served.model,
        &plan,
        "card-credit",
    );
    params["submissionId"] = json!(row.id);
    Some(params)
}

async fn settle_staged_row<T: Transport>(
    conn: &Connection<T>,
    persona_id: Uuid,
    persona_name: &str,
    row: &StagedCredit,
    passed: bool,
) -> Result<bool, ClientError> {
    let Some(params) = staged_submission_params(persona_id, persona_name, row, passed) else {
        return Ok(false);
    };
    match reviewed::reserve_transfer(conn, persona_name, row, None).await {
        Ok(()) => {}
        Err(reviewed::CreditBindingError::Storage(error)) => return Err(error),
        Err(error) => {
            crate::probe!(
                class = "training.credit.settle_overlap",
                persona = %persona_name,
                card = %row.card_id,
                revision = %row.id,
                error = %error,
                "generation evidence is already reserved; revision retained without another submission"
            );
            return Ok(false);
        }
    }
    let receipt = submit_training(conn, params).await?;
    if receipt
        .acceptance
        .as_ref()
        .is_none_or(|accepted| accepted.submission_id != row.id)
    {
        crate::probe!(
            class = "training.credit.settle_deferred",
            persona = %persona_name,
            card = %row.card_id,
            submission = %row.id,
            error_kind = ?receipt.error_kind,
            "training destination did not acknowledge this revision; evidence retained"
        );
        return Ok(false);
    }
    let deleted = conn.commands().execute_value(
        "data/delete",
        json!({ "collection": StagedCredit::COLLECTION, "id": row.id, "dbPath": format!("@persona:{persona_name}") }),
    ).await?;
    storage_ok(&deleted, "data/delete", StagedCredit::COLLECTION)?;
    crate::probe!(
        class = "training.credit.transferred",
        persona = %persona_name,
        card = %row.card_id,
        submission = %row.id,
        replayed = receipt.acceptance.as_ref().is_some_and(|accepted| accepted.replayed),
        dispatch_success = receipt.success,
        "destination durably accepted this staged revision; training is a separate outcome"
    );
    Ok(true)
}

/// Every card an INSTANCE names (a round's cards for it) settles when its verdict
/// is written — the hook `record_verdict` fires, spawned so a verdict never waits
/// on a curriculum write.
pub fn settle_instance_credit(instance: &str, passed: bool) {
    let cards = crate::cognition::bench_round::cards_for_instance(instance);
    if cards.is_empty() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        crate::probe!(
            class = "training.credit.settle_deferred",
            instance,
            "verdict written outside the runtime — its cards' staged credit settles on the next verdict of this instance"
        );
        return;
    };
    for card in cards {
        handle.spawn(settle_card_credit(card, passed));
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    // what this catches: a FABRICATED claim receipt. `WorkCard::owner` and `claim_id`
    // are both Option. A turn rooted at a card carrying neither is STILL card-linked
    // — she stood in that checkout and her edits landed there — so it must stage. But
    // it must never acquire a receipt it did not have, and no later settlement may
    // supply one. If this ever defaulted the missing pair (to the acting persona, to
    // a nil claim), that turn becomes stampable and a citizen is credited for
    // accountability nobody recorded. Contract stated by Astra 2026-09-08.
    #[test]
    fn a_claimless_card_stages_as_card_linked_without_inventing_a_receipt() {
        fn card(
            owner: Option<airc_core::PeerId>,
            claim: Option<airc_work::ClaimId>,
        ) -> airc_work::WorkCard {
            airc_work::WorkCard {
                card_id: airc_work::WorkCardId::new(),
                repo: airc_work::RepoId::new("acme/continuum").expect("valid repo id in fixture"),
                title: "t".to_string(),
                body: None,
                priority: airc_work::Priority::P2,
                lane_id: None,
                state: airc_work::CardState::Claimed,
                owner,
                claim_id: claim,
                claim_expires_at_ms: None,
                last_heartbeat_at_ms: None,
                pull_request: None,
                created_by: airc_core::PeerId::new(),
                created_at_ms: 1,
                updated_at_ms: 1,
                reviews: None,
                submissions: Vec::new(),
                last_submission_rejection: None,
            }
        }

        let bare = card(None, None);
        let captured = CapturedCredit::from_selected_card(&bare);
        assert_eq!(captured.card_id, bare.card_id.as_uuid());
        assert!(
            captured.is_card_linked(),
            "a claimless card is still card-linked — it stages, her edits landed there"
        );
        assert!(
            captured.claim.is_none(),
            "no receipt may be invented for a card the work store never claimed"
        );

        // POSITIVE CONTROL: a real owner+claim pair MUST produce a receipt. Without
        // this the assertion above would pass identically if `from_selected_card`
        // never populated a receipt at all.
        let owner = airc_core::PeerId::new();
        let claimed = card(
            Some(owner),
            Some(airc_work::ClaimId::from_uuid(Uuid::new_v4())),
        );
        let receipt = CapturedCredit::from_selected_card(&claimed)
            .claim
            .expect("a real owner+claim pair must produce a receipt");
        assert_eq!(
            receipt.owner, owner,
            "the receipt carries the store's own owner"
        );
        assert_eq!(receipt.role, CreditRole::Owner, "a claim yields Owner");

        // HALF A PAIR IS NOT A PAIR: an owner with no claim is not an accepted
        // claim, and a claim with no owner names nobody.
        assert!(
            CapturedCredit::from_selected_card(&card(Some(owner), None))
                .claim
                .is_none(),
            "an owner without a claim is not an accepted claim"
        );
        assert!(
            CapturedCredit::from_selected_card(&card(
                None,
                Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()))
            ))
            .claim
            .is_none(),
            "a claim without an owner names nobody"
        );
    }

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

        // The PARENT indexes what settlement looks up by. `submittedRequestId` is NOT
        // here any more: a cycle dispatches many generations, the receipts are an
        // opaque JSON column (the ORM has no collection FieldType), and nothing inside
        // a JSON column is indexable — so the correlation index moved to the child
        // rows. Asserting it on the parent would now assert a lie.
        for key in ["cardId", "claimId"] {
            assert!(
                indexed.contains(key),
                "staged_credit must INDEX {key:?} — settlement looks up by card; \
                 indexed fields are {indexed:?}"
            );
        }
        assert!(
            !indexed.contains("submittedRequestId"),
            "the correlation index belongs to staged_credit_generation now; leaving a \
             stale one here would suggest a lookup the parent cannot actually serve"
        );

        // POSITIVE CONTROL: a field deliberately NOT indexed must not be, or this test
        // would pass with `indexed` blanket-set and assert nothing about my attributes.
        assert!(
            !indexed.contains("prompt"),
            "prompt must not be indexed — if it is, this test cannot distinguish my \
             deliberate indexes from every field being indexed by default"
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
            .settle(SettlementVerdict::from_activity(
                &crate::cognition::activity::Verdict::pass("tests pass on the staged checkout"),
            ))
            .expect("test: a captured claim receipt can be stamped");
        assert!(
            passed.outcome,
            "settle must carry a PASSING verdict through"
        );
        let planned = plan(
            &classifier,
            "How do I pool websockets?",
            substantial,
            Some(passed),
        )
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
            .settle(SettlementVerdict::from_activity(
                &crate::cognition::activity::Verdict::fail("the patch did not apply"),
            ))
            .expect("test: a receipt stamps regardless of which way the verdict went");
        assert!(
            !failed.outcome,
            "settle must carry a FAILING verdict through rather than defaulting to success"
        );
        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
                Some(failed)
            )
            .is_none(),
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
            claimless
                .settle(SettlementVerdict::from_activity(
                    &crate::cognition::activity::Verdict::pass("tests pass on the staged checkout")
                ))
                .is_none(),
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
            meta["domain"],
            json!(stamped.trait_kind),
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
        let code_params = build_submit_params(
            Uuid::from_u128(7),
            "Cass",
            "qwen",
            &code_stamped,
            "live-turn",
        );
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
        let p = plan(
            &classifier,
            "Why does my Rust function panic?",
            code_reply,
            None,
        )
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

    /// The one crate-wide home lock (`crate::test_env`, #4082) — pins HOME, USERPROFILE,
    /// HF_HOME and CONTINUUM_HOME together and restores them on drop.
    use crate::test_env::HomeGuard;

    /// A real `DataModule` over a real SQLite adapter, reached through the SAME
    /// dispatch path production uses — `stage_credit` gets no special seam.
    fn data_runtime() -> Arc<CommandExecutor> {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        registry.register(Arc::new(crate::modules::data::DataModule::new()));
        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        registry.install_executor_on_all(executor.clone());
        executor
    }

    /// Shared real SQLite fixture for the production turn-driver regression.
    pub(crate) fn turn_capture(
        persona_id: Uuid,
        persona_name: String,
        prompt: String,
        credit: CapturedCredit,
    ) -> TurnCreditCapture {
        TurnCreditCapture::with_executor(data_runtime(), persona_id, persona_name, prompt, credit)
    }

    fn conn_as(executor: Arc<CommandExecutor>, persona: Uuid) -> Connection<InProcessTransport> {
        Connection::new(InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona),
            )),
        ))
    }

    fn receipt(id: &str) -> crate::cognition::provenance::GenerationReceipt {
        crate::cognition::provenance::GenerationReceipt::faulted(id, "irrelevant to staging")
    }

    // what this catches: 41f4e3ef — a turn can change serving lanes; a scalar
    // summary must never attribute all its generations to the first model.
    #[test]
    fn served_summary_requires_one_actual_lane_and_retains_request_identity() {
        use crate::cognition::provenance::{GenerationOutcome, GenerationReceipt};
        let mut receipts = vec![receipt("failed-before-serving")];
        assert!(served_provenance(&receipts).is_none());
        receipts.push(GenerationReceipt {
            submitted_request_id: "submitted-a".into(),
            outcome: GenerationOutcome::Served {
                model: "actual-a".into(),
                provider: "provider-a".into(),
                provider_request_id: Some("provider-request-a".into()),
            },
        });
        receipts.push(receipt("failed-after-serving"));
        let homogeneous = served_provenance(&receipts).unwrap();
        assert_eq!(homogeneous.model, "actual-a");
        assert_eq!(homogeneous.request_id, "provider-request-a");
        receipts.push(GenerationReceipt {
            submitted_request_id: "submitted-b".into(),
            outcome: GenerationOutcome::Served {
                model: "actual-b".into(),
                provider: "provider-a".into(),
                provider_request_id: None,
            },
        });
        assert!(served_provenance(&receipts).is_none());
        let last = receipts.last_mut().unwrap();
        last.outcome = GenerationOutcome::Served {
            model: "actual-a".into(),
            provider: "provider-b".into(),
            provider_request_id: None,
        };
        assert!(served_provenance(&receipts).is_none());
        assert_eq!(
            served_provenance(&receipts[3..]).unwrap().request_id,
            "submitted-b"
        );
    }

    // what this catches: Astra/S6 on 799b8fe9 — `stage_credit` DISCARDED the results of
    // data/ensure-schema and data/batch. Those commands report adapter failure INSIDE the
    // payload as StorageResult{success:false}, and InProcessTransport forwards it
    // unchanged, so `.await?` caught only transport breakage. A child constraint
    // violation rolled the batch back and the caller still returned Ok(submission_id) and
    // emitted `training.credit.staged` for rows that do not exist. THE TEST PASSED
    // BECAUSE THE CHECK COULD NOT FAIL (card 36e8fdd6).
    //
    // The rollback is provoked the way production would hit it: TWO receipts sharing one
    // submittedRequestId, violating the unique index on (stagedCreditId,
    // submittedRequestId) that persona::register_substrate_orm_entities exists to keep.
    #[tokio::test]
    async fn a_rolled_back_batch_is_never_reported_as_staged_credit() {
        let home = tempfile::tempdir().expect("tempdir");
        let _guard = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .expect("register staging entities");

        let persona = Uuid::new_v4();
        // Built directly rather than through a WorkCard fixture: what `stage_credit`
        // consumes is the CREDIT, and a claimless card is the honest minimum — a card
        // with no accepted claim still stages, it simply can never be stamped.
        let credit = CapturedCredit {
            card_id: Uuid::new_v4(),
            claim: None,
        };
        let executor = data_runtime();

        // POSITIVE CONTROL FIRST. Without it, a `stage_credit` that refused everything
        // would satisfy the rollback assertion below for entirely the wrong reason.
        let ok = stage_credit(
            &conn_as(executor.clone(), persona),
            "anwen",
            &credit,
            vec![receipt("req-a"), receipt("req-b")],
            "prompt".to_string(),
            "completion".to_string(),
            Uuid::new_v4(),
            &[],
        )
        .await;
        let submission = ok.expect("two DISTINCT receipts stage cleanly");

        // THE REGRESSION: one submittedRequestId used twice violates the unique index,
        // the batch rolls back, and `stage_credit` must REPORT that rather than hand back
        // a submission id for rows the database threw away.
        let rolled_back = stage_credit(
            &conn_as(executor, persona),
            "anwen",
            &credit,
            vec![receipt("req-dup"), receipt("req-dup")],
            "prompt".to_string(),
            "completion".to_string(),
            Uuid::new_v4(),
            &[],
        )
        .await;
        let err =
            rolled_back.expect_err("a rolled-back batch must NOT be reported as staged credit");
        assert_ne!(
            format!("{err}"),
            String::new(),
            "the failure must name itself, not surface as an empty error"
        );

        // And the two runs are distinguishable: the control produced a real id.
        assert!(
            !submission.is_nil(),
            "the clean stage returned a real submission id"
        );
    }

    // what this catches: the ACTED chain rendering non-deterministically or dropping
    // the calls — the completion a passed card lifts into her curriculum.
    #[test]
    fn the_acted_chain_renders_intent_then_every_call_in_order() {
        let calls = vec![
            crate::ai::types::ToolCall {
                id: "a".into(),
                name: "code/read".into(),
                input: json!({"path": "x.py"}),
            },
            crate::ai::types::ToolCall {
                id: "b".into(),
                name: "code/edit".into(),
                input: json!({"path": "x.py", "mode": "replace"}),
            },
        ];
        let c = acted_completion("  fix the qop quoting  ", &calls);
        assert_eq!(c, "fix the qop quoting\ncode/read({\"path\":\"x.py\"})\ncode/edit({\"mode\":\"replace\",\"path\":\"x.py\"})\n");
        assert_eq!(acted_completion("", &[]), "");
    }

    // what this catches: the chain dropping an act or reordering it — the completion a
    // passed card lifts must be the turn as she took it.
    #[test]
    fn the_act_chain_keeps_every_act_in_order() {
        let c = |n: &str| crate::ai::types::ToolCall {
            id: n.into(),
            name: n.into(),
            input: json!({}),
        };
        let chain = acted_chain(&[
            ("look".into(), vec![c("code/read")]),
            ("fix".into(), vec![c("code/edit"), c("code/run")]),
        ]);
        assert_eq!(
            chain,
            "look\ncode/read({})\nfix\ncode/edit({})\ncode/run({})\n"
        );
    }

    /// Storage stays on the real SQLite command route. Only the destination's
    /// reply is scripted, with an optional pause at the ownership transfer.
    struct SettlementTransport {
        data: InProcessTransport,
        submit: continuum_client::mock::MockTransport,
        pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    }

    #[async_trait::async_trait]
    impl Transport for SettlementTransport {
        async fn execute(
            &self,
            command: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, ClientError> {
            if command == "genome/training-trigger/submit" {
                if let Some((entered, release)) = &self.pause {
                    entered.notify_one();
                    release.notified().await;
                }
                self.submit.execute(command, params).await
            } else {
                self.data.execute(command, params).await
            }
        }

        async fn subscribe(
            &self,
            class: &str,
        ) -> Result<continuum_client::event::EventStream, ClientError> {
            self.data.subscribe(class).await
        }

        async fn emit(&self, class: &str, payload: serde_json::Value) -> Result<(), ClientError> {
            self.data.emit(class, payload).await
        }

        async fn provide(
            &self,
            command: &str,
            handler: Arc<dyn continuum_client::ServeHandler>,
        ) -> Result<(), ClientError> {
            self.data.provide(command, handler).await
        }

        async fn revoke(&self, command: &str) -> Result<(), ClientError> {
            self.data.revoke(command).await
        }

        async fn close(&self) -> Result<(), ClientError> {
            self.data.close().await
        }
    }

    fn settlement_connection(
        executor: Arc<CommandExecutor>,
        persona: Uuid,
        submit: continuum_client::mock::MockTransport,
        pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> Connection<SettlementTransport> {
        Connection::new(SettlementTransport {
            data: InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(persona),
                )),
            ),
            submit,
            pause,
        })
    }

    async fn stored_credit_rows(
        conn: &Connection<InProcessTransport>,
        name: &str,
    ) -> Vec<StagedCredit> {
        let listed = conn.commands().execute_value(
            "data/list",
            json!({"collection": StagedCredit::COLLECTION, "dbPath": format!("@persona:{name}")}),
        ).await.unwrap();
        staged_credit_from_list(listed).unwrap()
    }

    fn settlement_acts(count: usize) -> Vec<(String, Vec<crate::ai::types::ToolCall>)> {
        (0..count)
            .map(|n| {
                (
                    format!(
                        "Inspect source file {n} and verify its behavior against the regression."
                    ),
                    vec![crate::ai::types::ToolCall {
                        id: format!("read-{n}"),
                        name: "code/read".into(),
                        input: json!({"path":format!("src/file_{n}.rs")}),
                    }],
                )
            })
            .collect()
    }

    fn served_receipt(id: &str, model: &str) -> crate::cognition::provenance::GenerationReceipt {
        crate::cognition::provenance::GenerationReceipt {
            submitted_request_id: id.into(),
            outcome: crate::cognition::provenance::GenerationOutcome::Served {
                model: model.into(),
                provider: "fixture-provider".into(),
                provider_request_id: None,
            },
        }
    }

    async fn stage_settlement_fixture(
        executor: Arc<CommandExecutor>,
        persona: Uuid,
        name: &str,
        card: Uuid,
        receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
    ) -> (TurnCreditCapture, StagedCredit) {
        let mut capture = TurnCreditCapture::with_executor(
            executor.clone(),
            persona,
            name.into(),
            "Inspect the Rust source and report the verified regression behavior.".into(),
            CapturedCredit {
                card_id: card,
                claim: Some(ClaimReceipt {
                    claim_id: Uuid::new_v4(),
                    owner: airc_core::PeerId::from_uuid(persona),
                    role: CreditRole::Owner,
                }),
            },
        );
        assert!(
            capture
                .record(&settlement_acts(4), &receipts, None, false)
                .await
        );
        let row = stored_credit_rows(&conn_as(executor, persona), name)
            .await
            .into_iter()
            .find(|row| row.receipts == receipts)
            .unwrap();
        (capture, row)
    }

    fn script_settlement_reply(
        submit: &continuum_client::mock::MockTransport,
        row: &StagedCredit,
        reply: Option<serde_json::Value>,
    ) {
        let id = row.id;
        let model = row.served.as_ref().unwrap().model.clone();
        let completion = row.completion.clone();
        submit.respond_to("genome/training-trigger/submit", move |params| {
            assert_eq!(
                params["submissionId"],
                json!(id),
                "retry carries this immutable snapshot's identity"
            );
            assert_eq!(
                params["baseModel"],
                json!(model),
                "each row supplies its actual serving model"
            );
            assert_eq!(params["examples"][0]["completion"], json!(completion));
            match &reply {
                Some(value) => Ok(value.clone()),
                None => Err(ClientError::Transport(
                    "acceptance acknowledgement was lost".into(),
                )),
            }
        });
    }

    // what this catches: 6c36c24d — the actual public verbs bind before signed
    // publication, preserve caller/room, and report review versus credit honestly.
    #[tokio::test]
    async fn work_submission_commands_bind_before_publication_and_preserve_identity() {
        use crate::modules::work::submission::*;
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let airc_home = home.path().join("isolated-airc");
        let airc = Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(&airc_home, &airc_home)
                .await
                .unwrap(),
        );
        let room = airc.join("ordinary-project").await.unwrap();
        let persona = airc.peer_id().as_uuid();
        let name = "public-submission-owner";
        let registry = crate::persona::PersonaAircRuntimeRegistry::new();
        registry.register(crate::persona::PersonaAircRuntime::from_attached(
            persona,
            name,
            airc_home,
            airc.clone(),
            room.channel,
            crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
        ));
        let executor = data_runtime();
        let slot = Arc::new(LateBound::new("work submission fixture"));
        slot.install(executor.clone());
        let ctx = Ctx {
            caller: Some(CallerIdentity::local_persona(airc.peer_id())),
            ..Default::default()
        };
        let repo = airc_work::RepoId::new("acme/ordinary-project").unwrap();
        let card = airc
            .create_work_card(airc_lib::CreateWorkCard::new(
                repo.clone(),
                "verified ordinary work",
                airc_work::Priority::P1,
            ))
            .await
            .unwrap();
        let claim = airc
            .claim_work_card(airc_lib::ClaimWorkCard {
                card_id: card,
                ttl_ms: 60_000,
            })
            .await
            .unwrap();
        let mut capture = TurnCreditCapture::with_executor(
            executor.clone(),
            persona,
            name.into(),
            "Inspect and repair the actual implementation, with evidence.".into(),
            CapturedCredit {
                card_id: card.as_uuid(),
                claim: Some(ClaimReceipt {
                    claim_id: claim.as_uuid(),
                    owner: airc.peer_id(),
                    role: CreditRole::Owner,
                }),
            },
        );
        assert!(
            capture
                .record(
                    &settlement_acts(4),
                    &[served_receipt("public-command-request", "served-model")],
                    None,
                    true
                )
                .await
        );
        let selected = stored_credit_rows(&conn_as(executor, persona), name)
            .await
            .pop()
            .unwrap();
        let params = WorkSubmitParams {
            room: room.channel.as_uuid().to_string(),
            submission_id: Uuid::new_v4(),
            card_id: card.as_uuid(),
            claim_id: claim.as_uuid(),
            instance: "generic-project-work".into(),
            base_sha: "a".repeat(40),
            artifact: WorkArtifactReference {
                hash: "b".repeat(64),
                size_bytes: 20,
                mime: Some("text/x-diff".into()),
            },
            staged_revision_id: Some(selected.id),
        };
        let submit = WorkSubmit {
            registry: registry.clone(),
            executor_slot: slot.clone(),
        };
        let published = submit.run(&ctx, params.clone()).await.unwrap();
        assert_eq!(published.publisher, persona);
        assert_eq!(published.bound_staged_revision_id, Some(selected.id));
        let mut overlap = params.clone();
        overlap.submission_id = Uuid::new_v4();
        assert!(submit.run(&ctx, overlap.clone()).await.is_err());
        assert!(
            !airc
                .work_board_in(&room)
                .await
                .unwrap()
                .card(card)
                .unwrap()
                .submissions
                .iter()
                .any(|s| s.submission_id.as_uuid() == overlap.submission_id),
            "failed binding cannot publish its artifact"
        );
        let review_card = airc
            .create_work_card(
                airc_lib::CreateWorkCard::new(
                    repo,
                    "review exact candidate",
                    airc_work::Priority::P1,
                )
                .reviewing(card),
            )
            .await
            .unwrap();
        let review_claim = airc
            .claim_work_card(airc_lib::ClaimWorkCard {
                card_id: review_card,
                ttl_ms: 60_000,
            })
            .await
            .unwrap();
        let focus = airc.join("unrelated-focus").await.unwrap();
        let replay = submit.run(&ctx, params.clone()).await.unwrap();
        assert_eq!(replay.submitted_at_ms, published.submitted_at_ms);
        let reviewed = WorkReview {
            registry: registry.clone(),
            executor_slot: slot.clone(),
        }
        .run(
            &ctx,
            WorkReviewParams {
                room: params.room.clone(),
                review_id: Uuid::new_v4(),
                card_id: card.as_uuid(),
                submission_id: params.submission_id,
                artifact: params.artifact.clone(),
                review_card_id: review_card.as_uuid(),
                review_claim_id: review_claim.as_uuid(),
                outcome: ReviewOutcome::Passed,
                evidence: params.artifact.clone(),
            },
        )
        .await
        .unwrap();
        assert!(
            reviewed.credit_error.is_none(),
            "{:?}",
            reviewed.credit_error
        );
        assert_eq!(
            reviewed.credit.unwrap().state,
            reviewed::ReviewedCreditState::IndependentReviewRequired
        );
        let inspected = WorkSubmission {
            registry,
            executor_slot: slot,
        }
        .run(
            &ctx,
            WorkSubmissionParams {
                room: params.room,
                card_id: card.as_uuid(),
                submission_id: params.submission_id,
            },
        )
        .await
        .unwrap();
        assert_eq!(inspected.submission.publisher, persona);
        assert_eq!(inspected.reviews.len(), 1);
        assert_eq!(inspected.reviews[0].reviewer, persona);
        assert_eq!(
            inspected.credit.unwrap().state,
            reviewed::ReviewedCreditState::AwaitingReview
        );
        let safe = serde_json::to_value(&inspected.submission).unwrap();
        assert!(!safe.to_string().contains(&selected.prompt));
        assert_eq!(
            airc.current_room().await.unwrap().channel,
            focus.channel,
            "all explicit work verbs preserve unrelated focus"
        );
    }

    // what this catches: 6c36c24d — only an accepted independent exact-artifact
    // review may transfer a bound snapshot; lost ACK/replacement/reopen cannot
    // relabel the evidence or submit it under a fresh destination identity.
    #[tokio::test]
    async fn reviewed_credit_replays_exact_authority_and_recovers_after_replacement() {
        use airc_work::{WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
        use reviewed::{ReviewedCreditState as State, SubmissionSelection};
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let owner = airc_core::PeerId::from_uuid(persona);
        let reviewer = airc_core::PeerId::new();
        let card_id = airc_work::WorkCardId::new();
        let room = Uuid::new_v4();
        let name = "reviewed-project-work";
        let receipts = vec![served_receipt("reviewed-request", "actual-model")];
        let (mut capture, selected) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card_id.as_uuid(),
            receipts.clone(),
        )
        .await;
        let selection = SubmissionSelection {
            submission_id: Uuid::new_v4(),
            room_id: room,
            card_id: card_id.as_uuid(),
            claim_id: selected.claim_id.unwrap(),
            staged_revision_id: selected.id,
            instance: "ordinary-project-work".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: serde_json::from_value(json!({"hash":"b".repeat(64),"size_bytes":12}))
                .unwrap(),
        };
        let data = conn_as(executor.clone(), persona);
        reviewed::bind_submission(&data, name, persona, selection.clone())
            .await
            .unwrap();
        let repo = airc_work::RepoId::new("acme/ordinary-project").unwrap();
        let create = |card_id, reviews| {
            WorkEvent::CardCreated(airc_work::CardCreated {
                card_id,
                repo: repo.clone(),
                title: "ordinary work with reviewed deliverable".into(),
                body: None,
                priority: airc_work::Priority::P1,
                lane_id: None,
                created_by: owner,
                created_at_ms: 1,
                reviews,
                origin: None,
            })
        };
        let claim = |card_id, claim_id, holder| {
            WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
                card_id,
                claim_id,
                owner: holder,
                ttl_ms: 1000,
                claimed_at_ms: 2,
            })
        };
        let submitted = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(selection.submission_id),
            card_id,
            claim_id: airc_work::ClaimId::from_uuid(selection.claim_id),
            instance: selection.instance.clone(),
            base_sha: selection.base_sha.clone(),
            artifact: selection.artifact.clone(),
            publisher: owner,
            submitted_at_ms: 3,
        };
        let review_card = airc_work::WorkCardId::new();
        let review_claim = airc_work::ClaimId::new();
        let pass = WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            card_id,
            submission_id: submitted.submission_id,
            artifact: submitted.artifact.clone(),
            review_card_id: review_card,
            review_claim_id: review_claim,
            reviewer,
            outcome: WorkReviewOutcome::Passed,
            evidence: submitted.artifact.clone(),
            reviewed_at_ms: 4,
        };
        let prefix = [
            (create(card_id, None), owner),
            (claim(card_id, submitted.claim_id, owner), owner),
            (WorkEvent::WorkSubmitted(submitted.clone()), owner),
            (create(review_card, Some(card_id)), reviewer),
            (claim(review_card, review_claim, reviewer), reviewer),
        ];
        // Existing codec/replay boundary, including authoritative transcript peer.
        let project = |tail: Vec<(WorkEvent, airc_core::PeerId)>| {
            airc_work::project_transcript_work_events(
                prefix
                    .iter()
                    .cloned()
                    .chain(tail)
                    .enumerate()
                    .map(|(n, (event, peer_id))| {
                        let (headers, body) = airc_work::encode_work_event(&event).unwrap();
                        airc_core::TranscriptEvent {
                            event_id: airc_core::EventId::new(),
                            room_id: airc_core::RoomId::from_uuid(room),
                            peer_id,
                            client_id: airc_core::ClientId::new(),
                            kind: airc_core::TranscriptKind::System,
                            occurred_at_ms: event.occurred_at_ms(),
                            lamport: n as u64 + 1,
                            target: airc_core::MentionTarget::All,
                            headers,
                            body: Some(body),
                            attachment: None,
                            receipt: None,
                            metadata: serde_json::Value::Null,
                        }
                    }),
            )
            .unwrap()
        };
        let never_submit = continuum_client::mock::MockTransport::new();
        never_submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("unaccepted/failed/unknown review must not submit")
        });
        let refused_conn = settlement_connection(executor.clone(), persona, never_submit, None);
        for outcome in [WorkReviewOutcome::Failed, WorkReviewOutcome::Unknown] {
            let review = WorkSubmissionReview {
                outcome,
                ..pass.clone()
            };
            let board = project(vec![(WorkEvent::WorkSubmissionReviewed(review), reviewer)]);
            let result = reviewed::consume_review(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                pass.review_id,
            )
            .await
            .unwrap();
            assert_eq!(
                result.state,
                if outcome == WorkReviewOutcome::Failed {
                    State::FailedReview
                } else {
                    State::UnknownReview
                }
            );
        }
        let self_card = airc_work::WorkCardId::new();
        let self_claim = airc_work::ClaimId::new();
        let self_review = WorkSubmissionReview {
            review_card_id: self_card,
            review_claim_id: self_claim,
            reviewer: owner,
            ..pass.clone()
        };
        let self_board = project(vec![
            (create(self_card, Some(card_id)), owner),
            (claim(self_card, self_claim, owner), owner),
            (WorkEvent::WorkSubmissionReviewed(self_review), owner),
        ]);
        assert_eq!(
            reviewed::consume_review(
                &refused_conn,
                name,
                persona,
                room,
                &self_board,
                pass.review_id
            )
            .await
            .unwrap()
            .state,
            State::IndependentReviewRequired,
            "signed self-review remains visible without positive learning credit"
        );
        let spoofed = project(vec![(
            WorkEvent::WorkSubmissionReviewed(pass.clone()),
            owner,
        )]);
        assert!(reviewed::consume_review(
            &refused_conn,
            name,
            persona,
            room,
            &spoofed,
            pass.review_id
        )
        .await
        .is_err());
        let board = project(vec![(
            WorkEvent::WorkSubmissionReviewed(pass.clone()),
            reviewer,
        )]);
        let unrelated_transport = continuum_client::mock::MockTransport::new();
        unrelated_transport.respond_to("data/ensure-schema", |_| {
            panic!("unrelated observer must not open credit storage")
        });
        unrelated_transport.respond_to("genome/training-trigger/submit", |_| {
            panic!("unrelated observer must not transfer publisher credit")
        });
        let unrelated = Connection::new(unrelated_transport);
        assert!(reviewed::consume_observed_review(
            &unrelated,
            "unrelated-resident",
            reviewer.as_uuid(),
            room,
            &board,
            pass.review_id
        )
        .await
        .unwrap()
        .is_none());
        assert!(reviewed::consume_review(
            &refused_conn,
            name,
            persona,
            Uuid::new_v4(),
            &board,
            pass.review_id
        )
        .await
        .is_err());
        let submit = continuum_client::mock::MockTransport::new();
        let selected_id = selected.id;
        let review_id = pass.review_id.as_uuid();
        submit.respond_to("genome/training-trigger/submit", move |params| {
            assert_eq!(params["submissionId"], json!(selected_id));
            assert_eq!(params["baseModel"], "actual-model");
            assert_eq!(
                params["examples"][0]["metadata"]["reviewedSubmission"]["review"]["review_id"],
                json!(review_id)
            );
            Err(ClientError::Transport("destination ACK lost".into()))
        });
        let conn = settlement_connection(executor, persona, submit, None);
        assert!(reviewed::consume_observed_review(
            &conn,
            name,
            persona,
            room,
            &board,
            pass.review_id
        )
        .await
        .is_err());
        let pending = reviewed::credit_status(&data, name, selection.submission_id)
            .await
            .unwrap();
        assert_eq!(pending.state, State::AwaitingAcceptance);
        assert_eq!(pending.decision_review_id, Some(review_id));
        let mut later = receipts;
        later.push(served_receipt("later-request", "actual-model"));
        assert!(
            capture
                .record(&settlement_acts(8), &later, None, true)
                .await
        );
        assert!(
            reviewed::read_one::<_, StagedCredit>(&data, name, &selected.id.to_string())
                .await
                .unwrap()
                .is_none()
        );
        let competing = WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            ..pass.clone()
        };
        let board = project(vec![
            (WorkEvent::WorkSubmissionReviewed(pass.clone()), reviewer),
            (
                WorkEvent::WorkSubmissionReviewed(competing.clone()),
                reviewer,
            ),
        ]);
        assert_eq!(
            reviewed::consume_review(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                competing.review_id
            )
            .await
            .unwrap()
            .state,
            State::AnotherReviewSelected
        );
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &selected,
            Some(json!({"success":false,"errorKind":"DispatchFailed",
            "acceptance":{"submissionId":selected.id,"replayed":true}})),
        );
        let reopened = settlement_connection(data_runtime(), persona, submit, None);
        let result = reviewed::consume_observed_review(
            &reopened,
            name,
            persona,
            room,
            &board,
            pass.review_id,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            result.state,
            State::Accepted,
            "durable acceptance is independent of dispatch success"
        );
        assert_eq!(
            result
                .destination
                .unwrap()
                .acceptance
                .unwrap()
                .submission_id,
            selected.id
        );
        assert_eq!(
            reviewed::consume_review(&refused_conn, name, persona, room, &board, pass.review_id)
                .await
                .unwrap()
                .state,
            State::Accepted,
            "accepted replay does not dispatch again"
        );
        assert_eq!(
            stored_credit_rows(&data, name).await.len(),
            1,
            "newer staging remains intact"
        );
    }

    // what this catches: 6c36c24d — publication pins a selected immutable copy,
    // not the periodically replaced row; overlapping bindings roll back whole.
    #[tokio::test]
    async fn submission_binding_survives_replacement_and_reserves_generations_atomically() {
        use reviewed::{
            CreditBindingError, CreditGenerationReservation, CreditTransferIntent,
            SubmissionSelection, WorkCreditBinding,
        };
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let name = "immutable-work-binding";
        let receipts = vec![served_receipt("generation-a", "model-a")];
        let (mut capture, selected) =
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts.clone()).await;
        let data = conn_as(executor.clone(), persona);
        let selection = SubmissionSelection {
            submission_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            card_id: card,
            claim_id: selected.claim_id.unwrap(),
            staged_revision_id: selected.id,
            instance: "ordinary-project-card".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: serde_json::from_value(json!({
                "hash":"b".repeat(64),"size_bytes":5,"mime":"text/x-diff"
            }))
            .unwrap(),
        };
        let binding = reviewed::bind_submission(&data, name, persona, selection.clone())
            .await
            .unwrap();
        assert_eq!(binding.transfer_intent_id, selected.id);
        let mut later = receipts;
        later.push(served_receipt("generation-b", "model-a"));
        assert!(
            capture
                .record(
                    &settlement_acts(8),
                    &later,
                    Some("The later revision includes an additional inspection."),
                    true
                )
                .await
        );
        let current = stored_credit_rows(&data, name).await.pop().unwrap();
        assert_ne!(current.id, selected.id);
        assert!(
            reviewed::read_one::<_, StagedCredit>(&data, name, &selected.id.to_string())
                .await
                .unwrap()
                .is_none()
        );

        // New caller/executor uses the durable binding, including after a lost
        // publication acknowledgement. It does not require the old working row.
        let reopened = conn_as(data_runtime(), persona);
        let replay = reviewed::bind_submission(&reopened, name, persona, selection.clone())
            .await
            .unwrap();
        let intent = reviewed::read_one::<_, CreditTransferIntent>(
            &reopened,
            name,
            &replay.transfer_intent_id.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(intent.snapshot.completion, selected.completion);
        assert_eq!(intent.snapshot.receipts, selected.receipts);
        let mut changed = selection.clone();
        changed.artifact.size_bytes += 1;
        assert!(matches!(
            reviewed::bind_submission(&data, name, persona, changed).await,
            Err(CreditBindingError::ConflictingSubmission(_))
        ));

        let mut overlapping = selection.clone();
        overlapping.submission_id = Uuid::new_v4();
        overlapping.staged_revision_id = current.id;
        assert!(
            reviewed::bind_submission(&data, name, persona, overlapping.clone())
                .await
                .is_err()
        );
        assert!(
            reviewed::read_one::<_, WorkCreditBinding>(
                &data,
                name,
                &overlapping.submission_id.to_string()
            )
            .await
            .unwrap()
            .is_none(),
            "failed child reservation rolls back the new binding"
        );
        assert!(
            reviewed::read_one::<_, CreditGenerationReservation>(&data, name, "generation-b")
                .await
                .unwrap()
                .is_none(),
            "rollback leaves no partial reservation of the new generation"
        );
        assert!(
            reviewed::read_one::<_, CreditTransferIntent>(&data, name, &current.id.to_string())
                .await
                .unwrap()
                .is_none(),
            "failed binding also rolls back its preserved snapshot"
        );
        assert!(
            matches!(
                reviewed::reserve_transfer(&data, name, &selected, None).await,
                Err(CreditBindingError::Overlap)
            ),
            "the legacy grader cannot train a bound snapshot"
        );
        reviewed::reserve_transfer(&data, name, &intent.snapshot, Some(selection.submission_id))
            .await
            .unwrap();
        assert_eq!(stored_credit_rows(&data, name).await[0].id, current.id);
    }

    // what this catches: 6c36c24d — legacy refusal/uncertain acknowledgement
    // cannot leave generation reservations whose replaced payload is lost.
    #[tokio::test]
    async fn refused_legacy_transfer_reopens_its_exact_payload_after_periodic_replacement() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        for (name, refused) in [
            (
                "legacy-refused",
                Some(
                    json!({"success":false,"errorKind":"PersistenceFailed","error":"unavailable"}),
                ),
            ),
            ("legacy-uncertain", None),
        ] {
            let executor = data_runtime();
            let persona = Uuid::new_v4();
            let receipts = vec![served_receipt("original-generation", "actual-model")];
            let (mut capture, original) = stage_settlement_fixture(
                executor.clone(),
                persona,
                name,
                Uuid::new_v4(),
                receipts.clone(),
            )
            .await;
            let submit = continuum_client::mock::MockTransport::new();
            script_settlement_reply(&submit, &original, refused);
            let conn = settlement_connection(executor.clone(), persona, submit, None);
            assert!(!matches!(
                settle_staged_row(&conn, persona, name, &original, true).await,
                Ok(true)
            ));
            let mut later_receipts = receipts;
            later_receipts.push(served_receipt("later-generation", "actual-model"));
            assert!(
                capture
                    .record(
                        &settlement_acts(8),
                        &later_receipts,
                        Some("Later progress is preserved separately."),
                        true
                    )
                    .await
            );
            let reopened_executor = data_runtime();
            let reopened = conn_as(reopened_executor.clone(), persona);
            assert!(reviewed::read_one::<_, StagedCredit>(
                &reopened,
                name,
                &original.id.to_string()
            )
            .await
            .unwrap()
            .is_none());
            reviewed::ensure_storage(&reopened, name).await.unwrap();
            let intent = reviewed::read_one::<_, reviewed::CreditTransferIntent>(
                &reopened,
                name,
                &original.id.to_string(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(intent.work_submission_id, None);
            assert_eq!(intent.snapshot.id, original.id);
            assert_eq!(intent.snapshot.prompt, original.prompt);
            assert_eq!(intent.snapshot.completion, original.completion);
            assert_eq!(intent.snapshot.receipts, original.receipts);
            assert_eq!(intent.snapshot.served, original.served);
            let newer = stored_credit_rows(&reopened, name).await.pop().unwrap();
            assert_ne!(newer.id, original.id);

            let submit = continuum_client::mock::MockTransport::new();
            script_settlement_reply(
                &submit,
                &original,
                Some(json!({
                    "success":true,"outcome":"AlreadyAccepted",
                    "acceptance":{"submissionId":original.id,"replayed":true}
                })),
            );
            submit.respond_to("genome/training-trigger/submit", |_| {
                panic!("overlap reached destination")
            });
            let conn = settlement_connection(reopened_executor, persona, submit, None);
            assert!(
                settle_staged_row(&conn, persona, name, &intent.snapshot, true)
                    .await
                    .unwrap()
            );
            assert!(!settle_staged_row(&conn, persona, name, &newer, true)
                .await
                .unwrap());
            assert_eq!(stored_credit_rows(&reopened, name).await[0].id, newer.id);
            assert!(
                reviewed::read_one::<_, reviewed::CreditTransferIntent>(
                    &reopened,
                    name,
                    &original.id.to_string()
                )
                .await
                .unwrap()
                .is_some(),
                "accepted evidence remains inspectable"
            );
        }
    }

    // what this catches: 41f4e3ef / 6c36c24d — an older verdict drain must never
    // delete newer progress, nor permit its cumulative successor to train twice.
    #[tokio::test]
    async fn settlement_of_an_older_snapshot_cannot_delete_newer_progress() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let name = "overlapping-settlement";
        let receipts: Vec<_> = (0..4)
            .map(|n| served_receipt(&format!("generation-{n}"), "model-a"))
            .collect();
        let (mut capture, older) =
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts.clone()).await;
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &older,
            Some(json!({
                "success":true, "outcome":"BatchAppended",
                "acceptance":{"submissionId":older.id,"replayed":false}
            })),
        );
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let conn = settlement_connection(
            executor.clone(),
            persona,
            submit,
            Some((entered.clone(), release.clone())),
        );
        let mut drain = Box::pin(settle_staged_row(&conn, persona, name, &older, true));
        tokio::select! {
            _ = entered.notified() => {},
            result = &mut drain => panic!("drain settled before its paused receipt: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => panic!("drain never submitted"),
        }
        let mut later_receipts = receipts;
        later_receipts
            .extend((4..8).map(|n| served_receipt(&format!("generation-{n}"), "model-a")));
        assert!(
            capture
                .record(
                    &settlement_acts(8),
                    &later_receipts,
                    Some("The final source review is complete."),
                    true
                )
                .await
        );
        let data = conn_as(executor.clone(), persona);
        let before = stored_credit_rows(&data, name).await;
        assert_eq!(before.len(), 1);
        let newer = before.into_iter().next().unwrap();
        assert_ne!(
            newer.id, older.id,
            "changed content is a new immutable snapshot"
        );
        release.notify_one();
        assert!(drain.await.unwrap());
        let remaining = stored_credit_rows(&data, name).await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, newer.id);
        assert_eq!(remaining[0].receipts, later_receipts);
        let children = data
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection":StagedCreditGeneration::COLLECTION,
                    "dbPath":format!("@persona:{name}")
                }),
            )
            .await
            .unwrap();
        assert_eq!(children["items"].as_array().unwrap().len(), 8);

        let submit = continuum_client::mock::MockTransport::new();
        submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("overlapping cumulative revision reached destination submission")
        });
        let conn = settlement_connection(executor, persona, submit, None);
        assert!(
            !settle_staged_row(&conn, persona, name, &newer, true)
                .await
                .unwrap(),
            "a cumulative successor cannot inherit a verdict and train the same generations again"
        );
        assert_eq!(stored_credit_rows(&data, name).await[0].id, newer.id);
    }

    // what this catches: 41f4e3ef — one row's served model cannot label its
    // siblings, and neither a refusal nor unknown attribution transfers ownership.
    #[tokio::test]
    async fn settlement_retains_unaccepted_rows_and_uses_each_rows_actual_model() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let name = "settlement-models";
        let (_, a) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![served_receipt("a", "model-a")],
        )
        .await;
        let (_, b) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![served_receipt("b", "model-b")],
        )
        .await;
        let (_, mixed) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![
                served_receipt("mixed-a", "model-a"),
                served_receipt("mixed-b", "model-b"),
            ],
        )
        .await;
        let (_, faulted) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![receipt("all-faulted")],
        )
        .await;
        assert!(mixed.served.is_none() && faulted.served.is_none());
        let data = conn_as(executor.clone(), persona);
        let submit = continuum_client::mock::MockTransport::new();
        for reply in [
            Some(
                json!({"success":false,"errorKind":"PersistenceFailed","error":"disk unavailable"}),
            ),
            Some(json!({"success":true,"outcome":"BatchAppended"})),
            Some(
                json!({"success":true,"acceptance":{"submissionId":Uuid::new_v4(),"replayed":false}}),
            ),
            None,
        ] {
            script_settlement_reply(&submit, &a, reply);
        }
        let conn = settlement_connection(executor.clone(), persona, submit.clone(), None);
        for _ in 0..4 {
            assert!(!matches!(
                settle_staged_row(&conn, persona, name, &a, true).await,
                Ok(true)
            ));
            assert_eq!(stored_credit_rows(&data, name).await.len(), 4,
                "refusal, missing/mismatched receipt, or unknown acknowledgement preserves staged ownership");
        }
        // This response must remain queued until A is eligible. A deferred row
        // reaching submit either mismatches A's identity or consumes its receipt.
        script_settlement_reply(
            &submit,
            &a,
            Some(json!({
                "success":true,"outcome":"AlreadyAccepted",
                "acceptance":{"submissionId":a.id,"replayed":true}
            })),
        );
        assert!(!settle_staged_row(&conn, persona, name, &mixed, true)
            .await
            .unwrap());
        assert!(!settle_staged_row(&conn, persona, name, &faulted, true)
            .await
            .unwrap());
        assert!(!settle_staged_row(&conn, persona, name, &a, false)
            .await
            .unwrap());
        let mut claimless = a.clone();
        claimless.claim_id = None;
        claimless.owner = None;
        assert!(!settle_staged_row(&conn, persona, name, &claimless, true)
            .await
            .unwrap());
        let mut foreign = a.clone();
        foreign.owner = Some(Uuid::new_v4());
        assert!(!settle_staged_row(&conn, persona, name, &foreign, true)
            .await
            .unwrap());
        assert_eq!(stored_credit_rows(&data, name).await.len(), 4);
        assert!(settle_staged_row(&conn, persona, name, &a, true)
            .await
            .unwrap());
        script_settlement_reply(
            &submit,
            &b,
            Some(json!({
                "success":false,"errorKind":"DispatchFailed","error":"provider unavailable",
                "acceptance":{"submissionId":b.id,"replayed":false}
            })),
        );
        assert!(settle_staged_row(&conn, persona, name, &b, true)
            .await
            .unwrap());
        let remaining = stored_credit_rows(&data, name).await;
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|row| row.id == mixed.id));
        assert!(remaining.iter().any(|row| row.id == faulted.id));
    }
}
