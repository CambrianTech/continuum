//! The deploy consumer's log — the only place its output can land under a scheduler
//! with no stdout — and the stamped notes written to it beside stdout/stderr, including
//! the child-safe handle a bash warm build can run under.

use std::path::{Path, PathBuf};

/// The consumer's log — the only place its output can land, since the scheduled task
/// has no stdout. Set once by `deploy-consume`; read by the warm build to redirect
/// the build's own output there too.
pub(super) static DEPLOY_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The parent's OWN notes: `.append(true)` is the atomic append (FILE_APPEND_DATA
/// positions every write at the end, whoever else is writing); a Rust `writeln!` is
/// fine with it. Only a CHILD needs `open_log_for_child` (Fable, #4233).
fn deploy_log_file() -> Option<std::fs::File> {
    let path = DEPLOY_LOG.get()?;
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

/// A log handle a CHILD can be handed as its stdout/stderr. NOT `.append(true)`:
/// on Windows that opens the handle with FILE_APPEND_DATA and no FILE_WRITE_DATA,
/// and an MSYS bash handed such a handle as its stdout exits 1 before running a
/// line — every unattended deploy on the 5090 died in one second with an EMPTY
/// log, "warm build exited exit code: 1" and nothing else, for a day (2026-09-19).
/// Full write access, positioned at the end: the same append, in a handle every
/// child accepts.
pub(super) fn open_log_for_child(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Seek;
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(std::io::SeekFrom::End(0))?;
    Ok(file)
}

/// Say it on stdout AND in the log, stamped.
pub(super) fn deploy_note(line: &str) {
    println!("{line}");
    record_deploy_note(line);
}

/// Protocol commands reserve stdout for their result; progress remains visible
/// on stderr and in the same deployment log as interactive consumption.
pub(super) fn prepare_note(line: &str) {
    eprintln!("{line}");
    record_deploy_note(line);
}

fn record_deploy_note(line: &str) {
    if let Some(mut f) = deploy_log_file() {
        use std::io::Write;
        let stamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let _ = writeln!(f, "{stamp} {line}");
    }
}
