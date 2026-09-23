//! The OS half of the teardown: taking a capability on Windows and spending it.
//!
//! The DECISION — which process may be ended, in what order, and what a refusal means —
//! lives in `continuum_cli_lifecycle::elevated_teardown`, where it is exercised on every
//! machine in seconds rather than on the one platform that cannot link it. What stays
//! here is the part that needs the OS with the problem: ONE handle opened for QUERY and
//! TERMINATE, held across the drain, and the consent boundary that borrows the privilege
//! the CLI does not hold.
//!
//! Measured 2026-09-22 on the Windows node: `OpenProcess(25040, TERMINATE)` returned
//! NULL with Win32 error 5. There is no permission to grant to the caller.

use std::path::{Path, PathBuf};

use continuum_cli_lifecycle::elevated_teardown::{
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
// The Windows halves. Everything above is pure and pinned; everything below needs
// the OS that has the problem.
// ---------------------------------------------------------------------------

/// ONE handle, opened for both QUERY and TERMINATE, held open across everything.
/// Closed exactly once, on drop.
///
/// A HANDLE NAMES A PROCESS; A PID NAMES A SLOT. This first opened a query handle, read
/// the image, closed it, and re-opened for terminate — so the identity that was
/// validated and the process that would be killed were two separate lookups of a
/// recyclable number with nothing tying them together: a token alone does not bind a
/// target. Validating on the SAME handle that does the terminating is what makes the
/// check mean anything.
///
/// The point of holding it rather than re-opening later: between a capability CHECK and
/// the terminate, the check can stop being true. Holding the handle across the drain
/// means the authority is not re-asked for — it is already in hand.
struct HeldTerminate {
    pid: i32,
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl Drop for HeldTerminate {
    fn drop(&mut self) {
        // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

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
        HeldTerminate::image(self)
    }
    fn spend(self) -> Result<(), String> {
        self.terminate()
    }
}

pub(crate) async fn teardown_elevated<F, Fut>(
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
        let (image, graceful) =
            teardown_sequence(&plan, HeldTerminate::take, drain).await?;
        // The digest is RECORDED, not compared: nobody could have known it in advance
        // (see `target_is_our_core`), but the receipt should say exactly what was ended.
        let sha = digest_file(Path::new(&image)).unwrap_or_else(|e| format!("unreadable ({e})"));
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
pub(crate) async fn request_elevated_teardown(pid: i32, install_dir: &str) -> Result<(), String> {
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

