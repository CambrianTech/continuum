//! The service host runs the core with its user's hands, not an administrator's.
//!
//! Measured 2026-10-04 on the 5090: the ContinuumCore task is `RunLevel=Limited`,
//! `LogonType=S4U`, yet the core's own shell reported `High Mandatory Level` with
//! `BUILTIN\Administrators` enabled. Windows does not filter an S4U logon's token, so
//! `Limited` is a no-op there and every citizen's shell was elevated: her `airc update`
//! refused ("requires a normal user terminal") while the same command ran from a normal
//! session, and elevated writes leave admin-owned files in her user's tree.
//!
//! The fix is ONE drop of privilege at the one place the core is launched: an elevated
//! service host relaunches itself once with a NORMALUSER token (Administrators
//! deny-only, Medium integrity) and passes on that child's exit code. Everything the
//! core spawns afterwards inherits the user's token; no spawn site has to know.
//!
//! The decision is pure and tested; the OS half below is the Windows API.

/// Set in the relaunched child's environment: it is already the unelevated host, so it
/// never relaunches again (a token that still read as elevated would otherwise loop).
pub(crate) const UNELEVATED_MARKER: &str = "CONTINUUM_SERVICE_UNELEVATED";

/// High Mandatory Level's RID; System is above it. The integrity level is what decides
/// whether a process holds an administrator's hands, so it is what "elevated" means here.
/// `TokenElevation` is not: measured on the 5090 (2026-10-04), a Safer NORMALUSER token
/// derived from the elevated S4U token, Medium with Administrators deny-only, still
/// reported `TokenIsElevated = 1`, and the relaunched host said it was elevated.
pub(crate) const HIGH_INTEGRITY_RID: u32 = 0x3000;

/// Whether a token at this integrity RID runs with an administrator's hands.
pub(crate) fn is_elevated_rid(rid: u32) -> bool {
    rid >= HIGH_INTEGRITY_RID
}

/// Whether this service host must relaunch itself unelevated before launching the core.
pub(crate) fn must_relaunch(token_elevated: bool, marker_present: bool) -> bool {
    token_elevated && !marker_present
}

pub(crate) use os::{relaunch_unelevated, token_is_elevated};

