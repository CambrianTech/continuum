//! The credit STAMP: which card a turn is credited to, by which hand, and with what
//! settled verdict. Captured once when a turn roots at a card, and carried unchanged onto
//! every training example it becomes. Moved out of `training_producer.rs` whole (card
//! 67af4fca), so the stamp's owner is one file.

use super::*;

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
    /// Which graded task the card is, read from its title at the same instant (goal 8):
    /// carried onto the staged row and every example it becomes, so a gene is never judged
    /// on a task it trained on (`cognition::held_out`).
    pub task: crate::cognition::held_out::CardTask,
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

/// A REAL settlement outcome — the second fact, arriving later than [`CapturedCredit`].
///
/// **A TERMINAL CARD STATE IS NOT A VERDICT.** `is_terminal_card_state` accepts
/// `closed | done | merged`; `merged` implies success but `closed` covers abandonment,
/// so deriving the outcome from the state string would collapse "settled" into
/// "succeeded" — the same conflation, one layer down. This type therefore carries a
/// verdict a grader actually produced, and there is no constructor from a state name.
#[derive(Debug, Clone, PartialEq)]
pub struct SettlementOutcome {
    passed: bool,
    /// The judge's normalized score, kept for the staged record.
    score: f64,
    /// WHY. `activity::Verdict` calls this "the receipt a citizen and a human both
    /// read, and the text the curriculum keeps when the task failed" — so a
    /// settlement that discards it throws away the only part a failure leaves behind.
    reason: String,
}

impl SettlementOutcome {
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
    /// The ONLY way to obtain an [`OutcomeStamp`]. Consuming a [`SettlementOutcome`]
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
    pub fn settle(self, verdict: SettlementOutcome) -> Option<OutcomeStamp> {
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
            task: crate::cognition::held_out::CardTask::of_card_title(&card.title),
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
