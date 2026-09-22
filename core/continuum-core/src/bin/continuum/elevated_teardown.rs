//! Ending a core this caller has no handle on, from inside `continuum install`.
//!
//! Measured 2026-09-22 on Astra's Windows node: `OpenProcess(25040, TERMINATE)`
//! returned NULL with Win32 error 5, and so did the same call asking only for
//! `QUERY_LIMITED_INFORMATION`. The core runs under the supervisor's S4U principal;
//! the CLI does not. There is no permission to grant to the caller — a better-reported
//! `taskkill` is a better-reported impossibility — so the process can only be ended by
//! a principal that holds the privilege, exactly as the core can only be REGISTERED by
//! one.
//!
//! That boundary already exists in this binary, and this module reuses it rather than
//! inventing a second one: the unelevated half writes a typed plan, asks Windows for
//! ONE consent (`Start-Process -Verb RunAs` on this same binary), carries the plan's
//! SHA-256 on the RunAs argv, and the elevated child re-derives that digest from the
//! bytes it reads before doing the one thing it was consented for. The digest lives on
//! the argv because a same-user process can rewrite a file in `%TEMP%` between the
//! write and the consent, but not the argv of a process already spawned (Fable, review
//! of #4232).
//!
//! WHY A PID IS NOT THE PLAN. A pid is reusable, and an elevated `TerminateProcess` on
//! a number that has been recycled kills something nobody consented to. So the plan
//! names the pid AND the image the unelevated half observed at that pid AND that
//! image's digest, and the elevated child re-observes all three before it terminates
//! anything. That narrows the target to "the process still running the executable we
//! looked at"; it is not a proof against a reuse that also reloads the same image, and
//! this module does not claim to be one.
//!
//! WHO MAY SPEND A CONSENT. A consent prompt needs a human at the machine. The deploy
//! consumer runs under the supervisor, unattended, every ten minutes — escalating from
//! there would hang the tick on a dialog nobody can see until the consent times out. So
//! the privilege is borrowed on exactly one path: `continuum install`, a command a
//! human typed, which already spends one consent for the supervisor arm. Every
//! unattended caller refuses instead, and says what it could not do.
//!
//! There is no `stop --escalate` for an operator to discover. Joel's contract is that
//! the same command works every time (`continuum install`), and a repair verb would be
//! a second thing to know about on the node whose operator does not know it exists.

// Gated with the halves that use them: on a non-Windows build without `test`, this
// module is only `StopOptions` — the argv shape, which every platform parses.
#[cfg(any(windows, test))]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(any(windows, test))]
use serde::{Deserialize, Serialize};

/// What the consent is given for: this process, running this image, whose bytes hash
/// to this digest. All three are re-checked by the elevated child.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TeardownPlan {
    pub pid: i32,
    /// The executable the unelevated half observed at `pid` — or, when it could not
    /// observe one (the usual case: that is WHY we are escalating), the image the
    /// running core reports for itself.
    pub image_path: String,
    /// SHA-256 (hex) of `image_path`'s bytes at plan time.
    pub image_sha256: String,
}

/// `stop`'s options.
///
/// Plain `stop` is unchanged and is the whole user-facing surface. `--elevated --plan
/// --plan-sha` IS the consented child of an escalation and does nothing else — it is
/// not a verb anyone types, it is how this binary re-enters itself through
/// `Start-Process -Verb RunAs`, the same three-flag shape `install --supervisor` uses.
///
/// There is deliberately NO `--escalate` for an operator to find: the user-facing
/// contract is one command, `continuum install`, which reaches this boundary itself
/// when it has to (Joel, 2026-09-22; Astra's review). A separate repair verb would be a
/// second thing to know about, and the node that needs it is the one whose operator
/// does not know it exists.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct StopOptions {
    pub elevated: bool,
    pub plan: Option<String>,
    pub plan_sha: Option<String>,
}

