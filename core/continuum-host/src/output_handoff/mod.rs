//! A child's stderr handed to a reader process that joins the child's process group, so the
//! reader outlives the process that spawned both (card 1d7d6d4c: an engine's log relay survives
//! a core restart). What differs by OS is whether the handoff exists: Unix passes the pipe as
//! the reader's stdin and places it in the child's group, so a group kill aimed at the child
//! takes its reader too; Windows has no such group here, and hands the stderr back for the
//! caller to drain itself.

use std::ffi::OsStr;
use std::io;
use std::path::Path;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Whether this host hands a child's stderr to a reader process at all; where it does not, no
/// reader exists to follow.
pub const SUPPORTED: bool = os::SUPPORTED;

/// What became of a child's stderr.
#[derive(Debug)]
pub enum Handoff {
    /// A reader process holds it. An async child: awaiting its exit never parks a thread, so a
    /// reader meant to outlive this process never holds its runtime's shutdown.
    Reader(tokio::process::Child),
    /// This host has no handoff: the stderr comes back to the caller, unread.
    Kept(tokio::process::ChildStderr),
}

/// Spawn `program` with `args` as the reader of `stderr` (its stdin), in the process group led by
/// `group` (the child that writes it). The reader's stdout is discarded and its own stderr is
/// this process's. This process's copy of the pipe is closed once the reader holds it, so the
/// reader sees the end exactly when every writer has closed it. A spawn that fails (for one, a
/// group whose leader has already exited) returns the error, and the pipe is closed with it.
pub fn spawn_reader<I, S>(program: &Path, args: I, stderr: tokio::process::ChildStderr, group: u32) -> io::Result<Handoff>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    os::spawn_reader(program, args, stderr, group)
}