mod os {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, SetTokenInformation, TokenIntegrityLevel, SAFER_LEVEL_HANDLE,
        SECURITY_MAX_SID_SIZE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    };
    use windows_sys::Win32::Security::AppLocker::{
        SaferCloseLevel, SaferComputeTokenFromLevel, SaferCreateLevel, SAFER_LEVELID_NORMALUSER, SAFER_LEVEL_OPEN,
        SAFER_SCOPEID_USER,
    };
    use windows_sys::Win32::Security::{CreateWellKnownSid, WinMediumLabelSid};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    use windows_sys::Win32::System::Environment::GetCommandLineW;
    use windows_sys::Win32::System::SystemServices::SE_GROUP_INTEGRITY;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, ResumeThread,
        TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
        INFINITE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
    };

    pub(super) struct Owned(pub(super) HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    /// The integrity RID of a token (0x2000 Medium, 0x3000 High).
    pub(super) fn integrity_rid(token: HANDLE) -> Result<u32, String> {
        use windows_sys::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount};
        let mut buf = vec![0u8; 256];
        let mut len = 0u32;
        if unsafe { GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr().cast(), buf.len() as u32, &mut len) } == 0 {
            return Err(last_error("GetTokenInformation(TokenIntegrityLevel)"));
        }
        let label = unsafe { &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
        let count = unsafe { *GetSidSubAuthorityCount(label.Label.Sid) } as u32;
        Ok(unsafe { *GetSidSubAuthority(label.Label.Sid, count - 1) })
    }

    fn last_error(what: &str) -> String {
        format!("{what} failed (Win32 error {})", unsafe { GetLastError() })
    }

    /// Whether this process's own token is elevated: its integrity is High or above.
    pub(crate) fn token_is_elevated() -> Result<bool, String> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(last_error("OpenProcessToken"));
        }
        let token = Owned(token);
        Ok(super::is_elevated_rid(integrity_rid(token.0)?))
    }

    /// A NORMALUSER token derived from this process's own (Administrators deny-only),
    /// lowered to Medium integrity: a restricted copy of the caller's token, so creating
    /// a process with it needs no privilege.
    pub(super) fn normal_user_token() -> Result<Owned, String> {
        let mut level: SAFER_LEVEL_HANDLE = std::ptr::null_mut();
        if unsafe {
            SaferCreateLevel(SAFER_SCOPEID_USER, SAFER_LEVELID_NORMALUSER, SAFER_LEVEL_OPEN, &mut level, std::ptr::null())
        } == 0
        {
            return Err(last_error("SaferCreateLevel(NORMALUSER)"));
        }
        let mut token: HANDLE = std::ptr::null_mut();
        let computed =
            unsafe { SaferComputeTokenFromLevel(level, std::ptr::null_mut(), &mut token, 0, std::ptr::null_mut()) };
        unsafe { SaferCloseLevel(level) };
        if computed == 0 {
            return Err(last_error("SaferComputeTokenFromLevel"));
        }
        let token = Owned(token);
        // A Safer NORMALUSER token keeps the source's integrity level; lower it to Medium,
        // the integrity of a normal user's session.
        let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
        let mut sid_len = sid.len() as u32;
        if unsafe { CreateWellKnownSid(WinMediumLabelSid, std::ptr::null_mut(), sid.as_mut_ptr().cast(), &mut sid_len) } == 0 {
            return Err(last_error("CreateWellKnownSid(MediumLabel)"));
        }
        let mut label: TOKEN_MANDATORY_LABEL = unsafe { std::mem::zeroed() };
        label.Label.Sid = sid.as_mut_ptr().cast();
        label.Label.Attributes = SE_GROUP_INTEGRITY as u32;
        let size = std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + sid_len;
        if unsafe { SetTokenInformation(token.0, TokenIntegrityLevel, (&label as *const TOKEN_MANDATORY_LABEL).cast(), size) } == 0 {
            return Err(last_error("SetTokenInformation(TokenIntegrityLevel=Medium)"));
        }
        Ok(token)
    }

    /// This process's environment plus the marker, as a CREATE_UNICODE_ENVIRONMENT block.
    fn environment_with_marker() -> Vec<u16> {
        let mut block: Vec<u16> = Vec::new();
        for (key, value) in std::env::vars_os() {
            if key.eq_ignore_ascii_case(super::UNELEVATED_MARKER) {
                continue;
            }
            let mut entry = key.clone();
            entry.push("=");
            entry.push(&value);
            block.extend(std::os::windows::ffi::OsStrExt::encode_wide(entry.as_os_str()));
            block.push(0);
        }
        block.extend(super::UNELEVATED_MARKER.encode_utf16().chain("=1".encode_utf16()));
        block.push(0);
        block.push(0);
        block
    }

    /// A job that ends its processes when its last handle closes: the elevated parent holds
    /// it, so if the task kills the parent the unelevated host dies with it, never a second
    /// core left on the node.
    fn kill_on_close_job() -> Result<Owned, String> {
        let job = Owned(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
        if job.0.is_null() {
            return Err(last_error("CreateJobObjectW"));
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(last_error("SetInformationJobObject(KILL_ON_JOB_CLOSE)"));
        }
        Ok(job)
    }

    /// Relaunch this exact command line under the normal user's token and wait for it;
    /// returns its exit code. The child shares this console's standard handles, so the
    /// task's logs are unchanged.
    pub(crate) fn relaunch_unelevated() -> Result<i32, String> {
        let token = normal_user_token()?;
        let job = kill_on_close_job()?;
        // CreateProcessAsUserW may write into the command line buffer: give it a copy.
        let mut command_line: Vec<u16> = unsafe {
            let raw = GetCommandLineW();
            let mut len = 0usize;
            while *raw.add(len) != 0 {
                len += 1;
            }
            std::slice::from_raw_parts(raw, len + 1).to_vec()
        };
        let mut environment = environment_with_marker();
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        unsafe {
            startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
            startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
            startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
        }
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let created = unsafe {
            CreateProcessAsUserW(
                token.0,
                std::ptr::null(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW | CREATE_SUSPENDED,
                environment.as_mut_ptr().cast(),
                std::ptr::null(),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(last_error("CreateProcessAsUserW(normal user)"));
        }
        let child = Owned(process.hProcess);
        let thread = Owned(process.hThread);
        // Into the job BEFORE it runs a single instruction, so it can never outlive us.
        if unsafe { AssignProcessToJobObject(job.0, child.0) } == 0 {
            let why = last_error("AssignProcessToJobObject");
            unsafe { TerminateProcess(child.0, 1) };
            return Err(why);
        }
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            let why = last_error("ResumeThread");
            unsafe { TerminateProcess(child.0, 1) };
            return Err(why);
        }
        drop(thread);
        unsafe { WaitForSingleObject(child.0, INFINITE) };
        let mut code = 0u32;
        if unsafe { GetExitCodeProcess(child.0, &mut code) } == 0 {
            return Err(last_error("GetExitCodeProcess"));
        }
        Ok(code as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the token the core is relaunched with still carrying admin
    // rights. Built from whatever session runs the test (elevated or not): it must be
    // unelevated and at Medium integrity, the 5090 citizen shell's target.
    #[test]
    fn the_normal_user_token_is_unelevated_at_medium_integrity() {
        let token = os::normal_user_token().expect("a NORMALUSER token can be computed from any session");
        let rid = os::integrity_rid(token.0).expect("integrity readable");
        assert_eq!(rid, 0x2000, "Medium Mandatory Level");
        assert!(!is_elevated_rid(rid), "the relaunched host reads itself as unelevated");
    }

    // what this catches: the elevated S4U host launching the core directly (every citizen
    // shell elevated, 5090 2026-10-04), and a relaunched host relaunching again forever.
    // what this catches (5090, 2026-10-04): "elevated" judged by TokenElevation, which
    // stayed 1 on the Medium, Administrators-deny-only relaunched token. Integrity decides.
    #[test]
    fn elevation_is_the_integrity_level() {
        assert!(!is_elevated_rid(0x2000), "Medium: a normal user's hands");
        assert!(is_elevated_rid(0x3000), "High: an administrator's");
        assert!(is_elevated_rid(0x4000), "System is above High");
    }

    #[test]
    fn only_an_elevated_host_without_the_marker_relaunches() {
        assert!(must_relaunch(true, false), "elevated and not yet relaunched: drop privilege");
        assert!(!must_relaunch(true, true), "the relaunched child never relaunches again");
        assert!(!must_relaunch(false, false), "a normal user's host launches the core directly");
    }
}
