//! held_claims — what SHE holds, recorded at claim time, so the renewal reaches a card
//! on ANY room.
//!
//! The claim path follows a card to its room (accept-or-redirect, `work.followed_card_room`);
//! the renewal path walked the node's subscription set. A card claimed on a room this scope
//! does not subscribe to was therefore never renewed, while `persona.claim.renewed` read
//! green for the one card the walk could see. Measured on the 5090, 2026-09-26 (card
//! 2d9df546): Kimi's claim d9b90309 on d33e928a (a #cambriantech-board card) got renewals
//! at 20:45, 20:51, 20:57, 21:03 — every one for a bench card on the continuum board — and
//! the lease that mattered lapsed at 21:06 across 28 acts of hers. Correct parts composing
//! into silence ([[a-correction-path-that-needs-the-thing-it-corrects-never-fires]]).
//!
//! This record is keyed by her airc peer id, written at every successful claim (the
//! followed-room claim included) with the room the claim landed in, dropped on release
//! or when the board refuses the heartbeat repeatedly, and persisted beside her airc scope
//! (`<home>/held-claims.json`) so a core restart does not forget a followed claim inside
//! its first lease-length. The renewal loop unions it with the subscription walk.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The file beside her airc scope. One per scope = one per persona.
pub(crate) const FILE_NAME: &str = "held-claims.json";

/// After this many consecutive refused heartbeats the record is dropped: the hold is gone
/// (released elsewhere, lapsed and re-claimed by a peer, or the card settled) and retrying
/// forever would be the loop the sweeper exists to end.
pub(crate) const FORGET_AFTER_REFUSALS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeldClaim {
    /// The room the claim landed in — the followed room when the claim followed the card.
    pub room: airc_lib::Room,
    pub card_id: Uuid,
    pub claim_id: Uuid,
    pub recorded_at_ms: u64,
    /// Consecutive refused heartbeats; reset by a success.
    #[serde(default)]
    pub refusals: u32,
}

static HELD: Mutex<Option<HashMap<Uuid, Vec<HeldClaim>>>> = Mutex::new(None);

fn with<R>(peer: Uuid, f: impl FnOnce(&mut Vec<HeldClaim>) -> R) -> R {
    let mut guard = HELD.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned lock keeps the last record; it never invents a claim
    let map = guard.get_or_insert_with(HashMap::new);
    f(map.entry(peer).or_default())
}

/// A successful claim: replaces any record for the same card, persists.
pub fn record(peer: Uuid, home: &Path, claim: HeldClaim) {
    let entries = with(peer, |held| {
        held.retain(|h| h.card_id != claim.card_id);
        held.push(claim);
        held.clone()
    });
    persist(home, &entries);
}

/// She released it, or the board refused it for good: drop the record, persist.
pub fn forget(peer: Uuid, home: &Path, card_id: Uuid) -> bool {
    let (gone, entries) = with(peer, |held| {
        let before = held.len();
        held.retain(|h| h.card_id != card_id);
        (held.len() != before, held.clone())
    });
    if gone {
        persist(home, &entries);
    }
    gone
}

/// A refused heartbeat: count it; at [`FORGET_AFTER_REFUSALS`] the record goes.
/// Returns whether the record was dropped.
pub fn note_refusal(peer: Uuid, home: &Path, card_id: Uuid) -> bool {
    let (dropped, entries) = with(peer, |held| {
        let mut dropped = false;
        if let Some(h) = held.iter_mut().find(|h| h.card_id == card_id) {
            h.refusals += 1;
            dropped = h.refusals >= FORGET_AFTER_REFUSALS;
        }
        if dropped {
            held.retain(|h| h.card_id != card_id);
        }
        (dropped, held.clone())
    });
    persist(home, &entries);
    dropped
}

/// A renewed heartbeat clears the refusal count.
pub fn note_renewed(peer: Uuid, card_id: Uuid) {
    with(peer, |held| {
        if let Some(h) = held.iter_mut().find(|h| h.card_id == card_id) {
            h.refusals = 0;
        }
    });
}

/// Everything recorded for her, whole.
pub fn held(peer: Uuid) -> Vec<HeldClaim> {
    with(peer, |held| held.clone())
}

