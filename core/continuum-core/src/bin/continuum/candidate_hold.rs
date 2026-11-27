//! A node's owner deploys a candidate with `continuum reboot` and reads its live window; the deploy
//! tracker must not put canary back under that window (the M5, 2026-11-27: candidate 4da0f81bfa,
//! deployed and verified 04:15Z, replaced by canary 584a0bf4c at 04:26Z, 11 minutes into its window).
//! The one owner of that rule: a reboot onto a build that is not the canary tip holds the tracker for
//! one window, and a reboot onto the canary tip releases the hold a candidate took. A hold written by
//! anyone else is never touched.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use continuum_cli_lifecycle::deploy_tracker::Hold;

/// One measured window, the project's comparison unit (CLAUDE.md, plan rule 3: "compare exact
/// 15-minute windows on real residents"). A longer read is another reboot, or a hold by hand.
const WINDOW: Duration = Duration::from_secs(15 * 60);

/// What marks a hold as this module's, so only its own is released.
const OWNER: &str = "candidate deployed by `continuum reboot`";

/// After a verified deploy of `deployed` built from the checkout at `repo`: hold the tracker when it is
/// a candidate, release a candidate's hold when it is the canary tip. The outcome is printed; a hold
/// that cannot be written is an error, since without it the window is silently cut short.
pub(super) fn after_verified_deploy(repo: &Path, deployed: &str) -> Result<(), String> {
    let state = continuum_core::paths::continuum_home()?.join("state");
    let path = state.join("deploy-hold");
    let tip = canary_tip(repo)?;
    match decide(deployed, &tip, read(&path).as_ref(), now_ms()?) {
        Act::Nothing => Ok(()),
        Act::Release => {
            std::fs::remove_file(&path).map_err(|e| format!("releasing the candidate's deploy hold at {}: {e}", path.display()))?;
            println!("▶ deploy hold released: this build is the canary tip, the tracker follows canary again");
            Ok(())
        }
        Act::KeepOthers(reason) => {
            println!("▶ deploy hold kept as it is (not a candidate's): {reason}");
            Ok(())
        }
        Act::Hold(hold) => {
            let text = serde_json::to_string(&hold).map_err(|e| format!("encoding the deploy hold: {e}"))?;
            std::fs::create_dir_all(&state).map_err(|e| format!("creating {}: {e}", state.display()))?;
            std::fs::write(&path, text).map_err(|e| format!("writing the deploy hold at {}: {e}", path.display()))?;
            println!("▶ deploy hold: candidate {deployed} is not the canary tip ({}); the tracker leaves it for {} min", &tip[..tip.len().min(10)], WINDOW.as_secs() / 60);
            Ok(())
        }
    }
}

/// What a verified deploy does to the hold.
#[derive(Debug, PartialEq)]
enum Act {
    Nothing,
    Release,
    KeepOthers(String),
    Hold(Hold),
}

fn decide(deployed: &str, tip: &str, current: Option<&Hold>, now: u64) -> Act {
    let ours = current.is_some_and(|hold| hold.reason.starts_with(OWNER));
    if tip.starts_with(deployed) || deployed.starts_with(tip) {
        return if ours { Act::Release } else { Act::Nothing };
    }
    if let Some(other) = current.filter(|hold| !hold.reason.starts_with(OWNER) && hold.is_active(now)) {
        return Act::KeepOthers(other.reason.clone());
    }
    Act::Hold(Hold { reason: format!("{OWNER}: {deployed}, for its live window"), created_ms: now, ttl_ms: Some(WINDOW.as_millis() as u64) })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the M5, 2026-11-27): a verified candidate replaced by canary 11 minutes into
    // its window, and a hold someone else wrote being overwritten or released by a reboot.
    #[test]
    fn a_candidate_holds_its_window_and_canary_releases_only_its_own_hold() {
        let tip = "f6648967d7657ba8ed34e32a4a7584fcf3cdccdd";
        let held = match decide("4da0f81bfa", tip, None, 1_000) {
            Act::Hold(hold) => hold,
            other => panic!("a candidate must hold the tracker, got {other:?}"),
        };
        assert_eq!(held.ttl_ms, Some(WINDOW.as_millis() as u64));
        assert_eq!(decide("f6648967d", tip, Some(&held), 2_000), Act::Release);
        assert_eq!(decide("f6648967d", tip, None, 2_000), Act::Nothing);
        let operator = Hold { reason: "operator: live read".into(), created_ms: 1_000, ttl_ms: Some(60_000) };
        assert_eq!(decide("f6648967d", tip, Some(&operator), 2_000), Act::Nothing);
        assert_eq!(decide("4da0f81bfa", tip, Some(&operator), 2_000), Act::KeepOthers("operator: live read".into()));
    }
}

fn read(path: &Path) -> Option<Hold> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn canary_tip(repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "origin/canary"])
        .output()
        .map_err(|e| format!("reading origin/canary in {}: {e}", repo.display()))?;
    if !out.status.success() {
        return Err(format!("origin/canary is not known in {}: {}", repo.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).map_err(|e| format!("the clock is before 1970: {e}"))
}
