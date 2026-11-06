//! A mind's live sequence against her older saved page (the M5, core ed142f14c,
//! 2026-11-06 16:13-19:17Z): Benchy lost 254,147 tokens and 4,420 prefill-seconds over 7 acts.
//! A context refit dropped her first admission before it sent; the drop forgot her residency;
//! the retry restored her OLD page (22,222 slot tokens, checkpoint 9,078 every time) over her
//! live, newer sequence, and the completion probe called it "warm" because cached > 0.
//!
//! The real admission (`admit_turn`), the real engine adapter (`kv_page_action`) and the real
//! pool run against the lifecycle fixture's fake engine, whose slots hold "mind:tokens".
#![cfg(test)]
use super::lifecycle::{act, boot, fake_engine, SaveHold, Slots};
use super::*;
use std::sync::atomic::Ordering::SeqCst;

fn key(mind: u128) -> PageKey {
    PageKey::mind(Uuid::from_u128(mind)).expect("test: non-nil id")
}

struct Rig {
    root: String,
    slots: Slots,
    hold: Arc<SaveHold>,
    pool: Arc<KvSlotPool>,
    lanes: Arc<Semaphore>,
    client: reqwest::Client,
    server: tokio::task::JoinHandle<()>,
}

/// Benchy's state before the refit: mind 1 resident on slot 1 at 150 tokens, her saved page an
/// older 100 (paged out by mind 3, restored, then grown); mind 3 resident on slot 0.
async fn resident_ahead_of_her_page(dir: &std::path::Path) -> Rig {
    let client = reqwest::Client::new();
    let lanes = Arc::new(Semaphore::new(2));
    let (root, slots, hold, server) = fake_engine(dir.to_path_buf()).await;
    let (_endpoint, pool) = boot(&root, dir).await;
    for (mind, tokens) in [(1, 100), (2, 200), (3, 300), (1, 150)] {
        act(&lanes, &pool, &client, &root, &slots, mind, tokens).await;
    }
    let held = slots.lock().expect("test: fixture mutex").get(&1).cloned();
    assert_eq!(held.as_deref(), Some("1:150"), "test: her live sequence, slot 1");
    let page = std::fs::read_to_string(dir.join(crate::inference::slots::page_filename(&key(1)))).ok();
    assert_eq!(page.as_deref(), Some("1:100"), "test: her older page");
    Rig { root, slots, hold, pool, lanes, client, server }
}

impl Rig {
    async fn admit(&self, mind: u128, tokens: u64) -> TurnAdmission {
        admit_turn(&self.lanes, crate::inference::slots::SlotClass::Turn, Some(key(mind)), Some(self.pool.clone()), &self.client, &self.root, tokens, std::time::Duration::ZERO)
            .await
            .expect("test: admitted")
    }

    fn held(&self, turn: &TurnAdmission) -> Option<String> {
        self.slots.lock().expect("test: fixture mutex").get(&turn.slot().expect("test: a slot")).cloned()
    }
}

// what this catches (Benchy, M5 ed142f14c, 7 acts, 4,420 prefill-seconds): an admission dropped
// before it ever sent (a ContextExceeded refit) forgets her residency, so her retry restores her
// older page over the live, newer sequence in her slot.
#[tokio::test]
async fn an_admission_that_never_sent_keeps_her_residency_and_her_retry_resumes_her_live_sequence() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let rig = resident_ahead_of_her_page(dir.path()).await;
    let restores = rig.hold.restores.load(SeqCst);

    let refit = rig.admit(1, 160).await;
    assert_eq!(refit.page_in(), PageIn::Resident, "test: her KV is resident");
    drop(refit); // refused before the POST: nothing reached the engine

    let retry = rig.admit(1, 160).await;
    assert_eq!(retry.page_in(), PageIn::Resident, "her retry resumes her live sequence");
    assert_eq!(rig.hold.restores.load(SeqCst), restores, "no restore reached the engine");
    assert_eq!(rig.held(&retry).as_deref(), Some("1:150"), "her slot still holds her live sequence, not her older page");
    drop(retry);
    rig.server.abort();
}

