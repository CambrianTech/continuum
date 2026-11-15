//! Windows consent and teardown policy, using the host's held process capability.
//!
//! The DECISION — which process may be ended, in what order, and what a refusal means —
//! lives in `crate::elevated_teardown`, where it is exercised on every
//! machine in seconds rather than on the one platform that cannot link it. The host owns
//! the ONE handle opened for QUERY and TERMINATE across the drain. This module owns
//! the consent boundary that borrows the privilege the CLI does not hold.
//!
//! Measured 2026-09-22 on the Windows node: `OpenProcess(25040, TERMINATE)` returned
//! NULL with Win32 error 5. There is no permission to grant to the caller.

use std::path::{Path, PathBuf};

use crate::elevated_teardown::{
    bind_plan_bytes, digest, elevation_outcome, teardown_sequence, HeldCapability, TeardownPlan,
};

pub(crate) fn receipt_path(plan: &Path) -> PathBuf {
    plan.with_extension("receipt.txt")
}

/// SHA-256 (hex) of some bytes. The same function binds the supervisor plan; a digest
/// is a digest, and having two of them would be the first place they drift.
pub(crate) fn read_bound_plan(plan_path: &Path, plan_sha: &str) -> Result<TeardownPlan, String> {
    let bytes = std::fs::read(plan_path)
        .map_err(|e| format!("cannot read the teardown plan {}: {e}", plan_path.display()))?;
    bind_plan_bytes(&bytes, plan_sha, plan_path)
}

/// The digest gate itself, over bytes rather than a path.
pub(crate) fn digest_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(digest(&bytes))
}

// ---------------------------------------------------------------------------
// Adapt the host capability to the existing, platform-independent teardown sequence.
// ---------------------------------------------------------------------------

struct HeldTerminate(continuum_host::termination_authority::HeldProcess);

/// The elevated child, which owns BOTH HALVES OF THE TEARDOWN for its one target.
///
/// The drain and the terminate cannot be split across the consent boundary. If the
/// parent drained and then asked, a declined or failed consent would strand a drained
/// core answering ping — the very thing the preflight exists to prevent, moved later
/// — a declined or failed consent would strand a drained core again. So the order here is: bind the plan, TAKE the capability and hold it, observe the target THROUGH
/// that same handle, drain, then spend the capability.
///
/// WHAT THAT DOES AND DOES NOT BUY. Every way this can fail before the drain fails with
/// nothing drained — that part is a guarantee. After the drain, what has been removed is
/// the ACQUISITION failure: the terminate no longer needs a fresh `OpenProcess` that
/// could be denied, time out, or find the pid recycled. `TerminateProcess` itself can
/// still return failure and the code reports it; a held handle narrows the window, it
/// does not abolish it.
///
/// `drain` is INJECTED rather than called from here. The drain speaks the core socket and
/// lives in the bin root; reaching up for it made this the only module in the tree with a
/// dependency on its parent — the other five modules in this tree have zero. Taking it as an
/// argument keeps the leaf a leaf — and makes the ORDER testable, because a fake drain can
/// record that it ran after the handle was taken and before it was spent, which a direct
/// call never could.
/// A teardown capability that has been TAKEN and is being HELD.
///
/// The sequence below is written against this trait rather than against the Windows
/// handle so the ORDER — take, observe through what was taken, drain, spend — is
/// provable on any machine. A test that exercises helpers instead of orchestration proves
/// nothing about the order; a Windows-only sequence could only ever
/// have been asserted in a comment from here.
impl HeldCapability for HeldTerminate {
    fn image(&self) -> Result<String, String> {
        self.0.image().map_err(|error| error.to_string())?
            .into_os_string().into_string()
            .map_err(|_| "the process image is not representable by the teardown plan".to_owned())
    }
    fn spend(self) -> Result<(), String> {
        self.0.terminate(1).map_err(|error| error.to_string())
    }
}

pub async fn teardown_elevated<F, Fut>(
    plan_path: &Path,
    plan_sha: &str,
    drain: F,
) -> Result<(), String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let receipt = receipt_path(plan_path);
    let result = async {
        let plan = read_bound_plan(plan_path, plan_sha)?;
        let (image, graceful) = teardown_sequence(&plan, |pid| {
            let pid = u32::try_from(pid).map_err(|error| error.to_string())?;
            continuum_host::termination_authority::HeldProcess::take(pid)
                .map(HeldTerminate).map_err(|error| error.to_string())
        }, drain).await?;
        // The digest is RECORDED, not compared: nobody could have known it in advance
        // (see `target_is_our_core`), but the receipt should say exactly what was ended.
        let sha = digest_file(Path::new(&image)).unwrap_or_else(|e| format!("unreadable ({e})")); // unwrap_or_else: the digest is RECORDED, never compared — an unreadable image must not block a teardown that is otherwise proven
        Ok::<String, String>(format!(
            "elevated teardown: drained ({graceful}) and terminated pid {} running {image} (sha256 {sha})",
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

/// What a finished elevation MEANS for the caller. Pure, so the one decision that
/// separates "nothing was touched" from "something ran and may have half-finished" is
/// pinned by a test rather than living inside a PowerShell call nobody can exercise.
///
/// A REFUSED CONSENT IS NOT A FAILED TEARDOWN. The operator declined, the child never
/// ran, and the core is exactly as it was — that has to be said in those words, because
/// the alternative reading ("the teardown failed") sends the next reader hunting for
/// damage that does not exist. Every other outcome carries the child's receipt, or says
/// plainly that there was none.
pub async fn request_elevated_teardown(pid: i32, install_dir: &str) -> Result<(), String> {
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
         @('stop','--elevated','--plan','{}','--plan-sha','{plan_sha}') -Verb RunAs -WindowStyle Hidden -Wait \
         -PassThru; exit $p.ExitCode",
        quote(exe.display().to_string()),
        quote(plan_path.display().to_string()),
    );
    // The consent prompt waits for a human; ten minutes is the wall past which nobody
    // is there. Same budget the supervisor's one consent gets.
    let elevated =
        super::supervisor_install::powershell(&script, std::time::Duration::from_secs(600)).await;
    // unwrap_or_default: an absent receipt is reported below as "none", never as success.
    let receipt_text = std::fs::read_to_string(&receipt).unwrap_or_default(); // unwrap_or_default: an absent receipt is reported below as "none", never as success
    let _ = std::fs::remove_file(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    elevation_outcome(pid, elevated, &receipt_text)?;
    if !receipt_text.is_empty() {
        println!("{}", receipt_text.trim());
    }
    // THE PROCESS BEING GONE IS THE PROOF, not the child's exit code — the whole defect
    // this module exists for was a kill path that trusted its own return. The caller
    // proves it on the bounded deadline; an immediate probe here would see a process
    // that is exiting perfectly normally and call it a survivor.
    Ok(())
}

