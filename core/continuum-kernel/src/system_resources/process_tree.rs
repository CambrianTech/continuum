//! What ending a process tree came out as: the identity of one process the table saw, and the
//! end of an owned child with everything it started. Machine facts the engine's process owner
//! produces and the hands gate holds until a release confirms them, so they live in the kernel
//! with no layer above either.


/// This machine's host name, read once (it cannot change under a running process): the identity a
/// node signs its lines with, never a resource's size, so it lives with the process table's other
/// identity reads (card 72b767a4). `None` when the OS reports none, an honest unknown.
pub fn host_name() -> Option<&'static str> {
    static HOST_NAME: std::sync::LazyLock<Option<String>> = std::sync::LazyLock::new(sysinfo::System::host_name);
    HOST_NAME.as_deref()
}

use std::path::{Path, PathBuf};

/// One process as the process table saw it: its pid and start time. An act on it later is
/// refused if the pid now names another process (pid reuse never kills a stranger).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
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

// The kernel's one reader of the machine's process table. A full read costs a `sysctl` per
// process, plus a path resolution for each executable, so whoever needs the table reads it once
// and asks it every question it has. On the IntelMac (2026-11-04) the cargo-target pools built a
// fresh table per profile rung on every 5 s broker tick: ~1.5 cores of the core for a disk
// eviction that freed nothing (card b958bed4).


/// One process as one read of the table saw it.
#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub identity: ProcessIdentity,
    pub name: String,
    /// The executable, canonicalized once at the read so a symlinked target dir still matches
    /// (a vanished image compares by its recorded path). `None` when the table had no path.
    pub exe: Option<PathBuf>,
}

/// One read of the process table.
#[derive(Debug, Clone, Default)]
pub struct ProcessTable {
    rows: Vec<ProcessRow>,
}

impl ProcessTable {
    /// Read every process once. A table that cannot be read is empty, which callers already
    /// treat as "nobody" in the direction they err (derived artifacts the next build recreates).
    pub fn read() -> Self {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        let mut sys = System::new();
        // `remove_dead_processes = true`: an exited process holds nothing.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
        );
        let rows = sys
            .processes()
            .iter()
            .map(|(pid, process)| ProcessRow {
                identity: ProcessIdentity { pid: pid.as_u32(), start_time: process.start_time() },
                name: process.name().to_string_lossy().into_owned(),
                exe: process
                    .exe()
                    .map(|exe| exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf())), // unwrap_or_else: an exe that vanished mid-read compares by its recorded path
            })
            .collect();
        Self { rows }
    }

    /// A table holding exactly these rows: the test seam for owners that read through it.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn of(rows: Vec<ProcessRow>) -> Self {
        Self { rows }
    }

    /// The first process whose executable lives under `dir` (canonicalized), with that executable.
    pub fn executing_under(&self, dir: &Path) -> Option<(&ProcessRow, &Path)> {
        let dir = dir.canonicalize().ok()?;
        self.rows.iter().find_map(|row| {
            let exe = row.exe.as_deref()?;
            exe.starts_with(&dir).then_some((row, exe))
        })
    }

    /// The first compiler (`rustc`, `cargo`) running anywhere on the host, if any.
    pub fn compiler_running(&self) -> Option<&ProcessRow> {
        self.rows
            .iter()
            .find(|row| continuum_cli_lifecycle::deploy_claim::is_compiler(&row.name))
    }
}

/// One pass's table: read at most once, on its first question, and shared by every question the
/// pass asks after it. A pass that never needs the table never reads it.
pub struct PassTable {
    read: fn() -> ProcessTable,
    table: Option<ProcessTable>,
}

impl PassTable {
    pub fn new(read: fn() -> ProcessTable) -> Self {
        Self { read, table: None }
    }

    pub fn get(&mut self) -> &ProcessTable {
        let read = self.read;
        self.table.get_or_insert_with(read)
    }
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

/// What reading one process's accumulated CPU time found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuTime {
    /// That process (pid AND start time) has used this much CPU, user + system, since it started.
    Used(std::time::Duration),
    /// That process still runs, but this OS gave no reading: unknown, never zero.
    Unreadable,
    /// It exited, is a zombie, or its pid now names another process.
    Gone,
}