/// At bootstrap: the record beside her scope, into memory. Returns how many were read;
/// a missing or unreadable file is an empty record (a first boot, or nothing held).
pub fn load(peer: Uuid, home: &Path) -> usize {
    let path = home.join(FILE_NAME);
    let entries: Vec<HeldClaim> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default(); // unwrap_or_default: no file / unreadable = nothing recorded, never a boot failure
    let n = entries.len();
    with(peer, |held| *held = entries);
    n
}

/// PURE: the recorded claims the subscription walk did NOT already cover (`walked` = the
/// card ids the walk found held) — the ones the renewal must reach through their recorded
/// room. A card the walk sees is renewed by the walk; naming it twice would double-count
/// `renewed`.
pub fn beyond_the_walk(walked: &[Uuid], recorded: &[HeldClaim]) -> Vec<HeldClaim> {
    recorded
        .iter()
        .filter(|h| !walked.contains(&h.card_id))
        .cloned()
        .collect()
}

/// Atomic: a temp file then a rename, so a seam mid-write never leaves a half record.
fn persist(home: &Path, entries: &[HeldClaim]) {
    let path = home.join(FILE_NAME);
    let tmp = home.join(format!("{FILE_NAME}.partial"));
    let Ok(text) = serde_json::to_string_pretty(entries) else {
        return;
    };
    if std::fs::create_dir_all(home).is_err() {
        return;
    }
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(name: &str, home: &Path) -> airc_lib::Room {
        airc_lib::Room::from_name(home, name).expect("test: a room from a name")
    }

    fn claim(home: &Path, room_name: &str, card: u128) -> HeldClaim {
        HeldClaim {
            room: room(room_name, home),
            card_id: Uuid::from_u128(card),
            claim_id: Uuid::from_u128(card + 0x1000),
            recorded_at_ms: 1,
            refusals: 0,
        }
    }

    // regression for card 2d9df546 (Kimi, 2026-09-26 21:06Z): a claim that followed its
    // card to a room this scope does not subscribe to must still be a renewal target,
    // and the record must survive a core restart (the file) and end on release or on
    // repeated refusal — never renew a hold that is gone, forever.
    // what this catches: the renewal reaching only the walk; the record leaking past a
    // release; a refused hold retried without bound; the record lost at a seam.
    #[test]
    fn a_followed_claim_is_renewed_beyond_the_walk_survives_a_restart_and_ends_on_release_or_refusal() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let home = dir.path();
        let peer = Uuid::from_u128(0xfeed);
        let followed = claim(home, "cambriantech", 0xd33e);
        let walked = claim(home, "continuum", 0x3438);
        record(peer, home, followed.clone());
        record(peer, home, walked.clone());
        // The walk sees only the bench card on the subscribed board.
        let walk = vec![walked.card_id];
        let beyond = beyond_the_walk(&walk, &held(peer));
        assert_eq!(beyond, vec![followed.clone()], "exactly the card the walk cannot see");
        // A restart: memory empty, the file brings both back.
        with(peer, |h| h.clear());
        assert_eq!(load(peer, home), 2);
        assert_eq!(beyond_the_walk(&walk, &held(peer)).len(), 1);
        // A re-claim replaces, never duplicates.
        let mut again = followed.clone();
        again.claim_id = Uuid::from_u128(0x9999);
        record(peer, home, again.clone());
        assert_eq!(held(peer).iter().filter(|h| h.card_id == followed.card_id).count(), 1);
        assert_eq!(held(peer).iter().find(|h| h.card_id == followed.card_id).map(|h| h.claim_id), Some(again.claim_id));
        // Refusals: two are counted, the third drops the record; a success in between resets.
        assert!(!note_refusal(peer, home, followed.card_id));
        note_renewed(peer, followed.card_id);
        assert!(!note_refusal(peer, home, followed.card_id));
        assert!(!note_refusal(peer, home, followed.card_id));
        assert!(note_refusal(peer, home, followed.card_id), "the third consecutive refusal forgets");
        assert!(held(peer).iter().all(|h| h.card_id != followed.card_id));
        // Release forgets the other, and the file agrees.
        assert!(forget(peer, home, walked.card_id));
        assert!(!forget(peer, home, walked.card_id), "forgetting twice is a no-op");
        with(peer, |h| h.clear());
        assert_eq!(load(peer, home), 0, "the file carries the same truth as memory");
    }
}
