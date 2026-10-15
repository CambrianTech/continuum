//! lane_process.rs — the shared unix-process primitives the two lane-tracking
//! modules ([`crate::inference::lane_pidfile`] canonical-port reclaim and
//! [`crate::inference::lane_registry`] orphan accounting) both need.
//!
//! ## Why this is its own module
//!
//! Both modules answer a "is this recorded pid still one of MY llama-servers, and
//! if so may I reap it" question, and both must obey the SAME never-blind-kill
//! safety: verify the pid is actually a `llama-server` (via `ps`) before sending a
//! signal, because a recorded pid can be STALE (the process died and the OS reused
//! its number for something unrelated). Duplicating `is_alive` / `kill9` /
//! `command_name` across the two callers would be the exact compression violation
//! we forbid — one decision ("is this pid a live llama-server we own"), one place.
//!
//! No new dependency: `libc` is already in the tree; identity via `ps -p <pid> -o
//! comm=` works identically on macOS and Linux.

/// True if `pid` names a live process — the ONE liveness check (`deploy_claim::owner_alive`
/// and the grid's job table ask it too). The process table itself via `sysinfo`, on every
/// platform. The Windows arm used to scrape `tasklist` output for the pid as a SUBSTRING, so pid
/// 42 read alive whenever any pid containing "42" existed (the bug deploy_claim had already
/// fixed with exactly this lookup).

/// `SIGKILL` the pid. Best-effort: a race where it already exited is fine.
#[cfg(unix)]
pub fn kill9(pid: u32) {
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

/// Windows: force-terminate the process (and its child tree) via `taskkill`.
/// There is no SIGKILL; `/F /T` is the nearest equivalent. Best-effort.
#[cfg(windows)]
pub fn kill9(pid: u32) {
    let _ = airc_core::process::background("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
}

/// One process as the process table saw it: its pid and start time. An act on it later is
/// refused if the pid now names another process (pid reuse never kills a stranger).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
}

/// Every descendant of `pid` in the process table, DEEPEST FIRST, each with its identity: what
/// a tree kill must reach, read while `pid` is alive (once it dies its children are reparented
/// and the walk can no longer find them). A child that has double-forked away is not a
/// descendant and is not found; nothing claims it was.
pub fn descendants(pid: u32) -> Vec<ProcessIdentity> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut children: std::collections::HashMap<u32, Vec<ProcessIdentity>> = std::collections::HashMap::new();
    for (child, process) in sys.processes() {
        if let Some(parent) = process.parent() {
            children.entry(parent.as_u32()).or_default().push(ProcessIdentity { pid: child.as_u32(), start_time: process.start_time() });
        }
    }
    let mut found: Vec<(usize, ProcessIdentity)> = Vec::new();
    let mut frontier = vec![(pid, 0usize)];
    while let Some((at, depth)) = frontier.pop() {
        for &child in children.get(&at).map(Vec::as_slice).unwrap_or(&[]) {
            if found.iter().any(|(_, seen)| seen.pid == child.pid) {
                continue; // a table read mid-fork can list a pid twice; walk it once
            }
            found.push((depth + 1, child));
            frontier.push((child.pid, depth + 1));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, who)| who).collect()
}

/// The ones of `set` still running as the SAME process (same pid, same start time). A zombie
/// has exited and only awaits its reaper, so it is gone.
pub fn still_present(set: &[ProcessIdentity]) -> Vec<ProcessIdentity> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
    if set.is_empty() {
        return Vec::new();
    }
    let pids: Vec<Pid> = set.iter().map(|who| Pid::from_u32(who.pid)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), true, ProcessRefreshKind::nothing());
    set.iter()
        .copied()
        .filter(|who| {
            sys.process(Pid::from_u32(who.pid))
                .is_some_and(|p| p.start_time() == who.start_time && p.status() != ProcessStatus::Zombie)
        })
        .collect()
}

/// Kill `who` only while its pid still names that same process. True when the kill was sent.
pub fn kill_if_same(who: ProcessIdentity) -> bool {
    if still_present(&[who]).is_empty() {
        return false; // gone, or its pid now names another process: never signal a stranger
    }
    #[cfg(unix)]
    {
        // SAFETY: kill only sends a signal to a pid just verified to be `who`; no memory is touched
        unsafe { libc::kill(who.pid as libc::pid_t, libc::SIGKILL) == 0 }
    }
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/F", "/PID", &who.pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

/// How ending a child and everything it started came out (Astra on ea06701d and #5026): whether
/// the child's own reap was PROVEN, which of its descendants still run as the same process, and
/// why the tree could not be walked at all. Delivering a signal is not confirming an exit, so
/// nothing here is assumed.
#[derive(Debug)]
pub struct TreeEnd {
    pub reaped: Result<(), String>,
    pub survivors: Vec<ProcessIdentity>,
    /// Set when the walk for descendants itself failed: what the child started is UNKNOWN, so
    /// nothing may count it gone (a release holds the hands and names this).
    pub unwalked: Option<String>,
}

impl TreeEnd {
    /// Everything ended and confirmed: reaped, nothing surviving, nothing unknown.
    pub fn confirmed(&self) -> bool {
        self.reaped.is_ok() && self.survivors.is_empty() && self.unwalked.is_none()
    }

    /// What no re-check can confirm: an unproven reap of the child itself, and a tree no walk
    /// read. A holder of the end keeps these until its owner decides (Codex root on #5026).
    pub fn unknown(&self) -> impl Iterator<Item = String> + '_ {
        self.reaped.as_ref().err().cloned().into_iter().chain(self.unwalked.clone())
    }
}

/// End `child` and its tree: THE one way an owned child is ended (the shell's execution task and
/// `gh`'s owner). Its descendants are found while it is alive and still their parent, killed
/// deepest first (each only while its pid names the same process), then the child is killed and
/// reaped. SIGKILL can't be ignored, so the reap comes; no timeout is guessed. The process-table
/// walks and kills run off the async workers (they read the whole table, and on Windows each kill
/// runs `taskkill`).
pub async fn end_tree(child: &mut tokio::process::Child) -> TreeEnd {
    let walked = match child.id() {
        Some(pid) => tokio::task::spawn_blocking(move || {
            let tree = descendants(pid);
            for who in &tree {
                kill_if_same(*who);
            }
            tree
        })
        .await
        .map_err(|join| format!("its descendants could not be walked, so what it started is unknown: {join}")),
        None => Ok(Vec::new()),
    };
    let _ = child.start_kill(); // harmless when it already exited; the wait below is the proof
    let reaped = child.wait().await.map(|_| ()).map_err(|e| format!("could not confirm the process exited: {e}"));
    let (check, unwalked) = match walked {
        Ok(tree) => (tree, None),
        Err(why) => (Vec::new(), Some(why)),
    };
    let candidates = check.clone();
    // a re-check that didn't run confirms nothing: every candidate stays unconfirmed
    let survivors = tokio::task::spawn_blocking(move || still_present(&check)).await.unwrap_or(candidates);
    TreeEnd { reaped, survivors, unwalked }
}

/// How reading one of an owned child's streams ended. Kept apart from how the PROCESS ended:
/// an exit status says nothing about whether its output was read whole (Codex root on #5030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEnd {
    /// Read to EOF.
    Complete,
    /// The read itself failed; the prefix before the failure is kept.
    Failed(String),
    /// Reading was stopped (the bound, a cancel, an outliving holder); the prefix is kept.
    Cut(String),
}

/// One stream as read: the bytes and how reading ended.
#[derive(Debug, Clone)]
pub struct Captured {
    pub bytes: Vec<u8>,
    pub end: CaptureEnd,
}

/// One stream's capture as it is being read, shared with the operation that owns the run, so a
/// cut keeps its prefix reachable after the waiter is gone. The bytes stay in memory, as every
/// caller's did before; durable bytes are the tool-output spill's, and nothing here claims the
/// read is bounded.
#[derive(Debug, Default)]
pub struct StreamCapture {
    state: std::sync::Mutex<(Vec<u8>, Option<CaptureEnd>)>,
}

impl StreamCapture {
    fn append(&self, chunk: &[u8]) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).0.extend_from_slice(chunk); // unwrap_or_else: a poisoned capture still keeps its bytes
    }

    /// Set how reading ended, once: the first end wins (a reader's own EOF or error is not
    /// overwritten by a later cut).
    fn finish(&self, end: CaptureEnd) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: as above
        st.1.get_or_insert(end);
    }

    /// What has been read so far and how reading ended, or `Cut("still reading")` mid-read.
    pub fn snapshot(&self) -> Captured {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: as above
        Captured { bytes: st.0.clone(), end: st.1.clone().unwrap_or_else(|| CaptureEnd::Cut("still reading".into())) }
    }
}

