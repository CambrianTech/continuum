//! The dyno's restore reproductions (ONE-ACT slice 1, dyno 4 and 5; the proof list of §5b.1).
//!
//! On 2026-11-04 the M5's engine refused restores for CAPACITY ("failed to find 59144 available
//! cells" in a partly occupied 125,696-cell pool; "n_tokens = 35838 > size = 33280" after a small
//! relaunch). The core reduced each 400 to "rejected" and called `note_page_lost`, removing the
//! page's persisted confirmation, so every later turn of that mind went cold
//! (`page_on_disk_unconfirmed`) with a good page on disk. Here the real admission
//! (`admit_turn`), the real engine adapter (`kv_page_action`) and the real persisted ledger run
//! against the lifecycle fixture's fake engine, which obeys a unified pool of recorded size and
//! answers in the engine's own words. Expected outcomes are `capacity::dyno::expected`'s
//! test-side `RestoreOutcome`, which Codex's typed outcome replaces by rename.
//!
//! Not explained here, on purpose: Aris's confirmed 19:45Z save missing at 19:58Z with no refusal
//! (Cormac owns its separate reproduction).
#![cfg(test)]
use super::lifecycle::{act, boot, fake_engine, SaveHold, Slots};
use super::*;
use crate::capacity::dyno::expected::{CapacityShortfall, MissingSeam, RestoreOutcome};
use crate::capacity::dyno::recorded::*;
use std::sync::atomic::Ordering::SeqCst;

fn day() -> Recorded {
    Recorded::load().expect("test: the recorded 2026-11-04 data parses")
}

/// Dyno 4's recorded pool, page and residents.
fn free_cells() -> FreeCellRefusal {
    day().restore_free_cells.value
}

fn key(mind: u128) -> PageKey {
    PageKey::mind(Uuid::from_u128(mind)).expect("test: non-nil id")
}

/// What one admission handed her.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    /// Her KV, resident or restored: no prefill.
    Warm,
    /// A turn that prefills from its first token.
    Cold(PageIn),
    /// No turn now (refused or still waiting): she is deferred, her page untouched.
    Deferred,
}

/// One engine and core on a page dir.
struct Rig {
    root: String,
    slots: Slots,
    hold: Arc<SaveHold>,
    endpoint: Arc<crate::inference::slots::EndpointSlots>,
    pool: Arc<KvSlotPool>,
    server: tokio::task::JoinHandle<()>,
}

/// A core and its engine on `dir`, the engine's unified pool `cells` large (`None`: unbounded).
async fn rig(dir: &std::path::Path, cells: Option<u64>) -> Rig {
    let (root, slots, hold, server) = fake_engine(dir.to_path_buf()).await;
    *hold.cells.lock().expect("test: fixture mutex") = cells;
    let (endpoint, pool) = boot(&root, dir).await;
    Rig { root, slots, hold, endpoint, pool, server }
}

impl Rig {
    /// One act of `mind`: admitted, then her slot holds `tokens`.
    async fn act(&self, lanes: &Arc<Semaphore>, client: &reqwest::Client, mind: u128, tokens: u64) -> PageIn {
        act(lanes, &self.pool, client, &self.root, &self.slots, mind, tokens).await
    }

    /// One admission of `mind`. A warm turn generates (her slot then holds `tokens`); a cold one is
    /// dropped ungenerated, as a cancelled turn would be. Deferred when admission refuses or is
    /// still waiting after the adapter's own page-switch bound on an unmeasured node
    /// ([`kv_page_wedge_bound`]): a restore that has not resolved by then is not resolving.
    async fn attempt(&self, lanes: &Arc<Semaphore>, client: &reqwest::Client, mind: u128, tokens: u64) -> Attempt {
        let admitted = tokio::time::timeout(
            crate::inference::turn_admission::kv_page_wedge_bound(0),
            admit_turn(lanes, crate::inference::slots::SlotClass::Turn, Some(key(mind)), Some(self.pool.clone()), client, &self.root, tokens, std::time::Duration::ZERO),
        )
        .await;
        let Ok(Ok(mut turn)) = admitted else {
            return Attempt::Deferred;
        };
        match turn.page_in() {
            PageIn::Restored | PageIn::RestoredBehindLive | PageIn::Resident => {
                let slot = turn.slot().expect("test: a mind's turn holds a slot");
                self.slots.lock().expect("test: fixture mutex").insert(slot, format!("{mind}:{tokens}"));
                turn.generation_completed();
                Attempt::Warm
            }
            cold => Attempt::Cold(cold),
        }
    }

