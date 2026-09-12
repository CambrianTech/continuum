//! THE CARD LEDGER — the saved state of a thought.
//!
//! A held card's evidence ledger: what is known, the competing hypotheses with their
//! discriminating tests, the load-bearing unknown, the decided fix. Written by the
//! holder at the end of a work turn (`work/note`), read FIRST by whoever holds the card
//! next — the same mind after a reboot, a peer after a hand-off, the reviewer at the
//! gate, a bigger tier borrowed for one turn. Re-entry becomes "read the ledger, run
//! the pending test" instead of re-orientation.
//!
//! Measured 2026-09-12: 78 work acts opened with "let me re-establish ground truth" on
//! cards that were already worked; one holder re-read the same file range across turns
//! because the act survived in memory and the observation did not (card 516738b4).
//!
//! Storage: a wall record on the card's run room (category [`LEDGER_WALL_CATEGORY`]),
//! keyed by card id, newest wins — the same shape as the room's standing and children
//! records, durable and airc-replicated, visible to the human on the card. Never a
//! process-local note (a note dies with the process; the claim lives on the board).
//! Design: docs/planning/UNCERTAINTY-IS-THE-SCHEDULER-SIGNAL.md, build order 2.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const LEDGER_WALL_CATEGORY: &str = "card-ledger";

