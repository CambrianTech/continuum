// Only the Windows paths below reach the parent's items (parse_tasklist_image_name).
#[cfg(windows)]
use super::*;
use continuum_kernel::system_resources::disk_reporters::Kept;
use continuum_kernel::system_resources::process_tree::{ProcessIdentity, TreeEnd};


/// How often an owned execution's task looks for a kill request: the cadence a kill is noticed
/// at, and so the cadence a release checks a killed tree has ended at ([`confirm_gone`] waits
/// one of these). The shell's execution task polls at it too.
pub const KILL_POLL: std::time::Duration = std::time::Duration::from_millis(100);

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
    signal_if_same(who).is_ok_and(|sent| sent)
}

/// Preserve the native termination result for lifecycle callers. `false` means
/// the recorded incarnation is no longer present; it never authorizes another PID.
pub fn signal_if_same(who: ProcessIdentity) -> Result<bool, String> {
    #[cfg(unix)]
    if still_present(&[who]).is_empty() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        // SAFETY: the recorded incarnation was checked immediately above.
        if unsafe { libc::kill(who.pid as libc::pid_t, libc::SIGKILL) } == 0 {
            Ok(true)
        } else {
            Err(format!("SIGKILL pid={} started={}: {}", who.pid, who.start_time, std::io::Error::last_os_error()))
        }
    }
    #[cfg(windows)]
    {
        signal_windows_identity(who)
    }
}

#[cfg(windows)]
fn signal_windows_identity(who: ProcessIdentity) -> Result<bool, String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{FILETIME, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, PROCESS_SYNCHRONIZE,
    };
    // Pin the incarnation before inspecting or signalling it: taskkill's PID
    // lookup could otherwise hit a replacement between our check and its call.
    // SAFETY: scalar PID/access flags only; returned handle is owned below.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, who.pid) };
    if raw.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(false);
        }
        return Err(format!("OpenProcess pid={} started={}: {error}", who.pid, who.start_time));
    }
    // SAFETY: OpenProcess returned a new owned handle above.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: owned live handle and four valid writable FILETIME pointers.
    if unsafe { GetProcessTimes(process.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(format!("GetProcessTimes pid={}: {}", who.pid, std::io::Error::last_os_error()));
    }
    let ticks = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    let started = (ticks / 10_000_000).checked_sub(11_644_473_600)
        .ok_or("process creation time precedes the Unix epoch")?;
    // SAFETY: the owned handle stays open through the nonblocking wait.
    if started != who.start_time || unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
        return Ok(false);
    }
    terminate_windows_handle(&process, who)
}

// The caller has verified creation time while holding this handle. Keep the
// termination/exit race on that handle, including when signalling fails.
#[cfg(windows)]
fn terminate_windows_handle(
    process: &impl std::os::windows::io::AsRawHandle,
    who: ProcessIdentity,
) -> Result<bool, String> {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
    // SAFETY: terminate only this held, creation-time-verified incarnation.
    if unsafe { TerminateProcess(process.as_raw_handle(), 1) } == 0 {
        let error = std::io::Error::last_os_error();
        // It can exit between the preflight wait and TerminateProcess. Only a
        // signalled handle proves that race; a live/failed wait preserves the
        // original refusal, including genuine access denial. Never re-open a PID.
        // SAFETY: the same owned incarnation handle remains open through this wait.
        if unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
            return Ok(false);
        }
        return Err(format!("TerminateProcess pid={} started={}: {error}", who.pid, who.start_time));
    }
    Ok(true)
}