/// An owned run's two streams, shared with the operation that owns it (the seam the root
/// result-custody owner holds; a caller with no such owner passes a fresh one).
#[derive(Debug, Default)]
pub struct RunCapture {
    pub stdout: StreamCapture,
    pub stderr: StreamCapture,
    /// How the run's input (stdin) was delivered, when it had one: `Complete` = written whole
    /// to the pipe and closed. It can't say the child READ it all: a child that reads part,
    /// discards what the pipe already holds and exits is indistinguishable from one that read
    /// everything, when the input fit the pipe's buffer.
    pub input: StreamCapture,
    /// Set once the child itself has exited and been reaped: from then on its tree can't be
    /// walked, and a pipe still open is held by something it started.
    exited: std::sync::atomic::AtomicBool,
}

impl RunCapture {
    /// Whether the child itself has exited and been reaped.
    pub fn exited(&self) -> bool {
        self.exited.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Read `pipe` into `into` until EOF or a read error, appending as it goes so a cut keeps the
/// prefix. The chunk size is the pipe read size, not a budget.
async fn read_stream(pipe: Option<impl tokio::io::AsyncRead + Unpin>, into: std::sync::Arc<RunCapture>, which: fn(&RunCapture) -> &StreamCapture) {
    use tokio::io::AsyncReadExt;
    let Some(mut pipe) = pipe else {
        which(&into).finish(CaptureEnd::Complete);
        return;
    };
    let mut chunk = vec![0u8; 64 * 1024]; // context-budget-exempt: a pipe read size, never a window or a cap on what is kept
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => return which(&into).finish(CaptureEnd::Complete),
            Ok(n) => which(&into).append(&chunk[..n]),
            Err(e) => return which(&into).finish(CaptureEnd::Failed(e.to_string())),
        }
    }
}

/// A child the core waits on, owned until its whole lifecycle has ended: the child and the tree
/// it started, AND the reading of its output (Astra on #5026, Codex root on #5030). Ending it is
/// ONE path for a timeout and a dropped waiter (a cancelled command, a rebooting core) alike. It
/// keeps the admission of the work that started it (`code::hands::current`), so a release never
/// completes under it, and what it can't confirm is kept on that admission.
/// One pipe task a run owns: a reader of its stdout or stderr, or the writer of its stdin.
/// Every one has the SAME lifecycle (Codex root on #5035: the ownership defect repeated across
/// stream directions because the lifecycle was copied per direction): it records its own end on
/// its stream, it is awaited in place, a JoinError is a failed stream, and an unfinished one is
/// stopped and cut, never detached.
struct PipeTask {
    task: tokio::task::JoinHandle<()>,
    stream: fn(&RunCapture) -> &StreamCapture,
    /// What a holder of this pipe holds, in the words a fence names it with.
    name: &'static str,
}

/// A child the core waits on, owned until its whole lifecycle has ended: the child and the tree
/// it started, AND its pipes (Astra on #5026, Codex root on #5030 and #5035). Ending it is ONE
/// path for a timeout and a dropped waiter (a cancelled command, a rebooting core) alike. It
/// keeps the admission of the work that started it (`code::hands::current`), so a release never
/// completes under it, and what it can't confirm is kept on that admission.
struct OwnedChild {
    child: Option<tokio::process::Child>,
    pipes: Vec<PipeTask>,
    capture: std::sync::Arc<RunCapture>,
    held: Option<std::sync::Arc<crate::code::hands::HandsLease>>,
    /// What its caller holds for it ([`holding`]), kept until its tree has ended.
    kept: Vec<Kept>,
    label: &'static str,
}

/// A value an owned child keeps until its tree has ended (see [`holding`]).
pub type Kept = std::sync::Arc<dyn std::any::Any + Send + Sync>;

tokio::task_local! {
    /// What every owned child started on this task keeps until its tree has ended.
    static HOLDING: Vec<Kept>;
}

/// Run `work` with every owned child it starts keeping `value` until that child's tree has
/// ended, not merely until `work`'s future does. A cancel drops the future at once while the
/// tree's end runs after it ([`OwnedChild`]'s drop), so an exclusion held only by the future
/// (a checkout's sync lock) would admit the next holder beside a git still being ended
/// (Codex reviewer on #5037). Nested scopes add to what the outer ones hold. It needs an
/// admission ([`crate::code::hands::admitted`]): what an unconfirmed end keeps goes onto it.
pub async fn holding<F: std::future::Future>(value: Kept, work: F) -> F::Output {
    let mut all = HOLDING.try_with(Vec::clone).unwrap_or_default(); // unwrap_or_default: outside any scope nothing else is held
    all.push(value);
    HOLDING.scope(all, work).await
}

impl OwnedChild {
    /// Await every pipe to its end, IN PLACE: they stay owned here, so a cancel or panic
    /// mid-await finds them in `pipes` and [`Drop`] stops them. A pipe task that panicked or
    /// was aborted reads as a failed stream, never as whole.
    async fn settle_pipes(&mut self) {
        for pipe in self.pipes.iter_mut() {
            if let Err(join) = (&mut pipe.task).await {
                (pipe.stream)(&self.capture).finish(CaptureEnd::Failed(format!("its {} task did not finish: {join}", pipe.name)));
            }
        }
        self.pipes.clear(); // all settled: nothing left to stop
    }

    /// Stop every pipe that is still open, keeping what it read or wrote: each is marked cut with
    /// `why`. Returns the names of the pipes that were still open.
    fn cut_pipes(&mut self, why: &str) -> Vec<&'static str> {
        let mut open = Vec::new();
        for pipe in self.pipes.drain(..) {
            if !pipe.task.is_finished() {
                open.push(pipe.name);
            }
            pipe.task.abort();
            (pipe.stream)(&self.capture).finish(CaptureEnd::Cut(why.to_string())); // a pipe that already ended keeps its own end
        }
        open
    }

    /// After the child itself exited, a pipe still open is held by something it started, which
    /// the tree walk can no longer find: it is kept on the admission as unidentified, never
    /// claimed ended, and the release holds for it.
    fn fence_holders(&self, open: &[&'static str], when: &str) {
        if self.child.is_some() || open.is_empty() {
            return; // the child's own end walks its tree
        }
        let why = format!("{}: {when}, a process it started still held its {} (holders unidentified, not claimed ended)", self.label, open.join(", "));
        crate::probe!(class = "process.pipe_held", process = self.label, pipes = ?open, "an owned process exited but a process it started still held its pipes; the hands' release holds");
        if let Some(lease) = &self.held {
            lease.keep_unknown(why);
        }
        keep_with_lingering(self.held.as_ref(), &self.kept);
    }

    /// Hand the child to its end: a spawned task, so the caller's cancellation can't cut it
    /// short. `None` once it exited and was reaped, or with no runtime to run the end on.
    fn end(&mut self) -> Option<tokio::task::JoinHandle<TreeEnd>> {
        let child = self.child.take()?;
        let runtime = tokio::runtime::Handle::try_current().ok()?;
        Some(runtime.spawn(end_owned(child, self.held.clone(), self.kept.clone(), self.label)))
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // its waiter is gone mid-run (cancel, panic): the end still runs, holding its admission;
        // the pipes stop and what they carried stays on the shared capture
        let open = self.cut_pipes("the operation was cancelled");
        self.fence_holders(&open, "cancelled");
        let _ = self.end();
    }
}

/// End an owned child and its tree ([`end_tree`]) while holding the admission it was started
/// under. What can't be confirmed ended is kept ON the admission before it drops, so the
/// release that waits it out re-checks those processes and holds the hands for the rest.
async fn end_owned(mut child: tokio::process::Child, held: Option<std::sync::Arc<crate::code::hands::HandsLease>>, kept: Vec<Kept>, label: &'static str) -> TreeEnd {
    let end = end_tree(&mut child).await;
    if !end.confirmed() {
        crate::probe!(
            class = "process.unconfirmed_end",
            process = label,
            survivors = ?end.survivors.iter().map(|who| who.pid).collect::<Vec<_>>(),
            reaped = ?end.reaped,
            unwalked = ?end.unwalked,
            "an owned process was ended but not everything is confirmed ended; its hands' release re-checks or holds"
        );
    }
    if let Some(lease) = &held {
        lease.keep_unconfirmed(&end);
    }
    if !end.confirmed() {
        keep_with_lingering(held.as_ref(), &kept);
    }
    drop(kept); // confirmed: what its caller held for it is released now that the tree has ended
    end
}

/// What a child's caller held for it ([`holding`]) goes where what it left unconfirmed goes: onto
/// its admission's lingering record, released only when a re-check finds nothing left (Codex
/// reviewer on #5037). `kept` is non-empty only under an admission: [`run_owned_with_input`]
/// refuses `holding` without one before anything starts.
fn keep_with_lingering(held: Option<&std::sync::Arc<crate::code::hands::HandsLease>>, kept: &[Kept]) {
    if let Some(lease) = held {
        lease.keep_held(kept);
    }
}

