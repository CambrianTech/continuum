//! The pull: how an idle citizen takes the next card off a working round's
//! board. One policy, one file (carved out of `service_loop` 2026-09-05 — it
//! had been edited four times that day inside a 5,300-line file):
//! WIP = 1 per citizen, a settle window after her last pull, WIP = lanes across
//! the roster (board-true), residency as eligibility, then the claim through
//! her own hands. Nobody is told what to do; the world has, or has not, a slot.

use uuid::Uuid;

use crate::persona::service_loop::PersonaConversation;
use crate::persona::supervisor::HostedPersona;

/// PULL the next Open card off the shared team deck for a citizen who holds no
/// work — the kanban-pull half of team dynamics (Joel 2026-09-02: a team chooses
/// from the deck; they don't each work a fixed pushed pile). Deterministic: the
/// substrate claims the card when she is free, so a pull never depends on the
/// model emitting a claim tool call. WIP-limited to one by construction — the
/// caller only reaches here when she holds nothing workable, and once she holds
/// the pulled card the held-work branch works it before this fires again.
/// Returns true iff she pulled a card (now holds it).
/// When each citizen last pulled a card (ms). A pull is admitted only after the
/// previous claim has had time to land on the board: the WIP=1 read is eventually
/// consistent, and on 2026-09-05 two citizens each took two cards in one burst.
static LAST_PULL_MS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<Uuid, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
const PULL_SETTLE_MS: u64 = 120_000;

/// What the pull decided this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PullOutcome {
    /// She took a card; the held-work loop works it next tick.
    Pulled,
    /// The roster already holds as many cards as there are lanes: no slot for her.
    /// The self-cycle treats this as "watch the board" — no ambient deliberation,
    /// so the lanes stay with the holders.
    DeferredWip,
    /// Nothing to pull (she holds a card, nothing claimable, or a read failed).
    Nothing,
}

/// The deck entries a HOLDER may still take: review cards only, never for a parent
/// she holds herself. Pure, so the ordering rule is testable without a board.
pub(crate) fn review_candidates<C: HasCard>(
    candidates: Vec<C>,
    held: &[Uuid],
    review_parent: impl Fn(Uuid) -> Option<Uuid>,
) -> Vec<C> {
    candidates
        .into_iter()
        .filter(|c| match review_parent(c.card()) {
            Some(parent) => !held.contains(&parent),
            None => false,
        })
        .collect()
}

/// The one field the review filter needs off a deck entry.
pub(crate) trait HasCard {
    fn card(&self) -> Uuid;
}

impl HasCard for crate::cognition::bench_round::NextCard {
    fn card(&self) -> Uuid {
        self.card
    }
}


/// When this peer last pulled a card (ms), 0 if never this boot — the governor's hold
/// boundary.
pub(crate) fn last_pull_ms(peer: Uuid) -> u64 {
    LAST_PULL_MS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned clock still answers — the boundary must exist
        .get(&peer)
        .copied()
        .unwrap_or(0) // unwrap_or: never pulled this boot = 0 (every row counts, the old rule)
}

/// One row per citizen per minute on a no-pull exit: enough to name the reason
/// every time it changes, never a storm from a 3 s self-tick.
fn pull_probe_due(peer: Uuid) -> bool {
    static LAST: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<Uuid, u64>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    const EVERY_MS: u64 = 60_000;
    let now = crate::modules::chat::now_ms();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned throttle map still throttles — a probe cadence is never worth a panic
    let due = last.get(&peer).map_or(true, |t| now.saturating_sub(*t) >= EVERY_MS);
    if due {
        last.insert(peer, now);
    }
    due
}

