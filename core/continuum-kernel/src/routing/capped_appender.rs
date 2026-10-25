//! A log writer that cannot exceed a size bound, whatever the process does.
//!
//! # Why time-based rotation is not a bound
//!
//! `tracing_appender::rolling` rotates on a CLOCK: daily, hourly, minutely. Combined with
//! `max_log_files` that reads like a disk bound, and the code that used to configure it
//! said so in as many words — *"rotation daily, retention 7 files, disk usage stays
//! bounded."* It is not bounded. It is `7 × (whatever one day produces)`, and nothing
//! anywhere constrains the second factor.
//!
//! On 2026-08-05 the second factor was **1.2 GB per minute**: a serving slot wedged and
//! spun one line forever. The llama-server log reached 172 GB and took the machine to zero
//! bytes free — core dead, git unwritable, every peer link down. Hourly rotation would have
//! produced 70 GB files. The retention count was never the problem; the missing per-file
//! cap was. Joel, that day: *"Log files should never exceed a few mb each. Log files bring
//! down prod like nothing else."*
//!
//! Even absent a fault, the same gap was quietly visible: the core's own daily logs were
//! running 87–175 MB each, ~600 MB across a week, against a stated rule of a few MB.
//!
//! # What this does
//!
//! Rotate on SIZE. `x.log` → `x.log.1` → … → `x.log.N`, dropping the oldest. Total on disk
//! is at most `MAX_LOG_BYTES * (KEEP + 1)` — an arithmetic ceiling, not a hope about write
//! rate. The newest bytes always survive, because a fault is diagnosed from what the
//! process is doing NOW.
//!
//! # Relationship to [`crate::inference::child_log`]
//!
//! Same rule, two mechanisms, because the writers differ. `child_log` drains a FOREIGN
//! process's pipe asynchronously (we cannot make llama-server obey a bound, so we take the
//! pen away from it). This caps our OWN synchronous `fmt` layer. They share
//! [`MAX_LOG_BYTES`] so the bound is decided in exactly one place.
//!
//! # Failure posture
//!
//! A write that cannot land is reported to the caller, but a failed ROTATION never stops
//! logging: we keep writing to the current file and let the next attempt retry. Losing the
//! rotation is survivable; a logger that panics or wedges the process it is describing is
//! not.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The one place the per-log-file bound is decided.
///
/// "A few MB" per Joel's rule — large enough to hold a boot banner plus a fault trace,
/// small enough that a runaway writer is a rounding error instead of an outage.
pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// Rotated generations kept beside the live file for the core's own log.
///
/// More than [`crate::inference::child_log::KEEP`] because this is the substrate's primary
/// diagnostic record and an incident is usually reconstructed from more than the last few
/// minutes. Worst case on disk is `MAX_LOG_BYTES * (KEEP + 1)` = 72 MB — versus the ~600 MB
/// the unbounded daily scheme had already accumulated, with no ceiling above it.
pub const KEEP: usize = 8;

/// The rotation ceiling for ONE such directory, in bytes — the same
/// `MAX_LOG_BYTES * (KEEP + 1)` arithmetic the docs above describe,
/// derived rather than restated so the two can't drift.
///
/// This is what [`RotationLogPool`](crate::system_resources::RotationLogPool)
/// registers as its default budget with the `PressureBroker`. Rotation
/// enforcing this locally is NOT the same as the substrate governing
/// it: the writer's cap is a ceiling it maintains for itself, while the
/// pool's budget is a number an authority can LOWER, at which point the
/// broker actually reclaims generations. Same number at rest, different
/// powers under pressure — which is the whole reason the class needed an
/// owner instead of a constant.
pub fn rotation_budget_bytes() -> u64 {
    MAX_LOG_BYTES * (KEEP as u64 + 1)
}