    /// Stop the way the M5 went down between engines: the stop's checkpoint saves every resident.
    async fn checkpoint_and_stop(self, client: &reqwest::Client) {
        let stopped = crate::inference::slots::page_io::checkpoint_endpoint(client, &self.root, &self.endpoint).await;
        assert!(matches!(stopped, crate::inference::slots::page_io::StopCheckpoint::Saved), "test: the stop saves every resident: {stopped:?}");
        self.server.abort();
    }

    fn refused(&self) -> usize {
        self.hold.refused.load(SeqCst)
    }
}

/// Dyno 4's recorded state: a 125,696-cell unified pool; Aiko's 59,144-token page saved and
/// confirmed; two others resident, one holding 70,000 cells (more than the 66,552 the pool leaves
/// her page). Her return evicts the small resident and asks the engine to restore her page,
/// which the engine refuses: "failed to find 59144 available cells".
async fn refused_for_free_cells(dir: &std::path::Path, lanes: &Arc<Semaphore>, client: &reqwest::Client) -> Rig {
    let cells = free_cells();
    let rig = rig(dir, Some(cells.pool_cells)).await;
    rig.act(lanes, client, 1, cells.page_cells).await;
    rig.act(lanes, client, 2, cells.others_resident_cells).await;
    // Shape: a small third resident whose act pages Aiko out (the cheapest tail goes first).
    rig.act(lanes, client, 3, 1_000).await;
    assert!(rig.pool.confirms(&key(1)), "test: her page was saved and confirmed");
    rig
}

/// Release a resident's KV, as a verified erase or page-out would: the engine's cells come back.
fn reclaim(slots: &Slots, mind: u128) {
    slots.lock().expect("test: fixture mutex").retain(|_, held| !held.starts_with(&format!("{mind}:")));
}

// what this catches (the M5, 2026-11-04: restore 400 "failed to find 59144 available cells" in a
// 125,696-cell pool; the ledger's confirmed minds fell from four at 19:38Z to zero at 21:04Z, and
// later turns went cold `page_on_disk_unconfirmed`): a capacity refusal routed through
// `note_page_lost`. Her page is good; the pool was full. After a restart with no stop (the
// persisted ledger is all that carries her confirmation) she restores, never re-prefills.
#[tokio::test]
#[ignore = "ONE-ACT slice 1b: a capacity refusal keeps confirmation across a restart; only an invalid page loses it"]
async fn a_capacity_refusal_keeps_her_confirmation_across_a_restart() {
    let cells = free_cells();
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let core_a = refused_for_free_cells(dir.path(), &lanes, &client).await;
    let outcome = core_a.attempt(&lanes, &client, 1, cells.page_cells).await;
    let refused = core_a.refused();
    let expected = RestoreOutcome::DeferredCapacity(CapacityShortfall::FreeCells { requested: cells.page_cells });
    assert!(expected.keeps_confirmation());
    core_a.server.abort();

    let core_b = rig(dir.path(), None).await;
    assert!(
        core_b.pool.confirms(&key(1)),
        "{expected:?} keeps her confirmation across a restart; the persisted ledger lost it \
         (first core: {outcome:?}, {refused} engine refusal(s))"
    );
    assert_eq!(core_b.attempt(&lanes, &client, 1, cells.page_cells).await, Attempt::Warm, "she restores, never cold");
    core_b.server.abort();
}

// what this catches (the same receipt): a transient free-cell refusal answered with a cold turn.
// She is deferred, never handed a prefill from her first token; once the engine's cells are
// verifiably reclaimed (the other resident released), her next admission restores her page.
#[tokio::test]
#[ignore = "ONE-ACT slice 1b: transient pressure defers her; after verified reclamation she restores without a cold prefill"]
async fn verified_reclamation_restores_her_without_a_cold_prefill() {
    let cells = free_cells();
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let core = refused_for_free_cells(dir.path(), &lanes, &client).await;
    let first = core.attempt(&lanes, &client, 1, cells.page_cells).await;
    assert!(!matches!(first, Attempt::Cold(_)), "a capacity refusal handed her a cold turn: {first:?}");
    reclaim(&core.slots, 2);
    assert_eq!(core.attempt(&lanes, &client, 1, cells.page_cells).await, Attempt::Warm, "after reclamation she restores");
    assert!(core.pool.confirms(&key(1)));
    core.server.abort();
}