impl StopOptions {
    pub(super) fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--elevated" => options.elevated = true,
                "--plan" => options.plan = args.next(),
                "--plan-sha" => options.plan_sha = args.next(),
                other => return Err(format!("stop: unknown option {other}")),
            }
        }
        // The three go together or none do: a child without its digest could act on any
        // file in %TEMP%, and a digest without a child is a flag with nothing to bind.
        if options.elevated != options.plan.is_some() || options.elevated != options.plan_sha.is_some()
        {
            return Err(
                "stop: --elevated, --plan and --plan-sha go together (the consented child ends \
                 exactly one process, bound by its digest)"
                    .to_string(),
            );
        }
        Ok(options)
    }
}

/// The receipt the elevated child leaves beside the plan. The parent reads THIS, never
/// the child's console — an elevated child's stdout is a window that closes.
#[cfg(windows)]
pub(super) fn receipt_path(plan: &Path) -> PathBuf {
    plan.with_extension("receipt.txt")
}

/// SHA-256 (hex) of some bytes. The same function binds the supervisor plan; a digest
/// is a digest, and having two of them would be the first place they drift.
#[cfg(any(windows, test))]
pub(super) fn digest(bytes: &[u8]) -> String {
    super::supervisor_install::plan_digest(bytes)
}

/// The plan the consent was given for, or a refusal.
///
/// Pure, so the one gate between "a file in `%TEMP%`" and "an elevated terminate" is
/// pinned by a test on every platform, not only where it runs.
#[cfg(windows)]
pub(super) fn read_bound_plan(plan_path: &Path, plan_sha: &str) -> Result<TeardownPlan, String> {
    let bytes = std::fs::read(plan_path)
        .map_err(|e| format!("cannot read the teardown plan {}: {e}", plan_path.display()))?;
    bind_plan_bytes(&bytes, plan_sha, plan_path)
}

/// The digest gate itself, over bytes rather than a path.
#[cfg(any(windows, test))]
pub(super) fn bind_plan_bytes(
    bytes: &[u8],
    plan_sha: &str,
    shown_as: &Path,
) -> Result<TeardownPlan, String> {
    let actual = digest(bytes);
    if !actual.eq_ignore_ascii_case(plan_sha) {
        return Err(format!(
            "the teardown plan at {} is not the one the consent was given for \
             (sha {} on argv, {actual} on disk) — refusing to terminate anything",
            shown_as.display(),
            plan_sha.get(..12).unwrap_or(plan_sha)
        ));
    }
    serde_json::from_slice(bytes).map_err(|e| format!("the plan is not a TeardownPlan: {e}"))
}

/// Is the process the elevated child is looking at the one the consent named?
///
/// Pure over what was OBSERVED, so the rule is testable without a process to kill.
/// Paths compare the way Windows means them — case-insensitively, and `/` spelled `\`
/// is the same separator — because the scheduler and `QueryFullProcessImageName`
/// disagree about spelling far more often than they disagree about the file.
#[cfg(any(windows, test))]
pub(super) fn target_matches(
    plan: &TeardownPlan,
    observed_image: &str,
    observed_sha: &str,
) -> Result<(), String> {
    let norm = |s: &str| s.replace('/', "\\").trim_matches('"').to_ascii_lowercase();
    if norm(&plan.image_path) != norm(observed_image) {
        return Err(format!(
            "pid {} is running {observed_image}, not the {} the consent named — the pid was \
             recycled between the plan and the consent; refusing to terminate it",
            plan.pid, plan.image_path
        ));
    }
    if !plan.image_sha256.eq_ignore_ascii_case(observed_sha) {
        return Err(format!(
            "the image at {} is not the one the consent was given for (sha {} planned, \
             {observed_sha} now) — refusing to terminate pid {}",
            plan.image_path,
            plan.image_sha256.get(..12).unwrap_or(&plan.image_sha256),
            plan.pid
        ));
    }
    Ok(())
}

/// SHA-256 of a file on disk, read in one shot. Images are tens of megabytes; this is
/// not a hot path and it runs at most twice per escalation.
#[cfg(windows)]
pub(super) fn digest_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(digest(&bytes))
}

// ---------------------------------------------------------------------------
// The Windows halves. Everything above is pure and pinned; everything below needs
// the OS that has the problem.
// ---------------------------------------------------------------------------

