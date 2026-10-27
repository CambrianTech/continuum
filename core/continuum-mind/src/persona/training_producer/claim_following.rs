//! A turn's credit follows her claim while the turn runs (card 12ddd78f).
//!
//! A turn's card used to be read once, when it began. Kimi's initiative turn on the 5090
//! (2026-10-27) began holding nothing, so it had no capture; in the same turn she reopened
//! 5c5b7a08, claimed it, committed and submitted. Her submission named the generation that
//! issued it, no capture ever staged that generation, and its PASS review held forever
//! (`training.credit.provenance_held`, generation 822892a8). A turn that began holding one
//! claim and took another has the same hole one step later: its rows carry the claim it
//! began with, never the one she submitted under.
//!
//! So the held card is read again after every act that touched the board (`work/*`), and
//! when the card or its claim changed, the turn's credit becomes a new segment from THAT
//! act on. Generations before a claim are never credited to it: they were not taken under
//! it, which is the evidence floor [`CapturedCredit`] exists for. Each segment is an
//! ordinary [`TurnCreditCapture`]; this owner only decides where segments begin and end.

use std::sync::Arc;

use crate::ai::provenance::GenerationReceipt;
use crate::ai::types::ToolCall;
use crate::persona::active_work_source::AircWorkReader;

use super::{CapturedCredit, TurnCreditCapture};

/// Opens a capture for one claim: the live executor in production, a test's own runtime in
/// a fixture. `None` when no capture can be made (the executor is not installed).
pub type OpenCapture = Box<dyn Fn(&CapturedCredit) -> Option<TurnCreditCapture> + Send>;

/// One claim's stretch of the turn: the capture and where its acts and receipts begin.
struct Segment {
    credit: CapturedCredit,
    capture: TurnCreditCapture,
    from_act: usize,
    from_receipt: usize,
}

/// A point in the turn: an act and where its own generations begin.
#[derive(Debug, Clone, Copy)]
struct Boundary {
    act: usize,
    receipt: usize,
}

/// Her holds went unread after a board act: from `first` on, which claim the turn's
/// generations were taken under is unknown until a read answers. `last_board` is the latest
/// unread board act, the latest point a new claim could have been taken (Codex on #5523).
#[derive(Debug, Clone, Copy)]
struct Unsettled {
    first: Boundary,
    last_board: Boundary,
}

/// The turn's credit, following the card she holds.
pub struct ClaimFollowingCapture<R: AircWorkReader + ?Sized> {
    reader: Option<Arc<R>>,
    open: OpenCapture,
    current: Option<Segment>,
    /// The last claim a segment opened on, kept after a release: the turn was card-linked,
    /// so its speech is not ALSO submitted as unlinked conversation (`turn_settle`).
    last: Option<CapturedCredit>,
    /// Receipts already seen at the previous act, so an act's own generations are the ones
    /// after it.
    seen_receipts: usize,
    /// Set while a board act's hold is unread: nothing is staged past it until a read answers.
    unsettled: Option<Unsettled>,
}

/// Whether a credit is the same claim on the same card: a new claim on the same card is a
/// new segment (Kimi reopened and reclaimed 5c5b7a08 under e14d6bdc after 9fba1077).
fn same_claim(a: &CapturedCredit, b: &CapturedCredit) -> bool {
    a.card_id == b.card_id && a.claim.map(|c| c.claim_id) == b.claim.map(|c| c.claim_id)
}

/// PURE: an act that can change what she holds. Claims, releases, reopenings and submits are
/// all board verbs; nothing else changes a hold, so nothing else pays for a board read.
fn touches_the_board(calls: &[ToolCall]) -> bool {
    calls.iter().any(|call| call.name.starts_with("work/"))
}

