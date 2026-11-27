//! A node's owner deploys a candidate with `continuum reboot` and reads its live window; the deploy
//! tracker must not put canary back under that window (the M5, 2026-11-27: candidate 4da0f81bfa,
//! deployed and verified 04:15Z, replaced by canary 584a0bf4c at 04:26Z, 11 minutes into its window).
//! The rule: a reboot of a checkout that is not the canary tip holds the tracker from before its handoff
//! (Codex on 17660bf67f), renews at its verified deploy, and whoever measures the window renews it
//! through its end (`continuum deploy-hold renew`) and releases it when done (`... release`); its TTL is
//! the bounded expiry if they never do. A reboot of the canary tip releases only a candidate's hold, and a
//! hold anyone else wrote is never touched. Every change goes through the one hold owner,
//! `deploy_hold_file::change`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use continuum_cli_lifecycle::deploy_hold_file::{change, Change, HoldRead};
use continuum_cli_lifecycle::deploy_tracker::Hold;

/// One measured window, the project's comparison unit (CLAUDE.md, plan rule 3: "compare exact
/// 15-minute windows on real residents"); the measurer renews it for longer reads.
const WINDOW: Duration = Duration::from_secs(15 * 60);

/// What marks a hold as a candidate's, so only a candidate's is renewed or released.
const OWNER: &str = "candidate deployed by `continuum reboot`";

/// Where this reboot builds from, when it is a checkout: its root, its HEAD and origin/canary's tip.
pub(super) struct Checkout {
    head: String,
    tip: String,
}

impl Checkout {
    /// The checkout this reboot runs in. An installed node has none and follows canary: said, not skipped.
    pub(super) fn find() -> Result<Option<Self>, String> {
        let Ok(script) = super::locate_start_script() else {
            println!("▶ deploy hold: no checkout (an installed node follows canary), none taken");
            return Ok(None);
        };
        let root: PathBuf = script.ancestors().nth(3).ok_or("the start script is not inside a checkout")?.to_path_buf();
        Ok(Some(Self { head: rev(&root, "HEAD")?, tip: rev(&root, "origin/canary")? }))
    }

    fn is_tip(&self) -> bool {
        self.head == self.tip
    }
}

/// Before the handoff: a candidate holds the tracker so canary cannot be deployed under it.
pub(super) fn before_handoff(checkout: &Checkout) -> Result<(), String> {
    apply(&checkout.head, checkout.is_tip(), "before its handoff")
}

/// After the verified deploy: the window starts now, so a candidate's hold is renewed from now; the
/// canary tip releases a candidate's hold.
pub(super) fn after_verified_deploy(checkout: &Checkout) -> Result<(), String> {
    apply(&checkout.head, checkout.is_tip(), "at its verified deploy")
}

/// `continuum deploy-hold renew|release`: the measurer extends a candidate's hold by one window from now,
/// or releases it; anyone else's hold is left as it is.
pub(super) fn verb(args: &[String]) -> Result<(), String> {
    let state = state_dir()?;
    let now = now_ms()?;
    let applied = match args.first().map(String::as_str) {
        Some("renew") => change(&state, |current| match current {
            HoldRead::Held(hold) if hold.reason.starts_with(OWNER) => Change::Write(Hold { created_ms: now, ..hold.clone() }),
            _ => Change::Keep,
        }),
        Some("release") => change(&state, |current| match current {
            HoldRead::Held(hold) if hold.reason.starts_with(OWNER) => Change::Remove,
            _ => Change::Keep,
        }),
        _ => return Err("usage: continuum deploy-hold renew|release (a candidate's hold only)".into()),
    }
    .map_err(|e| format!("the deploy hold at {}: {e}", state.display()))?;
    say(&applied, "by its measurer");
    Ok(())
}

fn apply(head: &str, is_tip: bool, when: &str) -> Result<(), String> {
    let state = state_dir()?;
    let now = now_ms()?;
    let applied = change(&state, |current| decide(head, is_tip, current, now)).map_err(|e| format!("the deploy hold at {}: {e}", state.display()))?;
    say(&applied, when);
    Ok(())
}

fn say(applied: &Change, when: &str) {
    match applied {
        Change::Keep => {}
        Change::Remove => println!("▶ deploy hold released {when}: the tracker follows canary again"),
        Change::Write(hold) => println!("▶ deploy hold {when}: {} (for {} min)", hold.reason, WINDOW.as_secs() / 60),
    }
}

/// What a reboot of `head` does to the hold as it is now.
fn decide(head: &str, is_tip: bool, current: &HoldRead, now: u64) -> Change {
    let ours = matches!(current, HoldRead::Held(hold) if hold.reason.starts_with(OWNER));
    if is_tip {
        return if ours { Change::Remove } else { Change::Keep };
    }
    match current {
        HoldRead::Held(hold) if !ours && hold.is_active(now) => {
            println!("▶ deploy hold kept as it is (not a candidate's): {}", hold.reason);
            Change::Keep
        }
        HoldRead::Unreadable(why) => {
            println!("▶ deploy hold left as it is: the file is unreadable ({why})");
            Change::Keep
        }
        _ => Change::Write(Hold { reason: format!("{OWNER}: {head}, for its live window"), created_ms: now, ttl_ms: Some(WINDOW.as_millis() as u64) }),
    }
}

fn rev(root: &Path, what: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", what])
        .output()
        .map_err(|e| format!("reading {what} in {}: {e}", root.display()))?;
    if !out.status.success() {
        return Err(format!("{what} is not known in {}: {}", root.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn state_dir() -> Result<PathBuf, String> {
    Ok(continuum_core::paths::continuum_home()?.join("state"))
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).map_err(|e| format!("the clock is before 1970: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the M5, 2026-11-27; Codex on 17660bf67f): a verified candidate replaced by canary
    // under its window, a candidate overwriting an operator's hold (a legacy text one included), and a
    // canary reboot releasing a hold that is not a candidate's.
    #[test]
    fn a_candidate_holds_its_window_and_never_touches_anyone_elses_hold() {
        let held = match decide("4da0f81bfa", false, &HoldRead::Missing, 1_000) {
            Change::Write(hold) => hold,
            other => panic!("a candidate must hold the tracker, got {other:?}"),
        };
        assert_eq!(held.ttl_ms, Some(WINDOW.as_millis() as u64));
        assert_eq!(decide("f6648967d", true, &HoldRead::Held(held.clone()), 2_000), Change::Remove);
        assert_eq!(decide("f6648967d", true, &HoldRead::Missing, 2_000), Change::Keep);
        let legacy = HoldRead::Held(Hold { reason: "operator: live read".into(), created_ms: 1_000, ttl_ms: None });
        assert_eq!(decide("f6648967d", true, &legacy, 2_000), Change::Keep);
        assert_eq!(decide("4da0f81bfa", false, &legacy, 2_000), Change::Keep);
        assert_eq!(decide("4da0f81bfa", false, &HoldRead::Unreadable("denied".into()), 2_000), Change::Keep);
        // a candidate's own expired hold is renewed by the next candidate reboot
        let expired = HoldRead::Held(Hold { created_ms: 0, ..held });
        assert!(matches!(decide("5628ba5068", false, &expired, 10 * WINDOW.as_millis() as u64), Change::Write(_)));
    }
}