/// An `io::Write` that rotates its file whenever the next write would cross the cap.
///
/// Designed to sit inside `tracing_appender::non_blocking`, which takes any
/// `W: Write + Send + 'static` — so the write path stays off the caller's thread exactly as
/// before, and only the bound changes.
pub struct CappedAppender {
    path: PathBuf,
    file: Option<File>,
    written: u64,
    max_bytes: u64,
    keep: usize,
}

impl CappedAppender {
    /// Append to `dir/file_name`, rotating at [`MAX_LOG_BYTES`] and keeping [`KEEP`].
    ///
    /// Opening eagerly means a bad path fails at INSTALL, where the operator can see it,
    /// instead of silently swallowing every log line thereafter.
    pub fn new(dir: &Path, file_name: &str) -> io::Result<Self> {
        Self::with_limits(dir, file_name, MAX_LOG_BYTES, KEEP)
    }

    pub fn with_limits(
        dir: &Path,
        file_name: &str,
        max_bytes: u64,
        keep: usize,
    ) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(file_name);
        keep_tail_of_oversized(&path, max_bytes)?;
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        // Adopt whatever is already on disk: a restart must not reset the accounting and
        // let the file grow by another full cap per boot.
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file: Some(file),
            written,
            max_bytes,
            keep,
        })
    }

    /// Shift generations: `.N-1` → `.N`, …, `.1` → `.2`, live → `.1`, oldest dropped.
    fn rotate(&mut self) -> io::Result<()> {
        // Drop the file handle first: on Windows a rename over an open handle fails, and
        // the platform that would have caught it is the one nobody runs.
        self.file = None;

        for generation in (1..=self.keep).rev() {
            let from = if generation == 1 {
                self.path.clone()
            } else {
                self.generation_path(generation - 1)
            };
            let to = self.generation_path(generation);
            if generation == self.keep {
                // The oldest generation is about to be overwritten — remove it so the
                // rename cannot fail on an existing target.
                let _ = std::fs::remove_file(&to);
            }
            match std::fs::rename(&from, &to) {
                Ok(()) => {}
                // A missing source is normal before the ring has filled.
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?,
        );
        self.written = 0;
        Ok(())
    }

    fn generation_path(&self, generation: usize) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{generation}"));
        self.path.with_file_name(name)
    }
}

/// A file already over `max_bytes` when adopted (left by a build that wrote it without a bound)
/// keeps only its newest `max_bytes`, from the first line boundary in them. Rotating it whole would
/// park the oversized file as a generation until the live file next filled, so the bound the
/// writer advertises would not hold (BigMama on #4951: a 3.0 GB workspace trace on the 5090 at
/// 98% would have stayed as `.1` for days). The tail is what the rotation keeps anyway: the
/// newest bytes. Bytes before it are dropped once.
fn keep_tail_of_oversized(path: &Path, max_bytes: u64) -> io::Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let len = match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if len <= max_bytes {
        return Ok(());
    }
    let mut tail = Vec::with_capacity(max_bytes as usize);
    {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(len - max_bytes))?;
        file.take(max_bytes).read_to_end(&mut tail)?;
    }
    // start at a line boundary: a partial first line is not a record
    let start = tail.iter().position(|&b| b == b'\n').map_or(tail.len(), |i| i + 1);
    let mut staged = path.as_os_str().to_os_string();
    staged.push(".tail");
    let staged = PathBuf::from(staged);
    std::fs::write(&staged, &tail[start..])?;
    // the shared replace, with its bounded retries on a Windows sharing violation (a reader such
    // as a tail or an indexer holding the file): a failed rename would leave the bound undone
    continuum_host::durable::replace(&staged, path)
}

