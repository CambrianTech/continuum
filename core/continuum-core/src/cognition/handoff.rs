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

/// WHY a volatile write happens — the one fact the handoff needs that the memory itself
/// cannot know. A periodic write says "the stop, if one comes, is unknown"; a seam write is
/// the last thing before the process ends and asks the deploy gate what is tearing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveReason {
    Periodic,
    Seam,
}

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
    let mut guard = SEEDS.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: poisoned lock = read the last state, same policy as every lock in this crate
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
/// here through the SAME held predicate the renewal uses (`held_of_owned`: lease live, not
/// claimable, not settled) — one definition of "held". A walk that finds no held work card
/// CLEARS the card: the walk is the freshest truth, and a card she released, that merged, or
/// whose lease lapsed must not ride into the next life as "you were working X" (Cormac on
/// #4407). Her cards in Review and the review cards she holds are obligations.
pub fn note_owned(persona: Uuid, owned: &[(airc_lib::Room, airc_lib::WorkCard)], now_ms: u64) {
    use airc_work::model::CardState;
    let held = crate::persona::airc_runtime::held_of_owned(owned.to_vec(), now_ms);
    let held_work = held
        .iter()
        .find(|(_, card)| card.reviews.is_none())
        .map(|(room, card)| (room.channel.as_uuid(), card.clone()));
    let mut obligations: Vec<airc_lib::WorkCard> = owned
        .iter()
        .filter(|(_, card)| matches!(card.state, CardState::Review))
        .map(|(_, card)| card.clone())
        .collect();
    obligations.extend(
        held.iter()
            .filter(|(_, card)| card.reviews.is_some())
            .map(|(_, card)| card.clone()),
    );
    // Dedup by card id across the whole list (a card can be both in Review and held as
    // a review), not adjacent-only.
    let mut seen = std::collections::HashSet::new();
    obligations.retain(|card| seen.insert(card.card_id));
    with_seed(persona, |s| {
        match held_work {
            Some((room, card)) => {
                s.room = Some(room);
                s.card = Some(card);
            }
            None => {
                s.room = None;
                s.card = None;
                s.rooted_at_ms = None;
            }
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

/// What tears this life, decided the way every other caller decides "is a deploy in
/// flight": through `deploy_claim::in_flight`, which reads an abandoned claim (owner dead,
/// or older than the cap) as Clear. A bare `read` would turn every stop after a crashed
/// reboot into "a deploy did NOT land" — a false alarm in the one line built to be trusted
/// (Cormac on #4407). Only a claim whose owner is alive is a `Deploy`; the claim rides whole.
pub fn torn_by_for(reason: SaveReason, root: Option<&std::path::Path>, now_ms: u64) -> TornBy {
    use crate::runtime::deploy_claim::{in_flight, read, DeployGate};
    match reason {
        SaveReason::Periodic => TornBy::Periodic,
        SaveReason::Seam => match root {
            Some(root) if matches!(in_flight(root, now_ms), DeployGate::InProgress { .. }) => {
                read(root).map(TornBy::Deploy).unwrap_or(TornBy::Stop) // unwrap_or: the claim vanished between the gate's read and this one — a deploy that is gone is a stop
            }
            _ => TornBy::Stop,
        },
    }
}

/// ONE budget for every resident's staged read at a seam, not one per git call: the
/// runtime's save phase is 2 s for every resident together, and 400 ms × 2 calls × N
/// residents would overrun it on three slow trees (Cormac on #4407). `write_volatile_all`
/// sets the deadline once; residents past it say `Unmeasured`, never wait.
// derived-or-floor: a floor; `git status` on a warm checkout is tens of ms, and the save
// phase (runtime.rs shutdown_within, 2 s per phase) is shared by every resident.
pub const STAGED_READ_BOUND: std::time::Duration = std::time::Duration::from_millis(400);

/// Compose the record at the seam from the seed plus what only the seam knows.
/// `None` when she holds nothing and stands nowhere: a citizen at home has nothing to hand.
/// `staged_deadline` is `Some` at a seam only: a periodic checkpoint never spawns git in
/// her live tree (it would take her `.git/index.lock` under her own `code/git` every
/// interval), and says so instead.
pub fn compose(
    persona: Uuid,
    torn_by: TornBy,
    acting_root: Option<&std::path::Path>,
    staged_deadline: Option<std::time::Instant>,
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
        staged: acting_root.map(|root| match staged_deadline {
            Some(deadline) => staged_state(root, deadline),
            None => Staged::Unmeasured {
                root: root.to_string_lossy().to_string(),
                why: "periodic checkpoint: the tree is read at the seam only".to_string(),
            },
        }),
        obligations: seed.obligations,
    })
}

/// `git status --porcelain` + sha256(`git diff`), both under the seam's shared deadline.
/// Runs on the blocking checkpoint worker; a tree that does not answer in time is
/// `Unmeasured`, named, and the git child is KILLED, not orphaned with her index lock.
fn staged_state(root: &std::path::Path, deadline: std::time::Instant) -> Staged {
    let root_s = root.to_string_lossy().to_string();
    let status = git_bounded(root, &["status", "--porcelain"], deadline);
    let dirty_paths = match status {
        Ok(out) => out
            .lines()
            .filter_map(|l| l.get(3..).map(str::to_string))
            .collect::<Vec<_>>(),
        Err(why) => return Staged::Unmeasured { root: root_s, why },
    };
    match git_bounded(root, &["diff"], deadline) {
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

/// A background READER of her tree: `--no-optional-locks` so it never takes
/// `.git/index.lock` from under her own `code/git`, spawned as a child that is killed at
/// the deadline rather than an `.output()` left running past it.
fn git_bounded(
    root: &std::path::Path,
    args: &[&str],
    deadline: std::time::Instant,
) -> Result<String, String> {
    let label = format!("git {}", args.join(" "));
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return Err(format!("{label}: the seam's staged budget was spent by earlier residents"));
    }
    let mut child = std::process::Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{label} could not run: {e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    // Drain the pipes off this thread so a chatty child never blocks on a full pipe; the
    // readers end when the child exits or is killed, so the joins below are bounded too.
    let out_reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_end(&mut buf); // a closed pipe ends the read; the bytes so far are the answer
        }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_end(&mut buf); // same: a closed pipe ends the read
        }
        buf
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill(); // already exited between try_wait and kill = nothing to kill
                let _ = child.wait(); // reap; a wait on a killed child cannot block
                break Err(format!(
                    "{label} did not answer before the seam's shared staged budget ({} ms for every resident) ran out, and was killed",
                    STAGED_READ_BOUND.as_millis()
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(e) => break Err(format!("{label}: {e}")),
        }
    };
    let out = out_reader.join().unwrap_or_default(); // unwrap_or_default: a panicked reader = no bytes; the exit status decides below
    let err = err_reader.join().unwrap_or_default(); // unwrap_or_default: same — no bytes, the status decides
    match status {
        Ok(status) if status.success() => Ok(String::from_utf8_lossy(&out).to_string()),
        Ok(status) => Err(format!(
            "{label} exited {status}: {}",
            String::from_utf8_lossy(&err).trim()
        )),
        Err(why) => Err(why),
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
        assert!(compose(me, TornBy::Stop, None, None, 5_000).is_none());
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 2_500);
        note_ledger(me, &ledger(3_000));
        let fresh = compose(me, TornBy::Stop, None, None, 5_000).expect("held work hands off");
        assert_eq!(fresh.card.as_ref().map(|c| c.claim_expires_at_ms), Some(Some(2_000)));
        assert_eq!(fresh.ledger.as_ref().map(|l| l.next_test.as_str()), Some("run the failing test"));
        assert!(!fresh.acts_after_ledger, "the note at 3000 was written after the rooting at 2500");
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 4_000);
        let stale = compose(me, TornBy::Stop, None, None, 5_000).expect("held");
        assert!(stale.acts_after_ledger, "a work turn rooted at 4000 postdates the note at 3000");
    }

    // what this catches: the seam walk partitions whole cards — a live-held work card is
    // `card`, a review card she holds and her card in Review are obligations, never dropped.
    #[test]
    fn owned_cards_partition_into_the_held_card_and_obligations() {
        let me = Uuid::from_u128(102);
        let room = airc_lib::Room {
            version: 1,
            name: "r".into(),
            wire: std::path::PathBuf::new(),
            channel: airc_core::RoomId::from_uuid(Uuid::from_u128(55)),
            joined_at_ms: 0,
        };
        // Three DISTINCT cards: the record dedups obligations by card id.
        let mut in_progress = card(airc_work::model::CardState::InProgress, false);
        in_progress.card_id = airc_lib::WorkCardId::from_uuid(Uuid::from_u128(71));
        let mut in_review = card(airc_work::model::CardState::Review, false);
        in_review.card_id = airc_lib::WorkCardId::from_uuid(Uuid::from_u128(72));
        let mut review_held = card(airc_work::model::CardState::Claimed, true);
        review_held.card_id = airc_lib::WorkCardId::from_uuid(Uuid::from_u128(73));
        let owned = vec![
            (room.clone(), in_progress),
            (room.clone(), in_review),
            (room.clone(), review_held),
        ];
        note_owned(me, &owned, 1_500);
        let h = compose(me, TornBy::Periodic, None, None, 9_000).expect("held");
        assert_eq!(h.room, Some(Uuid::from_u128(55)));
        assert_eq!(h.card.map(|c| c.state), Some(airc_work::model::CardState::InProgress));
        assert_eq!(h.obligations.len(), 2);
    }

    // what this catches (Cormac on #4407): a card she no longer holds must not ride into the
    // next life. After a rooting stamped it, a seam walk that shows no hold CLEARS it, and
    // the wake line says she held no card instead of naming finished work.
    #[test]
    fn a_walk_with_no_hold_clears_the_card_she_once_rooted_at() {
        let me = Uuid::from_u128(104);
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 1_000);
        assert!(compose(me, TornBy::Stop, None, None, 2_000).expect("held").card.is_some());
        note_owned(me, &[], 2_000);
        note_ledger(me, &ledger(1_500));
        let h = compose(me, TornBy::Stop, None, None, 3_000).expect("a ledger still hands off");
        assert!(h.card.is_none(), "the walk is the freshest truth");
        assert!(!h.acts_after_ledger, "no rooting survives the clear");
        let line = render_on_wake(&h, "abc", 3_000);
        assert!(line.contains("You held no card at the seam"), "{line}");
    }

    // what this catches (Cormac on #4407): a deploy claim abandoned by a reboot that died
    // must read as a plain STOP at the seam, not as "a deploy did NOT land"; a claim whose
    // owner is alive is a Deploy, whole; a periodic write is Periodic whatever the file says.
    #[test]
    fn a_dead_owners_claim_is_a_stop_and_a_live_owners_claim_is_a_deploy() {
        let root = std::env::temp_dir().join(format!("handoff-claim-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root");
        let now_ms = 10_000_000;
        let dead = DeployClaim {
            pid: 2_147_483_000, // no such process
            started_ms: now_ms - 1_000,
            target_sha: "deadbeef00".into(),
        };
        crate::runtime::deploy_claim::write(&root, &dead).expect("claim written");
        assert_eq!(torn_by_for(SaveReason::Seam, Some(&root), now_ms), TornBy::Stop);
        assert_eq!(torn_by_for(SaveReason::Periodic, Some(&root), now_ms), TornBy::Periodic);
        let live = DeployClaim {
            pid: std::process::id() as i32,
            started_ms: now_ms - 1_000,
            target_sha: "cafef00d00".into(),
        };
        crate::runtime::deploy_claim::write(&root, &live).expect("claim written");
        assert_eq!(torn_by_for(SaveReason::Seam, Some(&root), now_ms), TornBy::Deploy(live));
        assert_eq!(torn_by_for(SaveReason::Seam, None, now_ms), TornBy::Stop);
        let _ = std::fs::remove_dir_all(&root);
    }

    // what this catches (Kimi's #1, the Aris shape: live work reading as lapsed): the record
    // carries her claim id and lease edge — on the WHOLE card, never re-picked — and the
    // wake line names both and says holding is UNVERIFIED until the board answers.
    #[test]
    fn the_wake_line_names_the_claim_and_its_lease_edge_and_calls_holding_unverified() {
        let me = Uuid::from_u128(105);
        note_held(me, None, &card(airc_work::model::CardState::InProgress, false), 1_000);
        let h = compose(me, TornBy::Stop, None, None, 2_000).expect("held");
        let held = h.card.as_ref().expect("the whole card rides in the record");
        assert_eq!(held.claim_id.map(|c| c.as_uuid()), Some(Uuid::from_u128(9)));
        assert_eq!(held.claim_expires_at_ms, Some(2_000));
        let line = render_on_wake(&h, "abc", 2_000);
        assert!(line.contains("claim 00000000"), "{line}");
        assert!(line.contains("lease to 2000 Unix ms"), "{line}");
        assert!(line.contains("holding is UNVERIFIED until the board answers"), "{line}");
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
        let staged = staged_state(&dir, std::time::Instant::now() + STAGED_READ_BOUND);
        assert!(matches!(staged, Staged::Unmeasured { .. }), "{staged:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