/// One competing explanation and the observation that would settle it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/experience/LedgerHypothesis.ts")]
pub struct LedgerHypothesis {
    pub claim: String,
    #[serde(default)]
    pub evidence_for: Vec<String>,
    #[serde(default)]
    pub evidence_against: Vec<String>,
    /// The cheapest observation that would confirm or kill this claim.
    #[serde(default)]
    pub test: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/experience/CardLedger.ts")]
pub struct CardLedger {
    #[ts(type = "string")]
    pub card_id: uuid::Uuid,
    /// Facts established by observation this turn or before: file ranges read with what
    /// they showed, tests run with the failing assertion, commands and their outcome.
    #[serde(default)]
    pub known: Vec<String>,
    #[serde(default)]
    pub hypotheses: Vec<LedgerHypothesis>,
    /// The single unknown the answer turns on.
    #[serde(default)]
    pub unknown: String,
    /// The next observation to run — the first act of the next turn.
    #[serde(default)]
    pub next_test: String,
    /// Once decided: file:line and the intended edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decided_fix: Option<String>,
    #[ts(type = "number")]
    pub at_ms: u64,
    #[ts(type = "string")]
    pub by: uuid::Uuid,
}

/// WHERE A LEDGER LIVES — the adapter seam. One implementation today (a wall record on the
/// card's run room); the shape a positron-served card body, a grid-replicated engram, or a
/// test fixture would implement. Readers and writers hold `dyn LedgerStore`, never the wall.
#[async_trait::async_trait]
pub trait LedgerStore: Send + Sync {
    /// Record `ledger` for its card in `room`; the newest record wins on read.
    async fn write(&self, room: &airc_lib::Room, ledger: &CardLedger) -> Result<(), String>;
    /// The newest ledger for `card` in `room`, `None` when nobody has written one.
    async fn read(&self, room: &airc_lib::Room, card: uuid::Uuid) -> Result<Option<CardLedger>, String>;
}

/// The wall-record store: the same durable, airc-replicated shape as a room's standing and
/// children records, visible to the human on the card.
pub struct WallLedgerStore {
    airc: std::sync::Arc<airc_lib::Airc>,
}

impl WallLedgerStore {
    pub fn new(airc: std::sync::Arc<airc_lib::Airc>) -> Self {
        Self { airc }
    }
}

#[async_trait::async_trait]
impl LedgerStore for WallLedgerStore {
    async fn write(&self, room: &airc_lib::Room, ledger: &CardLedger) -> Result<(), String> {
        let body = serde_json::to_string(ledger).map_err(|e| e.to_string())?; // boundary: a wall record is airc's wire + store, read back by every later holder
        self.airc
            .publish_wall_post_in(room, LEDGER_WALL_CATEGORY.to_string(), body, None)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn read(&self, room: &airc_lib::Room, card: uuid::Uuid) -> Result<Option<CardLedger>, String> {
        let posts = self
            .airc
            .wall_posts_in(room, Some(LEDGER_WALL_CATEGORY))
            .await
            .map_err(|e| e.to_string())?;
        Ok(project_ledger(&posts, card))
    }
}

/// The newest ledger for `card` among a room's ledger posts, or `None` when no holder
/// has written one. A post that does not parse is skipped, loudly, never guessed at.
pub fn project_ledger(
    posts: &[airc_core::doctrine::WallPostPublished],
    card: uuid::Uuid,
) -> Option<CardLedger> {
    posts
        .iter()
        .rev()
        .filter_map(|post| match serde_json::from_str::<CardLedger>(&post.body) {
            Ok(l) => Some(l),
            Err(e) => {
                crate::probe!(
                    class = "card.ledger.unreadable",
                    post = %post.post_id,
                    error = %e.to_string(),
                    "a ledger record could not be read — skipped, not guessed"
                );
                None
            }
        })
        .find(|l| l.card_id == card)
}

/// The ledger as the fact her work turn opens with: compact, one line per entry, capped so
/// it is read, never a second transcript.
pub fn render_for_turn(l: &CardLedger) -> String {
    const CAP: usize = 6;
    let mut out = String::from("[ledger] What this card's ledger holds (yours or the previous holder's):");
    if !l.known.is_empty() {
        out.push_str("\n  Known:");
        for k in l.known.iter().take(CAP) {
            out.push_str(&format!("\n   - {}", k.trim()));
        }
        if l.known.len() > CAP {
            out.push_str(&format!("\n   - (+{} more)", l.known.len() - CAP));
        }
    }
    if !l.hypotheses.is_empty() {
        out.push_str("\n  Hypotheses:");
        for h in l.hypotheses.iter().take(CAP) {
            let mut line = format!("\n   - {}", h.claim.trim());
            if !h.evidence_for.is_empty() {
                line.push_str(&format!(" [for: {}]", h.evidence_for.join("; ")));
            }
            if !h.evidence_against.is_empty() {
                line.push_str(&format!(" [against: {}]", h.evidence_against.join("; ")));
            }
            if !h.test.trim().is_empty() {
                line.push_str(&format!(" → test: {}", h.test.trim()));
            }
            out.push_str(&line);
        }
    }
    if !l.unknown.trim().is_empty() {
        out.push_str(&format!("\n  Load-bearing unknown: {}", l.unknown.trim()));
    }
    if let Some(fix) = l.decided_fix.as_deref().filter(|f| !f.trim().is_empty()) {
        out.push_str(&format!("\n  Decided fix: {}", fix.trim()));
    }
    if !l.next_test.trim().is_empty() {
        out.push_str(&format!("\n  NEXT (do this first): {}", l.next_test.trim()));
    }
    out.push_str(
        "\n  Do not re-establish what is Known. End this turn with work/note to update the ledger.",
    );
    out
}

/// The fact pinned when no ledger exists yet — the instruction is present on act one.
pub fn render_absent() -> String {
    "[ledger] No ledger exists for this card yet. Before your turn ends, write one with \
     work/note: what you established (file:range → what it showed; test → failing assertion), \
     the competing hypotheses each with the observation that would settle it, the one unknown \
     the answer turns on, and the next test to run. The next turn — yours or a peer's — opens \
     from it instead of re-orienting."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::doctrine::WallPostPublished;
    use airc_core::{PeerId, RoomId};

    fn post(body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: RoomId::from_uuid(uuid::Uuid::nil()),
            post_id: uuid::Uuid::new_v4(),
            category: LEDGER_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    // what this catches: the newest ledger for THIS card winning over an older one and over
    // another card's ledger in the same room; an unreadable record skipped rather than read
    // as empty (which would erase a holder's work at the next turn).
    #[test]
    fn the_newest_ledger_for_the_card_wins_and_junk_is_skipped() {
        let card = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let mk = |c: uuid::Uuid, unknown: &str, at: u64| {
            serde_json::to_string(&CardLedger {
                card_id: c,
                unknown: unknown.into(),
                at_ms: at,
                by: uuid::Uuid::from_u128(1),
                ..Default::default()
            })
            .unwrap()
        };
        let posts = vec![
            post(&mk(card, "old", 1)),
            post(&mk(other, "not mine", 2)),
            post(&mk(card, "new", 3)),
            post("{ not json"),
        ];
        let l = project_ledger(&posts, card).expect("a ledger exists");
        assert_eq!(l.unknown, "new");
        assert_eq!(project_ledger(&posts, uuid::Uuid::from_u128(9)), None);
    }

    // what this catches: the rendered fact losing the one line that changes the next act
    // (NEXT), or growing into a second transcript.
    #[test]
    fn the_turn_fact_leads_with_next_and_stays_short() {
        let l = CardLedger {
            card_id: uuid::Uuid::from_u128(7),
            known: (0..10).map(|i| format!("k{i}")).collect(),
            hypotheses: vec![LedgerHypothesis {
                claim: "precedence bug".into(),
                evidence_for: vec!["fixture reproduces".into()],
                evidence_against: vec![],
                test: "run rule test".into(),
            }],
            unknown: "which branch".into(),
            next_test: "cargo test -p ruff_linter era".into(),
            decided_fix: None,
            at_ms: 1,
            by: uuid::Uuid::from_u128(1),
        };
        let s = render_for_turn(&l);
        assert!(s.starts_with("[ledger]"));
        assert!(s.contains("NEXT (do this first): cargo test"));
        assert!(s.contains("(+4 more)"), "known is capped: {s}");
        assert!(s.contains("→ test: run rule test"));
        assert!(render_absent().contains("work/note"));
    }
}
