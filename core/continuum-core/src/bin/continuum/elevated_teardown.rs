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
//! names the pid AND the installation directory, and the child — holding ONE handle
//! opened for both query and terminate — proves through that handle that the process is
//! a core of ours living there before it ends anything. That narrows the target to "a
//! continuum core in the directory we installed into"; it is not a proof against a
//! reuse that also happens to be one, and this module does not claim to be one.
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

/// What the consent is given for: this process, if it is a core of ours running out of
/// this installation directory. Both re-checked by the elevated child, through the
/// handle it will terminate with.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TeardownPlan {
    pub pid: i32,
    /// The directory the installed core lives in, from the `ContinuumCore` descriptor.
    /// A DIRECTORY and not a file, because the unelevated half cannot see which image
    /// the process is actually executing — see `target_is_our_core`.
    pub install_dir: String,
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

/// Is the process this handle names one of OUR cores?
///
/// WHY THIS IS NOT AN EXPECTED-IMAGE EQUALITY, which is what it was until Astra found
/// the case that breaks it. The unelevated half cannot open the process — that is the
/// whole reason it is escalating — so any image it puts in the plan is a GUESS, taken
/// from the `ContinuumCore` descriptor's `artifact` field. On the node this rail exists
/// for, that guess is wrong in a specific and predictable way: the descriptor names
/// `service-b/core.exe` (the new 108b slot) while pid 25040 is executing the RENAMED
/// predecessor. Gating on equality with a guess does not fail closed, it fails ALWAYS,
/// and precisely on the broken state we are trying to repair.
///
/// So the plan carries what the planner can actually know — the installation directory,
/// a stable fact from the same descriptor — and the child proves, through the handle it
/// will terminate with, that the running image is a core of ours living there. A pid
/// recycled into `notepad.exe` fails the directory; a pid recycled into `llama-server`
/// (which DOES live in that directory) fails the name. The exact image and its digest
/// are then reported in the receipt, so what was killed is recorded rather than assumed.
///
/// Pure over what was OBSERVED, so the rule is testable without a process to kill.
/// Paths compare the way Windows means them — case-insensitively, and `/` spelled `\`
/// is the same separator — because the scheduler and `QueryFullProcessImageName`
/// disagree about spelling far more often than they disagree about the file.
#[cfg(any(windows, test))]
pub(super) fn target_is_our_core(plan: &TeardownPlan, observed_image: &str) -> Result<(), String> {
    let norm = |s: &str| s.replace('/', "\\").trim_matches('"').to_ascii_lowercase();
    let root = norm(&plan.install_dir);
    let root = root.trim_end_matches('\\').to_string();
    let image = norm(observed_image);
    if !image.starts_with(&format!("{root}\\")) {
        return Err(format!(
            "pid {} is running {observed_image}, which is not under the installation \
             directory {} the consent named — the pid was recycled; refusing to terminate it",
            plan.pid, plan.install_dir
        ));
    }
    let name = image.rsplit('\\').next().unwrap_or(&image);
    if !CORE_IMAGE_NAMES.contains(&name) {
        return Err(format!(
            "pid {} is running {observed_image}, which is in the installation directory but \
             is not a core image ({}) — refusing to terminate it",
            plan.pid,
            CORE_IMAGE_NAMES.join(", ")
        ));
    }
    Ok(())
}

/// The file names a continuum core can legitimately be executing from. `.prev.exe` is
/// here deliberately: a core that outlived its own deploy is running from the parking
/// space, and that is the single most likely state at the moment this rail is used.
#[cfg(any(windows, test))]
const CORE_IMAGE_NAMES: &[&str] = &[
    "core.exe",
    "core.prev.exe",
    "continuum-core-server.exe",
    "continuum-core-server.prev.exe",
];

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

