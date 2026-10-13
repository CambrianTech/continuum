//! Whose hands a code call acts with: the caller, and the context the caller is acting in.
//!
//! A citizen can act in more than one context at once: her live session, an eval of her,
//! a solve on a fork. Each has its own workspace and shell. Before this, every hands map
//! (`CodeState.file_engines` / `shell_sessions`, the acting-root record)
//! was keyed by the caller alone, so an eval and her live session were the same hands: an
//! eval's root moved her live tools, and its restore moved them home
//! (docs/architecture/HER-HANDS.md, "What acts on a citizen's hands today").
//!
//! [`HandsContext`] is bound by the kernel side of a connection
//! ([`CallerIdentity::in_hands`](crate::routing::CallerIdentity::in_hands)), never read from
//! params, so a caller can't name another context's hands. [`HandsKey`] is the one key
//! every hands map uses. The caller stays the authorization identity; the key only says
//! WHICH of her hands.

use uuid::Uuid;

/// One acting context of a caller: a session of work with its own workspace and shell.
/// `None` on a caller identity is her default (live) context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandsContext {
    id: Uuid,
    kind: ContextKind,
}

/// What a context is, which decides what may reach it and whether it can hold a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextKind {
    /// An eval's, a fork's or a replay's own hands: isolated from every other set of the
    /// caller's hands (nothing of hers reaches in, and it reaches nothing of hers).
    Isolated,
    /// The hands of one claim of a card: bound once at its checkout, hers across her turns.
    Claim,
    /// The stand-in a card turn acts with when its claim's hands could not be bound: never
    /// given a workspace, so code tools refuse by name and nothing lands at home.
    Unbound,
}

impl ContextKind {
    fn tag(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Claim => "claim",
            Self::Unbound => "unbound",
        }
    }
}