// what this catches: the pool's own guard, beneath the admission. A slot that lost her attribution
// while still holding her CONFIRMED sequence, newer than her page (her save left it there and no
// restore started), is resumed in place, never restored over.
#[tokio::test]
async fn a_restore_never_lands_over_her_newer_confirmed_sequence() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let rig = resident_ahead_of_her_page(dir.path()).await;
    let restores = rig.hold.restores.load(SeqCst);
    rig.pool.forget_resident(1, key(1)); // attribution lost; the slot untouched

    let retry = rig.admit(1, 160).await;
    assert_eq!(retry.page_in(), PageIn::Resident, "her newer confirmed sequence wins over her older page");
    assert_eq!(rig.hold.restores.load(SeqCst), restores, "no restore reached the engine");
    assert_eq!(rig.held(&retry).as_deref(), Some("1:150"));
    drop(retry);
    rig.server.abort();
}

// what this catches (Codex on #5804 at 541919d6f): a SENT request (a changed prompt on the same page)
// that failed without a terminal receipt may have truncated or replaced her slot; its old completed
// revision must not claim a live hit. She falls back to her confirmed page.
#[tokio::test]
async fn a_sent_request_that_failed_never_claims_her_live_sequence() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let rig = resident_ahead_of_her_page(dir.path()).await;
    let restores = rig.hold.restores.load(SeqCst);

    let mut failed = rig.admit(1, 160).await;
    failed.dispatching(); // the POST went out
    let slot = failed.slot().expect("test: a slot");
    rig.slots.lock().expect("test: fixture mutex").insert(slot, "1:40".into()); // the changed prompt truncated her slot
    drop(failed); // no terminal receipt

    let retry = rig.admit(1, 160).await;
    assert_ne!(retry.page_in(), PageIn::Resident, "an unconfirmed slot proves no live hit");
    assert_eq!(rig.hold.restores.load(SeqCst), restores + 1, "her confirmed page is restored");
    assert_eq!(rig.held(&retry).as_deref(), Some("1:100"), "her page, never the truncated slot");
    drop(retry);
    rig.server.abort();
}

// what this catches: a restore judged "warm" because the engine reused tokens, when the page is
// older than the sequence she last completed (that sequence was overwritten unsaved). It is the
// best left, and it is named a regression, typed, for the completion probe to report.
#[tokio::test]
async fn a_restore_older_than_her_newest_sequence_is_named_a_regression() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let rig = resident_ahead_of_her_page(dir.path()).await;

    let mut failed = rig.admit(1, 160).await;
    failed.dispatching();
    drop(failed); // her attribution is gone, so the next page in her slot saves nothing of hers
    act(&rig.lanes, &rig.pool, &rig.client, &rig.root, &rig.slots, 2, 250).await; // evicts her lease, the cheapest
    let overwritten = rig.slots.lock().expect("test: fixture mutex").get(&1).cloned();
    assert_eq!(overwritten.as_deref(), Some("2:250"), "test: mind 2 took her slot");

    let back = rig.admit(1, 160).await;
    assert_eq!(back.page_in(), PageIn::RestoredBehindLive, "her page (100) is older than her newest sequence (150)");
    assert_eq!(rig.held(&back).as_deref(), Some("1:100"), "test: the restore landed her older page");
    drop(back);
    rig.server.abort();
}

// what this catches: the regression class swallowing a current restore. A page that captured her
// newest sequence restores as `Restored`, never `RestoredBehindLive`.
#[tokio::test]
async fn a_restore_of_her_newest_sequence_is_current() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let rig = resident_ahead_of_her_page(dir.path()).await;
    act(&rig.lanes, &rig.pool, &rig.client, &rig.root, &rig.slots, 2, 250).await; // pages her out, saved at 150

    let back = rig.admit(1, 160).await;
    assert_eq!(back.page_in(), PageIn::Restored);
    assert_eq!(rig.held(&back).as_deref(), Some("1:150"), "her newest sequence, restored");
    drop(back);
    rig.server.abort();
}
