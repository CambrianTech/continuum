//! The gate that admits work to one set of hands, and the lease an admitted command or turn
//! holds. The dispatcher admits every command bound to a context for its whole life
//! (`runtime::dispatch_hook`), the engine's process owner keeps the lease until a child's tree
//! has ended, and a release (`code/release-hands`) closes the gate and waits the admitted work
//! out. Every one of those reads the same gate, so it is a kernel fact beside the context it
//! gates ([`super::hands_context`]); the admission POLICY (which caller is admitted) stays with
//! the code layer's dispatch hook.
//!
//! (Not the one-per-persona `system_resources::hands_lease::PersonaHandsLease`, a different
//! exclusion.)

use super::hands_context::HandsContext;

/// Admission to one set of hands: whether they take new work, and how much is in flight.
/// The DISPATCHER admits every command bound to a context for the command's whole life,
/// foreground or background (`CommandExecutor::dispatch`), and a turn holds one lease from
/// its binding to its end. Releasing the hands closes them: new work is refused and the
/// release waits the admitted work out, so nothing in flight loses the engine or shell it
/// runs in (Codex root and Astra on fc9e9fd7 and #5026).
#[derive(Default)]
struct Gate {
    /// Private operator scopes have no externally reusable binding/tombstone.
    scoped: Option<HandsContext>,
    closing: bool,
    in_flight: usize,
    /// Detached shell tasks drain after release requests their existing kill path.
    processes_in_flight: usize,
    /// What admitted work started and couldn't confirm ended (Codex root on #5026).
    lingering: Lingering,
    /// What that work's callers held for it (the engine's `continuum_process::holding`, a
    /// checkout's sync lock): kept while anything lingers, released when a re-check finds
    /// nothing left (Codex reviewer on #5037).
    kept: Vec<crate::system_resources::disk_reporters::Kept>,
}

/// What admitted work started and couldn't confirm ended when it ended it (a `gh` and its
/// `git`): processes still running as themselves, which a release re-checks, and trees that
/// couldn't be walked at all, which no re-check can confirm. A release keeps the hands while
/// either is non-empty, the same rule it applies to a shell's survivors.
#[derive(Debug, Clone, Default)]
pub struct Lingering {
    pub processes: Vec<crate::system_resources::process_tree::ProcessIdentity>,
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
    process_custody: bool,
}

impl Drop for HandsLease {
    fn drop(&mut self) {
        let (gate, ended) = &*self.cell;
        let mut g = gate.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still counts down
        let count = if self.process_custody { &mut g.processes_in_flight } else { &mut g.in_flight };
        *count = count.saturating_sub(1);
        ended.notify_waiters();
        let scoped = g.scoped;
        drop(g);
        if let Some(context) = scoped { retire_scoped(context); }
    }
}

fn retire_scoped(context: HandsContext) {
    GATES.remove_if(&context, |_, cell| {
        let gate = cell.0.lock().unwrap_or_else(|error| error.into_inner());
        gate.scoped == Some(context) && gate.closing && gate.in_flight == 0 && gate.processes_in_flight == 0
            && gate.lingering.is_empty() && gate.kept.is_empty()
    });
}

impl HandsLease {
    /// Register detached process custody while its launching command is still
    /// admitted. It must not keep the foreground gate open: release waits that
    /// gate before asking the shell owner to kill, then drains this custody.
    pub fn process_custody(&self) -> Self {
        self.cell.0.lock().unwrap_or_else(|error| error.into_inner()).processes_in_flight += 1;
        Self { cell: self.cell.clone(), process_custody: true }
    }
    /// Record what ending a process this admitted work owned left unconfirmed, BEFORE the lease
    /// drops: the release that waits the lease out then finds it. A surviving descendant is
    /// re-checked; an unproven reap and an unwalked tree are held (`TreeEnd::unknown`).
    pub fn keep_unconfirmed(&self, end: &crate::system_resources::process_tree::TreeEnd) {
        if end.confirmed() {
            return;
        }
        let mut g = self.cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.lingering.processes.extend(end.survivors.iter().copied());
        g.lingering.unknown.extend(end.unknown());
    }

    /// Record an uncertainty no re-check can settle (a process that held an owned child's
    /// output past its bound, holders unidentified), BEFORE the lease drops: the release holds
    /// the hands and names it (Codex root on #5030).
    pub fn keep_unknown(&self, why: String) {
        let mut g = self.cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.lingering.unknown.push(why);
    }