impl<R: AircWorkReader + ?Sized> ClaimFollowingCapture<R> {
    /// `held` is the card the turn began with, if any; `reader` reads her holds again after
    /// a board act (`None` when the turn has no citizen to read them through).
    pub fn new(held: Option<&CapturedCredit>, reader: Option<Arc<R>>, open: OpenCapture) -> Self {
        let current = held.and_then(|credit| {
            open(credit).map(|capture| Segment { credit: credit.clone(), capture, from_act: 0, from_receipt: 0 })
        });
        // The card she began on is the turn's card even when no capture could open for it.
        let last = held.cloned();
        Self { reader, open, current, last, seen_receipts: 0, unsettled: None }
    }

    /// The last card the turn's credit stood on: the card it began with, or the one she
    /// claimed during it.
    pub fn into_credit(self) -> Option<CapturedCredit> {
        self.last
    }

    /// Her held card as the board reads it now. `Err` when it cannot be read.
    async fn held_now(reader: &R) -> Result<Option<CapturedCredit>, String> {
        let held = reader.active_claims().await.map_err(|error| error.to_string())?;
        Ok(crate::persona::work_focus::focus_workspace_card(held.iter()).map(CapturedCredit::from_selected_card))
    }

    /// Close the current segment with what it holds before `end`, as its final snapshot.
    async fn close_before(&mut self, acts: &[(String, Vec<ToolCall>)], receipts: &[GenerationReceipt], end: Boundary) -> bool {
        let Some(mut done) = self.current.take() else { return false };
        let acts_of = acts.get(done.from_act..end.act).unwrap_or_default(); // unwrap_or_default: a segment begins inside its turn; an empty slice stages nothing
        let receipts_of = receipts.get(done.from_receipt..end.receipt).unwrap_or_default(); // unwrap_or_default: as above
        done.capture.record(acts_of, receipts_of, None, true).await
    }

    /// Read her holds after `board_act` (or to settle an unread one) and move the segment
    /// boundary when the card or its claim changed.
    async fn follow(&mut self, acts: &[(String, Vec<ToolCall>)], receipts: &[GenerationReceipt], act_start: usize, board_act: bool) {
        let Some(reader) = self.reader.clone() else { return };
        let this = Boundary { act: acts.len().saturating_sub(1), receipt: act_start };
        let now = match Self::held_now(&reader).await {
            Ok(now) => now,
            Err(error) => {
                if board_act {
                    self.unsettled = Some(match self.unsettled {
                        None => Unsettled { first: this, last_board: this },
                        Some(open) => Unsettled { last_board: this, ..open },
                    });
                }
                continuum_base::probe!(
                    class = "training.credit.claim_unreadable",
                    acts = acts.len() as u64,
                    error = %error,
                    "her holds could not be read after a board act: nothing is staged from that act until a read says which claim it was taken under"
                );
                return;
            }
        };
        // An answering read on a board act: that act is the latest that could have taken
        // the claim the read shows, not the unread one before it (Codex on #5523).
        let pending = self.unsettled.take().map(|open| if board_act { Unsettled { last_board: this, ..open } } else { open });
        let unchanged = match (&self.current, &now) {
            (Some(segment), Some(now)) => same_claim(&segment.credit, now),
            (None, None) => true,
            _ => false,
        };
        // Unchanged with at most one board act in question: that act took no claim, so every
        // act since stays on the segment it was on. Unchanged across SEVERAL board acts proves
        // nothing (claim B, release B, back to A: Codex on #5523), so that stretch is no one's.
        let several = pending.is_some_and(|unread| unread.first.act != unread.last_board.act);
        if unchanged && !several {
            return;
        }
        // The old claim ends where its hold was last known; the new one begins at the latest
        // act that could have taken it. Between them (more than one unread board act) is
        // credited to neither. Known exactly, the claiming act belongs to the new claim and an
        // act that only let the card go stays with the card it let go.
        let after_this = Boundary { act: acts.len(), receipt: receipts.len() };
        let (end, start) = match pending {
            Some(unread) if unchanged => (unread.first, after_this),
            Some(unread) => (unread.first, unread.last_board),
            None if now.is_some() => (this, this),
            None => (after_this, this),
        };
        self.close_before(acts, receipts, end).await;
        if start.act > end.act {
            continuum_base::probe!(
                class = "training.credit.claim_uncertain_dropped",
                from_act = end.act as u64,
                to_act = start.act as u64,
                "acts between two unread board acts are credited to no claim: which one they were taken under is unknown"
            );
        }
        continuum_base::probe!(
            class = "training.credit.claim_followed",
            card = ?now.as_ref().map(|c| c.card_id),
            claim = ?now.as_ref().and_then(|c| c.claim.map(|r| r.claim_id)),
            from_act = start.act as u64,
            "the card she holds changed during the turn: its credit continues on the new claim from this act"
        );
        self.current = now.and_then(|credit| {
            (self.open)(&credit).map(|capture| Segment { credit, capture, from_act: start.act, from_receipt: start.receipt })
        });
        if let Some(segment) = &self.current {
            self.last = Some(segment.credit.clone());
        }
    }
}

