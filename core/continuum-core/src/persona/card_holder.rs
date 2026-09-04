//! `CardHolder` — the ONE answer to "who holds this card, and is the hold still
//! good", shared by every surface that renders a work board.
//!
//! ### Why this module exists (six renderings of one fact)
//!
//! A work card's holder is ONE fact with two axes — WHO (a person) and WHETHER
//! the hold is still live (a lease). Enumerated 2026-08-06, six surfaces
//! answered it independently and disagreed:
//!
//! | surface | owner rendered as | lease |
//! |---|---|---|
//! | [`super::room_board_source`] (persona reads the board) | `YOU` for self, **8-hex** for peers | yes |
//! | [`super::service_loop`]'s work-board anchor | not rendered at all | was blind |
//! | `modules::work` (`work/list`) | 8-hex short id | yes |
//! | [`crate::ipc::positron_kanban_source`] (the human UI) | name — but only for a peer **present in the roster** | not carried |
//! | `airc work board` (the operator CLI) | published alias, `me` for self | yes |
//!
//! Only the operator's CLI rendered a person. Every surface a CITIZEN reads
//! showed an id no teammate can recognize — and Joel's rule is explicit
//! (2026-08-06): *"Should never say taken by 'someone' — tell them WHO.
//! Otherwise they can't reach out. And a persona could go down too. They
//! should be able, like you are me, to claim a card, diagnose etc. You've
//! turned convenience into disability."*
//!
//! The roster-based resolution the human UI uses is NOT sufficient for that
//! rule: the roster is PRESENCE-scoped, so the owner most worth naming — a
//! teammate who went down still holding a card — is exactly the one it cannot
//! name. Holder resolution therefore rides a durable per-peer alias lookup
//! ([`PeerNames`]), not presence.
//!
//! ### Lease liveness is part of the holder, not a separate question
//!
//! A lapsed claim is not "someone else's work"; the substrate already treats it
//! as reclaimable (`airc work next` offers exactly these). Rendering owner
//! WITHOUT lease state is what produced the live failure this module fixes:
//! 19 cards, 17 leases expired, and six citizens across two machines spent a
//! night announcing "there are no open tasks available" — each reading its own
//! expired claim as someone's active hold. They read the board correctly; the
//! board lied by omission. So [`Hold`] and the owner name are resolved
//! TOGETHER, once, and every consumer renders the same answer.
//!
//! [[the-compression-principle]] — one logical decision, one place.

use airc_work::WorkCard;

/// Whether a card's claim is still good. The lease, not the column, decides:
/// a `Claimed` card whose lease expired is takeable work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// A live claim — someone is genuinely on this card right now.
    Held,
    /// A claim whose lease expired. The holder stopped; the card is takeable
    /// (by the original holder, who most often is the reader, or by anyone).
    Lapsed,
    /// No claim at all.
    Unclaimed,
}

/// Durable peer-name lookup. Deliberately NOT the room roster: the roster is
/// presence-scoped, and the owner most worth naming is a teammate who went
/// down still holding a card. Production rides airc's published alias store;
/// callers pre-resolve the board's distinct owners into a map (the same shape
/// `airc work board` uses) so rendering stays synchronous.
pub trait PeerNames {
    /// The peer's published display name, or `None` when nothing has been
    /// published for it. `None` is honest — it renders as the short id, which
    /// is still addressable — never as "someone".
    fn name_of(&self, peer: &airc_core::PeerId) -> Option<String>;
}

/// A pre-resolved map satisfies the lookup. This is the production shape: the
/// caller does ONE async pass over the board's distinct owners, then renders.
impl PeerNames for std::collections::HashMap<airc_core::PeerId, String> {
    fn name_of(&self, peer: &airc_core::PeerId) -> Option<String> {
        self.get(peer).cloned()
    }
}

/// Nothing is known about any peer — every owner renders as its short id.
/// The honest degradation when the alias store is unreachable.
pub struct NoNames;

impl PeerNames for NoNames {
    fn name_of(&self, _peer: &airc_core::PeerId) -> Option<String> {
        None
    }
}

/// Who holds a card, whether the hold is live, and how to say it to a reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardHolder {
    pub hold: Hold,
    /// The holder's peer id — carried even when lapsed, because "who WAS on
    /// this" is how a citizen knows whom to reach out to.
    pub owner: Option<airc_core::PeerId>,
    /// The reader IS the holder. Her own claim must read as HERS — glass-boxed
    /// 2026-07-11: cards she held rendered as a hex prefix she cannot recognize
    /// as herself, so claimed work carried zero self-relevance.
    pub is_self: bool,
    /// How to name the holder to a reader: `YOU`, a published name, or the
    /// short id. Never empty, never "someone".
    pub display: String,
}