#[cfg(all(test, windows))]
#[path = "kill9_windows_tests.rs"]
mod windows_termination_tests;


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
    // A SIGKILL can't be ignored, but the kernel tears the process down after the signal is sent,
    // and an orphan is reaped by init after its parent (the shell, reaped above) is gone: one
    // snapshot taken right after the kill can still see a descendant that is dying. Measured on the
    // M5 Pro (2026-10-15T19:34Z, macOS 26.5, against canary 8eaed76f3's process model; a Python
    // driver of a shell's `sleep` grandchild, n=200): gone 2.2 ms p50, 3.7 ms max after its
    // SIGKILL; a loaded CI runner saw one still present at the first snapshot (canary 8eaed76f3,
    // shell_session test_kill_execution). So the killed set is re-checked, at a doubling interval
    // from 1 ms, until it is gone or one KILL_POLL has passed, the cadence a release already checks
    // a killed execution at. What remains after that is recorded, never assumed gone: a release
    // re-checks it, as before.
    let survivors = confirm_gone(check, KILL_POLL).await;
    TreeEnd { reaped, survivors, unwalked }
}

/// Check `killed`, already sent their kill, until none is present or `within` has passed, and
/// return what is still present. It only looks: the kill was sent (re-sending it per check would
/// spawn a taskkill per survivor per check on Windows; Cormac's review of #5070), and a release's
/// recheck re-kills what remains. A check that couldn't run confirms nothing: everything it was
/// given stays unconfirmed.
pub async fn confirm_gone(killed: Vec<ProcessIdentity>, within: std::time::Duration) -> Vec<ProcessIdentity> {
    let deadline = tokio::time::Instant::now() + within;
    let mut still = killed;
    let mut wait = std::time::Duration::from_millis(1);
    loop {
        let check = still.clone();
        still = match tokio::task::spawn_blocking(move || still_present(&check)).await {
            Ok(present) => present,
            Err(_) => return still,
        };
        let now = tokio::time::Instant::now();
        if still.is_empty() || now >= deadline {
            return still;
        }
        tokio::time::sleep(wait.min(deadline - now)).await;
        wait *= 2;
    }
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
    pub(super) state: std::sync::Mutex<(Vec<u8>, Option<CaptureEnd>)>,
}

impl StreamCapture {
    pub(super) fn append(&self, chunk: &[u8]) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).0.extend_from_slice(chunk); // unwrap_or_else: a poisoned capture still keeps its bytes
    }

    /// Set how reading ended, once: the first end wins (a reader's own EOF or error is not
    /// overwritten by a later cut).
    pub(super) fn finish(&self, end: CaptureEnd) {
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
    pub(super) exited: std::sync::atomic::AtomicBool,
}

