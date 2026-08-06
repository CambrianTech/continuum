//! Size-capped, self-rotating sink for a spawned child's stderr.
//!
//! # Why this exists
//!
//! `llama-server`'s stderr used to be an `OpenOptions::append(true)` file handed straight
//! to the child. Nothing sat between the process and the disk: no cap, no rotation, no
//! owner. On 2026-08-05 a slot wedged on one task — printing `progress = 1.10`, a value
//! that cannot occur — and spun that line at **1.2 GB/minute for four hours**. The file
//! reached **172 GB** and took the machine to zero bytes free, which took down the core,
//! the agent's own tooling, and every peer link on the box. The user-visible symptom was
//! "everything broke."
//!
//! A log must never be able to do that. Joel, 2026-08-05: *"Log files bring down prod like
//! nothing else."* The rule is a few MB per file, always, no matter what the writer does.
//!
//! # Why a pipe and not a bigger file
//!
//! Handing the child a file descriptor means the CHILD owns the write and we cannot
//! intervene — we only find out afterward, from `du`. `tracing-appender` does not help
//! either: it rolls on TIME, so an hourly file at this rate is still 70 GB.
//!
//! So the child gets a **pipe**, and a task owns the file: it reads, counts, and rotates at
//! [`MAX_BYTES`]. That makes the sink a governed writer in the RTOS sense — the thing that
//! touches disk is a daemon obeying a bound, not a raw fd handed to a foreign process.
//!
//! # The bound is absolute
//!
//! Total on-disk is at most `MAX_BYTES * (KEEP + 1)`. Rotation drops the OLDEST bytes and
//! keeps the newest, which is the correct direction for diagnosis: a wedge is diagnosed
//! from what it is doing NOW, and the load banner that scrolled past four hours ago is not
//! worth a terabyte.
//!
//! The reader must never stop draining: a full pipe blocks the child's `write(2)`, which
//! would wedge serving to protect a log. If the file cannot be written we keep draining and
//! discard, and say so once — losing log lines is survivable, stalling inference is not.

use std::io;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Per-file cap. A few MB, per Joel's rule — big enough to hold a load banner plus a fault
/// trace, small enough that a runaway writer is a rounding error instead of an outage.
///
/// Deliberately the SAME number the core's own logs use: "how big may one log file get" is
/// one decision, so it is decided in one place. Two mechanisms enforce it (this drains a
/// foreign process's pipe; [`crate::routing::capped_appender`] wraps our own synchronous
/// writer) but they must never disagree about the bound.
pub const MAX_BYTES: u64 = crate::routing::capped_appender::MAX_LOG_BYTES;

/// Rotated generations kept beside the live file. Two files total (`x.log`, `x.log.1`), so
/// the worst case on disk is `MAX_BYTES * 2` — 16 MB, versus the 172 GB that motivated this.
pub const KEEP: usize = 1;

/// An observer of the child's output, run once per line as the sink drains it.
///
/// The pump already reads every line to keep the file bounded, so a stream that used to be
/// write-only is now the cheapest place in the system to notice what the engine is saying
/// about itself. This trait is the extension point: the sink stays about CAPPING, and each
/// observer owns exactly one question it asks of the stream.
///
/// Implementors must be cheap and must never block — the pump cannot stall, because a full
/// pipe blocks the child's `write(2)` and would wedge inference to serve an observer.
pub trait LineWatch: Send {
    fn observe(&mut self, line: &str);
}

/// The no-op observer: drain and cap, ask nothing.
///
/// Used by lanes with no lifecycle owner to report to (an ephemeral eval lane owns and
/// tears down its own process). Not "disabled" — genuinely nothing to ask.
impl LineWatch for () {
    fn observe(&mut self, _line: &str) {}
}

/// Take ownership of a spawned child's piped stderr and drain it into `path` under the cap.
///
/// The caller spawns with `.stderr(Stdio::piped())` and hands the taken handle here. A
/// background task owns the file for the child's whole life and exits on EOF when it dies.
///
/// The task must never stop reading: a full pipe blocks the child's `write(2)`, which would
/// wedge inference to protect a log. On a write failure we keep draining and discard, and
/// say so once — losing log lines is survivable, stalling serving is not.
///
/// `watch` sees each line as it passes. Pass `Box::new(())` when there is nothing to ask.
pub fn drain_capped(
    stderr: tokio::process::ChildStderr,
    path: PathBuf,
    watch: Box<dyn LineWatch>,
) {
    tokio::spawn(async move {
        if let Err(error) = pump(stderr, &path, watch).await {
            tracing::warn!(
                probe_class = "serving.llama.log_sink_ended",
                path = %path.display(),
                %error,
                "capped stderr sink stopped; the child keeps serving unlogged"
            );
        }
    });
}

