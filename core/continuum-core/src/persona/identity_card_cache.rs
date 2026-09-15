//! Identity cards, fetched once per peer — never per tick.
//!
//! 2026-09-12: the positron presence emitter rendered every room every 2 s through
//! airc-lib's `room_roster_cards_in`, which round-trips the daemon for EVERY live
//! member's identity card on EVERY call. Fifty rooms × their members ÷ 2 s ≈ 250
//! daemon requests a second whenever citizens were seated (zero with none), the
//! daemon slowed under them, the requests outran their completions, and both
//! descriptor tables filled — the host reached `ENFILE`. A card names a peer; it
//! changes when the peer republishes it, not every two seconds. This cache is the
//! one place a card is fetched, with a TTL and an explicit invalidate; a peer with
//! no card is remembered as such for the same TTL so it is not re-asked per tick.
use std::sync::atomic::{AtomicU64, Ordering};

use airc_core::identity::Identity;
use airc_lib::PeerId;

/// How long a fetched card (or a confirmed absence) stands before it is re-asked.
pub const CARD_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
struct Entry {
    identity: Option<Identity>,
    fetched_ms: u64,
}

/// The cache itself, pure over a clock so its policy is testable without a daemon.
#[derive(Default)]
pub struct CardCache {
    entries: dashmap::DashMap<PeerId, Entry>,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
}

impl CardCache {
    /// The cached identity for `peer` if fresher than `ttl_ms` at `now_ms`.
    pub fn get(&self, peer: PeerId, now_ms: u64, ttl_ms: u64) -> Option<Option<Identity>> {
        let e = self.entries.get(&peer)?;
        if now_ms.saturating_sub(e.fetched_ms) < ttl_ms {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(e.identity.clone())
        } else {
            None
        }
    }

    pub fn put(&self, peer: PeerId, identity: Option<Identity>, now_ms: u64) {
        self.misses.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(peer, Entry { identity, fetched_ms: now_ms });
    }

    /// Forget a peer's card — call when an identity publish for that peer is seen.
    pub fn invalidate(&self, peer: PeerId) {
        self.entries.remove(&peer);
    }
}

fn global() -> &'static CardCache {
    static CACHE: std::sync::OnceLock<CardCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(CardCache::default)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a pre-epoch clock reads every entry as stale — a refetch, never a wrong card
}

/// The identity for `peer`: from the cache, else ONE daemon round-trip, then cached
/// (absence included) for [`CARD_TTL_MS`].
pub async fn identity_for(
    airc: &airc_lib::Airc,
    peer: PeerId,
) -> Result<Option<Identity>, airc_lib::AircError> {
    let now = now_ms();
    if let Some(hit) = global().get(peer, now, CARD_TTL_MS) {
        return Ok(hit);
    }
    let identity = airc.peer_identity_card(peer).await?.map(|card| card.identity);
    global().put(peer, identity.clone(), now);
    let (h, m) = (global().hits.load(Ordering::Relaxed), global().misses.load(Ordering::Relaxed));
    crate::probe!(
        class = "roster.identity_card.fetched",
        peer = %peer.as_uuid(),
        known = identity.is_some(),
        hits = h,
        misses = m,
        "identity card fetched once for this peer; the roster reads the cache for the next 10 min"
    );
    Ok(identity)
}

/// Forget a peer's card so the next roster read fetches the republished one.
pub fn invalidate(peer: PeerId) {
    global().invalidate(peer);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(name: &str) -> Identity {
        Identity { name: name.into(), ..Default::default() }
    }

    // what this catches: the card being re-asked inside its TTL (the 250-requests-a-
    // second class), an unknown peer being re-asked per tick, and a stale entry or an
    // invalidated one NOT being re-asked.
    #[test]
    fn a_card_is_fetched_once_per_ttl_and_absence_is_remembered() {
        let c = CardCache::default();
        let p = PeerId::new();
        assert!(c.get(p, 1_000, CARD_TTL_MS).is_none(), "cold: a miss");
        c.put(p, Some(ident("Atlas")), 1_000);
        assert_eq!(c.get(p, 2_000, CARD_TTL_MS).flatten().map(|i| i.name), Some("Atlas".into()));
        assert!(c.get(p, 1_000 + CARD_TTL_MS, CARD_TTL_MS).is_none(), "stale after the TTL");
        let q = PeerId::new();
        c.put(q, None, 5_000);
        assert_eq!(c.get(q, 6_000, CARD_TTL_MS), Some(None), "a known absence is a hit, not a refetch");
        c.invalidate(p);
        assert!(c.get(p, 2_000, CARD_TTL_MS).is_none(), "invalidated = re-asked");
        assert_eq!(c.hits.load(Ordering::Relaxed), 2);
        assert_eq!(c.misses.load(Ordering::Relaxed), 2);
    }
}
