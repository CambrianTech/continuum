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
    bind_plan_bytes, digest, elevation_outcome, teardown_sequence, ElevationError, HeldCapability,
    TeardownPlan, TeardownPlanError,
};

pub(crate) fn receipt_path(plan: &Path) -> PathBuf {
    plan.with_extension("receipt.txt")
}

/// SHA-256 (hex) of some bytes. The same function binds the supervisor plan; a digest
/// is a digest, and having two of them would be the first place they drift.
pub(crate) fn read_bound_plan(
    plan_path: &Path,
    plan_sha: &str,
) -> Result<TeardownPlan, TeardownPlanError> {
    let bytes = std::fs::read(plan_path).map_err(|source| TeardownPlanError::Read {
        path: plan_path.to_path_buf(),
        source,
    })?;
    bind_plan_bytes(&bytes, plan_sha, plan_path)
}

/// The digest gate itself, over bytes rather than a path.
pub(crate) fn digest_file(path: &Path) -> Result<String, TeardownPlanError> {
    let bytes = std::fs::read(path).map_err(|source| TeardownPlanError::Read {
        path: path.to_path_buf(),
        source,
    })?;
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
    fn image(&self) -> Result<PathBuf, TeardownPlanError> {
        self.0.image().map_err(TeardownPlanError::Image)
    }
    fn spend(self) -> Result<(), TeardownPlanError> {
        self.0.terminate(1).map_err(TeardownPlanError::Terminate)
    }
}

pub async fn teardown_elevated<F, Fut>(
    plan_path: &Path,
    plan_sha: &str,
    drain: F,
) -> Result<(), TeardownPlanError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let receipt = receipt_path(plan_path);
    let result = async {
        let plan = read_bound_plan(plan_path, plan_sha)?;
        let (image, graceful) = teardown_sequence(&plan, |pid| {
            let pid = continuum_host::process_id::ProcessId::try_from(pid)
                .map_err(TeardownPlanError::InvalidPid)?;
            continuum_host::termination_authority::HeldProcess::take(pid)
                .map(HeldTerminate).map_err(TeardownPlanError::Take)
        }, drain).await?;
        // The digest is RECORDED, not compared: nobody could have known it in advance
        // (see `target_is_our_core`), but the receipt should say exactly what was ended.
        let sha = digest_file(&image).unwrap_or_else(|e| format!("unreadable ({e})")); // unwrap_or_else: the digest is RECORDED, never compared — an unreadable image must not block a teardown that is otherwise proven
        Ok::<String, TeardownPlanError>(format!(
            "elevated teardown: drained ({graceful}) and terminated pid {} running {} (sha256 {sha})",
            plan.pid, image.display()
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
pub async fn request_elevated_teardown(pid: i32, install_dir: &Path) -> Result<(), ElevationError> {
    let plan = TeardownPlan {
        pid,
        install_dir: install_dir.to_path_buf(),
    };
    let plan_path =
        std::env::temp_dir().join(format!("continuum-teardown-{}.json", std::process::id()));
    let receipt = receipt_path(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    let bytes = serde_json::to_vec_pretty(&plan).map_err(ElevationError::Encode)?;
    let plan_sha = digest(&bytes);
    std::fs::write(&plan_path, &bytes).map_err(ElevationError::Write)?;
    let exe = std::env::current_exe().map_err(ElevationError::OwnPath)?;
    let script = consent_script(&exe, &plan_path, &plan_sha);
    // Consent waits for a human, bounded by the supervisor's existing ten-minute budget.
    let elevated =
        super::supervisor_install::powershell(&script, std::time::Duration::from_secs(600)).await;
    let receipt_text = match std::fs::read_to_string(&receipt) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ElevationError::Receipt(error)),
    };
    let _ = std::fs::remove_file(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    let outcome: crate::elevated_teardown::ConsentOutcome = serde_json::from_str(&elevated?)?;
    let receipt_text = receipt_text?;
    elevation_outcome(pid, outcome, receipt_text.as_deref())?;
    if let Some(text) = receipt_text {
        println!("{}", text.trim());
    }
    // THE PROCESS BEING GONE IS THE PROOF, not the child's exit code — the whole defect
    // this module exists for was a kill path that trusted its own return. The caller
    // proves it on the bounded deadline; an immediate probe here would see a process
    // that is exiting perfectly normally and call it a survivor.
    Ok(())
}

fn consent_script(exe: &Path, plan_path: &Path, plan_sha: &str) -> String {
    let quote = |s: String| s.replace('\'', "''");
    format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath '{}' -ArgumentList \
         @('stop','--elevated','--plan','{}','--plan-sha','{plan_sha}') -Verb RunAs -WindowStyle Hidden -Wait \
         -PassThru }} catch [System.ComponentModel.Win32Exception] {{ \
         if ($_.Exception.NativeErrorCode -eq 1223) {{ @{{kind='refused'}} | ConvertTo-Json -Compress; exit 0 }}; throw }}; \
         @{{kind='completed';exitCode=[int]$p.ExitCode}} | ConvertTo-Json -Compress",
        quote(exe.display().to_string()),
        quote(plan_path.display().to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elevated_teardown::ConsentOutcome;
    use crate::supervisor_install::{powershell, PowerShellError};

    // Exercise the real Windows PowerShell exception/protocol boundary without
    // asking for consent or touching a resident process.
    #[tokio::test]
    async fn native_consent_codes_do_not_confuse_refusal_and_child_failure() {
        let script = consent_script(Path::new("unused.exe"), Path::new("unused.json"), "digest");
        for (mock, expected) in [
            (
                "function Start-Process { throw [ComponentModel.Win32Exception]::new(1223, 'operator declined') }",
                "refused",
            ),
            (
                "function Start-Process { [pscustomobject]@{ExitCode=1223} }",
                "completed",
            ),
        ] {
            let raw = powershell(
                &format!(
                    "{mock}
{script}"
                ),
                std::time::Duration::from_secs(30),
            )
            .await
            .expect("the typed launcher protocol succeeds");
            let outcome: ConsentOutcome = serde_json::from_str(&raw).expect("typed launch outcome");
            match (expected, outcome) {
                ("refused", ConsentOutcome::Refused) => {}
                ("completed", ConsentOutcome::Completed { exit_code: 1223 }) => {}
                other => panic!("wrong consent outcome: {other:?}"),
            }
        }
        let denied = powershell(
            &format!(
                "function Start-Process {{ throw [ComponentModel.Win32Exception]::new(5) }}
{script}"
            ),
            std::time::Duration::from_secs(30),
        )
        .await;
        assert!(
            matches!(denied, Err(PowerShellError::Exit { .. })),
            "access denied is a launch failure, never an untouched/refused claim"
        );
    }
}