pub(crate) async fn try_pull_next_card(ctx: &HostedPersona, conversation: &dyn PersonaConversation) -> PullOutcome {
    let Some(citizen) = conversation.stream_citizen() else {
        if pull_probe_due(ctx.identity.peer_id.as_uuid()) {
            crate::probe!(
                class = "bench.round.pull_none",
                persona = %ctx.identity.agent_name,
                reason = "no_citizen_stream",
                "no pull: this conversation has no airc citizen to pull through"
            );
        }
        return PullOutcome::Nothing;
    };
    // WIP = 1, enforced HERE and not by call order: a citizen who already holds a
    // card never pulls a second, even on a tick where the held-work gate deferred
    // (lane busy, env building). Also what keeps the board reads below to the idle.
    // …except a REVIEW card. The gate makes an owner's `done` a review card a
    // NON-owner must take, and with WIP = lanes every resident already holds a card,
    // so nobody was ever idle to take one: measured 2026-09-06 20:03Z, two owners done
    // for 40 minutes and zero reviewer pulls. A holder may take one review beside her
    // own card (review cards never count against the lane cap); never her own card's.
    let held: Vec<Uuid> = match citizen.active_claims().await {
        Ok(held) => held.iter().map(|c| c.card_id.as_uuid()).collect(),
        Err(e) => {
            crate::probe!(
                class = "bench.round.pull_failed",
                persona = %ctx.identity.agent_name,
                error = %e.to_string(),
                "her claims are unreadable — no pull this tick (a second card on a \
                 misread would break WIP=1)"
            );
            return PullOutcome::Nothing;
        }
    };
    let reviews_only = !held.is_empty();
    {
        let me = ctx.identity.peer_id.as_uuid();
        let last = LAST_PULL_MS.lock().unwrap_or_else(|e| e.into_inner()).get(&me).copied().unwrap_or(0); // unwrap_or: never pulled = 0
        if crate::modules::chat::now_ms().saturating_sub(last) < PULL_SETTLE_MS {
            // Not probed: a pull two minutes ago is the normal case, not an absence.
            return PullOutcome::Nothing;
        }
    }
    // ELIGIBILITY IS RESIDENCY: she pulls from the run rooms she is standing in. A
    // card is content of its room; any resident may work it.
    let resident: std::collections::HashSet<Uuid> = match citizen.subscribed_rooms().await {
        Ok(rooms) => rooms.into_iter().collect(),
        Err(e) => {
            crate::probe!(
                class = "bench.round.pull_failed",
                persona = %ctx.identity.agent_name,
                error = %e.to_string(),
                "her subscription set is unreadable — no pull this tick"
            );
            return PullOutcome::Nothing;
        }
    };
    // WIP = LANES (2026-09-05, Joel: "get this working"). Twelve citizens on five
    // lanes gave each ~two model calls an hour: 23 lane grants, 19 acts, 0 writes in
    // 55 minutes on a fully claimed round. A card only progresses when its holder
    // can decode, so the roster holds no more cards than the server has lanes; the
    // others stay resident, watch the board, and take review cards as they open.
    // BOARD-TRUE: in-flight = dispatched − settled − open-on-the-board per working
    // citizen round (the tracker's own owner column lags the board and read zero,
    // so the first cut of this cap never held). Organic: nobody is told what to
    // do — the world simply has no free slot.
    if !reviews_only {
        let lanes = crate::cognition::resource_admission::served_lane_count();
        let now = crate::modules::chat::now_ms();
        let mut in_flight = 0usize;
        for round in crate::cognition::bench_round::live_rounds() {
            if !round.stage.eq_ignore_ascii_case("working")
                || !round.driver.to_ascii_lowercase().contains("citizen")
            {
                continue;
            }
            let Ok(room) = Uuid::parse_str(&round.round_id) else { continue };
            if !resident.contains(&room) {
                continue;
            }
            // BOARD TRUTH, ONE PREDICATE: live holds in a holder's column
            // (card_holder::in_flight_now). The tracker arithmetic this replaces
            // (dispatched − settled − claimable) counted ownerless reviews and lapsed
            // holds of absent citizens as lanes in use — 2026-09-13 07:16Z: 5/5 "in
            // flight" with one live hold, every pull deferred, three cards open.
            in_flight += citizen.in_flight_cards_in(room, now).await.unwrap_or(round.dispatched.saturating_sub(round.settled)); // unwrap_or: an unreadable board counts every unsettled card as in flight (conservative, as before)
        }
        if lanes > 0 && in_flight >= lanes {
            // Once a minute per citizen, never sampled: a 1/50 sample on a four-coder
            // node hid every deferral for an hour (2026-09-12).
            if pull_probe_due(ctx.identity.peer_id.as_uuid()) {
                crate::probe!(
                    class = "bench.round.pull_deferred_wip",
                    persona = %ctx.identity.agent_name,
                    in_flight = in_flight as u64,
                    lanes = lanes as u64,
                    "no pull: the roster already holds as many cards as there are lanes"
                );
            }
            return PullOutcome::DeferredWip;
        }
    }
    let candidates =
        crate::cognition::bench_round::pullable_cards(ctx.identity.peer_id.as_uuid(), &resident);
    let candidates: Vec<_> = if reviews_only {
        review_candidates(candidates, &held, crate::cognition::bench_round::review_parent)
    } else {
        candidates
    };
    if candidates.is_empty() {
        if pull_probe_due(ctx.identity.peer_id.as_uuid()) {
            crate::probe!(
                class = "bench.round.pull_none",
                persona = %ctx.identity.agent_name,
                reason = if reviews_only { "no_review_cards" } else { "no_candidates" },
                resident_rooms = resident.len() as u64,
                "no pull: the decks in the rooms she stands in offer nothing"
            );
        }
        return PullOutcome::Nothing;
    }
    let candidate_count = candidates.len();
    // BOARD TRUTH decides what is takeable: the round tracker knows the deck, the
    // board knows who holds what. One board read per run room per self-tick.
    let now_ms = crate::persona::trace::now_ms();
    let mut claimable_by_room: std::collections::HashMap<Uuid, std::collections::HashSet<Uuid>> =
        std::collections::HashMap::new();
    let mut next = None;
    for cand in candidates {
        if !claimable_by_room.contains_key(&cand.run_room) {
            let open = match citizen.claimable_cards_in(cand.run_room, now_ms).await {
                Ok(cards) => cards.into_iter().collect(),
                Err(e) => {
                    crate::probe!(
                        class = "bench.round.pull_failed",
                        persona = %ctx.identity.agent_name,
                        room = %cand.run_room,
                        error = %e.to_string(),
                        "the run room's board is unreadable — no pull from it this tick"
                    );
                    std::collections::HashSet::new()
                }
            };
            claimable_by_room.insert(cand.run_room, open);
        }
        if claimable_by_room[&cand.run_room].contains(&cand.card) {
            next = Some(cand);
            break;
        }
    }
    let Some(next) = next else {
        if pull_probe_due(ctx.identity.peer_id.as_uuid()) {
            let rooms: Vec<String> = claimable_by_room
                .iter()
                .map(|(room, open)| format!("{}:{}", &room.to_string()[..8], open.len()))
                .collect();
            crate::probe!(
                class = "bench.round.pull_none",
                persona = %ctx.identity.agent_name,
                reason = "none_claimable",
                candidates = candidate_count as u64,
                rooms = rooms.join(","),
                "no pull: every candidate reads unclaimable on its room's board"
            );
        }
        return PullOutcome::Nothing;
    };
    let card_id = airc_work::WorkCardId::from_uuid(next.card);
    match citizen.claim_card(card_id).await {
        Ok(true) => {
            LAST_PULL_MS
                .lock()
                .unwrap_or_else(|e| e.into_inner())  // poisoned lock = read the last state, same policy as every lock in this crate
                .insert(ctx.identity.peer_id.as_uuid(), crate::modules::chat::now_ms());
            crate::probe!(
                class = "bench.round.pulled",
                persona = %ctx.identity.agent_name,
                card_id = %next.card,
                room = %next.run_room,
                "pulled the next Open card off the shared team deck — kanban pull; \
                 the held-work loop works it next tick"
            );
            PullOutcome::Pulled
        }
        // A teammate pulled it first — a lost race on a shared deck is normal, not
        // a fault; she simply tries the next card on a later tick.
        Ok(false) => PullOutcome::Nothing,
        Err(e) => {
            crate::probe!(
                class = "bench.round.pull_failed",
                persona = %ctx.identity.agent_name,
                card_id = %next.card,
                error = %e,
                "pull (claim) failed — will retry next tick"
            );
            PullOutcome::Nothing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Entry(Uuid);
    impl HasCard for Entry {
        fn card(&self) -> Uuid {
            self.0
        }
    }

    // what this catches (2026-09-06 20:03Z): two owners done, zero reviewer pulls —
    // holders never reached the deck. A holder takes review cards whose parent is not
    // hers; never a round card, never her own card's review.
    #[test]
    fn a_holder_takes_only_reviews_of_cards_she_does_not_hold() {
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let review_of_mine = Uuid::new_v4();
        let review_of_theirs = Uuid::new_v4();
        let round_card = Uuid::new_v4();
        let parent = move |c: Uuid| {
            if c == review_of_mine { Some(mine) } else if c == review_of_theirs { Some(theirs) } else { None }
        };
        let deck = vec![Entry(round_card), Entry(review_of_mine), Entry(review_of_theirs)];
        let takeable: Vec<Uuid> = review_candidates(deck, &[mine], parent).into_iter().map(|e| e.0).collect();
        assert_eq!(takeable, vec![review_of_theirs]);
    }
}