impl RunCapture {
    /// Whether the child itself has exited and been reaped.
    pub fn exited(&self) -> bool {
        self.exited.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Read `pipe` into `into` until EOF or a read error, appending as it goes so a cut keeps the
/// prefix. The chunk size is the pipe read size, not a budget.
pub(super) async fn read_stream(pipe: Option<impl tokio::io::AsyncRead + Unpin>, into: std::sync::Arc<RunCapture>, which: fn(&RunCapture) -> &StreamCapture) {
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
/// keeps the admission of the work that started it (`identity::hands_gate::current`), so a release never
/// completes under it, and what it can't confirm is kept on that admission.
/// One pipe task a run owns: a reader of its stdout or stderr, or the writer of its stdin.
/// Every one has the SAME lifecycle (Codex root on #5035: the ownership defect repeated across
/// stream directions because the lifecycle was copied per direction): it records its own end on
/// its stream, it is awaited in place, a JoinError is a failed stream, and an unfinished one is
/// stopped and cut, never detached.
pub(super) struct PipeTask {
    pub(super) task: tokio::task::JoinHandle<()>,
    pub(super) stream: fn(&RunCapture) -> &StreamCapture,
    /// What a holder of this pipe holds, in the words a fence names it with.
    pub(super) name: &'static str,
}

/// A child the core waits on, owned until its whole lifecycle has ended: the child and the tree
/// it started, AND its pipes (Astra on #5026, Codex root on #5030 and #5035). Ending it is ONE
/// path for a timeout and a dropped waiter (a cancelled command, a rebooting core) alike. It
/// keeps the admission of the work that started it (`identity::hands_gate::current`), so a release never
/// completes under it, and what it can't confirm is kept on that admission.
pub(super) struct OwnedChild {
    pub(super) child: Option<tokio::process::Child>,
    pub(super) pipes: Vec<PipeTask>,
    pub(super) capture: std::sync::Arc<RunCapture>,
    pub(super) held: Option<std::sync::Arc<continuum_kernel::identity::hands_gate::HandsLease>>,
    /// What its caller holds for it ([`holding`]), kept until its tree has ended.
    pub(super) kept: Vec<Kept>,
    pub(super) label: &'static str,
}

tokio::task_local! {
    /// What every owned child started on this task keeps until its tree has ended.
    static HOLDING: Vec<Kept>;
}

/// Run `work` with every owned child it starts keeping `value` until that child's tree has
/// ended, not merely until `work`'s future does. A cancel drops the future at once while the
/// tree's end runs after it ([`OwnedChild`]'s drop), so an exclusion held only by the future
/// (a checkout's sync lock) would admit the next holder beside a git still being ended
/// (Codex reviewer on #5037). Nested scopes add to what the outer ones hold. It needs an
/// admission ([`continuum_kernel::identity::hands_gate::admitted`]): what an unconfirmed end keeps goes onto it.
pub async fn holding<F: std::future::Future>(value: Kept, work: F) -> F::Output {
    let mut all = HOLDING.try_with(Vec::clone).unwrap_or_default(); // unwrap_or_default: outside any scope nothing else is held
    all.push(value);
    HOLDING.scope(all, work).await
}

impl OwnedChild {
    /// Await every pipe to its end, IN PLACE: they stay owned here, so a cancel or panic
    /// mid-await finds them in `pipes` and [`Drop`] stops them. A pipe task that panicked or
    /// was aborted reads as a failed stream, never as whole.
    pub(super) async fn settle_pipes(&mut self) {
        for pipe in self.pipes.iter_mut() {
            if let Err(join) = (&mut pipe.task).await {
                (pipe.stream)(&self.capture).finish(CaptureEnd::Failed(format!("its {} task did not finish: {join}", pipe.name)));
            }
        }
        self.pipes.clear(); // all settled: nothing left to stop
    }

    /// Stop every pipe that is still open, keeping what it read or wrote: each is marked cut with
    /// `why`. Returns the names of the pipes that were still open.
    pub(super) fn cut_pipes(&mut self, why: &str) -> Vec<&'static str> {
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
    pub(super) fn fence_holders(&self, open: &[&'static str], when: &str) {
        if self.child.is_some() || open.is_empty() {
            return; // the child's own end walks its tree
        }
        let why = format!("{}: {when}, a process it started still held its {} (holders unidentified, not claimed ended)", self.label, open.join(", "));
        continuum_base::probe!(class = "process.pipe_held", process = self.label, pipes = ?open, "an owned process exited but a process it started still held its pipes; the hands' release holds");
        if let Some(lease) = &self.held {
            lease.keep_unknown(why);
        }
        keep_with_lingering(self.held.as_ref(), &self.kept);
    }

    /// Hand the child to its end: a spawned task, so the caller's cancellation can't cut it
    /// short. `None` once it exited and was reaped, or with no runtime to run the end on.
    pub(super) fn end(&mut self) -> Option<tokio::task::JoinHandle<TreeEnd>> {
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
pub(super) async fn end_owned(mut child: tokio::process::Child, held: Option<std::sync::Arc<continuum_kernel::identity::hands_gate::HandsLease>>, kept: Vec<Kept>, label: &'static str) -> TreeEnd {
    let end = end_tree(&mut child).await;
    if !end.confirmed() {
        continuum_base::probe!(
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
pub(super) fn keep_with_lingering(held: Option<&std::sync::Arc<continuum_kernel::identity::hands_gate::HandsLease>>, kept: &[Kept]) {
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
    pub(super) fn input_note(&self) -> Option<String> {
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
    let held = continuum_kernel::identity::hands_gate::current();
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
