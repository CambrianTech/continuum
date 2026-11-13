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

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

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
pub fn drain_capped(stderr: tokio::process::ChildStderr, path: PathBuf, pid: Option<u32>, watch: Box<dyn LineWatch>) {
    let clock = start_clock(pid);
    tokio::spawn(async move {
        let result = pump(stderr, &path, watch, Arc::clone(&clock)).await;
        retire_clock(pid, &clock);
        if let Err(error) = result {
            tracing::warn!(
                probe_class = "serving.llama.log_sink_ended",
                path = %path.display(),
                %error,
                "capped stderr sink stopped; the child keeps serving unlogged"
            );
        }
    });
}

/// The engine's own stderr log for the lane on `port`: the one path its spawn drains into, its
/// failure tail is read from, and its stall evidence is written beside.
pub fn llama_server_log(port: u16) -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("logs").join(format!("llama-server-{port}.log")))
}

/// When each child last wrote a line its sink drained (Unix ms), by the child's pid. A fresh
/// clock starts with each sink, so a new engine never inherits a predecessor's timing, and an
/// engine this process does not drain (adopted across a core restart) reads as unknown rather
/// than as some other child's silence (Codex on #5528). The stall evidence reads it before the
/// engine is condemned (card 3913621a).
static LAST_LINE_MS: LazyLock<parking_lot::Mutex<HashMap<u32, Arc<AtomicU64>>>> = LazyLock::new(Default::default);

/// How long child `pid` has written nothing, in ms: since its last line, or since its sink
/// started when it has written none. Kept by this process's own sink of the child, or by its
/// follower of the child's relay while a current heartbeat names it ([`super::stderr_relay::follow`]).
/// `None` when neither holds a clock for `pid`.
pub fn silent_ms(pid: u32) -> Option<u64> {
    let last = LAST_LINE_MS.lock().get(&pid)?.load(Ordering::Relaxed);
    Some(continuum_base::time::EpochMs::now().millis().saturating_sub(last))
}

/// A fresh clock for child `pid`'s sink, replacing any earlier child's that held the number.
pub(super) fn start_clock(pid: Option<u32>) -> Arc<AtomicU64> {
    let clock = Arc::new(AtomicU64::new(continuum_base::time::EpochMs::now().millis()));
    if let Some(pid) = pid {
        LAST_LINE_MS.lock().insert(pid, Arc::clone(&clock));
    }
    clock
}

/// Forget child `pid`'s clock when its sink ends, but only if the map still holds THIS sink's
/// clock: a replacement child that reused the pid keeps its own (Codex on #5528).
pub(super) fn retire_clock(pid: Option<u32>, clock: &Arc<AtomicU64>) {
    let Some(pid) = pid else { return };
    let mut clocks = LAST_LINE_MS.lock();
    if clocks.get(&pid).is_some_and(|held| Arc::ptr_eq(held, clock)) {
        clocks.remove(&pid);
    }
}

/// Lines discarded between attempts to reopen a sink file that could not be written: a full
/// disk is not asked again on every line, and a recovered one is written again soon after.
// derived-or-floor: a FLOOR on retry spacing for a log sink; it bounds syscalls on a failed disk, never serving.
const REOPEN_AFTER_LINES: u64 = 1024;

/// Open `path` to append, with the bytes it already holds. A length that cannot be read counts
/// as full, so the next line rotates: the cap holds even for a file reopened after a failure
/// (Codex on #5532).
async fn open_counted(path: &Path) -> io::Result<(tokio::fs::File, u64)> {
    let file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await?;
    let held = file.metadata().await.map_or(MAX_BYTES, |m| m.len()); // map_or: an unreadable length is treated as full, never as empty
    Ok((file, held))
}