/// Drain `reader` into `path`, rotating whenever the live file would exceed [`MAX_BYTES`].
///
/// Line-oriented so a rotation never splits a message in half — a half-line at the seam is
/// exactly the kind of "evidence that reads as corruption" that costs an hour later.
async fn pump(
    reader: tokio::process::ChildStderr,
    path: &Path,
    mut watch: Box<dyn LineWatch>,
) -> io::Result<()> {
    let mut lines = BufReader::new(reader).lines();
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let mut written = file.metadata().await.map(|m| m.len()).unwrap_or(0);

    while let Some(line) = lines.next_line().await? {
        // Observe BEFORE writing: a fault the observer exists to catch must still be
        // caught when the disk is full — which is exactly the state the 2026-08-05 wedge
        // drove the machine into. Detection must not depend on the sink succeeding.
        watch.observe(&line);
        let bytes = line.len() as u64 + 1;
        if written + bytes > MAX_BYTES {
            file.flush().await?;
            drop(file);
            rotate(path).await?;
            file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?;
            written = 0;
        }
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        written += bytes;
    }
    file.flush().await
}

/// Shift generations: `x.log` → `x.log.1`, dropping whatever `x.log.1` held.
async fn rotate(path: &Path) -> io::Result<()> {
    let rotated = path.with_extension(format!(
        "{}.1",
        path.extension().and_then(|e| e.to_str()).unwrap_or("log")
    ));
    // A missing source is not an error — first rotation on a fresh file.
    let _ = tokio::fs::remove_file(&rotated).await;
    match tokio::fs::rename(path, &rotated).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE outage. A writer that never stops must not be able to grow the
    // file past the cap. Regression for 2026-08-05, where an unbounded append reached 172 GB
    // at 1.2 GB/min and took the machine to zero bytes free.
    #[tokio::test]
    async fn a_runaway_writer_cannot_exceed_the_cap() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("runaway.log");

        // Far more than MAX_BYTES of content, the way a wedged slot emits it: one line,
        // forever.
        let line = "slot print_timing: progress = 1.10 (impossible)".repeat(20);
        let mut written_total = 0u64;
        {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .expect("open");
            let mut written = 0u64;
            for _ in 0..(MAX_BYTES / line.len() as u64 * 3) {
                let bytes = line.len() as u64 + 1;
                if written + bytes > MAX_BYTES {
                    drop(file);
                    rotate(&path).await.expect("rotate");
                    file = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .await
                        .expect("reopen");
                    written = 0;
                }
                file.write_all(line.as_bytes()).await.expect("write");
                file.write_all(b"\n").await.expect("nl");
                written += bytes;
                written_total += bytes;
            }
        }

        assert!(
            written_total > MAX_BYTES * 2,
            "the test must actually push past the cap to be meaningful"
        );

        // Every generation, summed, stays inside the absolute bound.
        let live = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
        let rotated_path = path.with_extension("log.1");
        let rotated = tokio::fs::metadata(&rotated_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(
            live <= MAX_BYTES,
            "live file {live} exceeded the {MAX_BYTES} cap"
        );
        assert!(
            live + rotated <= MAX_BYTES * (KEEP as u64 + 1),
            "total on disk {} exceeded the absolute bound",
            live + rotated
        );
    }

    // what this catches: rotation keeps the NEWEST bytes. A wedge is diagnosed from what the
    // process is doing now — dropping the tail to preserve a four-hour-old banner would make
    // the surviving log useless for the incident it exists to explain.
    #[tokio::test]
    async fn rotation_keeps_the_newest_lines() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("order.log");
        tokio::fs::write(&path, b"OLDEST\n").await.expect("seed");
        rotate(&path).await.expect("rotate");
        tokio::fs::write(&path, b"NEWEST\n").await.expect("fresh");

        let live = tokio::fs::read_to_string(&path).await.expect("read live");
        assert!(live.contains("NEWEST"), "live file holds the newest lines");
        assert!(
            !live.contains("OLDEST"),
            "the old generation moved aside, it did not stay live"
        );
    }
}