/// What an owned run left: each stream as read, and how the process ended.
#[derive(Debug)]
pub struct OwnedOutput {
    pub stdout: Captured,
    pub stderr: Captured,
    /// How its input was delivered, when it had one (see [`RunCapture::input`]).
    pub input: Option<CaptureEnd>,
    pub ended: Ended,
}

/// A run's answer, read ONE way by every consumer whose result is its stdout (an authored
/// command, a git): see [`OwnedOutput::answer`].
#[derive(Debug)]
pub enum Answer<'a> {
    /// It exited 0 with its input delivered and its output read whole: `stdout` is the answer.
    Whole(&'a Captured),
    /// Not an answer, carried with everything the run left (Codex root on #5038): render it
    /// with `Display`, which says why, the note, and both streams as read.
    Not(Unanswered<'a>),
}

/// Why a run is not an answer.
#[derive(Debug)]
pub enum Why<'a> {
    /// It exited non-zero: ITS failure, outranking any delivery note.
    Exited(std::process::ExitStatus),
    /// It exited 0, but its input wasn't delivered or its output isn't whole.
    NotWhole,
    /// It outlived its bound and was ended with what it started; these pids are not confirmed gone.
    TimedOut { unconfirmed: &'a [u32] },
}

/// A run that is not an answer, with every dimension it left: why, what of its input or
/// output wasn't whole (`note`, beside the why and never in its place), and both streams.
#[derive(Debug)]
pub struct Unanswered<'a> {
    pub why: Why<'a>,
    pub note: Option<String>,
    pub stdout: &'a Captured,
    pub stderr: &'a Captured,
}

impl std::fmt::Display for Unanswered<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.why {
            // a signal has no exit code: say the status itself
            Why::Exited(status) => match status.code() {
                Some(code) => write!(f, "exit {code}")?,
                None => write!(f, "{status}")?,
            },
            Why::NotWhole => write!(f, "exited 0, but it is not an answer")?,
            Why::TimedOut { unconfirmed } if unconfirmed.is_empty() => write!(f, "timed out")?,
            Why::TimedOut { unconfirmed } => write!(f, "timed out; not confirmed gone: {unconfirmed:?}")?,
        }
        if let Some(note) = &self.note {
            write!(f, " ({note})")?;
        }
        // both streams whole as read: what to show is the budgeter's call, never a fixed cut
        // (Joel, 2026-10-11)
        for (name, read) in [("stderr", self.stderr), ("stdout", self.stdout)] {
            let text = String::from_utf8_lossy(&read.bytes);
            if !text.trim().is_empty() {
                write!(f, "\n{name}:\n{}", text.trim_end())?;
            }
        }
        Ok(())
    }
}

impl OwnedOutput {
    /// The run's answer. A non-zero exit is the child's own verdict and outranks a delivery
    /// note: a child may legitimately exit before reading its input, and whether the write lands
    /// first is a race (canary run 38541241843, Codex root on #5035). A zero exit is an answer
    /// only when its input was delivered whole and its output read whole. Every other shape keeps
    /// its note and both streams, a timeout included.
    pub fn answer(&self) -> Answer<'_> {
        let notes: Vec<String> = [self.input_note(), self.capture_note()].into_iter().flatten().collect();
        let note = (!notes.is_empty()).then(|| notes.join("; "));
        let why = match self.ended {
            Ended::TimedOut { ref unconfirmed } => Why::TimedOut { unconfirmed },
            Ended::Exited(status) if !status.success() => Why::Exited(status),
            Ended::Exited(_) if note.is_some() => Why::NotWhole,
            Ended::Exited(_) => return Answer::Whole(&self.stdout),
        };
        Answer::Not(Unanswered { why, note, stdout: &self.stdout, stderr: &self.stderr })
    }

    /// Why its input wasn't delivered whole, when it wasn't: read through [`Self::answer`], where
    /// it decides a zero exit and sits beside a non-zero one.
    fn input_note(&self) -> Option<String> {
        match self.input.as_ref()? {
            CaptureEnd::Complete => None,
            CaptureEnd::Failed(why) => Some(format!("its input was not delivered ({why})")),
            CaptureEnd::Cut(why) => Some(format!("its input was cut ({why})")),
        }
    }

    /// Why its output isn't whole, when it isn't: the note a caller surfaces beside the
    /// process's own verdict, never in place of it.
    pub fn capture_note(&self) -> Option<String> {
        let note = |name: &str, end: &CaptureEnd| match end {
            CaptureEnd::Complete => None,
            CaptureEnd::Failed(why) => Some(format!("{name} could not be read whole ({why})")),
            CaptureEnd::Cut(why) => Some(format!("{name} was cut ({why})")),
        };
        let notes: Vec<String> = [note("stdout", &self.stdout.end), note("stderr", &self.stderr.end)].into_iter().flatten().collect();
        (!notes.is_empty()).then(|| notes.join("; "))
    }
}

/// How an owned run's process ended.
#[derive(Debug)]
pub enum Ended {
    /// It exited and was reaped. Its output may still be cut: a process it started can hold
    /// the pipes past its exit (see [`run_owned`]).
    Exited(std::process::ExitStatus),
    /// It outlived its bound and was ended with everything it started. These pids are NOT
    /// confirmed gone (the child itself among them when its own end is unproven).
    TimedOut { unconfirmed: Vec<u32> },
}

/// THE one way the core runs a child it waits on (`gh`, `cargo`, a snippet, a grader's test
/// run, an external agent). Spawned in the core's process group, so a stop ends it too. Its
/// lifecycle is the child's exit AND both pipes reaching EOF, under ONE `bound`, and it stays
/// owned until that lifecycle ends:
/// - past the bound before the child exits, the child and its tree are ended through
///   [`end_tree`], and the pids it couldn't confirm gone are named;
/// - when the child exits but a process it started still holds its output at the bound, that
///   holder can't be found (the walk lost it when the child died) and isn't claimed ended: the
///   reading is cut and the uncertainty is kept on the hands' admission, which holds the
///   release (Codex root on #5030);
/// - a dropped waiter ends it the same way, and what it read stays on `capture`, the
///   operation's.
pub async fn run_owned(
    cmd: tokio::process::Command,
    bound: Option<std::time::Duration>,
    label: &'static str,
    capture: std::sync::Arc<RunCapture>,
) -> std::io::Result<OwnedOutput> {
    run_owned_with_input(cmd, None, bound, label, capture).await
}

