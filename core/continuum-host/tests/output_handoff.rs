//! The output handoff with real processes. Its own test binary, deliberately: it forks, and a
//! fork that lands while a test in the same process is creating a socket can carry that socket
//! into the child (on macOS, close-on-exec is set after the socket exists), so another test's
//! endpoint looks served after its listener is gone (14 of 60 runs of the lib tests, 2026-10-27).
#![cfg(unix)]

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use continuum_host::output_handoff::{spawn_reader, Handoff};
use tokio::process::Command;

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
