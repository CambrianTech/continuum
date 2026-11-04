//! What ending a process tree came out as: the identity of one process the table saw, and the
//! end of an owned child with everything it started. Machine facts the engine's process owner
//! produces and the hands gate holds until a release confirms them, so they live in the kernel
//! with no layer above either.

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

/// Is this exact process (pid AND start time) still running? One process's entry, not the
/// whole table: what a holder of a remembered identity asks before paying for a full read.
pub fn alive(identity: ProcessIdentity) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(identity.pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
    sys.process(pid).is_some_and(|process| process.start_time() == identity.start_time)
}