/// The lease truth, mirroring airc-lib's `is_active_claim` (work_roster.rs).
/// airc's roster already drops an expired lease from `active_claims` (that
/// transition fires the #156 lost-claim fact); a board view that kept rendering
/// the same card as HELD made one perception say "your claim lapsed" while the
/// next said "you HOLD it". Duplicated rather than imported because continuum
/// deps airc-protocol/-core, not airc-lib; this doc-link is the drift guard.
pub fn hold_of(card: &WorkCard, now_ms: u64) -> Hold {
    match (card.owner, card.claim_id) {
        (Some(_), Some(_)) if card.claim_expires_at_ms.is_some_and(|e| e > now_ms) => Hold::Held,
        (Some(_), _) => Hold::Lapsed,
        (None, _) => Hold::Unclaimed,
    }
}

/// The states `work/claim` refuses outright, mirroring airc-lib's guard
/// (`work.rs` → `AircError::WorkCardNotClaimable`). Duplicated rather than
/// imported because continuum deps airc-protocol/-core, not airc-lib — the same
/// drift-guard arrangement as [`hold_of`], and this doc-link is the guard.
///
/// A card in one of these states is DONE or under review; offering it as
/// available work can only produce a refusal the citizen cannot distinguish
/// from her own failure.
pub fn refused_by_claim(state: airc_work::model::CardState) -> bool {
    matches!(
        state,
        airc_work::model::CardState::Review
            | airc_work::model::CardState::Merged
            | airc_work::model::CardState::Closed
    )
}

/// Is this card takeable by anyone right now? The ONE claimability decision,
/// composing the lease ([`hold_of`]) with the column ([`refused_by_claim`]).
///
/// Every read surface that advertises available work must call THIS rather than
/// re-deriving it: before 2026-08-07 `room_board_source` and `CardHolder` each
/// filtered terminal states by hand, both missed `Review`, and the board offered
/// 11 unclaimable cards to citizens who had no way to know.
pub fn claimable_now(card: &WorkCard, now_ms: u64) -> bool {
    if refused_by_claim(card.state) {
        return false;
    }
    match hold_of(card, now_ms) {
        Hold::Held => false,
        Hold::Lapsed => true,
        Hold::Unclaimed => card.state == airc_work::model::CardState::Open,
    }
}

