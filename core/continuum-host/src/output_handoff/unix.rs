use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use super::Handoff;

pub(super) const SUPPORTED: bool = true;

pub(super) fn spawn_reader<I, S>(program: &Path, args: I, stderr: tokio::process::ChildStderr, group: u32) -> io::Result<Handoff>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let group = i32::try_from(group).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("process group {group} is past the platform's pid range")))?;
    let pipe = stderr.into_owned_fd()?;
    Command::new(program).args(args).stdin(Stdio::from(pipe)).stdout(Stdio::null()).process_group(group).spawn().map(Handoff::Reader)
}

