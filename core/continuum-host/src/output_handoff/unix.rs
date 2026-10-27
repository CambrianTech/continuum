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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // what this catches (card 1d7d6d4c): a reader outside the writer's group (a group kill of the
    // engine would leave it), one that misses lines, or one that never sees the end because the
    // spawner kept a copy of the pipe.
    #[tokio::test]
    async fn the_reader_joins_the_writers_group_reads_every_line_and_ends_with_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("out");
        let mut writer = Command::new("sh");
        writer.args(["-c", "echo one >&2; sleep 1; echo two >&2"]).stderr(Stdio::piped()).process_group(0);
        let mut writer = writer.spawn().expect("spawn the writer");
        let group = writer.id().expect("a running writer has a pid");
        let script = format!("cat > '{}'", out.display());
        let Handoff::Reader(mut reader) = spawn_reader(Path::new("sh"), ["-c", script.as_str()], writer.stderr.take().expect("piped"), group).expect("spawn the reader") else {
            panic!("unix hands the pipe to the reader");
        };
        let pgid = std::process::Command::new("ps").args(["-o", "pgid=", "-p", &reader.id().expect("a running reader has a pid").to_string()]).output().expect("ps");
        assert_eq!(String::from_utf8_lossy(&pgid.stdout).trim(), group.to_string(), "the reader is in the writer's group");
        writer.wait().await.expect("the writer exits");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = reader.try_wait().expect("poll the reader") {
                break status;
            }
            assert!(Instant::now() < deadline, "the reader ends once its writer has");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&out).expect("output"), "one\ntwo\n");
    }
}