    /// Keep what an unconfirmed end's caller held for it (the engine's `continuum_process::holding`)
    /// with what lingers, so an exclusion over a tree nobody confirmed ended stays held until a
    /// re-check finds nothing left ([`retain_processes`]), never only until the end's task returns.
    pub fn keep_held(&self, kept: &[crate::system_resources::disk_reporters::Kept]) {
        let mut g = self.cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.kept.extend(kept.iter().cloned());
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
    Ok(HandsLease { cell, process_custody: false })
}

tokio::task_local! {
    /// The admission of the command running on this task: set by the dispatcher for the
    /// command's whole life, read by the owner of a child process the command starts, which
    /// keeps it until the child's tree has ended.
    static ADMITTED: Option<std::sync::Arc<HandsLease>>;
}

/// Run `work` (one dispatched command) holding `admission` as the task's current one.
pub async fn admitted<F: std::future::Future>(admission: Option<std::sync::Arc<HandsLease>>, work: F) -> F::Output {
    ADMITTED.scope(admission, work).await
}

/// The admission of the command running on this task, for an owner of work that can outlive
/// the command's future (a child process) to keep until that work ends.
pub fn current() -> Option<std::sync::Arc<HandsLease>> {
    ADMITTED.try_with(|a| a.clone()).ok().flatten()
}

/// Operator work without contextual hands still needs the same child custody
/// when it holds a resource. Reuse an existing admission or own one scoped set;
/// cancellation closes its gate and unresolved children retain held resources.
pub async fn scoped_admitted<F: std::future::Future>(work: F) -> Result<F::Output, String> {
    if current().is_some() {
        return Ok(work.await);
    }
    struct Scope(HandsContext);
    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some(cell) = GATES.get(&self.0) {
                cell.0
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .closing = true;
            }
            retire_scoped(self.0);
        }
    }
    let scope = Scope(HandsContext::fresh());
    let admission = std::sync::Arc::new(admit(scope.0)?);
    admission.cell.0.lock().unwrap_or_else(|error| error.into_inner()).scoped = Some(scope.0);
    let output = admitted(Some(admission), work).await;
    let lingering = close(scope.0).await;
    if !lingering.is_empty() {
        crate::clog_warn!(
            "Scoped process admission {} retains unresolved children and their resources: {:?}",
            scope.0.id(),
            lingering
        );
    }
    Ok(output)
}

/// Close `context`'s hands to new work and wait until every admitted command and turn has
/// ended: the first step of releasing them (`code/release-hands`). A closed gate STAYS
/// closed: a context is never reused (a re-acquired card is a new claim), so a stale binding
/// can never re-open old hands (Astra on #5026). The tombstone is an id and a flag, kept for
/// the process's life, one per context released. Returns what admitted work recorded as
/// started and not confirmed ended ([`HandsLease::keep_unconfirmed`]), for the release.
pub async fn close(context: HandsContext) -> Lingering {
    drain(context, false).await
}

/// Called after the shell owner requests termination. Includes output draining
/// and any uncertainty recorded on task cancellation before forgetting the world.
pub async fn drain_processes(context: HandsContext) -> Lingering {
    drain(context, true).await
}

async fn drain(context: HandsContext, processes: bool) -> Lingering {
    let cell = GATES.entry(context).or_default().clone();
    loop {
        let ended = cell.1.notified();
        {
            let mut g = cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still closes
            g.closing = true;
            if g.in_flight == 0 && (!processes || g.processes_in_flight == 0) {
                return g.lingering.clone();
            }
        }
        ended.await;
    }
}

