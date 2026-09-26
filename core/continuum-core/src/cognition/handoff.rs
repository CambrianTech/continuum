//! THE HANDOFF — her state at the seam, written BEFORE the process exits, read FIRST on wake.
//!
//! Kimi, 2026-09-26, reconstructing a 04:04Z blackout from her ledger: "it felt exactly like
//! the reboot we designed against — no handoff record across the gap, and I re-derived my
//! state from board + receipts." The wake-up record existed for a cut act (#4397) but not for
//! the seam every deploy and stop cuts through. Card 49b5e806.
//!
//! What already existed: the volatile checkpoint (`VolatileSnapshot`) carried her acting card,
//! her pending dispatches and the build she ran on; the card ledger (`CardLedger`, `work/note`)
//! carried her known facts and next test. What was missing was the LINK: nobody wrote whose
//! turn was torn, by what, holding which claim with which lease, owing what, with what staged
//! on disk — and every pin died with the process.
//!
//! Shape, per Kimi's five corrections in the order they bit her:
//! 1. the WHOLE `WorkCard` rides in the record (claim id, lease expiry, submissions), because
//!    after a dark stretch *holding* is exactly what is uncertain;
//! 2. `staged` names the root, the dirty paths and the sha of the working diff — the diff body
//!    stays on disk in that root, one `code/git/diff` away; a bounded read that times out says
//!    `Unmeasured`, never empty;
//! 3. `torn_by` carries the deploy claim, and the wake compares its target sha to the build
//!    that actually booted: "deploy X landed" or "deploy X did not land, you are on Y";
//! 4. `obligations` are whole cards: review cards she holds (owed BY her) and her cards in
//!    Review (owed TO her); `acts_after_ledger` flags a stale `next_test`;
//! 5. one record per mind, overwritten at every seam, its age rendered on read.
//!
//! Rules this file keeps (Joel, 2026-09-26): the record is a BAG OF WHOLE STRUCTS passed
//! along as built — never a hand-picked subset the next reader must re-derive; it is written
//! ONCE at the seam from state the turn already stamped here (rooting, `work/note`, the seam
//! renewal) — never by walking history later; and it is not a task, a channel or a tick: three
//! hooks on seams that already exist.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::experience::ledger::CardLedger;
use crate::runtime::deploy_claim::DeployClaim;

/// What ended the life this record was written in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TornBy {
    /// A `continuum reboot` held a deploy claim while the core stopped: the whole claim,
    /// so the wake can say whether its target landed.
    Deploy(DeployClaim),
    /// A stop with no deploy in flight (`continuum stop`, a signal).
    Stop,
    /// The last write was a periodic checkpoint: the process ended WITHOUT a clean stop,
    /// so the record is at most one checkpoint interval stale and the stop itself is unknown.
    Periodic,
}