/// [`run_owned`] with `input` written to the child's stdin, which is then closed: an authored
/// command's params, a patch for `git apply`. The write runs BESIDE the wait, so a child that
/// never reads its stdin can't wedge the run past its bound; the writer is owned and stopped
/// with the run.
pub async fn run_owned_with_input(
    mut cmd: tokio::process::Command,
    input: Option<Vec<u8>>,
    bound: Option<std::time::Duration>,
    label: &'static str,
    capture: std::sync::Arc<RunCapture>,
) -> std::io::Result<OwnedOutput> {
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let had_input = input.is_some();
    if input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    let deadline = bound.map(|b| tokio::time::Instant::now() + b);
    let kept = HOLDING.try_with(Vec::clone).unwrap_or_default(); // unwrap_or_default: started outside any `holding` scope, it keeps nothing extra
    let held = crate::code::hands::current();
    if !kept.is_empty() && held.is_none() {
        // what a caller holds for a child is released when the child's end is confirmed, or by
        // the admission's re-check when it isn't; with no admission nothing could release it
        // after an unconfirmed end, so it is refused before anything starts (Codex reviewer on #5037)
        return Err(std::io::Error::other(format!("{label}: `holding` needs an admission to keep what it holds past an unconfirmed end; none is current")));
    }
    // This owner captures output: it is a background run, not an interactive
    // terminal. Reuse the substrate's platform policy after admission validation;
    // custody, cancellation and child-tree teardown remain with this owner.
    airc_core::process::configure_background(cmd.as_std_mut());
    let mut child = cmd.spawn()?;
    let (out_pipe, err_pipe) = (child.stdout.take(), child.stderr.take());
    let writer = match (input, child.stdin.take()) {
        (Some(bytes), Some(mut stdin)) => {
            let into = capture.clone();
            Some(tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                // a child that closes its stdin before taking the input ends the write with an
                // error (a broken pipe): recorded, never read as delivered
                let written = async {
                    stdin.write_all(&bytes).await?;
                    stdin.shutdown().await
                }
                .await;
                into.input.finish(match written {
                    Ok(()) => CaptureEnd::Complete,
                    Err(e) => CaptureEnd::Failed(e.to_string()),
                });
            }))
        }
        (Some(_), None) => {
            capture.input.finish(CaptureEnd::Failed("the child's stdin could not be opened".into()));
            None
        }
        _ => None,
    };
    let mut pipes = vec![
        PipeTask { task: tokio::spawn(read_stream(out_pipe, capture.clone(), |c| &c.stdout)), stream: |c| &c.stdout, name: "stdout" },
        PipeTask { task: tokio::spawn(read_stream(err_pipe, capture.clone(), |c| &c.stderr)), stream: |c| &c.stderr, name: "stderr" },
    ];
    pipes.extend(writer.map(|task| PipeTask { task, stream: |c| &c.input, name: "stdin" }));
    let mut owned = OwnedChild {
        pipes,
        child: Some(child),
        capture: capture.clone(),
        held,
        kept,
        label,
    };
    let Some(child) = owned.child.as_mut() else {
        return Err(std::io::Error::other("the spawned process vanished before it was owned"));
    };
    let waited = match deadline {
        Some(at) => tokio::time::timeout_at(at, child.wait()).await,
        None => Ok(child.wait().await),
    };
    let ended = match waited {
        Ok(status) => {
            let status = status?; // a wait that failed leaves the child to the owner's end
            owned.child = None; // exited and reaped by wait()
            capture.exited.store(true, std::sync::atomic::Ordering::Release);
            // the lifecycle isn't over until every pipe is settled: a process it started can hold
            // its output open, or its stdin unread
            let in_time = match deadline {
                Some(at) => tokio::time::timeout_at(at, owned.settle_pipes()).await.is_ok(),
                None => {
                    owned.settle_pipes().await;
                    true
                }
            };
            if !in_time {
                // still owned: the timed-out settle left them in place
                let open = owned.cut_pipes("a process it started held it past the bound (holders unidentified, not claimed ended)");
                owned.fence_holders(&open, "past the bound");
            }
            Ended::Exited(status)
        }
        Err(_elapsed) => {
            let pid = child.id();
            owned.cut_pipes("the bound elapsed"); // the child's own end walks its tree
            let end = owned
                .end()
                .ok_or_else(|| std::io::Error::other("no runtime to end the timed-out process on"))? // never inside the core's async code
                .await
                .map_err(|join| std::io::Error::other(format!("ending the timed-out process failed to run: {join}")))?;
            // the child itself counts as unconfirmed when its reap or its tree couldn't be confirmed
            let unconfirmed: Vec<u32> = end.survivors.iter().map(|who| who.pid).chain(end.unknown().next().and(pid)).collect();
            Ended::TimedOut { unconfirmed }
        }
    };
    drop(owned); // the lifecycle is over: the admission is released here, not before
    let input = had_input.then(|| capture.input.snapshot().end);
    Ok(OwnedOutput { stdout: capture.stdout.snapshot(), stderr: capture.stderr.snapshot(), input, ended })
}

/// Kill each of `survivors` again while its pid still names it, and return the ones still
/// present: one recheck, so a later one (a release retried) can confirm what this couldn't.
pub fn recheck(survivors: &[ProcessIdentity]) -> Vec<ProcessIdentity> {
    for who in survivors {
        kill_if_same(*who);
    }
    still_present(survivors)
}

/// The command name (basename) of `pid` via `ps -p <pid> -o comm=`. `None` if the
/// pid is gone or `ps` is unavailable — callers treat `None` as "unverifiable" and
/// refuse to kill.
#[cfg(not(windows))]
pub fn command_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim();
    if line.is_empty() {
        return None;
    }
    // `comm` can be a full path on Linux — take the basename so the
    // "llama-server" match is on the binary name, not its install dir.
    Some(line.rsplit('/').next().unwrap_or(line).to_string())
}

/// Windows has no `ps`. Without this arm `command_name` returned `None` for EVERY
/// pid on Windows, which the doctrine above turns into "unverifiable — refuse to
/// kill". Combined with the same gap in [`pid_listening_on_port`], the whole
/// reap/verify subsystem was inert on Windows: the core could not identify even
/// its OWN llama-server child, so any unclean teardown left the port held by a
/// holder it could not name and serving stayed wedged permanently. Measured on
/// BigMama 2026-09-05 (four spawn/refuse cycles, `held by None` while netstat
/// showed the pid plainly). [[silently-unwired-capability]]
#[cfg(windows)]
pub fn command_name(pid: u32) -> Option<String> {
    let out = airc_core::process::background("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_tasklist_image_name(&String::from_utf8_lossy(&out.stdout))
}

/// Parse `tasklist /NH /FO CSV` output to the image name. Split out as a pure
/// function so the parse is testable on any host — the shell-out is not.
///
/// A miss must be `None`, never a guess: tasklist answers a NO-MATCH filter with
/// the prose line `INFO: No tasks are running which match the specified criteria.`
/// on stdout AND exit status 0, so "success" does not imply "found".
#[cfg(any(windows, test))]
pub(crate) fn parse_tasklist_image_name(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| l.trim_start().starts_with('"'))?;
    let name = line.trim().trim_start_matches('"');
    let name = name.split('"').next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Positively identify `pid` as one of OUR llama-server children — the guard every
/// reap decision runs before signalling. `false` for a dead pid, a reused pid now
/// naming an unrelated process, or an unverifiable one (no `ps`). Never a guess:
/// only a confirmed `llama-server` `comm` returns `true`.
pub fn is_llama_server(pid: u32) -> bool {
    command_name(pid).is_some_and(|comm| comm.contains("llama-server"))
}

/// The pid LISTENING on local TCP `port`, via `lsof -ti tcp:<port> -sTCP:LISTEN`
/// (macOS + Linux). `None` = nothing listening, or `lsof` unavailable — callers
/// treat `None` as "unverifiable" and refuse to kill, same doctrine as
/// [`command_name`]. This is the kill-VERIFY half of the 2026-07-23 flap case:
/// a spawn must never race a port whose holder it can't name.
#[cfg(not(windows))]
pub fn pid_listening_on_port(port: u16) -> Option<u32> {
    let out = std::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u32>().ok())
}