impl HandsContext {
    /// A fresh context, distinct from every other: an eval or a fork opens one.
    pub fn fresh() -> Self {
        Self { id: Uuid::new_v4(), kind: ContextKind::Isolated }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn kind(&self) -> ContextKind {
        self.kind
    }

    /// The context a citizen works ONE CLAIM of a card in: the same for every turn of that
    /// claim, so her engine, shell and execution handles persist across those turns, and a
    /// different one for any other claim (a released and re-acquired card is a new claim, so
    /// it never inherits the old one's hands or authority: Codex root on fc9e9fd7). Derived,
    /// not stored: any turn that knows the claim names the same hands.
    pub fn for_claim(persona: Uuid, card: Uuid, claim: Uuid) -> Self {
        Self { id: claim_id(persona, card, claim), kind: ContextKind::Claim }
    }

    /// The hands a turn on `card` acts with when the card's own claim hands could not be
    /// bound (no checkout on this node, no claim id, a rooting failure): NEVER rooted, so every
    /// code tool refuses by name while every other tool still works, and nothing meant for the
    /// card lands in her home (Astra on #5026: a failed binding ran at home). Stable per card.
    pub fn unbound(persona: Uuid, card: Uuid) -> Self {
        Self { id: claim_id(persona, card, Uuid::nil()), kind: ContextKind::Unbound }
    }
}

fn claim_id(persona: Uuid, card: Uuid, claim: Uuid) -> Uuid {
    let mut name = [0u8; 32];
    name[..16].copy_from_slice(card.as_bytes());
    name[16..].copy_from_slice(claim.as_bytes());
    Uuid::new_v5(&persona, &name)
}

/// Admission to one set of hands: whether they take new work, and how much is in flight.
/// The DISPATCHER admits every command bound to a context for the command's whole life,
/// foreground or background (`CommandExecutor::dispatch`), and a turn holds one lease from
/// its binding to its end. Releasing the hands closes them: new work is refused and the
/// release waits the admitted work out, so nothing in flight loses the engine or shell it
/// runs in (Codex root and Astra on fc9e9fd7 and #5026).
#[derive(Default)]
struct Gate {
    closing: bool,
    in_flight: usize,
    /// What admitted work started and couldn't confirm ended (Codex root on #5026).
    lingering: Lingering,
}

/// What admitted work started and couldn't confirm ended when it ended it (a `gh` and its
/// `git`): processes still running as themselves, which a release re-checks, and trees that
/// couldn't be walked at all, which no re-check can confirm. A release keeps the hands while
/// either is non-empty, the same rule it applies to a shell's survivors.
#[derive(Debug, Clone, Default)]
pub struct Lingering {
    pub processes: Vec<crate::inference::lane_process::ProcessIdentity>,
    pub unknown: Vec<String>,
}

impl Lingering {
    pub fn is_empty(&self) -> bool {
        self.processes.is_empty() && self.unknown.is_empty()
    }
}

type GateCell = std::sync::Arc<(std::sync::Mutex<Gate>, tokio::sync::Notify)>;

static GATES: std::sync::LazyLock<dashmap::DashMap<HandsContext, GateCell>> = std::sync::LazyLock::new(dashmap::DashMap::new);

/// One admitted command's or turn's hold on a set of hands. Dropping it lets a release finish.
pub struct HandsLease {
    cell: GateCell,
}

impl Drop for HandsLease {
    fn drop(&mut self) {
        let (gate, ended) = &*self.cell;
        let mut g = gate.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still counts down
        g.in_flight = g.in_flight.saturating_sub(1);
        if g.in_flight == 0 {
            ended.notify_waiters();
        }
    }
}

impl HandsLease {
    /// Record what ending a process this admitted work owned left unconfirmed, BEFORE the lease
    /// drops: the release that waits the lease out then finds it. A survivor (the child itself
    /// when its reap is unproven, or a descendant) is re-checked; an unwalked tree is held.
    pub fn keep_unconfirmed(&self, end: &crate::inference::lane_process::TreeEnd) {
        if end.survivors.is_empty() && end.unwalked.is_none() {
            return;
        }
        let mut g = self.cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.lingering.processes.extend(end.survivors.iter().copied());
        g.lingering.unknown.extend(end.unwalked.clone());
    }
}

/// Admit work to `context`'s hands, or refuse by name when they are closing.
pub fn admit(context: HandsContext) -> Result<HandsLease, String> {
    let cell = GATES.entry(context).or_default().clone();
    {
        let mut g = cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still admits or refuses
        if g.closing {
            return Err(format!("the hands {} are released (their claim or their eval ended); nothing new runs in them", context.id()));
        }
        g.in_flight += 1;
    }
    Ok(HandsLease { cell })
}

tokio::task_local! {
    /// The admission of the command running on this task: set by the dispatcher for the
    /// command's whole life, read by [`spawn_blocking`] so work the command hands to a blocking
    /// thread keeps it.
    static ADMITTED: Option<std::sync::Arc<HandsLease>>;
}

/// Run `work` (one dispatched command) holding `admission` as the task's current one.
pub async fn admitted<F: std::future::Future>(admission: Option<std::sync::Arc<HandsLease>>, work: F) -> F::Output {
    ADMITTED.scope(admission, work).await
}

std::thread_local! {
    /// The admission of the blocking work running on this thread ([`spawn_blocking`] sets it for
    /// the work's life), so work it starts in turn (a nested blocking spawn, a child process)
    /// keeps the same admission: a task-local doesn't reach a blocking thread.
    static BLOCKING_ADMITTED: std::cell::RefCell<Option<std::sync::Arc<HandsLease>>> = const { std::cell::RefCell::new(None) };
}

/// The admission of the command running here, on its task or in blocking work it started, for
/// an owner of work that can outlive the command's future (a blocking thread, a child process)
/// to keep until that work ends.
pub fn current() -> Option<std::sync::Arc<HandsLease>> {
    ADMITTED
        .try_with(|a| a.clone())
        .ok()
        .flatten()
        .or_else(|| BLOCKING_ADMITTED.with(|a| a.borrow().clone()))
}

/// THE shared owner of blocking work started by a dispatched command: the work keeps the
/// command's admission until IT returns, not until the command's future does. A dispatch
/// that is cancelled or times out drops its future, but a blocking thread can't be cancelled
/// (a git operation runs to its end), and a release that finished under it would remove the
/// workspace the operation is still writing (Astra on #5026). Every blocking spawn in a hands
/// command goes through here; none is special-cased per tool.
pub fn spawn_blocking<F, R>(work: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let held = current();
    tokio::task::spawn_blocking(move || {
        // held for the work's life and visible to what it starts; restored by a guard (even when
        // the work panics) before the thread is pooled, so no later work on it inherits it
        let _current = BlockingAdmission::enter(held);
        work()
    })
}

/// The admission current on a blocking thread for one piece of work: set on entry, and the one
/// it replaced put back on drop, so a panicking `work()` can't leave it on the pooled thread
/// (Codex root on #5026).
struct BlockingAdmission {
    previous: Option<std::sync::Arc<HandsLease>>,
}

impl BlockingAdmission {
    fn enter(admission: Option<std::sync::Arc<HandsLease>>) -> Self {
        Self { previous: BLOCKING_ADMITTED.with(|a| a.replace(admission)) }
    }
}

impl Drop for BlockingAdmission {
    fn drop(&mut self) {
        let previous = self.previous.take();
        // try_with: during thread teardown the slot may already be gone, and then there is nothing to restore
        let _ = BLOCKING_ADMITTED.try_with(|a| *a.borrow_mut() = previous);
    }
}

/// Close `context`'s hands to new work and wait until every admitted command and turn has
/// ended: the first step of releasing them (`code/release-hands`). A closed gate STAYS
/// closed: a context is never reused (a re-acquired card is a new claim), so a stale binding
/// can never re-open old hands (Astra on #5026). The tombstone is an id and a flag, kept for
/// the process's life, one per context released. Returns what admitted work recorded as
/// started and not confirmed ended ([`HandsLease::keep_unconfirmed`]), for the release.
pub async fn close(context: HandsContext) -> Lingering {
    let cell = GATES.entry(context).or_default().clone();
    loop {
        let ended = cell.1.notified();
        {
            let mut g = cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still closes
            g.closing = true;
            if g.in_flight == 0 {
                return g.lingering.clone();
            }
        }
        ended.await;
    }
}

/// After a release re-checked `context`'s lingering processes: keep only the ones still
/// present, so a retried release re-checks exactly those. Unknown trees stay as they are.
pub fn retain_processes(context: HandsContext, still: Vec<crate::inference::lane_process::ProcessIdentity>) {
    if let Some(cell) = GATES.get(&context) {
        let mut g = cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.lingering.processes = still;
    }
}

/// The key of one set of hands: whose (the caller) and in which context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandsKey {
    /// The caller's identity string (`code_commands::caller_id`): who acts, and whose
    /// citizen layer and permissions apply.
    pub caller: String,
    /// The acting context, or `None` for the caller's default context.
    pub context: Option<HandsContext>,
}

