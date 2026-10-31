use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A store that fails its first `fail` commits, then commits `winner` (or the proposal when `None`),
/// counting attempts and optionally pausing inside each (to race and to cancel).
struct Store {
    fail: AtomicUsize,
    winner: Option<TaskEnd>,
    attempts: AtomicUsize,
    pause: Duration,
}

impl Store {
    fn new(fail: usize, winner: Option<TaskEnd>, pause: Duration) -> Self {
        Store { fail: AtomicUsize::new(fail), winner, attempts: AtomicUsize::new(0), pause }
    }
}

#[async_trait::async_trait]
impl TerminalStore for Store {
    async fn finish(&self, _task: Uuid, _owner: Uuid, end: TaskEnd) -> Result<TaskEnd, String> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.pause).await;
        if self.fail.load(Ordering::SeqCst) > 0 {
            self.fail.fetch_sub(1, Ordering::SeqCst);
            return Err("injected durability failure".into());
        }
        Ok(self.winner.clone().unwrap_or(end)) // unwrap_or: no forced winner means the store commits the proposal itself
    }
}

fn held(task: u128, end: TaskEnd) -> HeldTerminal {
    HeldTerminal { task: Uuid::from_u128(task), owner: Uuid::from_u128(99), end, command: "code/shell".into(), elapsed_ms: 5 }
}