/// Drain `reader` into `path`, rotating whenever the live file would exceed [`MAX_BYTES`].
/// Line-oriented so a rotation never splits a message in half — a half-line at the seam is
/// exactly the kind of "evidence that reads as corruption" that costs an hour later.
/// Returns only when the child's stderr ends (or cannot be read): a sink that cannot be written
/// keeps draining and discards, said once, because a full pipe would block the child (card
/// a6481313). Lines are read as bytes and decoded lossily, so output that is not UTF-8 is
/// logged with replacement characters instead of ending the drain.
pub(crate) async fn pump<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    path: &Path,
    mut watch: Box<dyn LineWatch>,
    last_line: Arc<AtomicU64>,
) -> io::Result<()> {
    let mut reader = BufReader::new(reader);
    let (mut file, mut written) = match open_counted(path).await {
        Ok((file, held)) => (Some(file), held),
        Err(_) => (None, 0),
    };
    let (mut degraded, mut discarded) = (file.is_none(), 0u64);
    if degraded {
        sink_degraded(path, "the log file could not be opened");
    }
    let mut raw = Vec::new();
    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw).await? == 0 {
            break;
        }
        last_line.store(continuum_base::time::EpochMs::now().millis(), Ordering::Relaxed);
        let line = String::from_utf8_lossy(raw.strip_suffix(b"\n").unwrap_or(&raw)); // unwrap_or: the child's last line may end without a newline
        // Observe BEFORE writing: a fault the observer exists to catch must still be
        // caught when the disk is full — which is exactly the state the 2026-08-05 wedge
        // drove the machine into. Detection must not depend on the sink succeeding.
        watch.observe(&line);
        if file.is_none() {
            discarded += 1;
            if discarded % REOPEN_AFTER_LINES != 0 {
                continue;
            }
            match open_counted(path).await {
                Ok((reopened, held)) => (file, written) = (Some(reopened), held),
                Err(_) => continue,
            }
        }
        match write_line(&mut file, path, &line, &mut written).await {
            Ok(()) => {
                if degraded {
                    degraded = false;
                    tracing::warn!(probe_class = "serving.llama.log_sink_recovered", path = %path.display(), discarded,
                        "the log sink is written again after discarding lines");
                    discarded = 0;
                }
            }
            Err(error) => {
                file = None;
                if !degraded {
                    degraded = true;
                    sink_degraded(path, &error.to_string());
                }
            }
        }
    }
    if let Some(mut file) = file {
        let _ = file.flush().await; // the child has ended; a flush that fails loses only its tail
    }
    Ok(())
}

/// Write one line, rotating first when it would pass [`MAX_BYTES`].
async fn write_line(file: &mut Option<tokio::fs::File>, path: &Path, line: &str, written: &mut u64) -> io::Result<()> {
    let bytes = line.len() as u64 + 1;
    if *written + bytes > MAX_BYTES {
        if let Some(mut old) = file.take() {
            old.flush().await?;
        }
        rotate(path).await?;
        let (fresh, held) = open_counted(path).await?;
        *file = Some(fresh);
        *written = held;
    }
    let Some(live) = file.as_mut() else { return Err(io::Error::other("no open log file")) };
    live.write_all(line.as_bytes()).await?;
    live.write_all(b"\n").await?;
    *written += bytes;
    Ok(())
}