/// After a release re-checked `context`'s lingering processes: keep only the ones still
/// present, so a retried release re-checks exactly those. Unknown trees stay as they are. When
/// nothing lingers any more, what callers held for that work is released.
pub fn retain_processes(context: HandsContext, still: Vec<crate::system_resources::process_tree::ProcessIdentity>) {
    if let Some(cell) = GATES.get(&context) {
        let mut g = cell.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned gate still records
        g.lingering.processes = still;
        if g.lingering.is_empty() {
            g.kept.clear();
        }
    }
    retire_scoped(context);
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#5058): the release deadlock where a detached shell held FOREGROUND
    // admission, so close() waited on a task only the not-yet-sent kill could end. Its process
    // custody is counted apart: close() returns, the release sends the kill, and only then does
    // drain_processes wait out the task and read what it left unconfirmed.
    #[tokio::test]
    async fn release_can_request_kill_before_draining_detached_process_custody() {
        let context = HandsContext::fresh();
        let command = admit(context).unwrap();
        let process = command.process_custody();
        let mut closing = Box::pin(close(context));
        assert!(futures::poll!(&mut closing).is_pending(), "launching command still owns admission");
        drop(command);
        assert!(closing.await.is_empty(), "release can now reach its shell kill authority");
        assert!(admit(context).is_err(), "release has sealed new commands");
        let mut draining = Box::pin(drain_processes(context));
        assert!(futures::poll!(&mut draining).is_pending(), "world must survive process/output lifetime");
        process.keep_unknown("cancelled output lifetime".into());
        drop(process);
        assert_eq!(draining.await.unknown, vec!["cancelled output lifetime"], "uncertainty is read after process drain");
    }

    // 5058: an operator scope must preserve resources with an unresolved child,
    // while refusing new work after the scope returns.
    #[tokio::test]
    async fn scoped_operator_custody_retains_unconfirmed_resources() {
        let witness = std::sync::Arc::new(());
        let weak = std::sync::Arc::downgrade(&witness);
        let context = scoped_admitted(async move {
            let lease = current().expect("operator admission");
            let context = GATES.iter().find(|entry| std::sync::Arc::ptr_eq(entry.value(), &lease.cell)).map(|entry| *entry.key()).unwrap();
            lease.keep_unconfirmed(&crate::system_resources::process_tree::TreeEnd {
                reaped: Ok(()), survivors: vec![crate::system_resources::process_tree::ProcessIdentity { pid: 4242, start_time: 7 }], unwalked: None,
            });
            lease.keep_held(&[witness]);
            context
        }).await.unwrap();
        assert!(admit(context).is_err());
        assert!(weak.upgrade().is_some(), "scope completion is not confirmed process completion");
        retain_processes(context, Vec::new());
        assert!(weak.upgrade().is_none(), "existing release authority drops custody after confirmation");
        assert!(!GATES.contains_key(&context), "private scope needs no stale-binding tombstone");
        let captured = std::sync::Mutex::new(None);
        let mut cancelled = Box::pin(scoped_admitted(async {
            let lease = current().unwrap();
            let context = GATES.iter().find(|entry| std::sync::Arc::ptr_eq(entry.value(), &lease.cell)).map(|entry| *entry.key()).unwrap();
            *captured.lock().unwrap() = Some(context);
            std::future::pending::<()>().await;
        }));
        assert!(futures::poll!(&mut cancelled).is_pending());
        drop(cancelled);
        assert!(!GATES.contains_key(&captured.lock().unwrap().unwrap()), "cancellation before child spawn retires the private admission");
    }

    // what this catches (Codex root on #5026 at f014371be and 9986ab232): an owned process's
    // end that wasn't confirmed reached the release as nothing, so the release confirmed what
    // nothing had checked: a tree no walk read, and an unproven reap even with no survivors and
    // no walk error. Each case alone: a surviving descendant is re-checked, the others are held.
    #[tokio::test]
    async fn what_an_owned_process_left_unconfirmed_reaches_the_release() {
        use crate::system_resources::process_tree::{ProcessIdentity, TreeEnd};
        let descendant = ProcessIdentity { pid: 4242, start_time: 7 };
        let cases = [
            ("an unproven reap alone", TreeEnd { reaped: Err("could not confirm the process exited".into()), survivors: Vec::new(), unwalked: None }, 0, 1),
            ("an unwalked tree alone", TreeEnd { reaped: Ok(()), survivors: Vec::new(), unwalked: Some("its descendants could not be walked".into()) }, 0, 1),
            ("a surviving descendant alone", TreeEnd { reaped: Ok(()), survivors: vec![descendant], unwalked: None }, 1, 0),
            ("a confirmed end", TreeEnd { reaped: Ok(()), survivors: Vec::new(), unwalked: None }, 0, 0),
        ];
        for (case, end, processes, unknown) in cases {
            let context = HandsContext::fresh();
            let lease = admit(context).expect("open hands");
            lease.keep_unconfirmed(&end);
            drop(lease);
            let lingering = close(context).await;
            assert_eq!((lingering.processes.len(), lingering.unknown.len()), (processes, unknown), "{case}: {lingering:?}");
        }
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