/// The CPU time `who` has accumulated, fenced by pid AND start time. The counter is read FIRST
/// and the identity checked AFTER: a process that still holds the pid with the same start time
/// after the read held it continuously since it started, so the reading was its own. A reused
/// pid is `Gone`, never another process's CPU read as "busy". The OS call is
/// [`continuum_host::process_usage::cpu_time`]'s adapter.
pub fn cpu_time(who: ProcessIdentity) -> CpuTime {
    let used = continuum_host::process_usage::cpu_time(who.pid);
    if still_present(&[who]).is_empty() {
        return CpuTime::Gone;
    }
    used.map_or(CpuTime::Unreadable, CpuTime::Used)
}

/// What one read of a watched process said about the interval since the read before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuSample {
    /// The same process (pid AND start time) still runs. `spent` is the CPU time it accumulated
    /// since the previous read (zero = its counter did not advance); `None` when there is no
    /// previous reading to compare (the first read, or this OS gave none).
    Alive { spent: Option<std::time::Duration> },
    /// It exited, is a zombie, or its pid now names another process.
    Gone,
}

/// One process's accumulated CPU time, read checkpoint after checkpoint. The question it answers
/// without asking the process anything: did it spend CPU since the last read? An engine whose
/// request queue is busy with one long batch cannot answer an HTTP probe, but its CPU counter
/// still advances (the IntelMac, 2026-11-04 16:24:20Z: a page restore was judged a dead engine
/// while its main thread was in decode).
pub struct CpuWatch {
    who: ProcessIdentity,
    last: Option<std::time::Duration>,
}

impl CpuWatch {
    pub fn new(who: ProcessIdentity) -> Self {
        Self { who, last: None }
    }

    /// Read the process once more: gone, or alive with the CPU it spent since the last read.
    pub fn sample(&mut self) -> CpuSample {
        let now = match cpu_time(self.who) {
            CpuTime::Gone => return CpuSample::Gone,
            CpuTime::Unreadable => None,
            CpuTime::Used(used) => Some(used),
        };
        let before = std::mem::replace(&mut self.last, now);
        CpuSample::Alive { spent: before.zip(now).map(|(before, now)| now.saturating_sub(before)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_identity() -> ProcessIdentity {
        let pid = std::process::id();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        let start_time = sys.process(sysinfo::Pid::from_u32(pid)).map(sysinfo::Process::start_time);
        ProcessIdentity { pid, start_time: start_time.expect("test: this process is in the table") }
    }

    // what this catches (the IntelMac, 2026-11-04 16:24Z, an engine replaced while it worked; and
    // window 45 on the M5, where a usage percentage read 0 for a spinning process): a CPU read is
    // the ACCUMULATED counter, fenced by pid AND start time. A working process's counter advances;
    // the same pid with another start time (a reused pid) and an exited, reaped child read gone.
    #[test]
    fn a_cpu_read_is_fenced_by_pid_and_start_time() {
        let me = own_identity();
        let mut watch = CpuWatch::new(me);
        assert_eq!(watch.sample(), CpuSample::Alive { spent: None }, "a first read has nothing to compare");
        // Spend CPU well past the coarsest counter's resolution (Linux clock ticks).
        let spin_until = std::time::Instant::now() + std::time::Duration::from_millis(400);
        let mut spun = 0u64;
        while std::time::Instant::now() < spin_until {
            spun = std::hint::black_box(spun.wrapping_add(1));
        }
        let readable = continuum_host::process_usage::cpu_time(me.pid).is_some();
        match watch.sample() {
            CpuSample::Alive { spent: Some(spent) } => assert!(spent > std::time::Duration::ZERO, "a spinning process's counter advanced"),
            CpuSample::Alive { spent: None } => assert!(!readable, "this OS reads the counter, so a second read compares"),
            CpuSample::Gone => panic!("test: this process is running"),
        }
        let reused = ProcessIdentity { pid: me.pid, start_time: me.start_time.wrapping_add(1) };
        assert_eq!(cpu_time(reused), CpuTime::Gone, "a pid naming another process is gone");
        assert_eq!(CpuWatch::new(reused).sample(), CpuSample::Gone);
        // This test binary, asked to run a test that does not exist: a child on every OS that exits at once.
        let mut child = std::process::Command::new(std::env::current_exe().expect("test: own executable"))
            .args(["--exact", "no_such_test"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("test: spawn a child");
        let exited = ProcessIdentity { pid: child.id(), start_time: 0 };
        child.wait().expect("test: reap the child");
        assert_eq!(cpu_time(exited), CpuTime::Gone, "an exited, reaped process is gone");
    }
}