// what this catches: a terminal lost when its durable commit fails (background.rs logged and
// returned): it stays held through N failures and is delivered once the store recovers.
#[tokio::test]
async fn a_failed_commit_keeps_the_terminal_until_a_flush_commits_it() {
    let custody = TerminalCustody::new();
    let store = Store::new(2, None, Duration::ZERO);
    custody.hold(held(1, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    assert!(matches!(custody.commit(&store, Uuid::from_u128(1)).await, Err(CustodyError::Store(_))));
    let second = custody.flush(&store).await.expect("custody is open"); // test: a fixture step that must succeed
    assert!(second.committed.is_empty(), "second failure: still held");
    assert_eq!(second.failed.len(), 1, "and the failure is reported with its reason, not a silent zero");
    assert_eq!(custody.held(), 1);
    let delivered = custody.flush(&store).await.expect("custody is open").committed; // test: a fixture step that must succeed
    assert_eq!(delivered.len(), 1, "recovered: committed once");
    assert_eq!(delivered[0].winner, TaskEnd::Stopped);
    assert_eq!(custody.held(), 0);
    assert!(custody.flush(&store).await.expect("custody is open").committed.is_empty(), "never delivered twice"); // test: a fixture step that must succeed
}

// what this catches: delivering this writer's losing proposal when another supervisor committed a
// different end first: only the store's durable winner is delivered.
#[tokio::test]
async fn only_the_durable_winner_is_delivered() {
    let custody = TerminalCustody::new();
    let store = Store::new(0, Some(TaskEnd::Lost), Duration::ZERO);
    custody.hold(held(2, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    let done = custody.commit(&store, Uuid::from_u128(2)).await.expect("the store commits").expect("committed"); // test: a fixture step that must succeed
    assert_eq!(done.winner, TaskEnd::Lost);
    assert_eq!(done.held.end, TaskEnd::Stopped, "the proposal is kept only as the record");
}

// what this catches: a finish and a flush racing on one task both delivering it: one attempt holds
// the claim, the other yields, and the store sees exactly one commit.
#[tokio::test]
async fn racing_commits_deliver_once() {
    let custody = TerminalCustody::new();
    let store = Store::new(0, None, Duration::from_millis(30));
    custody.hold(held(3, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    let (a, b) = tokio::join!(custody.commit(&store, Uuid::from_u128(3)), custody.flush(&store));
    let delivered = usize::from(a.expect("the store commits").is_some()) + b.expect("custody is open").committed.len(); // test: a fixture step that must succeed
    assert_eq!(delivered, 1);
    assert_eq!(store.attempts.load(Ordering::SeqCst), 1);
}

// what this catches: a commit cancelled mid-IO (its future dropped) leaving the task claimed forever
// or erasing the held terminal: the claim is released and the terminal stays held.
#[tokio::test]
async fn a_cancelled_commit_releases_its_claim_and_keeps_the_terminal() {
    let custody = TerminalCustody::new();
    let slow = Store::new(0, None, Duration::from_secs(5));
    custody.hold(held(4, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    assert!(tokio::time::timeout(Duration::from_millis(20), custody.commit(&slow, Uuid::from_u128(4))).await.is_err());
    assert_eq!(custody.held(), 1, "cancellation must not erase the terminal");
    let fast = Store::new(0, None, Duration::ZERO);
    assert_eq!(custody.flush(&fast).await.expect("custody is open").committed.len(), 1, "the claim was released"); // test: a fixture step that must succeed
}

// what this catches: shutdown dropping held terminals silently, or admitting new ones after it
// closed: it waits for in-flight work, tries once more, and returns what it still holds.
#[tokio::test]
async fn shutdown_returns_every_unresolved_terminal_and_refuses_new_ones() {
    let custody = TerminalCustody::new();
    let broken = Store::new(usize::MAX, None, Duration::ZERO);
    custody.hold(held(5, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    custody.hold(held(6, TaskEnd::Lost)).expect("custody is open"); // test: a fixture step that must succeed
    let ShutdownReport { last_pass, mut unresolved } = custody.shutdown(&broken).await;
    assert!(last_pass.committed.is_empty());
    assert_eq!(last_pass.failed.len(), 2, "the final pass reports why each one is still held");
    unresolved.sort_by_key(|t| t.task);
    assert_eq!(unresolved.iter().map(|t| t.task).collect::<Vec<_>>(), vec![Uuid::from_u128(5), Uuid::from_u128(6)]);
    assert_eq!(custody.hold(held(7, TaskEnd::Stopped)), Err(CustodyError::Closed));
}

// what this catches: shutdown returning while a commit is still in flight (its result would be lost
// between the store and the report).
#[tokio::test]
async fn shutdown_waits_for_an_in_flight_commit() {
    let custody = TerminalCustody::new();
    let store = Store::new(0, None, Duration::from_millis(40));
    custody.hold(held(8, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    let in_flight = custody.commit(&store, Uuid::from_u128(8));
    let (done, report) = tokio::join!(in_flight, async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        custody.shutdown(&store).await
    });
    assert!(done.expect("the store commits").is_some(), "the in-flight attempt committed"); // test: a fixture step that must succeed
    assert!(report.last_pass.committed.is_empty() && report.unresolved.is_empty(), "nothing left, nothing double-delivered");
}

// what this catches: shutdown returning with store IO in flight because an outside commit or flush
// claimed a terminal after shutdown's empty check (Codex on #5664). Once draining begins, outside
// callers claim nothing; shutdown commits the rest itself, and when it returns no attempt is running.
#[tokio::test]
async fn draining_shutdown_is_exclusive_and_returns_with_no_io_in_flight() {
    let custody = TerminalCustody::new();
    let store = Store::new(0, None, Duration::from_millis(40));
    custody.hold(held(9, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    custody.hold(held(10, TaskEnd::Stopped)).expect("custody is open"); // test: a fixture step that must succeed
    // Task 9 is mid-commit when shutdown begins.
    let in_flight = custody.commit(&store, Uuid::from_u128(9));
    let outside = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let shutdown = custody.shutdown(&store);
        tokio::pin!(shutdown);
        // Let shutdown enter Draining, then race an outside flush and commit against it.
        assert!(futures::poll!(shutdown.as_mut()).is_pending(), "shutdown waits for task 9's attempt");
        assert!(matches!(custody.flush(&store).await, Err(CustodyError::Closed)), "an outside flush claims nothing while draining");
        assert!(matches!(custody.commit(&store, Uuid::from_u128(10)).await, Err(CustodyError::Closed)), "an outside commit claims nothing while draining");
        custody.hold(held(11, TaskEnd::Lost)).expect("a hold while draining is kept, to be committed or reported"); // test: a fixture step that must succeed
        let report = shutdown.await;
        assert!(custody.state.lock().committing.is_empty(), "shutdown returned with no attempt in flight");
        report
    };
    let (done, report) = tokio::join!(in_flight, outside);
    assert!(done.expect("the store commits").is_some(), "the in-flight attempt finished"); // test: a fixture step that must succeed
    let mut last: Vec<Uuid> = report.last_pass.committed.iter().map(|c| c.held.task).collect();
    last.sort();
    assert_eq!(last, vec![Uuid::from_u128(10), Uuid::from_u128(11)], "shutdown committed the rest itself");
    assert!(report.unresolved.is_empty());
    assert_eq!(store.attempts.load(Ordering::SeqCst), 3, "one attempt each: nothing outside claimed one while draining");
    assert_eq!(custody.hold(held(12, TaskEnd::Stopped)), Err(CustodyError::Closed), "closed after shutdown");
}