fn sink_degraded(path: &Path, why: &str) {
    tracing::warn!(probe_class = "serving.llama.log_sink_degraded", path = %path.display(), why,
        "the log sink cannot be written: the child is still drained and its lines discarded until the file opens again");
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

    /// A shell child writing `lines` stderr lines, each carrying bytes that are not UTF-8.
    #[cfg(unix)]
    fn noisy_child(lines: u32) -> (tokio::process::Child, tokio::process::ChildStderr) {
        let script = format!("i=0; while [ $i -lt {lines} ]; do printf 'line %d \\377\\376 padding padding padding\\n' $i >&2; i=$((i+1)); done");
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("sh runs in the test");
        let stderr = child.stderr.take().expect("piped stderr");
        (child, stderr)
    }

    // what this catches (card a6481313): a sink that stopped reading when its file could not be
    // written, contradicting its own contract. A child writing far past the pipe's buffer into
    // an unwritable sink must still run to completion: nothing blocks it.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_unwritable_sink_keeps_draining_the_child() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("no-such-dir").join("engine.log");
        let (mut child, stderr) = noisy_child(4000);
        let drained = tokio::spawn({
            let path = path.clone();
            async move { pump(stderr, &path, Box::new(()), Arc::new(AtomicU64::new(0))).await }
        });
        let status = tokio::time::timeout(std::time::Duration::from_secs(60), child.wait())
            .await
            .expect("the child is never blocked on a full pipe")
            .expect("the child is reaped");
        assert!(status.success());
        assert!(drained.await.expect("the drain task").is_ok(), "the drain ends when the child does");
        assert!(!path.exists());
    }

    // what this catches (Codex on #5532): a sink reopened after a failure counting the file it
    // found as empty, so repeated reopens grew one file past MAX_BYTES. The directory is missing
    // at first; while the child pauses a near-full log appears there, and the reopen counts it
    // and rotates, so neither generation passes the cap.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_reopened_sink_counts_the_file_it_finds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        let path = logs.join("engine.log");
        let script = "i=0; while [ $i -lt 1000 ]; do echo \"early $i\" >&2; i=$((i+1)); done; sleep 2; i=0; while [ $i -lt 1100 ]; do echo \"late $i\" >&2; i=$((i+1)); done";
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("sh runs in the test");
        let stderr = child.stderr.take().expect("piped stderr");
        let drained = tokio::spawn({
            let path = path.clone();
            async move { pump(stderr, &path, Box::new(()), Arc::new(AtomicU64::new(0))).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        std::fs::create_dir_all(&logs).expect("the log directory appears");
        std::fs::write(&path, vec![b'x'; (MAX_BYTES - 64) as usize]).expect("a near-full log");
        child.wait().await.expect("the child is reaped");
        drained.await.expect("the drain task").expect("the drain ends cleanly");
        let live = std::fs::metadata(&path).expect("the live log").len();
        let rotated = std::fs::metadata(path.with_extension("log.1")).expect("the reopen rotated").len();
        assert!(live <= MAX_BYTES && rotated <= MAX_BYTES, "live {live}, rotated {rotated}, cap {MAX_BYTES}");
    }

    // what this catches (card a6481313): a line that is not UTF-8 ending the drain. It is logged
    // with replacement characters, and the lines after it are logged too.
    #[cfg(unix)]
    #[tokio::test]
    async fn output_that_is_not_utf8_is_logged_lossily_and_draining_continues() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("engine.log");
        let (mut child, stderr) = noisy_child(3);
        pump(stderr, &path, Box::new(()), Arc::new(AtomicU64::new(0))).await.expect("the drain ends cleanly");
        child.wait().await.expect("the child is reaped");
        let logged = std::fs::read_to_string(&path).expect("the log");
        assert_eq!(logged.lines().count(), 3, "{logged}");
        assert!(logged.contains('\u{fffd}') && logged.contains("line 2"), "{logged}");
    }

    // what this catches (Codex on #5528): a new engine inheriting its predecessor's stderr
    // timing. A pid's next sink starts a fresh clock, so an old child's long silence never
    // reads as the new child's.
    #[test]
    fn a_new_sink_starts_its_own_silence_clock() {
        let pid = u32::MAX - 7;
        assert_eq!(silent_ms(pid), None, "no sink for this pid, no clock");
        start_clock(Some(pid)).store(1, Ordering::Relaxed); // the old child: silent since 1970
        assert!(silent_ms(pid).is_some_and(|ms| ms > 1_000_000));
        let old = LAST_LINE_MS.lock().get(&pid).cloned().expect("the old clock");
        let fresh = start_clock(Some(pid));
        assert!(silent_ms(pid).is_some_and(|ms| ms < 60_000), "the new child's clock starts now");
        retire_clock(Some(pid), &old);
        assert!(silent_ms(pid).is_some(), "the old sink ending never removes its replacement's clock");
        retire_clock(Some(pid), &fresh);
        assert_eq!(silent_ms(pid), None, "a sink that ends retires its own clock");
    }

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
        let live = tokio::fs::metadata(&path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
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