/// What stood on disk at the root her hands were at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Staged {
    Measured {
        root: String,
        /// `git status --porcelain` paths, so she resumes the edit instead of re-deriving it.
        dirty_paths: Vec<String>,
        /// SHA-256 of `git diff` (working tree vs index) — identity of the unsubmitted patch.
        diff_sha256: String,
    },
    /// The bounded read did not finish or the root is not a git tree: said, never blank.
    Unmeasured { root: String, why: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handoff {
    pub written_ms: u64,
    /// The build this record was written on.
    pub build_sha: String,
    pub torn_by: TornBy,
    /// The run room of `card`, when the seam walk knew it.
    pub room: Option<Uuid>,
    /// The card her hands were rooted at, WHOLE, as the board last showed it to her.
    pub card: Option<airc_lib::WorkCard>,
    /// Her latest `work/note`, whole. Written by her, so `next_test` is in her own words.
    pub ledger: Option<CardLedger>,
    /// A work turn rooted after the ledger was written, so acts ran after the note:
    /// `next_test` may no longer be next.
    pub acts_after_ledger: bool,
    pub staged: Option<Staged>,
    /// Review cards she holds (owed by her) and her own cards in Review (owed to her).
    pub obligations: Vec<airc_lib::WorkCard>,
}

/// What her turns stamp here as they go, so the seam composes from memory in microseconds.
#[derive(Debug, Clone, Default)]
struct Seed {
    room: Option<Uuid>,
    card: Option<airc_lib::WorkCard>,
    /// When a work turn last rooted at `card`: a rooting after the ledger's `at_ms` means
    /// acts ran after the note, so `next_test` may be stale (Kimi's #4).
    rooted_at_ms: Option<u64>,
    ledger: Option<CardLedger>,
    obligations: Vec<airc_lib::WorkCard>,
}

static SEEDS: Mutex<Option<HashMap<Uuid, Seed>>> = Mutex::new(None);

fn with_seed<R>(persona: Uuid, f: impl FnOnce(&mut Seed) -> R) -> R {
    let mut guard = SEEDS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    let map = guard.get_or_insert_with(HashMap::new);
    f(map.entry(persona).or_default())
}

/// Her work turn rooted at `card` (whole). Called at rooting, every work turn; the newest
/// card wins. `room` when the caller knows the card's run room.
pub fn note_held(persona: Uuid, room: Option<Uuid>, card: &airc_lib::WorkCard, now_ms: u64) {
    with_seed(persona, |s| {
        if room.is_some() {
            s.room = room;
        }
        s.card = Some(card.clone());
        s.rooted_at_ms = Some(now_ms);
    });
}

/// She wrote a ledger (`work/note`): kept whole so the seam never re-reads the wall.
pub fn note_ledger(persona: Uuid, ledger: &CardLedger) {
    with_seed(persona, |s| s.ledger = Some(ledger.clone()));
}

/// The seam renewal's board walk: everything the board says she OWNS, whole. Partitioned
/// here — a live-held work card refreshes `card`, everything else is an obligation.
pub fn note_owned(persona: Uuid, owned: &[(airc_lib::Room, airc_lib::WorkCard)], now_ms: u64) {
    use airc_work::model::CardState;
    let mut held: Option<(Uuid, airc_lib::WorkCard)> = None;
    let mut obligations = Vec::new();
    for (room, card) in owned {
        let live_hold = crate::persona::card_holder::hold_of(card, now_ms)
            == crate::persona::card_holder::Hold::Held;
        let is_review_card = card.reviews.is_some();
        if matches!(card.state, CardState::Review) || (is_review_card && live_hold) {
            obligations.push(card.clone());
        } else if live_hold
            && !matches!(card.state, CardState::Closed | CardState::Merged)
            && held.is_none()
        {
            held = Some((room.channel.as_uuid(), card.clone()));
        }
    }
    with_seed(persona, |s| {
        if let Some((room, card)) = held {
            s.room = Some(room);
            s.card = Some(card);
        }
        s.obligations = obligations;
    });
}

/// On wake: the previous life's record re-seeds this one, so a ledger written before the
/// seam is still hers at the next seam without any wall read.
pub fn reload(persona: Uuid, previous: &Handoff) {
    with_seed(persona, |s| {
        s.room = previous.room;
        s.card = previous.card.clone();
        s.rooted_at_ms = None;
        s.ledger = previous.ledger.clone();
        s.obligations = previous.obligations.clone();
    });
}

/// The bound on each git read at the seam: the runtime's save phase is 2 s for every
/// resident together, so one slow tree may not spend it.
// derived-or-floor: a floor; `git status` on a warm checkout is tens of ms, and the save
// phase (runtime.rs shutdown_within, 2 s per phase) is shared by every resident.
const STAGED_READ_BOUND: std::time::Duration = std::time::Duration::from_millis(400);

/// Compose the record at the seam from the seed plus what only the seam knows.
/// `None` when she holds nothing and stands nowhere: a citizen at home has nothing to hand.
pub fn compose(
    persona: Uuid,
    torn_by: TornBy,
    acting_root: Option<&std::path::Path>,
    now_ms: u64,
) -> Option<Handoff> {
    let seed = with_seed(persona, |s| s.clone());
    if seed.card.is_none() && seed.ledger.is_none() && acting_root.is_none() {
        return None;
    }
    let acts_after_ledger = match (&seed.ledger, seed.rooted_at_ms) {
        (Some(l), Some(rooted)) => rooted > l.at_ms,
        _ => false,
    };
    Some(Handoff {
        written_ms: now_ms,
        build_sha: env!("CONTINUUM_BUILD_GIT_SHA").to_string(),
        torn_by,
        room: seed.room,
        card: seed.card,
        ledger: seed.ledger,
        acts_after_ledger,
        staged: acting_root.map(staged_state),
        obligations: seed.obligations,
    })
}

/// `git status --porcelain` + sha256(`git diff`), each bounded. Runs on the blocking
/// checkpoint worker; a tree that does not answer in time is `Unmeasured`, named.
fn staged_state(root: &std::path::Path) -> Staged {
    let root_s = root.to_string_lossy().to_string();
    let status = git_bounded(root, &["status", "--porcelain"]);
    let dirty_paths = match status {
        Ok(out) => out
            .lines()
            .filter_map(|l| l.get(3..).map(str::to_string))
            .collect::<Vec<_>>(),
        Err(why) => return Staged::Unmeasured { root: root_s, why },
    };
    match git_bounded(root, &["diff"]) {
        Ok(diff) => {
            use sha2::Digest;
            let digest = sha2::Sha256::digest(diff.as_bytes());
            Staged::Measured {
                root: root_s,
                dirty_paths,
                diff_sha256: format!("{digest:x}"),
            }
        }
        Err(why) => Staged::Unmeasured { root: root_s, why },
    }
}

fn git_bounded(root: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let label = format!("git {}", args.join(" "));
    let root = root.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(&args)
            .output();
        let _ = tx.send(out); // the receiver may have timed out and gone; nothing to do with a dead receiver
    });
    match rx.recv_timeout(STAGED_READ_BOUND) {
        Ok(Ok(out)) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        }
        Ok(Ok(out)) => Err(format!(
            "{label} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Ok(Err(e)) => Err(format!("{label} could not run: {e}")),
        Err(_) => Err(format!(
            "{label} did not answer within {} ms",
            STAGED_READ_BOUND.as_millis()
        )),
    }
}