impl Write for CappedAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + buf.len() as u64 > self.max_bytes {
            // A failed rotation must not stop logging — keep the current file and let the
            // next write try again. The cap can be briefly exceeded; going silent cannot be
            // undone.
            let _ = self.rotate();
        }
        let Some(file) = self.file.as_mut() else {
            // Rotation failed to reopen. Report it rather than pretending the bytes landed.
            return Err(io::Error::other("capped appender has no open log file"));
        };
        let n = file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE outage class, at the core's own log. A writer that never stops
    // must not be able to grow the directory without bound. Regression for 2026-08-05, where
    // an uncapped log reached 172 GB — and for the daily core logs that were already running
    // 87–175 MB each under a comment claiming disk usage was bounded.
    #[test]
    fn a_runaway_writer_cannot_exceed_the_total_bound() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let max = 1024u64;
        let keep = 3usize;
        let mut appender =
            CappedAppender::with_limits(dir.path(), "runaway.log", max, keep).expect("open");

        let line = vec![b'x'; 100];
        for _ in 0..500 {
            appender.write_all(&line).expect("write");
        }
        appender.flush().expect("flush");

        // 50,000 bytes offered; the ceiling is (keep + 1) generations.
        let total: u64 = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        let ceiling = max * (keep as u64 + 1);
        assert!(
            total <= ceiling,
            "total on disk {total} exceeded the {ceiling} ceiling"
        );
        // And the bound is meaningful — the test really did push far past it.
        assert!(
            total < 50_000,
            "the cap must actually discard, not accumulate"
        );
    }

    // what this catches: rotation keeps the NEWEST bytes. An incident is diagnosed from what
    // the process is doing now; preserving the oldest generation and dropping the tail would
    // make the surviving log useless for the fault it exists to explain.
    #[test]
    fn the_live_file_holds_the_newest_bytes() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut appender =
            CappedAppender::with_limits(dir.path(), "order.log", 64, 2).expect("open");
        appender.write_all(&vec![b'o'; 60]).expect("oldest");
        appender.write_all(b"NEWEST").expect("newest");
        appender.flush().expect("flush");

        let live = std::fs::read_to_string(dir.path().join("order.log")).expect("read live");
        assert!(live.contains("NEWEST"), "live file holds the newest bytes");
        assert!(
            !live.contains(&"o".repeat(60)),
            "the old generation moved aside, it did not stay live"
        );
    }

    // what this catches (BigMama on #4951; the 5090 2026-10-11, a 3.0 GB workspace trace with C:
    // at 98%): a file already over the cap when adopted. Rotating it whole parks it as `.1` until
    // the live file next fills, so the bound does not hold for days. It keeps only its newest
    // bytes, from a line boundary, so every surviving line is whole.
    #[test]
    fn an_oversized_file_keeps_only_its_newest_whole_lines_when_adopted() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("legacy.jsonl");
        let lines: String = (0..200).map(|i| format!("{{\"n\":{i}}}\n")).collect();
        std::fs::write(&path, &lines).expect("seed");
        let appender = CappedAppender::with_limits(dir.path(), "legacy.jsonl", 256, 1).expect("open");
        let kept = std::fs::read_to_string(&path).expect("read");
        assert!(kept.len() <= 256, "within the cap: {} bytes", kept.len());
        assert!(kept.ends_with("{\"n\":199}\n"), "the newest line survives");
        assert!(kept.lines().all(|l| l.starts_with("{\"n\":") && l.ends_with('}')), "every line whole: {kept}");
        assert_eq!(appender.written, kept.len() as u64, "accounting adopts what was kept");
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 1, "no generation and no staging file left behind");
    }

    // what this catches: a restart must not reset the byte accounting. Re-opening with a
    // fresh counter would let the file grow by a full cap on EVERY boot — a slow version of
    // the same unbounded growth, and one that only shows up after many restarts.
    #[test]
    fn reopening_adopts_the_existing_file_size() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("resume.log"), vec![b'a'; 900]).expect("seed");

        let appender =
            CappedAppender::with_limits(dir.path(), "resume.log", 1024, 2).expect("open");
        assert_eq!(
            appender.written, 900,
            "a restart must inherit what is already on disk, not start from zero"
        );
    }
}