#[async_trait::async_trait]
impl<R: AircWorkReader + ?Sized + 'static> crate::cognition::act_observe::TurnCredit for ClaimFollowingCapture<R> {
    async fn record(
        &mut self,
        acts: &[(String, Vec<ToolCall>)],
        receipts: &[GenerationReceipt],
        report: Option<&str>,
        final_snapshot: bool,
    ) -> bool {
        let act_start = self.seen_receipts.min(receipts.len());
        self.seen_receipts = receipts.len();
        // The final call repeats the last act, so only a new board act reads; an unread hold
        // is read again at every call until it answers.
        let board_act = !final_snapshot && acts.last().is_some_and(|(_, calls)| touches_the_board(calls));
        if board_act || self.unsettled.is_some() {
            self.follow(acts, receipts, act_start, board_act).await;
        }
        if let Some(unread) = self.unsettled {
            if !final_snapshot {
                return false;
            }
            // The turn ended with the hold still unread: what came before it is the claim's;
            // what came after is credited to no claim rather than guessed.
            continuum_base::probe!(
                class = "training.credit.claim_uncertain_dropped",
                from_act = unread.first.act as u64,
                to_act = acts.len() as u64,
                "the turn ended before her holds could be read again: its acts from the unread board act on are credited to no claim"
            );
            return self.close_before(acts, receipts, unread.first).await;
        }
        let Some(segment) = self.current.as_mut() else { return false };
        let acts_of = acts.get(segment.from_act..).unwrap_or_default(); // unwrap_or_default: a segment begins inside its turn; empty stages nothing
        let receipts_of = receipts.get(segment.from_receipt..).unwrap_or_default(); // unwrap_or_default: as above
        segment.capture.record(acts_of, receipts_of, report, final_snapshot).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> ToolCall {
        ToolCall { id: name.into(), name: name.into(), input: serde_json::json!({}) }
    }

    // what this catches: a hold read on every act (a board round trip per code/read), or
    // none after a claim. Only board verbs can change what she holds.
    #[test]
    fn only_a_board_act_rereads_her_holds() {
        assert!(touches_the_board(&[call("code/read"), call("work/claim")]));
        assert!(!touches_the_board(&[call("code/read"), call("code/shell")]));
        assert!(!touches_the_board(&[]));
    }

    // what this catches: a reclaim of the same card read as the same hold, so the turn's
    // rows keep the old claim and the new submission can never bind (Kimi, 9fba1077 then
    // e14d6bdc on 5c5b7a08).
    #[test]
    fn a_new_claim_on_the_same_card_is_a_new_hold() {
        let owner = airc_core::PeerId::new();
        let card = super::super::test_support::claimed_card_fixture(Some(owner), Some(airc_work::ClaimId::new()));
        let mut reclaimed = card.clone();
        reclaimed.claim_id = Some(airc_work::ClaimId::new());
        let first = CapturedCredit::from_selected_card(&card);
        assert!(same_claim(&first, &CapturedCredit::from_selected_card(&card)));
        assert!(!same_claim(&first, &CapturedCredit::from_selected_card(&reclaimed)));
    }
}