/// The 8-char short id every surface in the system uses to name a uuid.
fn short8(id: &uuid::Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// Resolve a card's holder ONCE, for every surface.
pub fn holder(
    card: &WorkCard,
    self_id: uuid::Uuid,
    now_ms: u64,
    names: &dyn PeerNames,
) -> CardHolder {
    let hold = hold_of(card, now_ms);
    let owner = card.owner;
    let is_self = owner.is_some_and(|o| o.as_uuid() == self_id);
    let display = match owner {
        None => "nobody".to_string(),
        Some(_) if is_self => "YOU".to_string(),
        // A published name when we have one; the short id when we don't. The
        // short id is addressable (it is what work/claim and airc DM take), so
        // an unnamed peer is still someone a citizen can reach — unlike
        // "someone", which is a dead end.
        Some(o) => names.name_of(&o).unwrap_or_else(|| short8(&o.as_uuid())),
    };
    CardHolder {
        hold,
        owner,
        is_self,
        display,
    }
}

impl CardHolder {
    /// Can the reader take this card right now? A lapsed hold is takeable —
    /// including (especially) her own. The card's column decides the rest:
    /// an `Open` card is claimable even with no claim ever made.
    ///
    /// Mirrors the WRITE side exactly. `work/claim` refuses `Review | Merged |
    /// Closed` outright (`airc-lib` `work.rs` → `WorkCardNotClaimable`), so a
    /// read surface that advertised them was handing a citizen a guaranteed
    /// bounce: measured 2026-08-07 on the live `#general` board, **11 of the 58
    /// cards offered as claimable were `Review`** — a citizen who picked one
    /// could only fail, and had no way to tell that from her own incapacity.
    /// Read and write must answer "can I take this" the same way, or the board
    /// is lying about what is available ([[the-compression-principle]]).
    pub fn claimable(&self, state: airc_work::model::CardState) -> bool {
        if refused_by_claim(state) {
            return false;
        }
        matches!(self.hold, Hold::Lapsed) || state == airc_work::model::CardState::Open
    }

    /// The holder phrase a citizen reads on a board line. Says WHO in every
    /// branch, and says plainly when the work is takeable.
    pub fn render(&self) -> String {
        match self.hold {
            Hold::Unclaimed => "unclaimed".to_string(),
            Hold::Held if self.is_self => "owner YOU".to_string(),
            Hold::Held => format!("owner {}", self.display),
            Hold::Lapsed if self.is_self => {
                "claim lapsed (was YOURS) — claimable, resume it".to_string()
            }
            Hold::Lapsed => format!(
                "claim lapsed (was {}) — claimable; reach out to {} before taking it",
                self.display, self.display
            ),
        }
    }

    /// The state tag a board line should LEAD with — what the card actually is
    /// **to a reader deciding whether to pick it up**, which is not always the
    /// column it sits in.
    ///
    /// A card whose claim lease has lapsed is takeable work. Its column still
    /// says `Claimed`, because nobody moved it back; the lease is what expired.
    /// Leading with the column therefore prints `[Claimed]` on work that is free,
    /// and a reader who scans state — which is what a state tag is FOR — concludes
    /// there is nothing to do.
    ///
    /// Found live 2026-08-06 by Benchy, who was right: *"The work board shows
    /// several claimed cards, but no open ones."* The board at that moment held
    /// 61 cards — 48 `Claimed`, 12 `Review`, 0 `Open` — with **59 of 61 leases
    /// stale**. Every one of those 59 was available, and every one of them read as
    /// taken. Worse, the same block contradicted itself: its lead said "N cards
    /// are claimable" and then printed N lines tagged `[Claimed]`. A reader
    /// resolves that contradiction toward the per-item label, every time.
    ///
    /// So the tag follows the HOLD, and the column is shown after it when the two
    /// disagree — nothing is hidden, the actionable fact just comes first.
    /// Terminal cards (`Merged` / `Closed`) are never claimable regardless of
    /// lease and keep their own column verbatim.
    pub fn state_tag(&self, card: &WorkCard) -> String {
        let terminal = matches!(
            card.state,
            airc_work::model::CardState::Merged | airc_work::model::CardState::Closed
        );
        match self.hold {
            Hold::Lapsed if !terminal => format!("CLAIMABLE (was {:?})", card.state),
            _ => format!("{:?}", card.state),
        }
    }

    /// Machine-readable lease word for command results (`work/list`), matching
    /// the CLI's vocabulary so one word means one thing everywhere.
    pub fn lease_word(&self) -> Option<&'static str> {
        match self.hold {
            Hold::Unclaimed => None,
            Hold::Held => Some("held"),
            Hold::Lapsed => Some("expired"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;
    use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};

    fn card(owner: Option<PeerId>, claimed: bool, expires_ms: Option<u64>) -> WorkCard {
        WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("CambrianTech/continuum").expect("valid repo id in fixture"),
            title: "a card".to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state: if owner.is_some() {
                CardState::Claimed
            } else {
                CardState::Open
            },
            owner,
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4())),
            claim_expires_at_ms: expires_ms,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: PeerId::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: None,
        }
    }

    // what this catches: a card the WRITE side refuses being advertised by a READ
    // surface as available work. `work/claim` rejects Review|Merged|Closed outright
    // (airc-lib WorkCardNotClaimable); before this, `claimable()` asked only
    // "lapsed-or-Open" and a Review card with a lapsed lease answered YES —
    // 11 of the 58 cards offered on the live #general board 2026-08-07. A citizen
    // who picked one got a refusal she could not tell from her own incapacity.
    // Regression for the claim/advertise split (see `refused_by_claim`).
    #[test]
    fn a_card_the_claim_verb_refuses_is_never_advertised_as_claimable() {
        let owner = PeerId::new();
        let lapsed = 1_000u64; // expiry in the past relative to `now`
        let now = 10_000u64;

        for state in [CardState::Review, CardState::Merged, CardState::Closed] {
            let mut c = card(Some(owner), true, Some(lapsed));
            c.state = state;
            let h = holder(&c, uuid::Uuid::new_v4(), now, &NoNames);
            assert_eq!(
                h.hold,
                Hold::Lapsed,
                "{state:?}: fixture must have a genuinely lapsed lease, or this \
                 test proves nothing"
            );
            assert!(
                !h.claimable(state),
                "{state:?} is refused by work/claim but was advertised as claimable"
            );
            assert!(
                !claimable_now(&c, now),
                "{state:?} must not appear in the board's [available work] lead"
            );
        }

        // Positive control: the SAME lapsed lease on a working card stays takeable,
        // so the guard above is excluding by state and not by lease.
        let mut live = card(Some(owner), true, Some(lapsed));
        live.state = CardState::InProgress;
        assert!(
            claimable_now(&live, now),
            "a lapsed claim on non-terminal work must remain takeable — that is the \
             whole reclaim path (#321)"
        );
    }

    fn named(peer: PeerId, name: &str) -> std::collections::HashMap<PeerId, String> {
        let mut m = std::collections::HashMap::new();
        m.insert(peer, name.to_string());
        m
    }

    #[test]
    fn a_peers_card_never_renders_as_an_unresolvable_id_when_a_name_is_published() {
        // what this catches: THE defect Joel named — "should never say taken by
        // 'someone', tell them WHO, otherwise they can't reach out". A live
        // peer-held card must name the peer, not an 8-hex prefix.
        let asha = PeerId::new();
        let me = uuid::Uuid::new_v4();
        let c = card(Some(asha), true, Some(u64::MAX));
        let h = holder(&c, me, 1_000, &named(asha, "Asha"));
        assert_eq!(h.render(), "owner Asha");
        assert!(!h.render().contains("someone"));
    }

    #[test]
    fn an_unnamed_peer_falls_back_to_an_addressable_short_id_never_to_someone() {
        // what this catches: the honest-degradation contract. When nothing is
        // published for a peer we must still hand the reader something she can
        // ACT on (work/claim + airc DM both take the short id) — never an
        // anonymous placeholder.
        let ghost = PeerId::new();
        let me = uuid::Uuid::new_v4();
        let c = card(Some(ghost), true, Some(u64::MAX));
        let h = holder(&c, me, 1_000, &NoNames);
        let short = short8(&ghost.as_uuid());
        assert_eq!(h.render(), format!("owner {short}"));
        assert!(!h.display.is_empty());
    }

    #[test]
    fn a_lapsed_peer_claim_names_who_to_reach_out_to_and_reads_as_claimable() {
        // what this catches: the two-axis contract. A lapsed hold must say WHO
        // held it (so a citizen can coordinate rather than silently steal) AND
        // that it is takeable. Rendering owner without lease state is what made
        // 17 stale claims read as active work.
        let atlas = PeerId::new();
        let me = uuid::Uuid::new_v4();
        let c = card(Some(atlas), true, Some(500)); // expired at now=1000
        let h = holder(&c, me, 1_000, &named(atlas, "Atlas"));
        assert_eq!(h.hold, Hold::Lapsed);
        assert!(h.render().contains("Atlas"));
        assert!(h.render().contains("claimable"));
        assert!(h.claimable(CardState::Claimed));
    }

    #[test]
    fn her_own_lapsed_claim_reads_as_hers_and_resumable() {
        // what this catches: the live stall. Every citizen was reading its OWN
        // expired claim and concluding the work was taken. Her own lapsed hold
        // must read as hers and as resumable — the exact sentence that was
        // missing while six citizens said "no open tasks available".
        let me_peer = PeerId::new();
        let c = card(Some(me_peer), true, Some(500));
        let h = holder(&c, me_peer.as_uuid(), 1_000, &NoNames);
        assert!(h.is_self);
        assert_eq!(
            h.render(),
            "claim lapsed (was YOURS) — claimable, resume it"
        );
        assert!(h.claimable(CardState::Claimed));
    }

    #[test]
    fn a_live_claim_is_not_claimable_and_an_open_card_always_is() {
        // what this catches: the guard against over-correcting. Fixing the
        // stale-lease blindness must NOT make live claims look takeable —
        // that would turn the stall into claim-stealing (#157).
        let peer = PeerId::new();
        let me = uuid::Uuid::new_v4();
        let live = holder(&card(Some(peer), true, Some(u64::MAX)), me, 1_000, &NoNames);
        assert_eq!(live.hold, Hold::Held);
        assert!(!live.claimable(CardState::Claimed));

        let open = holder(&card(None, false, None), me, 1_000, &NoNames);
        assert_eq!(open.hold, Hold::Unclaimed);
        assert_eq!(open.render(), "unclaimed");
        assert!(open.claimable(CardState::Open));
    }

    #[test]
    fn an_owner_with_no_claim_id_is_lapsed_not_held() {
        // what this catches: the half-claim. A card carrying an owner but no
        // claim (or no expiry) is not an active hold — treating it as one is
        // how a card becomes permanently unreachable with nobody working it.
        let peer = PeerId::new();
        let me = uuid::Uuid::new_v4();
        let h = holder(&card(Some(peer), false, None), me, 1_000, &NoNames);
        assert_eq!(h.hold, Hold::Lapsed);
        assert!(h.claimable(CardState::Claimed));
    }
}