// what this catches (the M5, 2026-11-04: a relaunch at -c 33,040 onto a 33,280-cell pool; pages
// of 35,842 and 33,542 tokens saved under the larger engine; restore 400 "n_tokens = 35838 >
// size = 33280"): asking the engine for a restore that cannot fit at all, then forgetting the
// page. A page larger than the whole compatible pool stays confirmed and deferred (placement or
// capacity must change), and admission knows it before asking: no restore request reaches the
// engine, not once and not again. (The launch half, that the launch never sizes below the
// residents' pages, is `the_launch_never_sizes_below_the_residents_pages`.)
#[tokio::test]
#[ignore = "ONE-ACT slice 1b: an oversize page stays confirmed and deferred; admission checks fit before asking"]
async fn a_page_larger_than_the_whole_pool_stays_confirmed_and_is_never_asked_for() {
    let oversize = day().oversize.value;
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let [first_page, second_page] = oversize.pages;
    let larger = rig(dir.path(), None).await;
    larger.act(&lanes, &client, 1, first_page).await;
    larger.act(&lanes, &client, 2, second_page).await;
    larger.checkpoint_and_stop(&client).await;

    let small = rig(dir.path(), Some(oversize.pool_cells)).await;
    assert!(u64::from(oversize.launch_ctx) < first_page && first_page > oversize.pool_cells, "test: the recorded page exceeds the pool");
    let expected = RestoreOutcome::DeferredCapacity(CapacityShortfall::ExceedsPool { tokens: first_page, pool: oversize.pool_cells });
    let tries = [
        small.attempt(&lanes, &client, 1, first_page).await,
        small.attempt(&lanes, &client, 1, first_page).await,
    ];
    let asked = small.hold.restores.load(SeqCst);
    assert!(tries.iter().all(|t| !matches!(t, Attempt::Cold(_))), "{expected:?} handed her a cold turn: {tries:?}");
    assert!(small.pool.confirms(&key(1)), "{expected:?} keeps her confirmation: {tries:?}");
    assert_eq!(asked, 0, "admission asked the engine {asked} time(s) for a page larger than its whole pool");
    small.server.abort();
}

// what this catches (ONE-ACT §5b.1, "incompatibility preserves the artifact"): a page refused by
// a destination it does not suit treated as gone. The file stays, its confirmation stays, and a
// compatible engine restores it. The refusal body is the fixture's placeholder until the fork
// names the reason (`recorded::INCOMPATIBLE_REFUSAL`).
#[tokio::test]
#[ignore = "ONE-ACT slice 1b: an incompatible destination keeps the artifact and its provenance"]
async fn an_incompatible_destination_keeps_the_page_for_a_compatible_one() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let core_a = rig(dir.path(), None).await;
    for (mind, tokens) in [(1, 100), (2, 200), (3, 300)] {
        core_a.act(&lanes, &client, mind, tokens).await;
    }
    core_a.hold.scripted.lock().expect("test: fixture mutex").push_back(INCOMPATIBLE_REFUSAL.to_owned());
    let outcome = core_a.attempt(&lanes, &client, 1, 100).await;
    assert_eq!(core_a.pool.page_on_disk(&key(1)), Some(true), "the artifact is never deleted by a refusal");
    let kept = core_a.pool.confirms(&key(1));
    core_a.server.abort();

    let core_b = rig(dir.path(), None).await;
    assert!(
        kept && core_b.pool.confirms(&key(1)),
        "{:?} keeps her provenance (kept in the refusing core: {kept}; first attempt {outcome:?})",
        RestoreOutcome::Incompatible
    );
    assert_eq!(core_b.attempt(&lanes, &client, 1, 100).await, Attempt::Warm, "a compatible engine restores it");
    core_b.server.abort();
}

