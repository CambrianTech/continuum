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

pub(crate) async fn try_pull_next_card(ctx: &HostedPersona, conversation: &dyn PersonaConversation) -> bool {
    let Some(citizen) = conversation.stream_citizen() else {
        return false;
    };
    // WIP = 1, enforced HERE and not by call order: a citizen who already holds a
    // card never pulls a second, even on a tick where the held-work gate deferred
    // (lane busy, env building). Also what keeps the board reads below to the idle.
    match citizen.active_claims().await {
        Ok(held) if held.is_empty() => {}
        Ok(_) => return false,
        Err(e) => {
            crate::probe!(
                class = "bench.round.pull_failed",
                persona = %ctx.identity.agent_name,
                error = %e.to_string(),
                "her claims are unreadable — no pull this tick (a second card on a \
                 misread would break WIP=1)"
            );
            return false;
        }
    }
    {
        let me = ctx.identity.peer_id.as_uuid();
        let last = LAST_PULL_MS.lock().unwrap_or_else(|e| e.into_inner()).get(&me).copied().unwrap_or(0); // unwrap_or: never pulled = 0
        if crate::modules::chat::now_ms().saturating_sub(last) < PULL_SETTLE_MS {
            return false;
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
            return false;
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
    {
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
            let open = citizen.claimable_cards_in(room, now).await.map(|o| o.len()).unwrap_or(0); // unwrap_or: an unreadable board counts every unsettled card as in flight (conservative)
            in_flight += round.dispatched.saturating_sub(round.settled).saturating_sub(open);
        }
        if lanes > 0 && in_flight >= lanes {
            static DEFERRED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            if DEFERRED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 50 == 0 {
                crate::probe!(
                    class = "bench.round.pull_deferred_wip",
                    persona = %ctx.identity.agent_name,
                    in_flight = in_flight as u64,
                    lanes = lanes as u64,
                    "no pull: the roster already holds as many cards as there are lanes (sampled 1/50)"
                );
            }
            return false;
        }
    }
    let candidates =
        crate::cognition::bench_round::pullable_cards(ctx.identity.peer_id.as_uuid(), &resident);
    if candidates.is_empty() {
        return false;
    }
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
        return false;
    };
    let card_id = airc_work::WorkCardId::from_uuid(next.card);
    match citizen.claim_card(card_id).await {
        Ok(true) => {
            LAST_PULL_MS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(ctx.identity.peer_id.as_uuid(), crate::modules::chat::now_ms());
            crate::probe!(
                class = "bench.round.pulled",
                persona = %ctx.identity.agent_name,
                card_id = %next.card,
                room = %next.run_room,
                "pulled the next Open card off the shared team deck — kanban pull; \
                 the held-work loop works it next tick"
            );
            true
        }
        // A teammate pulled it first — a lost race on a shared deck is normal, not
        // a fault; she simply tries the next card on a later tick.
        Ok(false) => false,
        Err(e) => {
            crate::probe!(
                class = "bench.round.pull_failed",
                persona = %ctx.identity.agent_name,
                card_id = %next.card,
                error = %e,
                "pull (claim) failed — will retry next tick"
            );
            false
        }
    }
}