/// Windows equivalent via `netstat -ano`. See [`command_name`]'s windows arm for
/// what the absence of this cost: `None` here is indistinguishable from "nothing
/// is listening", so the pre-spawn gate refused forever against a port whose
/// holder it was simply unable to look up.
#[cfg(windows)]
pub fn pid_listening_on_port(port: u16) -> Option<u32> {
    let out = airc_core::process::background("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_netstat_listening_pid(&String::from_utf8_lossy(&out.stdout), port)
}

/// Parse `netstat -ano -p TCP` for the pid LISTENING on `port`. Pure so the parse
/// is testable off-Windows.
///
/// Matching is on the local address's port SUFFIX after the final ':' — comparing
/// the whole line with `contains(":58057")` would also match a foreign address or
/// a longer port that merely ends in those digits.
#[cfg(any(windows, test))]
pub(crate) fn parse_netstat_listening_pid(stdout: &str, port: u16) -> Option<u32> {
    for line in stdout.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        // proto local foreign state pid
        if f.len() < 5 || !f[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some((_, p)) = f[1].rsplit_once(':') else {
            continue;
        };
        if p.parse::<u16>() == Ok(port) {
            return f[4].parse::<u32>().ok();
        }
    }
    None
}

/// Poll until local `port` is bindable (a successful bind-then-drop proves it) or
/// `budget` expires. THE shared port-release verifier: the pidfile reclaim and the
/// pre-spawn kill-verify gate both wait through this one primitive.
pub async fn wait_port_free(port: u16, budget: std::time::Duration) -> bool {
    const POLL: std::time::Duration = std::time::Duration::from_millis(100);
    let deadline = std::time::Instant::now() + budget;
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Every llama-server pid on this box, by name — the boot plan's census.
/// `pgrep -x` (exact name) so a path containing "llama-server" in an editor
/// or grep never matches; each pid is then identity-verified by callers via
/// [`is_llama_server`] before any signal (never-blind-kill).
pub fn owned_llama_pids() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep")
        .args(["-x", "llama-server"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// How many times the boot path asks a lane's `/health` before judging it, and how long
/// each ask may take. Sized for a LOADED box, not an idle one: on 2026-09-20 the M5's
/// deploy ran this check against a 32 GB lane 6,460 tokens into a turn while the box sat
/// at 0 GB free and 4.9 GB swap — the lane's HTTP thread answered late, a single 3 s
/// probe read that as dead, and the lane was killed mid-generation twice in one morning
/// (card b57b19fd; Joel: "timeouts are where AIs ruin a project"). Three asks over up to
/// fifteen seconds is the price of never killing a working lane for being slow.
pub const LANE_PROBE_ATTEMPTS: u32 = 3;
pub const LANE_PROBE_BOUND_S: u64 = 5;
/// The CPU share above which a lane that missed every probe is BUSY, not dead: a decoding
/// llama-server burns a core sampling and tokenizing; a wedged one is frozen at zero.
/// Known blind spot (Cormac, #4252): a lane still LOADING its weights answers `/health`
/// 503 (`non-2xx`) and can sit near 0% CPU in I/O wait — it reads Dead here exactly as
/// it did under the single-probe check (no regression), but its evidence line will say
/// "frozen" for a process that was merely mmap-reading; a boot landing seconds after a
/// relaunch is the case. If that ever shows in a receipt, the third signal is the
/// process's RSS / I/O advancing, not CPU — do not chase it as a wedge.
pub const LANE_BUSY_CPU_PCT: f32 = 1.0;

/// What the boot path decided about a lane it did not spawn, with the evidence it
/// decided on — every fate is a receipt, never a bare bool.
#[derive(Debug, Clone, PartialEq)]
pub enum LaneVerdict {
    /// `/health` answered 2xx within the bound (attempt number carried).
    Healthy { attempt: u32 },
    /// Every probe missed, but the process is working (CPU advancing) — a slow answer
    /// under load, not a wedge. ADOPTED: the core's own readiness check proves decode
    /// before any citizen is seated, and relaunches it if that fails.
    Busy { cpu_pct: f32, last_error: String },
    /// Every probe missed AND the process is frozen (or is not a lane at all). Reaped.
    Dead { reason: String },
}

impl LaneVerdict {
    /// A lane the boot keeps (warm weights, live KV) rather than reaps.
    pub fn adopt(&self) -> bool {
        !matches!(self, LaneVerdict::Dead { .. })
    }
    /// One line for the boot receipt / the probe stream.
    pub fn evidence(&self) -> String {
        match self {
            LaneVerdict::Healthy { attempt } => format!("healthy (answered on attempt {attempt})"),
            LaneVerdict::Busy { cpu_pct, last_error } => format!(
                "busy (every probe missed within {LANE_PROBE_ATTEMPTS} × {LANE_PROBE_BOUND_S} s: \
                 {last_error}; cpu {cpu_pct:.0}% — decoding, adopted)"
            ),
            LaneVerdict::Dead { reason } => format!("dead ({reason})"),
        }
    }
}

/// PURE: the verdict from the evidence. Two signals, never one: the probe AND the
/// process's own activity. A miss with activity is a slow answer under load (BUSY); a
/// miss without activity is a wedge (DEAD); `None` activity (the process could not be
/// sampled) counts as frozen — an absence never exonerates a lane, and this is the
/// deterministic direction the old single-probe check already took.
pub fn verdict_from(answered_on: Option<u32>, cpu_pct: Option<f32>, last_error: &str) -> LaneVerdict {
    if let Some(attempt) = answered_on {
        return LaneVerdict::Healthy { attempt };
    }
    match cpu_pct {
        Some(pct) if pct >= LANE_BUSY_CPU_PCT => LaneVerdict::Busy {
            cpu_pct: pct,
            last_error: last_error.to_string(),
        },
        Some(pct) => LaneVerdict::Dead {
            reason: format!("every probe missed ({last_error}) and the process is frozen (cpu {pct:.1}%)"),
        },
        None => LaneVerdict::Dead {
            reason: format!("every probe missed ({last_error}) and the process could not be sampled"),
        },
    }
}

/// One bounded `/health` ask on `port`: `Ok(())` on 2xx, else the reason.
fn probe_health_once(port: u16, bound_s: u64) -> Result<(), String> {
    let out = std::process::Command::new("curl")
        .args([
            "-sf",
            "--max-time",
            &bound_s.to_string(),
            &format!("http://127.0.0.1:{port}/health"),
        ])
        .output()
        .map_err(|e| format!("curl unavailable: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(match out.status.code() {
            Some(28) => format!("no answer within {bound_s} s"),
            Some(22) => "non-2xx".to_string(),
            Some(7) => "connection refused".to_string(),
            Some(c) => format!("curl exit {c}"),
            None => "curl killed".to_string(),
        })
    }
}

/// Ask `/health` up to `attempts` times, `bound_s` each. Returns the attempt that answered,
/// else the last reason. Pure over the port so a test can stand a slow listener behind it.
pub fn probe_health_bounded(port: u16, attempts: u32, bound_s: u64) -> Result<u32, String> {
    let mut last = String::from("no attempt");
    for attempt in 1..=attempts {
        match probe_health_once(port, bound_s) {
            Ok(()) => return Ok(attempt),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The process's CPU share over a short window (sysinfo, single-pid refresh twice across
/// its minimum interval). `None` if the pid is gone or unsampled.
pub fn cpu_pct_of(pid: u32) -> Option<f32> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let wanted = [Pid::from_u32(pid)];
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_cpu();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&wanted), true, kind);
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(500)));
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&wanted), true, kind);
    sys.process(wanted[0]).map(|p| p.cpu_usage())
}

/// The boot path's ONE verdict on a lane it did not spawn — identity-verified, then the
/// two signals: a bounded, retried `/health` and the process's own activity. Boot-path
/// only (blocking, up to `LANE_PROBE_ATTEMPTS × LANE_PROBE_BOUND_S` seconds), never on a
/// serving path. Every path that may kill a lane at boot calls THIS; there is no second
/// criterion in a shell script any more (the bash loop this superseded reaped the M5's
/// lane twice on 2026-09-20).
pub fn lane_verdict(pid: u32) -> LaneVerdict {
    if !is_llama_server(pid) {
        return LaneVerdict::Dead {
            reason: "not a llama-server (dead pid, or a reused one)".to_string(),
        };
    }
    let Some(argv) = command_args(pid) else {
        return LaneVerdict::Dead { reason: "argv unreadable".to_string() };
    };
    let port = {
        let mut it = argv.iter();
        let mut found = None;
        while let Some(a) = it.next() {
            if a == "--port" {
                found = it.next().and_then(|p| p.parse::<u16>().ok());
                break;
            }
            if let Some(v) = a.strip_prefix("--port=") {
                found = v.parse::<u16>().ok();
                break;
            }
        }
        found
    };
    let Some(port) = port else {
        return LaneVerdict::Dead { reason: "no --port on its command line".to_string() };
    };
    match probe_health_bounded(port, LANE_PROBE_ATTEMPTS, LANE_PROBE_BOUND_S) {
        Ok(attempt) => verdict_from(Some(attempt), None, ""),
        Err(last) => verdict_from(None, cpu_pct_of(pid), &last),
    }
}

/// Reap one lane the boot plan judged unhealthy — identity re-verified at the
/// signal (the pid may have died between census and kill).
pub fn kill_lane(pid: u32) {
    if is_llama_server(pid) {
        kill9(pid);
    }
}

/// The command line of `pid` (argv, one string per element) — the process's own truth
/// about what it was launched with, cross-platform via `sysinfo` (a SINGLE-pid refresh:
/// one `KERN_PROCARGS2` / `/proc/<pid>/cmdline` read, never the whole table). `None` if
/// the pid is gone or its argv is unreadable — an absence, never an empty launch.
pub fn command_args(pid: u32) -> Option<Vec<String>> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let wanted = [Pid::from_u32(pid)];
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&wanted),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let argv: Vec<String> = sys
        .process(wanted[0])?
        .cmd()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

/// PURE: the `--cache-ram <MiB>` a llama-server was launched with, read off its argv
/// (`--cache-ram 14396` or `--cache-ram=14396`). This is how a core recovers the grant of
/// a lane it did not spawn — a past form of ourself left up across a deploy and ADOPTED
/// — the same way it reads that lane's window and slots off `/props`: the process's
/// truth, never a re-derivation. `None` when the flag is absent or malformed — unknown,
/// never 0 (0 is a real llama-server value: no host cache).
pub fn cache_ram_mib_in(argv: &[String]) -> Option<u32> {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a == "--cache-ram" {
            return it.next()?.parse().ok();
        }
        if let Some(v) = a.strip_prefix("--cache-ram=") {
            return v.parse().ok();
        }
    }
    None
}

/// The KV cache decision a running llama-server was LAUNCHED with, read off its argv —
/// the process's truth about the geometry the plan cannot change without a relaunch.
/// `cache_type` is `--cache-type-k <t>` (or `=`); absent = the engine's default (f16).
/// `flash_attn` is `--flash-attn on` (a bare `--flash-attn` or `on`/`auto`; `off` is off).
/// Card 977842fd: the KV cache type is applied at SPAWN, an engine now survives a deploy
/// (#4284), and the adopt rail compared model + window + lanes + sight — so a new
/// decision (q8_0 on the 5090) never reached the f16 engine the new core adopted.
/// `cache_ram_mib` is its `--cache-ram` grant ([`cache_ram_mib_in`]), the host-RAM store of
/// saved KV states: fixed at spawn like the type, so the adopt rail must compare it too
/// (card d7294f0b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchKv {
    pub cache_type: Option<String>,
    pub flash_attn: bool,
    pub cache_ram_mib: Option<u32>,
}

