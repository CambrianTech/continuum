//! Replace a file with a freshly written sibling: the rename half of write-temp-then-rename.
//!
//! On Windows a rename onto (or of) a file another process holds open fails with a sharing
//! violation (os error 32) or access denied (os error 5), and the usual holder of a file this
//! process JUST wrote is a scanner reading the new temp. It lets go within milliseconds, so the
//! rename succeeds if it is asked again. Measured on the 5090 2026-10-10 15:28Z (job 00431d4d):
//! the engine-training footprint store's single rename failed with os error 32, and the
//! refusal's measured graph, the number the next dispatch sizes itself by, was lost.
//!
//! Every other error, and every error on other platforms, returns at once.

use std::path::Path;
use std::time::Duration;

/// How many times a sharing-violated rename is asked again, and the first wait (doubling).
/// derived-or-floor: a floor; a scanner releases a new file in milliseconds, and the waits
/// sum to ~1.6 s, so a holder that keeps the file longer than that is reported, not waited on.
const RETRIES: u32 = 6;
const FIRST_WAIT: Duration = Duration::from_millis(25);

/// `std::fs::rename(tmp, dest)`, asked again while Windows reports the file is in use.
pub fn replace_file(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    let mut wait = FIRST_WAIT;
    let mut attempt = 0;
    loop {
        match std::fs::rename(tmp, dest) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < RETRIES && in_use(&e) => {
                std::thread::sleep(wait);
                wait *= 2;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Windows' "another process has this file" errors; never true elsewhere.
fn in_use(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(32) | Some(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the replace still replaces (the common case is untouched), and an
    // error that is not "in use" (here, a missing temp) returns at once instead of being
    // retried, so a real failure is reported, never slept on.
    #[test]
    fn a_replace_replaces_and_a_real_error_is_not_retried() {
        let dir = tempfile::tempdir().expect("test: dir");
        let dest = dir.path().join("store.json");
        let tmp = dir.path().join("store.json.tmp");
        std::fs::write(&dest, "old").expect("test: dest");
        std::fs::write(&tmp, "new").expect("test: tmp");
        replace_file(&tmp, &dest).expect("test: replaced");
        assert_eq!(std::fs::read_to_string(&dest).expect("test: read"), "new");
        assert!(!tmp.exists(), "the temp became the file");
        let started = std::time::Instant::now();
        assert!(replace_file(&tmp, &dest).is_err(), "a missing temp is an error");
        assert!(started.elapsed() < FIRST_WAIT, "and it is not retried");
    }
}