impl HandsKey {
    pub fn new(caller: String, context: Option<HandsContext>) -> Self {
        Self { caller, context }
    }

    /// The string a hands map is keyed by. The default context keeps the bare caller, so
    /// a caller with no context has exactly the hands it had before contexts existed.
    pub fn map_key(&self) -> String {
        match self.context {
            None => self.caller.clone(),
            Some(context) => format!("{}#{}:{}", self.caller, context.kind().tag(), context.id()),
        }
    }

    /// Whether the session keyed `key` is one of the hands a handle lookup from THESE hands
    /// may reach: an isolated context (an eval, a fork) reaches only itself; her default hands
    /// and her card hands reach each other (a handle from claim A stays hers after her focus
    /// moves to B) but never into an isolated context (Astra on #5026).
    pub fn reaches(&self, key: &str) -> bool {
        match self.context {
            Some(context) if context.kind() == ContextKind::Isolated => key == self.map_key(),
            _ => {
                key == self.caller
                    || key.strip_prefix(&self.caller).and_then(|rest| rest.strip_prefix('#')).is_some_and(|rest| {
                        rest.starts_with(ContextKind::Claim.tag()) || rest.starts_with(ContextKind::Unbound.tag())
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two contexts of one caller sharing a hands map entry, which is
    // how an eval's root moved a citizen's live tools. The default context must keep the
    // pre-context key, so live hands are unchanged until something opens a context.
    #[test]
    fn each_context_of_a_caller_is_its_own_hands_and_the_default_keeps_the_caller() {
        let caller = "c0ffee00-0000-0000-0000-000000000001".to_string();
        let live = HandsKey::new(caller.clone(), None);
        let eval = HandsKey::new(caller.clone(), Some(HandsContext::fresh()));
        let solve = HandsKey::new(caller.clone(), Some(HandsContext::fresh()));
        assert_eq!(live.map_key(), caller, "the default context is the caller's existing hands");
        assert_ne!(eval.map_key(), live.map_key(), "an eval is not her live hands");
        assert_ne!(eval.map_key(), solve.map_key(), "two contexts are two sets of hands");
        assert!(eval.map_key().starts_with(&caller), "a context's hands stay attributable to their caller");
    }

    // what this catches (Codex root on #5026): work a blocking worker starts in turn lost the
    // command's admission, because a task-local doesn't reach a blocking thread, so a nested
    // worker (or a process it spawns) ran unleased. Blocking work sees its admission, and a
    // nested worker keeps it: the release waits for the nested one too.
    #[tokio::test]
    async fn a_nested_blocking_worker_keeps_the_admission_of_the_work_that_started_it() {
        let context = HandsContext::fresh();
        let lease = std::sync::Arc::new(admit(context).expect("open hands"));
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel::<bool>();
        let outer = admitted(Some(lease), async move {
            spawn_blocking(move || {
                let seen = current().is_some();
                let _nested = spawn_blocking(move || {
                    let _ = wait.recv(); // a nested worker still running after its starter returned
                });
                let _ = seen_tx.send(seen);
            })
        })
        .await;
        assert!(seen_rx.await.expect("the worker ran"), "blocking work sees the command's admission");
        outer.await.expect("the starting worker returned");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), close(context)).await.is_err(),
            "the release waits for the nested worker, not the one that started it"
        );
        go.send(()).expect("release the nested worker");
        tokio::time::timeout(std::time::Duration::from_secs(10), close(context))
            .await
            .expect("the release completes once the nested worker returns");
    }

    // what this catches (Codex root on #5026 at f014371be): a blocking worker that panicked left
    // its command's admission on the pooled thread (the restore ran after work()), so the next
    // work on that thread ran under it and the release never completed. One blocking thread, so
    // the second worker runs where the first panicked.
    #[test]
    fn a_panicking_blocking_worker_leaves_no_admission_on_its_pooled_thread() {
        let runtime = tokio::runtime::Builder::new_current_thread().max_blocking_threads(1).enable_all().build().expect("a runtime");
        runtime.block_on(async {
            let context = HandsContext::fresh();
            let lease = std::sync::Arc::new(admit(context).expect("open hands"));
            let worker = admitted(Some(lease), async { spawn_blocking(|| panic!("the work panics")) }).await;
            assert!(worker.await.is_err(), "the work panicked");
            assert!(!spawn_blocking(|| current().is_some()).await.expect("the next work ran"), "the pooled thread carries no admission");
            tokio::time::timeout(std::time::Duration::from_secs(10), close(context)).await.expect("the panicked work's admission was released");
        });
    }

    // what this catches (Codex root on #5026 at f014371be): an owned process whose tree couldn't
    // be walked was recorded as having no descendants, so its release confirmed what nothing had
    // checked. An unwalked tree, and an unproven reap among the survivors, reach the release.
    #[tokio::test]
    async fn what_an_owned_process_left_unconfirmed_reaches_the_release() {
        let context = HandsContext::fresh();
        let lease = admit(context).expect("open hands");
        let itself = crate::inference::lane_process::ProcessIdentity { pid: 4242, start_time: 7 };
        lease.keep_unconfirmed(&crate::inference::lane_process::TreeEnd {
            reaped: Err("could not confirm the process exited".into()),
            survivors: vec![itself],
            unwalked: Some("its descendants could not be walked".into()),
        });
        drop(lease);
        let lingering = close(context).await;
        assert_eq!(lingering.processes, vec![itself], "the unreaped child is re-checked by the release");
        assert_eq!(lingering.unknown.len(), 1, "the unwalked tree is held, never counted gone: {lingering:?}");
    }

    // what this catches (Astra on #5026): a dispatch cancelled while its blocking worker still
    // runs. The waiter's future drops its admission, but the worker (a git operation) can't be
    // cancelled, so a release that finished then would remove the workspace under it. Blocking
    // work keeps the admission until IT returns: the release waits for the worker, not the waiter.
    #[tokio::test]
    async fn a_release_waits_for_a_blocking_worker_whose_waiter_was_cancelled() {
        let key = HandsContext::fresh();
        let (finish, finished) = std::sync::mpsc::channel::<()>();
        let worker = admitted(Some(std::sync::Arc::new(admit(key).expect("open hands"))), async move {
            spawn_blocking(move || {
                let _ = finished.recv(); // the operation runs until told to end
            })
        })
        .await; // the dispatched command's scope has ended: its waiter is gone
        let closing = tokio::spawn(async move { close(key).await });
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(!closing.is_finished(), "the release waits for the running worker, not its cancelled waiter");
        finish.send(()).expect("the worker is still running");
        worker.await.expect("the worker ends");
        tokio::time::timeout(std::time::Duration::from_secs(5), closing).await.expect("the release completes once the worker returns").expect("close task");
    }

    // what this catches (Codex root and Astra on fc9e9fd7, #5026): closing a claim's hands
    // while a turn is admitted to them dropped its engine or shell under it; and a stale
    // admission (a call or turn that starts after the close) re-opened them. Closing refuses
    // new work at once, completes only after the admitted turn ends, and stays closed.
    #[tokio::test]
    async fn closing_a_claims_hands_refuses_new_turns_and_waits_out_the_admitted_one() {
        let key = HandsContext::fresh();
        let admitted = admit(key).expect("open hands admit a turn");
        let closing = tokio::spawn(async move { close(key).await });
        for _ in 0..8 {
            tokio::task::yield_now().await; // let the close run up to its wait
        }
        assert!(admit(key).is_err(), "closing hands admit no new turn");
        assert!(!closing.is_finished(), "the close waits for the admitted turn");
        drop(admitted);
        tokio::time::timeout(std::time::Duration::from_secs(5), closing).await.expect("closes once the turn ends").expect("close task");
        assert!(admit(key).is_err(), "a closed claim's hands stay closed: a stale binding can't re-open them");
    }
}