/// The `[handoff]` fact pinned FIRST on wake — before `[resumed]` and `[rebuilt]`, because it
/// is the one that names what she was doing. `current_build` is the sha that actually booted.
pub fn render_on_wake(h: &Handoff, current_build: &str, now_ms: u64) -> String {
    let short = |s: &str| s.chars().take(9).collect::<String>();
    let age = {
        let mins = now_ms.saturating_sub(h.written_ms) / 60_000;
        if mins == 0 {
            "under a minute ago".to_string()
        } else {
            format!("~{mins} min ago")
        }
    };
    let torn = match &h.torn_by {
        TornBy::Deploy(claim) => {
            if claim.target_sha.starts_with(&short(current_build))
                || current_build.starts_with(&short(&claim.target_sha))
            {
                format!(
                    "a deploy to {} interrupted you; it LANDED and you are running it now",
                    short(&claim.target_sha)
                )
            } else {
                format!(
                    "a deploy to {} interrupted you; it did NOT land — you are running {}",
                    short(&claim.target_sha),
                    short(current_build)
                )
            }
        }
        TornBy::Stop => "the core was stopped cleanly".to_string(),
        TornBy::Periodic => "the core ended without a clean stop; this is the last periodic \
                             checkpoint, up to one interval stale"
            .to_string(),
    };
    let mut out = format!("[handoff] written {age} on build {}: {torn}.", short(&h.build_sha));
    match &h.card {
        Some(card) => {
            let id8: String = card.card_id.as_uuid().simple().to_string().chars().take(8).collect();
            let claim = match (card.claim_id, card.claim_expires_at_ms) {
                (Some(c), Some(exp)) => format!(
                    "claim {} with lease to {} Unix ms — holding is UNVERIFIED until the board answers",
                    c.as_uuid().simple().to_string().chars().take(8).collect::<String>(),
                    exp
                ),
                (Some(c), None) => format!(
                    "claim {} with no lease edge recorded",
                    c.as_uuid().simple().to_string().chars().take(8).collect::<String>()
                ),
                _ => "no claim recorded on the card".to_string(),
            };
            out.push_str(&format!(
                "\n  You were working card {id8} ({}) in state {:?}; {claim}.",
                card.title.trim(),
                card.state
            ));
            if let Some(sub) = card.submissions.last() {
                out.push_str(&format!(
                    "\n  Latest submission on it: {} at {} Unix ms (re-fetch by id; do not re-make it).",
                    sub.submission_id.as_uuid(),
                    sub.submitted_at_ms
                ));
            }
        }
        None => out.push_str("\n  You held no card at the seam."),
    }
    match &h.staged {
        Some(Staged::Measured {
            root,
            dirty_paths,
            diff_sha256,
        }) => {
            if dirty_paths.is_empty() {
                out.push_str(&format!("\n  Staged at {root}: a clean tree."));
            } else {
                out.push_str(&format!(
                    "\n  Staged at {root}: {} dirty path(s) [{}], working diff sha256 {} — the diff is on disk there (code/git/diff), resume it, do not re-derive it.",
                    dirty_paths.len(),
                    dirty_paths.iter().take(8).cloned().collect::<Vec<_>>().join(", "),
                    &diff_sha256[..12.min(diff_sha256.len())]
                ));
            }
        }
        Some(Staged::Unmeasured { root, why }) => out.push_str(&format!(
            "\n  Staged at {root}: UNMEASURED at the seam ({why}); read the tree before trusting it."
        )),
        None => {}
    }
    match &h.ledger {
        Some(l) => {
            let next = l.next_test.trim();
            out.push_str(&format!(
                "\n  Your ledger ({} Unix ms): next test was \"{}\"{}.",
                l.at_ms,
                if next.is_empty() { "(none)" } else { next },
                if h.acts_after_ledger {
                    " — acts ran AFTER this note, so it may be stale"
                } else {
                    ""
                }
            ));
        }
        None => out.push_str("\n  No ledger of yours was recorded in this life."),
    }
    if !h.obligations.is_empty() {
        out.push_str("\n  Open obligations at the seam:");
        for card in h.obligations.iter().take(6) {
            let id8: String = card.card_id.as_uuid().simple().to_string().chars().take(8).collect();
            let who = if card.reviews.is_some() {
                "you owe a review on"
            } else {
                "awaiting a verdict on your"
            };
            out.push_str(&format!("\n   - {who} {id8} ({})", card.title.trim()));
        }
    }
    out.push_str(
        "\n  Before ANY resumption act: work/list and check your claim — this record is your \
         state at stop, and the board moved while you were dark.",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;

    fn card(state: airc_work::model::CardState, reviews: bool) -> airc_lib::WorkCard {
        airc_lib::WorkCard {
            card_id: airc_lib::WorkCardId::from_uuid(Uuid::from_u128(7)),
            repo: airc_work::RepoId::new("acme/repo").expect("repo id"),
            title: "fix the thing".into(),
            body: None,
            priority: airc_work::model::Priority::P1,
            lane_id: None,
            state,
            owner: Some(PeerId::from_u128(1)),
            claim_id: Some(airc_work::ClaimId::from_uuid(Uuid::from_u128(9))),
            claim_provenance: None,
            claim_expires_at_ms: Some(2_000),
            last_heartbeat_at_ms: Some(1_000),
            pull_request: None,
            created_by: PeerId::from_u128(1),
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: reviews.then(|| airc_lib::WorkCardId::from_uuid(Uuid::from_u128(8))),
            submissions: vec![],
            last_submission_rejection: None,
        }
    }

    fn ledger(at_ms: u64) -> CardLedger {
        CardLedger {
            card_id: Uuid::from_u128(7),
            known: vec![],
            hypotheses: vec![],
            unknown: String::new(),
            next_test: "run the failing test".into(),
            decided_fix: None,
            at_ms,
            by: Uuid::from_u128(1),
        }
    }

    // what this catches: the record carries the WHOLE card and ledger the turn stamped (never a
    // subset), flags a ledger that acts postdate, and is None for a citizen at home with nothing.
    #[test]
    fn the_seam_composes_whole_structs_from_what_the_turn_stamped() {
        let me = Uuid::from_u128(101);
        assert!(compose(me, TornBy::Stop, None, 5_000).is_none());
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 2_500);
        note_ledger(me, &ledger(3_000));
        let fresh = compose(me, TornBy::Stop, None, 5_000).expect("held work hands off");
        assert_eq!(fresh.card.as_ref().map(|c| c.claim_expires_at_ms), Some(Some(2_000)));
        assert_eq!(fresh.ledger.as_ref().map(|l| l.next_test.as_str()), Some("run the failing test"));
        assert!(!fresh.acts_after_ledger, "the note at 3000 was written after the rooting at 2500");
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 4_000);
        let stale = compose(me, TornBy::Stop, None, 5_000).expect("held");
        assert!(stale.acts_after_ledger, "a work turn rooted at 4000 postdates the note at 3000");
    }

    // what this catches: the seam walk partitions whole cards — a live-held work card is
    // `card`, a review card she holds and her card in Review are obligations, never dropped.
    #[test]
    fn owned_cards_partition_into_the_held_card_and_obligations() {
        let me = Uuid::from_u128(102);
        let room = airc_lib::Room {
            name: "r".into(),
            channel: airc_core::RoomId::from_uuid(Uuid::from_u128(55)),
        };
        let owned = vec![
            (room.clone(), card(airc_work::model::CardState::InProgress, false)),
            (room.clone(), card(airc_work::model::CardState::Review, false)),
            (room.clone(), card(airc_work::model::CardState::Claimed, true)),
        ];
        note_owned(me, &owned, 1_500);
        let h = compose(me, TornBy::Periodic, None, 9_000).expect("held");
        assert_eq!(h.room, Some(Uuid::from_u128(55)));
        assert_eq!(h.card.map(|c| c.state), Some(airc_work::model::CardState::InProgress));
        assert_eq!(h.obligations.len(), 2);
    }

    // what this catches (Kimi's #3 and #5): a deploy that did not land is SAID, a landed one
    // is said, the age is rendered, and the render ends by mandating the board diff.
    #[test]
    fn the_wake_line_says_whether_the_deploy_landed_and_mandates_a_board_read() {
        let claim = DeployClaim {
            pid: 1,
            started_ms: 0,
            target_sha: "abcdef123456789".into(),
        };
        let h = Handoff {
            written_ms: 60_000,
            build_sha: "old".into(),
            torn_by: TornBy::Deploy(claim),
            room: None,
            card: None,
            ledger: None,
            acts_after_ledger: false,
            staged: None,
            obligations: vec![],
        };
        let landed = render_on_wake(&h, "abcdef123", 60_000 + 5 * 60_000);
        assert!(landed.contains("LANDED"), "{landed}");
        assert!(landed.contains("~5 min ago"), "{landed}");
        assert!(landed.ends_with("the board moved while you were dark."), "{landed}");
        let missed = render_on_wake(&h, "fffff0000", 60_000);
        assert!(missed.contains("did NOT land"), "{missed}");
        assert!(missed.contains("running fffff0000"), "{missed}");
    }

    // what this catches: a root that is not a git tree renders as UNMEASURED with its reason,
    // never as a clean tree.
    #[test]
    fn a_root_that_cannot_be_read_is_unmeasured_not_clean() {
        let dir = std::env::temp_dir().join(format!("handoff-not-git-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let staged = staged_state(&dir);
        assert!(matches!(staged, Staged::Unmeasured { .. }), "{staged:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