// what this catches (ONE-ACT §5b.1, "proven corruption invalidates only the affected artifact"):
// the fix over-preserving (a corrupt page kept confirmed, retried forever) or over-invalidating
// (one bad page dropping another mind's). Green today, since today every refusal invalidates its
// own page only; it guards the typed outcome. When the fork names corruption, the scripted body
// (`recorded::CORRUPT_REFUSAL`) becomes that reason.
#[tokio::test]
async fn proven_corruption_invalidates_only_the_affected_page() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let core = rig(dir.path(), None).await;
    // Minds 1 and 2 are paged out by 3 and 4 (the cheapest tail goes first).
    for (mind, tokens) in [(1, 100), (2, 200), (3, 300), (4, 400)] {
        core.act(&lanes, &client, mind, tokens).await;
    }
    assert!(core.pool.confirms(&key(1)) && core.pool.confirms(&key(2)), "test: both pages saved and confirmed");
    core.hold.scripted.lock().expect("test: fixture mutex").push_back(CORRUPT_REFUSAL.to_owned());
    let corrupt = core.attempt(&lanes, &client, 1, 100).await;
    assert!(!RestoreOutcome::InvalidPage.keeps_confirmation());
    assert!(!core.pool.confirms(&key(1)), "the corrupt page is no longer offered: {corrupt:?}");
    assert!(core.pool.confirms(&key(2)), "another mind's page is untouched");
    assert_eq!(core.attempt(&lanes, &client, 2, 200).await, Attempt::Warm, "and it restores");
    core.server.abort();
}

// what this catches (ONE-ACT §5b.1, "concurrent admissions cannot double-spend free capacity"):
// two minds returning at once into a pool that holds either page but not both (59,144 + 70,000 >
// 125,696). Admission reserves through one authority: one restores, the other is deferred with
// her page confirmed, and the engine is never asked for cells another admission already took.
#[tokio::test]
#[ignore = "ONE-ACT slice 5: concurrent admissions reserve capacity once; none asks for cells another took"]
async fn concurrent_admissions_never_double_spend_free_cells() {
    let cells = free_cells();
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let larger = rig(dir.path(), None).await;
    larger.act(&lanes, &client, 1, cells.page_cells).await;
    larger.act(&lanes, &client, 2, cells.others_resident_cells).await;
    larger.checkpoint_and_stop(&client).await;

    let core = rig(dir.path(), Some(cells.pool_cells)).await;
    let (one, two) = tokio::join!(
        core.attempt(&lanes, &client, 1, cells.page_cells),
        core.attempt(&lanes, &client, 2, cells.others_resident_cells),
    );
    let attempts = [one, two];
    assert!(attempts.iter().all(|a| !matches!(a, Attempt::Cold(_))), "a mind went cold: {attempts:?}");
    assert!(attempts.contains(&Attempt::Warm), "one of them restores: {attempts:?}");
    assert!(core.pool.confirms(&key(1)) && core.pool.confirms(&key(2)), "both pages stay confirmed: {attempts:?}");
    assert_eq!(core.refused(), 0, "the engine refused {} restore(s) admission should never have asked for", core.refused());
    core.server.abort();
}

// what this catches (ONE-ACT §5b.1, "timeout and late receipt retain custody"): a page switch that
// times out and whose terminal receipt arrives later. Not drivable yet: the missing seam is named.
#[test]
#[ignore = "ONE-ACT slice 4: a timeout and a late receipt retain custody until a terminal receipt or a fence"]
fn a_timeout_and_a_late_receipt_retain_custody() {
    panic!("{}", MissingSeam::LateReceiptCustody.describe());
}

// what this catches (ONE-ACT §5b.1, "completed save and ledger replacement survive interruption"):
// a ledger replace interrupted mid-write (its sibling left truncated, the rename never done) that
// costs a completed save. The ledger replaces atomically, so the next core reads the last whole
// ledger and she restores. Green today; it guards the custody rewrite of the ledger.
#[tokio::test]
async fn a_completed_save_survives_an_interrupted_ledger_replace() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let (client, lanes) = (reqwest::Client::new(), Arc::new(Semaphore::new(2)));
    let core_a = rig(dir.path(), None).await;
    for (mind, tokens) in [(1, 100), (2, 200), (3, 300)] {
        core_a.act(&lanes, &client, mind, tokens).await;
    }
    assert!(core_a.pool.confirms(&key(1)), "test: her save completed and was confirmed");
    let ledger = core_a.pool.ledger_path().expect("test: the pool keeps a ledger");
    // The next replace's sibling, cut off mid-write (`SavedLedger::write`: write the sibling, then
    // replace), and the core gone before the rename.
    std::fs::write(ledger.with_extension("json.tmp"), b"{\"format\":2,\"fingerprint\":\"").expect("test: torn sibling");
    core_a.server.abort();

    let core_b = rig(dir.path(), None).await;
    assert!(core_b.pool.confirms(&key(1)), "the last whole ledger carries her completed save");
    assert_eq!(core_b.attempt(&lanes, &client, 1, 100).await, Attempt::Warm);
    core_b.server.abort();
}