/// The image `pid` is running, as the OS reports it. Requires a handle — which is
/// precisely what the unelevated caller does not have, so this is the elevated child's
/// half of the check.
#[cfg(windows)]
pub(super) fn observed_image(pid: i32) -> Result<String, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: a query-only handle, closed on every path below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle.is_null() {
        return Err(format!(
            "OpenProcess(pid {pid}, QUERY_LIMITED_INFORMATION) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut buf = [0u16; 32768];
    let mut len = buf.len() as u32;
    // SAFETY: `handle` is live, `buf` outlives the call, `len` is its capacity in
    // UTF-16 units and is updated to the written length.
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };
    // SAFETY: the handle came from OpenProcess above and is closed exactly once.
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return Err(format!(
            "QueryFullProcessImageName(pid {pid}) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

/// A TERMINATE handle, held open. Closed exactly once, on drop.
///
/// The point of holding it rather than re-opening later: between a capability CHECK and
/// the terminate, the check can stop being true. Holding the handle across the drain
/// means the authority is not re-asked for — it is already in hand.
#[cfg(windows)]
struct HeldTerminate {
    pid: i32,
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for HeldTerminate {
    fn drop(&mut self) {
        // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
impl HeldTerminate {
    /// Take the capability. This is the call that fails when the consented token does
    /// not hold the privilege either — and it fails BEFORE anything has been drained.
    fn take(pid: i32) -> Result<Self, String> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};
        // SAFETY: a terminate handle for exactly the consented pid; ownership passes to
        // the returned value, whose Drop closes it.
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid as u32) };
        if handle.is_null() {
            return Err(format!(
                "even elevated, OpenProcess(pid {pid}, TERMINATE) failed: {} — the privilege \
                 this escalation exists to borrow is not held by the consented token either. \
                 Nothing was drained",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { pid, handle })
    }

    /// Spend it.
    fn terminate(self) -> Result<(), String> {
        use windows_sys::Win32::System::Threading::TerminateProcess;
        // SAFETY: `self.handle` is live and was opened for termination.
        let ok = unsafe { TerminateProcess(self.handle, 1) };
        let err = std::io::Error::last_os_error();
        if ok == 0 {
            return Err(format!("TerminateProcess(pid {}) failed: {err}", self.pid));
        }
        Ok(())
    }
}

/// The elevated child, which owns BOTH HALVES OF THE TEARDOWN for its one target.
///
/// The drain and the terminate cannot be split across the consent boundary. If the
/// parent drained and then asked, a declined or failed consent would strand a drained
/// core answering ping — the very thing the preflight exists to prevent, moved later
/// (Astra, 2026-09-22: "declined/failed UAC strands drained core again"). So the order
/// here is: bind the plan, re-observe the target, TAKE the capability and hold it,
/// drain, then spend the capability. Every way this can fail before the drain fails
/// with nothing drained, and after the drain the terminate cannot be refused because
/// the handle is already in hand.
#[cfg(windows)]
pub(super) async fn teardown_elevated(plan_path: &Path, plan_sha: &str) -> Result<(), String> {
    let receipt = receipt_path(plan_path);
    let result = async {
        let plan = read_bound_plan(plan_path, plan_sha)?;
        let image = observed_image(plan.pid)?;
        let sha = digest_file(Path::new(&image))?;
        target_matches(&plan, &image, &sha)?;
        let held = HeldTerminate::take(plan.pid)?;
        // The drain runs under a capability already held, so this is the one place a
        // `MayDrain` is proven by possession rather than by a check.
        let graceful = super::request_graceful_stop(&super::MayDrain::proven_by_held_handle()).await;
        held.terminate()?;
        Ok::<String, String>(format!(
            "elevated teardown: drained ({graceful:?}) and terminated pid {} running {image}",
            plan.pid
        ))
    }
    .await;
    let text = match &result {
        Ok(line) => line.clone(),
        Err(why) => format!("elevated teardown failed: {why}"),
    };
    let _ = std::fs::write(&receipt, text);
    result.map(|_| ())
}

/// The unelevated half: write the plan, ask for ONE consent, read the receipt, and
/// verify by the process being GONE rather than by the child's exit code.
#[cfg(windows)]
pub(super) async fn request_elevated_teardown(
    pid: i32,
    image_path: &str,
    gone: impl Fn(i32) -> bool,
) -> Result<(), String> {
    let plan = TeardownPlan {
        pid,
        image_path: image_path.to_string(),
        image_sha256: digest_file(Path::new(image_path))?,
    };
    let plan_path =
        std::env::temp_dir().join(format!("continuum-teardown-{}.json", std::process::id()));
    let receipt = receipt_path(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    let bytes = serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?;
    let plan_sha = digest(&bytes);
    std::fs::write(&plan_path, &bytes)
        .map_err(|e| format!("install: cannot write the teardown plan: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("install: own path: {e}"))?;
    let quote = |s: String| s.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; $p = Start-Process -FilePath '{}' -ArgumentList \
         @('stop','--elevated','--plan','{}','--plan-sha','{plan_sha}') -Verb RunAs -Wait \
         -PassThru; exit $p.ExitCode",
        quote(exe.display().to_string()),
        quote(plan_path.display().to_string()),
    );
    // The consent prompt waits for a human; ten minutes is the wall past which nobody
    // is there. Same budget the supervisor's one consent gets.
    let elevated =
        super::supervisor_install::powershell(&script, std::time::Duration::from_secs(600)).await;
    // unwrap_or_default: an absent receipt is reported below as "none", never as success.
    let receipt_text = std::fs::read_to_string(&receipt).unwrap_or_default();
    let _ = std::fs::remove_file(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    if let Err(why) = elevated {
        if why.contains("canceled by the user") {
            return Err(format!(
                "install: the elevation consent was refused — pid {pid} is untouched \
                 and still serving"
            ));
        }
        return Err(format!(
            "install: the elevated teardown did not complete: {why}\n  receipt: {}",
            if receipt_text.is_empty() {
                "(none — consent refused or the child never ran)"
            } else {
                receipt_text.trim()
            }
        ));
    }
    if !receipt_text.is_empty() {
        println!("{}", receipt_text.trim());
    }
    // THE PROCESS BEING GONE IS THE PROOF, not the child's exit code — the whole
    // defect this module exists for was a kill path that trusted its own return.
    if !gone(pid) {
        return Err(format!(
            "install: the elevated child reported success but pid {pid} is still \
             running.\n  receipt: {}",
            if receipt_text.is_empty() { "(none)" } else { receipt_text.trim() }
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> TeardownPlan {
        TeardownPlan {
            pid: 25040,
            image_path: "C:\\Users\\a\\.continuum\\bin\\continuum-core-server.exe".to_string(),
            image_sha256: "a".repeat(64),
        }
    }

    // what this catches: a consented child that is not bound to a plan. --elevated
    // without --plan-sha would terminate whatever the file in %TEMP% currently names,
    // which is the entire attack the digest exists to close.
    #[test]
    fn the_consented_child_cannot_be_invoked_unbound() {
        let parse = |a: &[&str]| StopOptions::parse(a.iter().map(|s| s.to_string()));
        assert_eq!(parse(&[]).expect("plain stop"), StopOptions::default());

        let unbound = parse(&["--elevated", "--plan", "p.json"])
            .expect_err("a child without its digest must be refused");
        assert!(unbound.contains("go together"), "{unbound}");
        let stray = parse(&["--plan-sha", "abc"])
            .expect_err("a digest with no child is a flag bound to nothing");
        assert!(stray.contains("go together"), "{stray}");
        let unknown = parse(&["--escalate"])
            .expect_err("there is no operator-facing escalate verb — `continuum install` is the one command");
        assert!(unknown.contains("unknown option"), "{unknown}");

        let child = parse(&["--elevated", "--plan", "p.json", "--plan-sha", "abc"])
            .expect("the bound child parses");
        assert_eq!(child.plan.as_deref(), Some("p.json"));
        assert_eq!(child.plan_sha.as_deref(), Some("abc"));
    }

    // what this catches (Fable, review of #4232, carried to this module): a plan read
    // from %TEMP% that is not the plan the human approved. Any same-user process can
    // rewrite that file between the write and the consent; it cannot rewrite the argv
    // of a process already spawned. Without the digest gate, the consent for "terminate
    // the core" registers as consent for "terminate whatever this file now names".
    #[test]
    fn only_the_bytes_the_consent_was_given_for_can_be_acted_on() {
        let bytes = serde_json::to_vec_pretty(&plan()).expect("a plan serializes");
        let sha = digest(&bytes);
        let bound = bind_plan_bytes(&bytes, &sha, Path::new("plan.json"))
            .expect("the planned bytes bind to their own digest");
        assert_eq!(bound, plan());

        let mut swapped = plan();
        swapped.pid = 4;
        let swapped_bytes = serde_json::to_vec_pretty(&swapped).expect("a plan serializes");
        let refused = bind_plan_bytes(&swapped_bytes, &sha, Path::new("plan.json"))
            .expect_err("a rewritten plan must not bind to the approved digest");
        assert!(
            refused.contains("refusing to terminate anything"),
            "the refusal must say nothing was killed: {refused}"
        );
    }

    // what this catches (Astra, 2026-09-22: "declined/failed UAC strands drained core
    // again"): splitting the drain and the terminate across the consent boundary. Every
    // check the child makes before it takes the capability — the plan digest, the image,
    // the digest of that image — must be able to fail with NOTHING drained, which is only
    // true while they are refusals rather than partial work. This pins that each of them
    // returns an error naming what it refused, so a reordering that drains first would
    // have to delete an assertion rather than merely pass.
    #[test]
    fn every_pre_capability_check_refuses_instead_of_half_acting() {
        let p = plan();
        let bytes = serde_json::to_vec_pretty(&p).expect("a plan serializes");
        for (what, refusal) in [
            (
                "digest",
                bind_plan_bytes(&bytes, &"0".repeat(64), Path::new("plan.json")).unwrap_err(),
            ),
            (
                "image",
                target_matches(&p, "C:\\other.exe", &p.image_sha256).unwrap_err(),
            ),
            (
                "image digest",
                target_matches(&p, &p.image_path, &"c".repeat(64)).unwrap_err(),
            ),
        ] {
            assert!(
                refusal.to_lowercase().contains("refusing"),
                "the {what} check must REFUSE, so nothing has been drained when it fires: {refusal}"
            );
        }
    }

    // what this catches: an elevated terminate on a RECYCLED pid. The number alone is
    // not identity — between writing the plan and the human answering the prompt, the
    // core can exit and the OS can hand 25040 to anything. The elevated child re-reads
    // the image and refuses when it is not the one that was consented for.
    #[test]
    fn a_recycled_pid_is_refused_rather_than_terminated() {
        let p = plan();
        let other = "C:\\Windows\\System32\\notepad.exe";
        let refused = target_matches(&p, other, &p.image_sha256)
            .expect_err("a different image at the same pid must be refused");
        assert!(refused.contains("recycled"), "the reason must name reuse: {refused}");
        assert!(refused.contains(other), "the refusal names what is actually there: {refused}");

        // Same image, different bytes: the file was replaced under us (a staging swap
        // between the plan and the consent). Also refused.
        let restaged = target_matches(&p, &p.image_path, &"b".repeat(64))
            .expect_err("a re-staged image must be refused");
        assert!(
            restaged.contains("not the one the consent was given for"),
            "the reason must name the digest: {restaged}"
        );
    }

    // what this catches: path spelling read as a different file. The scheduler echoes
    // `Execute` as registered while QueryFullProcessImageName answers with the OS's own
    // spelling; treating `C:/x/Core.exe` and `c:\x\core.exe` as different targets would
    // refuse every legitimate teardown on some machines and none of the dangerous ones.
    #[test]
    fn windows_path_spelling_is_not_a_different_target() {
        let p = plan();
        let same_file = "c:/Users/a/.continuum/bin/CONTINUUM-CORE-SERVER.EXE";
        target_matches(&p, same_file, &p.image_sha256)
            .expect("case and separator spelling name the same file on Windows");
    }
}