impl LaunchKv {
    /// f16 is the engine's default, so "no flag" and "f16" are the same launch.
    pub fn cache_type_or_default(&self) -> &str {
        self.cache_type.as_deref().unwrap_or(crate::cognition::kv_cache_plan::F16) // unwrap_or: no flag = the engine's f16 default, a value not an absence
    }
}

/// PURE: the KV decision in a llama-server argv (see [`LaunchKv`]).
pub fn launch_kv_in(argv: &[String]) -> LaunchKv {
    let mut cache_type = None;
    let mut flash_attn = false;
    let mut it = argv.iter().peekable();
    while let Some(a) = it.next() {
        if a == "--cache-type-k" || a == "-ctk" {
            cache_type = it.next().cloned();
        } else if let Some(v) = a.strip_prefix("--cache-type-k=") {
            cache_type = Some(v.to_string());
        } else if a == "--flash-attn" || a == "-fa" {
            flash_attn = match it.peek().map(|s| s.as_str()) {
                Some("off") => {
                    it.next();
                    false
                }
                Some("on") | Some("auto") => {
                    it.next();
                    true
                }
                _ => true,
            };
        } else if let Some(v) = a.strip_prefix("--flash-attn=") {
            flash_attn = v != "off";
        }
    }
    LaunchKv { cache_type, flash_attn, cache_ram_mib: cache_ram_mib_in(argv) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (e92f227c): captured background commands allocating or
    // inheriting a console. The observation is made by the actual native child.
    #[cfg(windows)]
    #[test]
    #[ignore = "native observer invoked by owned_background_child_has_no_console"]
    fn background_console_observer() {
        // SAFETY: this API only observes the calling process's console handle.
        assert!(unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null());
        println!("background-console-observer-ok");
        eprintln!("background-console-stderr-ok");
    }

    // what this catches (e92f227c): every raw caller inherits windowless policy
    // at the shared spawn boundary, while exit and both captured streams survive.
    #[cfg(windows)]
    #[tokio::test]
    async fn owned_background_child_has_no_console() {
        let mut command = tokio::process::Command::new(std::env::current_exe().expect("test binary"));
        command.args(["--exact", "inference::lane_process::tests::background_console_observer", "--ignored", "--nocapture"]);
        let output = run_owned(command, None, "console-observer", std::sync::Arc::default()).await.expect("owned native child");
        assert!(matches!(&output.ended, Ended::Exited(status) if status.success()), "native child must exit successfully: {:?}", output.ended);
        assert!(String::from_utf8_lossy(&output.stdout.bytes).contains("background-console-observer-ok"));
        assert!(String::from_utf8_lossy(&output.stderr.bytes).contains("background-console-stderr-ok"));
        assert!(output.input_note().is_none());
        assert!(output.capture_note().is_none());
    }

    // what this catches (Astra and Codex root on #5026 and #5030, card de211a71): a child whose
    // waiter was cancelled (a dropped command, a rebooting core) kept running, and so did what
    // it started, while a release removed the workspace under them; and a grader that outlived
    // its ceiling had only its group killed after the fact. An owned run ends the child AND its
    // tree on either path, holds the admission until that end is done, and keeps what it read
    // reachable on the operation's capture even after the waiter is gone.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_owned_run_ends_its_tree_when_dropped_or_timed_out_and_holds_the_admission_until_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let (shell_pid, grandchild_pid) = (dir.path().join("sh.pid"), dir.path().join("gc.pid"));
        let script = format!("echo started; echo $$ > {}; sleep 600 & echo $! > {}; wait", shell_pid.display(), grandchild_pid.display());
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&script);
        // the strict order (Codex reviewer on #5037): what the caller held for the run (a
        // checkout's sync lock) is released only after the tree's end, so at the instant it
        // drops, the grandchild is already gone. The witness reads that AT its release.
        struct Witness {
            grandchild: std::path::PathBuf,
            gone_at_release: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
        }
        impl Drop for Witness {
            fn drop(&mut self) {
                let pid = std::fs::read_to_string(&self.grandchild).unwrap_or_default(); // unwrap_or_default: no pid file reads as no process, which the assertion below names
                let ps = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid.trim()]).output();
                let stat = ps.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(); // unwrap_or_default: no ps output reads as gone
                *self.gone_at_release.lock().unwrap() = Some(!pid.trim().is_empty() && (stat.is_empty() || stat.starts_with('Z'))); // a zombie has exited
            }
        }
        let gone_at_release = std::sync::Arc::new(std::sync::Mutex::new(None));
        let witness: Kept = std::sync::Arc::new(Witness { grandchild: grandchild_pid.clone(), gone_at_release: gone_at_release.clone() });
        let run = crate::code::hands::admitted(Some(lease), holding(witness, run_owned(cmd, None, "test", capture.clone())));
        let tree = tokio::select! {
            out = run => panic!("the run ended on its own: {out:?}"),
            tree = async {
                loop {
                    if let (Some(sh), Some(gc)) = (read_pid(&shell_pid), read_pid(&grandchild_pid)) {
                        let tree = descendants(sh);
                        if tree.iter().any(|who| who.pid == gc) && !capture.stdout.snapshot().bytes.is_empty() {
                            break tree;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => tree, // the run's future is dropped here: its waiter is gone
        };
        let lingering = tokio::time::timeout(std::time::Duration::from_secs(10), crate::code::hands::close(context))
            .await
            .expect("the release completes once the tree is ended, not before and not never");
        assert!(lingering.is_empty(), "nothing is left unconfirmed: {lingering:?}");
        assert_eq!(*gone_at_release.lock().unwrap(), Some(true), "what the caller held was released only once the grandchild was gone");
        // a killed descendant may wait for init to reap it; a zombie has exited, so it counts as gone
        assert!(still_present(&tree).is_empty(), "what the child started is gone when the release completes");
        let kept = capture.stdout.snapshot();
        assert_eq!((String::from_utf8_lossy(&kept.bytes).as_ref(), kept.end), ("started\n", CaptureEnd::Cut("the operation was cancelled".into())), "the prefix stays reachable on the operation's capture");

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg("echo before; sleep 600 & wait");
        let exclusion: Kept = std::sync::Arc::new(());
        let exclusion_held = std::sync::Arc::downgrade(&exclusion);
        let admission = std::sync::Arc::new(crate::code::hands::admit(crate::code::hands::HandsContext::fresh()).expect("open hands"));
        let timed = crate::code::hands::admitted(Some(admission), holding(exclusion, run_owned(cmd, Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())))
            .await
            .expect("ran");
        assert!(exclusion_held.upgrade().is_none(), "a confirmed end releases what its caller held for it (Codex reviewer on #5037)");
        assert!(matches!(timed.ended, Ended::TimedOut { ref unconfirmed } if unconfirmed.is_empty()), "a timeout ends the tree and confirms it: {timed:?}");
        assert_eq!(String::from_utf8_lossy(&timed.stdout.bytes), "before\n", "and what it wrote before the bound is kept");
        assert!(matches!(timed.stdout.end, CaptureEnd::Cut(_)), "a timed-out read is cut, never complete: {timed:?}");
    }

    // what this catches (Codex root on #5030 at 1105c9286): the child's exit was taken as the end
    // of the run while a process it started still held its output. The wait outlived the bound,
    // and the owner, having let go of the child, could no longer end anything. The lifecycle is
    // the exit AND the pipes' EOF under one bound: a holder past it cuts the read, is never
    // claimed ended, and holds the hands' release by name.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_holds_an_exited_childs_output_past_the_bound_is_held_never_claimed_ended() {
        let dir = tempfile::tempdir().unwrap();
        let holder_pid = dir.path().join("holder.pid");
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let mut cmd = tokio::process::Command::new("sh");
        // the shell exits at once; its background sleep inherited stdout and stderr
        cmd.arg("-c").arg(format!("echo out; sleep 600 & echo $! > {}", holder_pid.display()));
        // what the caller held for the run (a checkout's sync lock) stays held while the holder
        // lingers unconfirmed, never only until the end's task returns (Codex reviewer on #5037)
        let exclusion: Kept = std::sync::Arc::new(());
        let exclusion_held = std::sync::Arc::downgrade(&exclusion);
        let started = std::time::Instant::now();
        let out = crate::code::hands::admitted(Some(lease), holding(exclusion, run_owned(cmd, Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())))
            .await
            .expect("ran");
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "the bound covers the pipes, not only the exit");
        assert!(matches!(out.ended, Ended::Exited(status) if status.success()), "the shell itself exited: {out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stdout.bytes), "out\n", "what was read is kept");
        assert!(matches!(out.stdout.end, CaptureEnd::Cut(ref why) if why.contains("held it past the bound")), "the read is cut by the holder: {out:?}");
        let lingering = crate::code::hands::close(context).await;
        assert_eq!(lingering.unknown.len(), 1, "the holder is held by name on the hands, never claimed ended: {lingering:?}");
        assert!(exclusion_held.upgrade().is_some(), "and what its caller held for it stays held with it");
        if let Some(pid) = std::fs::read_to_string(&holder_pid).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
            kill9(pid); // the test's own cleanup of the holder it made
        }

        // A cancel DURING that drain (Codex root on #5030 at 3f597b9aa): the readers stay owned
        // and are stopped, and the holder is fenced on the hands just the same.
        let (shell_pid, holder_pid) = (dir.path().join("sh2.pid"), dir.path().join("holder2.pid"));
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(format!("echo $$ > {}; echo out; sleep 600 & echo $! > {}", shell_pid.display(), holder_pid.display()));
        let run = crate::code::hands::admitted(Some(lease), run_owned(cmd, None, "test", capture.clone()));
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        tokio::select! {
            out = run => panic!("the run ended while its output was held: {out:?}"),
            () = async {
                // the run itself says the shell has exited and been reaped, so the settle of its
                // pipes is under way: the cancel lands in that phase, not on a guessed delay
                while !capture.exited() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the run's future is dropped mid-drain
        }
        let lingering = crate::code::hands::close(context).await;
        assert_eq!(lingering.unknown.len(), 1, "a cancel mid-drain fences the unidentified holder: {lingering:?}");
        assert!(matches!(capture.stdout.snapshot().end, CaptureEnd::Cut(_)), "and its read is cut, its prefix kept on the capture");
        if let Some(pid) = read_pid(&holder_pid) {
            kill9(pid); // the test's own cleanup of the holder it made
        }
    }

    // what this catches (card 679ae815; Codex root on #5035): an authored command's params go to
    // the child's stdin. The old runner awaited that write BEFORE the wait, so a program that
    // never read it wedged the call past its bound, and the shared writer then discarded the
    // write's outcome, so a child that closed stdin early and exited 0 with valid JSON read as
    // having answered. Delivery is recorded: whole to a reader, failed for an early close, cut
    // at the bound or on cancel.
    #[cfg(unix)]
    #[tokio::test]
    async fn input_delivery_is_recorded_whole_failed_or_cut_and_never_read_as_delivered_when_it_wasnt() {
        let mut cat = tokio::process::Command::new("cat");
        cat.stdin(std::process::Stdio::piped());
        let echoed = run_owned_with_input(cat, Some(b"params on stdin".to_vec()), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert_eq!(echoed.stdout.bytes, b"params on stdin", "the input reached the child whole: {echoed:?}");

        let mut deaf = tokio::process::Command::new("sleep");
        deaf.arg("600");
        let big = vec![b'x'; 4 << 20]; // far past any pipe buffer: a write that waited for a reader would never finish
        let started = std::time::Instant::now();
        let ended = run_owned_with_input(deaf, Some(big), Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert!(matches!(ended.ended, Ended::TimedOut { .. }), "{ended:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "the unread input didn't hold the run past its bound");
        assert!(matches!(ended.input, Some(CaptureEnd::Cut(_))), "and its input reads as cut, never delivered: {:?}", ended.input);
        assert!(
            matches!(ended.answer(), Answer::Not(Unanswered { why: Why::TimedOut { .. }, note: Some(ref note), .. }) if note.contains("input")),
            "a timeout keeps its notes too (Codex root on #5038): {:?}",
            ended.answer()
        );
        assert_eq!(echoed.input, Some(CaptureEnd::Complete), "a child that took it all: delivered");

        // Codex root on #5035 at 549dce142: a child that closes its stdin without taking the
        // input, then answers valid JSON and exits 0, must not read as having answered THIS
        // input. The input is past the pipe's buffer, so the write can only end by the close.
        let mut early = tokio::process::Command::new("sh");
        early.arg("-c").arg("exec 0<&-; echo '{}'");
        let closed = run_owned_with_input(early, Some(vec![b'x'; 4 << 20]), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert!(matches!(closed.ended, Ended::Exited(status) if status.success()), "it exited 0: {closed:?}");
        assert_eq!(String::from_utf8_lossy(&closed.stdout.bytes).trim(), "{}", "with valid JSON");
        assert!(matches!(closed.input, Some(CaptureEnd::Failed(_))), "but its input was not delivered: {:?}", closed.input);
        assert!(matches!(closed.answer(), Answer::Not(Unanswered { why: Why::NotWhole, note: Some(ref note), ref stdout, .. }) if note.contains("input") && stdout.bytes == b"{}\n"), "and it is not an answer: {:?}", closed.answer());

        // canary run 38541241843 (Codex root on #5035): a child that exits non-zero BEFORE reading
        // its input lost its own verdict to the broken-pipe note whenever the write landed first.
        // Past the pipe's buffer the write can only end by the exit, so this is that race, always.
        let mut refuses = tokio::process::Command::new("sh");
        refuses.arg("-c").arg("echo boom >&2; exit 3");
        let failed = run_owned_with_input(refuses, Some(vec![b'x'; 4 << 20]), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        match failed.answer() {
            Answer::Not(un) => {
                assert!(matches!(un.why, Why::Exited(status) if status.code() == Some(3)), "its own exit status is kept: {un:?}");
                assert_eq!(String::from_utf8_lossy(&un.stderr.bytes).trim(), "boom", "and its own stderr");
                assert!(un.note.as_deref().is_some_and(|n| n.contains("input")), "the undelivered input is said beside it, never in place of it");
                let said = un.to_string();
                assert!(said.starts_with("exit 3 (") && said.contains("input") && said.ends_with("stderr:\nboom"), "and all of it is said: {said}");
            }
            other => panic!("a non-zero exit is the child's own failure: {other:?}"),
        }

        // A cancelled run: the input's end is on the operation's capture, never left open.
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut slow = tokio::process::Command::new("sleep");
        slow.arg("600");
        let run = run_owned_with_input(slow, Some(vec![b'x'; 4 << 20]), None, "test", capture.clone());
        let _ = tokio::time::timeout(std::time::Duration::from_millis(300), run).await; // the waiter is dropped mid-write
        assert_eq!(capture.input.snapshot().end, CaptureEnd::Cut("the operation was cancelled".into()), "a cancel cuts the input by name");
    }

    // what this catches (Codex root on #5035 at dc2a64c8e): a process the child started that
    // holds its STDIN after the child exits (reading nothing) kept the input's write open, and
    // the writer was taken out of the owner before it was awaited, so a cancel then detached
    // it and nothing fenced the release. Past the bound, or on a cancel during that wait, the
    // input is cut by name and the unidentified holder holds the hands, as an output holder does.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_holds_an_exited_childs_stdin_is_fenced_at_the_bound_and_on_a_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        // the shell exits at once; its background sleep keeps the stdin pipe and nothing else.
        // Through fd 3: a POSIX shell gives an async command /dev/null as stdin unless it has an
        // explicit redirection, and dash (CI's sh) applied that over `<&0` (a broken pipe on CI)
        let holder = |pidfile: &std::path::Path| {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.arg("-c").arg(format!("exec 3<&0; sleep 600 <&3 3<&- >/dev/null 2>&1 & echo $! > {}", pidfile.display()));
            cmd
        };
        let input = || Some(vec![b'x'; 4 << 20]); // past the pipe's buffer: the write waits on the holder

        // at the bound
        let first = dir.path().join("holder1.pid");
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let out = crate::code::hands::admitted(Some(lease), run_owned_with_input(holder(&first), input(), Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default()))
            .await
            .expect("ran");
        assert!(matches!(out.ended, Ended::Exited(_)), "the shell itself exited: {out:?}");
        assert!(matches!(out.input, Some(CaptureEnd::Cut(ref why)) if why.contains("held it past the bound")), "the input is cut by the holder: {:?}", out.input);
        assert_eq!(crate::code::hands::close(context).await.unknown.len(), 1, "and the holder holds the release by name");
        if let Some(pid) = read_pid(&first) {
            kill9(pid); // the test's own cleanup of the holder it made
        }

        // on a cancel AFTER the child exited, while the write is still waiting
        let second = dir.path().join("holder2.pid");
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let run = crate::code::hands::admitted(Some(lease), run_owned_with_input(holder(&second), input(), None, "test", capture.clone()));
        tokio::select! {
            out = run => panic!("the run ended while its stdin was held: {out:?}"),
            () = async {
                // the run itself says the shell has exited and been reaped: the writer's wait on the
                // holder is under way, and the cancel lands in it, not after a guessed delay
                while !capture.exited() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the run's future is dropped during the writer's wait
        }
        assert_eq!(capture.input.snapshot().end, CaptureEnd::Cut("the operation was cancelled".into()), "the cancel cuts the input by name");
        assert_eq!(crate::code::hands::close(context).await.unknown.len(), 1, "and the holder holds the release");
        if let Some(pid) = read_pid(&second) {
            kill9(pid);
        }
    }

    // what this catches (Codex root on #5030 at 1105c9286): a read error was swallowed and the
    // stream reported as whole, losing the prefix. A failed read is typed Failed, apart from the
    // process's verdict, and keeps what it read before the failure.
    #[tokio::test]
    async fn a_read_that_fails_is_typed_failed_and_keeps_its_prefix() {
        struct ThenFail(bool);
        impl tokio::io::AsyncRead for ThenFail {
            fn poll_read(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
                if self.0 {
                    return std::task::Poll::Ready(Err(std::io::Error::other("the pipe broke")));
                }
                self.0 = true;
                buf.put_slice(b"prefix");
                std::task::Poll::Ready(Ok(()))
            }
        }
        let capture = std::sync::Arc::new(RunCapture::default());
        read_stream(Some(ThenFail(false)), capture.clone(), |c| &c.stdout).await;
        let read = capture.stdout.snapshot();
        assert_eq!((read.bytes.as_slice(), read.end), (b"prefix".as_slice(), CaptureEnd::Failed("the pipe broke".into())));
    }

    // what this catches: the Windows port-owner lookup returning None for a port
    // that IS held. Regression for the 2026-09-05 wedge on BigMama — `lsof` does
    // not exist on Windows, so `pid_listening_on_port` returned None for every
    // port, the pre-spawn gate read that as "held by an unnameable stranger", and
    // serving refused to spawn FOREVER against the core's own orphaned child.
    #[test]
    fn netstat_names_the_listening_pid_for_the_right_port() {
        // Verbatim `netstat -ano -p TCP` output from the wedged box.
        let out = "  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
  TCP    127.0.0.1:58057        127.0.0.1:64707        TIME_WAIT       0
  TCP    0.0.0.0:445            0.0.0.0:0              LISTENING       4
";
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
        assert_eq!(parse_netstat_listening_pid(out, 445), Some(4));
        // Not listening anywhere = None, which callers treat as "refuse to kill".
        assert_eq!(parse_netstat_listening_pid(out, 9999), None);
        // TIME_WAIT is not a holder we may reap.
        assert!(!out.is_empty());
    }

    // what this catches: matching the port as a substring instead of the local
    // address's final component. `contains(":8057")` would match ":58057" and
    // hand a reap decision the WRONG pid — a blind kill of an unrelated process.
    #[test]
    fn netstat_port_match_is_not_a_substring_match() {
        let out = "  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
";
        assert_eq!(parse_netstat_listening_pid(out, 8057), None);
        assert_eq!(parse_netstat_listening_pid(out, 5805), None);
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
    }

    // what this catches: reading tasklist's NO-MATCH prose as an image name.
    // tasklist exits 0 and prints "INFO: No tasks are running..." on stdout, so
    // status success does NOT imply a hit; parsing that line as a name would make
    // is_llama_server true for a dead pid and authorise a kill.
    #[test]
    fn tasklist_parses_the_image_name_and_refuses_the_no_match_line() {
        let hit = "\"llama-server.exe\",\"3852\",\"Console\",\"2\",\"14,254,876 K\"
";
        assert_eq!(
            parse_tasklist_image_name(hit).as_deref(),
            Some("llama-server.exe")
        );
        let miss = "INFO: No tasks are running which match the specified criteria.
";
        assert_eq!(parse_tasklist_image_name(miss), None);
        assert_eq!(parse_tasklist_image_name(""), None);
    }

    // what this catches: THE safety invariant shared by both callers — our own
    // test-runner process is NOT a llama-server, so is_llama_server must refuse it.
    // A regression here would let either module SIGKILL an unrelated reused-pid
    // process (the blind-kill this guard exists to prevent).
    #[test]
    fn is_llama_server_false_for_self() {
        assert!(
            !is_llama_server(std::process::id()),
            "the test runner is not a llama-server; reaping it would be a blind kill"
        );
    }

    // what this catches: the grant recovered for an ADOPTED engine (every deploy leaves the
    // lane up for the next core) is read off its own argv — both spellings, absent = None,
    // malformed = None, and 0 stays 0 (a real "no host cache" launch, not an unknown).
    #[test]
    fn cache_ram_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            cache_ram_mib_in(&argv("llama-server -m x.gguf -c 133930 --parallel 2 --cache-ram 14396 --port 8080")),
            Some(14_396)
        );
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram=8704")), Some(8_704));
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram 0")), Some(0), "0 is a value");
        assert_eq!(cache_ram_mib_in(&argv("llama-server -c 4096")), None, "absent = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram")), None, "malformed = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram lots")), None);
    }

    // what this catches: busy is not dead. The verdict needs the probe AND the process's
    // activity: a miss with CPU advancing is a slow answer under load (adopted), a miss
    // with a frozen process is a wedge (reaped), an unsampled process never exonerates.
    // Regression for the M5 2026-09-20 (card b57b19fd): a 32 GB lane mid-generation was
    // killed on one 3 s probe under swap, twice in one morning.
    #[test]
    fn busy_is_not_dead_the_verdict_needs_two_signals() {
        assert_eq!(verdict_from(Some(2), None, ""), LaneVerdict::Healthy { attempt: 2 });
        let busy = verdict_from(None, Some(140.0), "no answer within 5 s");
        assert!(busy.adopt(), "a working lane that answers late is adopted: {busy:?}");
        assert!(matches!(busy, LaneVerdict::Busy { cpu_pct, .. } if cpu_pct == 140.0));
        let frozen = verdict_from(None, Some(0.0), "no answer within 5 s");
        assert!(!frozen.adopt(), "a frozen lane that never answers is dead: {frozen:?}");
        let unsampled = verdict_from(None, None, "connection refused");
        assert!(!unsampled.adopt(), "an absence never exonerates");
        assert!(busy.evidence().contains("adopted") && frozen.evidence().starts_with("dead"));
    }

    // what this catches: the probe is retried within a bound sized for a loaded box — a
    // listener that answers 4 s late (the M5 under swap) is HEALTHY on the first attempt
    // with a 5 s bound, where the old single 3 s shot read it as dead; and a port nobody
    // listens on misses every attempt with the reason carried.
    #[test]
    fn a_slow_answer_within_the_bound_is_healthy() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = sock.read(&mut buf);
                std::thread::sleep(std::time::Duration::from_secs(4));
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        assert_eq!(probe_health_bounded(port, 1, 5), Ok(1), "4 s late is inside a 5 s bound");
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let miss = probe_health_bounded(dead_port, 2, 1);
        assert!(miss.is_err(), "nobody listening → every attempt misses: {miss:?}");
    }

    // what this catches: a definitely-dead pid is neither alive nor a llama-server,
    // so a stale registry/pidfile entry can never trigger a kill.
    #[test]
    fn dead_pid_is_not_alive_or_llama() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        // (Tiny PID-reuse window is acceptable in a unit test; the invariant we
        // assert is "we do not treat a reaped pid as a live llama-server".)
        assert!(
            !is_llama_server(dead),
            "a reaped pid must not read as a live llama-server"
        );
    }

    // what this catches (card 977842fd): the running engine's KV decision is read off its
    // own argv in every shape the launcher and an operator write it — the fact the adopt
    // rail compares the plan's decision against.
    #[test]
    fn the_kv_decision_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        let kv = launch_kv_in(&argv("llama-server -m x.gguf --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on -c 53760"));
        assert_eq!(kv, LaunchKv { cache_type: Some("q8_0".into()), flash_attn: true, cache_ram_mib: None });
        let plain = launch_kv_in(&argv("llama-server -m x.gguf -c 26880 --parallel 1"));
        assert_eq!(plain, LaunchKv { cache_type: None, flash_attn: false, cache_ram_mib: None });
        assert_eq!(plain.cache_type_or_default(), "f16", "no flag is the engine's f16 default");
        assert_eq!(launch_kv_in(&argv("llama-server --cache-type-k=q4_0 --flash-attn=off")), LaunchKv { cache_type: Some("q4_0".into()), flash_attn: false, cache_ram_mib: None });
        assert_eq!(launch_kv_in(&argv("llama-server -c 51968 --cache-ram 3248")).cache_ram_mib, Some(3_248), "the grant an adopted lane was launched with");
        assert!(launch_kv_in(&argv("llama-server -fa --mmproj p.gguf")).flash_attn, "a bare flag is on and eats nothing");
        assert!(!launch_kv_in(&argv("llama-server --flash-attn off")).flash_attn);
    }
}