/// ONE handle, opened for both QUERY and TERMINATE, held open across everything.
/// Closed exactly once, on drop.
///
/// A HANDLE NAMES A PROCESS; A PID NAMES A SLOT. This first opened a query handle, read
/// the image, closed it, and re-opened for terminate — so the identity that was
/// validated and the process that would be killed were two separate lookups of a
/// recyclable number with nothing tying them together (Astra, review of 41a7dc2f5:
/// "token alone does not bind target"). Validating on the SAME handle that does the
/// terminating is what makes the check mean anything.
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
    /// Take the capability, BEFORE reading anything about the target. This is the call
    /// that fails when the consented token does not hold the privilege either — and it
    /// fails before anything has been drained. Opening a handle terminates nothing; what
    /// it buys is that every check after it describes the process THIS handle names.
    fn take(pid: i32) -> Result<Self, String> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };
        // SAFETY: ONE handle for exactly the consented pid, opened for the query the
        // validation needs AND the terminate that follows it; ownership passes to the
        // returned value, whose Drop closes it.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                0,
                pid as u32,
            )
        };
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

    /// The image THIS HANDLE's process is running. Read through the handle, never by
    /// pid: a second lookup by number could answer for a different process wearing the
    /// same number, which is exactly why the handle is taken first.
    fn image(&self) -> Result<String, String> {
        use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;
        let mut buf = [0u16; 32768];
        let mut len = buf.len() as u32;
        // SAFETY: `self.handle` is live and was opened with QUERY_LIMITED_INFORMATION;
        // `buf` outlives the call and `len` carries its capacity in UTF-16 units.
        let ok =
            unsafe { QueryFullProcessImageNameW(self.handle, 0, buf.as_mut_ptr(), &mut len) };
        if ok == 0 {
            return Err(format!(
                "QueryFullProcessImageName(pid {}) failed: {}",
                self.pid,
                std::io::Error::last_os_error()
            ));
        }
        Ok(String::from_utf16_lossy(&buf[..len as usize]))
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
/// here is: bind the plan, TAKE the capability and hold it, observe the target THROUGH
/// that same handle, drain, then spend the capability.
///
/// WHAT THAT DOES AND DOES NOT BUY. Every way this can fail before the drain fails with
/// nothing drained — that part is a guarantee. After the drain, what has been removed is
/// the ACQUISITION failure: the terminate no longer needs a fresh `OpenProcess` that
/// could be denied, time out, or find the pid recycled. `TerminateProcess` itself can
/// still return failure and the code reports it (Astra, review of 4936aac55: "syscall
/// still returns failure"); a held handle narrows the window, it does not abolish it.
#[cfg(windows)]
pub(super) async fn teardown_elevated(plan_path: &Path, plan_sha: &str) -> Result<(), String> {
    let receipt = receipt_path(plan_path);
    let result = async {
        let plan = read_bound_plan(plan_path, plan_sha)?;
        // THE HANDLE COMES FIRST, and everything after it is read THROUGH it. Opening a
        // handle kills nothing; what it buys is that the pid stops being a name two
        // different processes could answer to between the check and the act.
        let held = HeldTerminate::take(plan.pid)?;
        let image = held.image()?;
        target_is_our_core(&plan, &image)?;
        // The digest is RECORDED, not compared: nobody could have known it in advance
        // (see `target_is_our_core`), but the receipt should say exactly what was ended.
        let sha = digest_file(Path::new(&image)).unwrap_or_else(|e| format!("unreadable ({e})"));
        // The drain runs under a capability already held, so this is the one place a
        // `MayDrain` is proven by possession rather than by a check.
        let graceful = super::request_graceful_stop(&super::MayDrain::proven_by_held_handle()).await;
        held.terminate()?;
        Ok::<String, String>(format!(
            "elevated teardown: drained ({graceful:?}) and terminated pid {} running {image} (sha256 {sha})",
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

/// The unelevated half: write the plan, ask for ONE consent, read the receipt. Whether
/// the process actually WENT is proven by the caller on the same bounded deadline the
/// ordinary teardown uses — an immediate liveness probe after a terminate sees a process
/// mid-exit, which is the mistake this file already made once (Astra, 2026-09-22).
#[cfg(windows)]
pub(super) async fn request_elevated_teardown(pid: i32, install_dir: &str) -> Result<(), String> {
    let plan = TeardownPlan {
        pid,
        install_dir: install_dir.to_string(),
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
    // THE PROCESS BEING GONE IS THE PROOF, not the child's exit code — the whole defect
    // this module exists for was a kill path that trusted its own return. The caller
    // proves it on the bounded deadline; an immediate probe here would see a process
    // that is exiting perfectly normally and call it a survivor.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> TeardownPlan {
        TeardownPlan {
            pid: 25040,
            install_dir: "C:\\Users\\a\\.continuum\\bin".to_string(),
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
    // check the child makes before the drain — the plan digest, and the identity of the
    // process behind the handle — must be able to fail with NOTHING drained, which is
    // only true while they are refusals rather than partial work. This pins that each
    // returns an error naming what it refused, so a reordering that drains first would
    // have to delete an assertion rather than merely pass.
    #[test]
    fn every_pre_capability_check_refuses_instead_of_half_acting() {
        let p = plan();
        let bytes = serde_json::to_vec_pretty(&p).expect("a plan serializes");
        for (what, refusal) in [
            (
                "plan digest",
                bind_plan_bytes(&bytes, &"0".repeat(64), Path::new("plan.json")).unwrap_err(),
            ),
            (
                "outside the installation",
                target_is_our_core(&p, "C:\\Windows\\System32\\notepad.exe").unwrap_err(),
            ),
            (
                "inside but not a core",
                target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\llama-server.exe")
                    .unwrap_err(),
            ),
        ] {
            assert!(
                refusal.to_lowercase().contains("refusing"),
                "the {what} check must REFUSE, so nothing has been drained when it fires: {refusal}"
            );
        }
    }

    // what this catches: an elevated terminate on a RECYCLED pid. The number alone is not
    // identity — between writing the plan and the human answering the prompt, the core
    // can exit and the OS can hand 25040 to anything. Two ways that goes wrong and both
    // are refused: something else entirely, and something that merely LIVES in our
    // installation directory (llama-server does, and killing it would take the lane).
    #[test]
    fn a_recycled_pid_is_refused_rather_than_terminated() {
        let p = plan();
        let elsewhere = "C:\\Windows\\System32\\notepad.exe";
        let refused = target_is_our_core(&p, elsewhere)
            .expect_err("a process outside the installation must be refused");
        assert!(refused.contains("recycled"), "the reason must name reuse: {refused}");
        assert!(refused.contains(elsewhere), "the refusal names what is there: {refused}");

        let neighbour = "C:\\Users\\a\\.continuum\\bin\\llama-server.exe";
        let sibling = target_is_our_core(&p, neighbour)
            .expect_err("our own engine is not our core; terminating it would take the lane");
        assert!(sibling.contains("not a core image"), "{sibling}");

        // A path that merely STARTS with the directory's characters is not inside it.
        let lookalike = "C:\\Users\\a\\.continuum\\bin-evil\\core.exe";
        assert!(
            target_is_our_core(&p, lookalike).is_err(),
            "a sibling directory sharing a prefix is not the installation directory"
        );
    }

    // what this catches, and it is the case that made this gate an installation-directory
    // proof instead of an expected-image equality (Astra, 2026-09-22): the descriptor
    // names the NEW slot while the surviving core still executes the RENAMED predecessor.
    // An equality check against the planner's guess would refuse here — failing not
    // closed but ALWAYS, and exactly on the broken state this rail exists to repair.
    #[test]
    fn a_core_running_from_the_renamed_predecessor_is_still_our_core() {
        let p = plan();
        target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\core.prev.exe")
            .expect("a core that outlived its own deploy runs from the parking space");
        target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\core.exe")
            .expect("and the ordinary case still passes");
    }

    // what this catches: path spelling read as a different file. The scheduler echoes
    // `Execute` as registered while QueryFullProcessImageName answers with the OS's own
    // spelling; treating `C:/x/Core.exe` and `c:\x\core.exe` as different targets would
    // refuse every legitimate teardown on some machines and none of the dangerous ones.
    #[test]
    fn windows_path_spelling_is_not_a_different_target() {
        let p = plan();
        target_is_our_core(&p, "c:/Users/a/.continuum/BIN/CONTINUUM-CORE-SERVER.EXE")
            .expect("case and separator spelling name the same place on Windows");
    }
}
