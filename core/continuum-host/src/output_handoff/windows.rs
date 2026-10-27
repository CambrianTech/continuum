use std::ffi::OsStr;
use std::io;
use std::path::Path;

use super::Handoff;

pub(super) const SUPPORTED: bool = false;

pub(super) fn spawn_reader<I, S>(_program: &Path, _args: I, stderr: tokio::process::ChildStderr, _group: u32) -> io::Result<Handoff>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Ok(Handoff::Kept(stderr))
}
