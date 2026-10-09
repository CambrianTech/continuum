//! The Windows core-launch boundary: inherit only the handles this launch owns.
//!
//! Rust's stable `Command::spawn` inherits every inheritable process handle,
//! including a CLI host's otherwise-unused output pipes. A detached core then
//! holds that host's EOF open for its entire lifetime (card 9bc0fc5e).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::ptr;

use windows_sys::Win32::Foundation::{
    DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_INSUFFICIENT_BUFFER, HANDLE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Globalization::CompareStringOrdinal;
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess, GetExitCodeProcess,
    InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

/// Only the operations the CLI's existing startup observer needs. Dropping this
/// closes our process handle; it never terminates the detached core.
pub struct LaunchedCore {
    process: OwnedHandle,
    pid: u32,
}

impl LaunchedCore {
    pub fn id(&self) -> u32 {
        self.pid
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // Query the wait state first: exit code 259 is also a legal completed
        // program's status, so STILL_ACTIVE alone cannot prove liveness.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}

/// A service-owned worker and its descendants. The non-inherited job handle is
/// the lifetime authority: closing it (including on host death) ends the tree.
/// This is deliberately distinct from LaunchedCore's detached semantics.
pub struct OwnedProcessTree {
    child: LaunchedCore,
    _job: OwnedHandle,
    stop_input: Option<File>,
}

impl OwnedProcessTree {
    /// Close the service-owned lifeline. Workers opting into this contract
    /// observe stdin EOF and finish their cleanup before the job is dropped.
    pub fn request_stop(&mut self) {
        self.stop_input.take();
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Observe the kernel's process-exit event without a liveness polling loop.
    /// The observer owns only a process handle, never the job's lifetime. If the
    /// caller cancels this future and drops the tree, job cleanup wakes the wait.
    pub async fn wait(&self) -> io::Result<ExitStatus> {
        let process = self.child.process.try_clone()?;
        tokio::task::spawn_blocking(move || {
            if unsafe { WaitForSingleObject(process.as_raw_handle(), u32::MAX) } != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            let mut code = 0;
            if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(ExitStatus::from_raw(code))
        })
        .await
        .map_err(io::Error::other)?
    }
}

/// Assign the worker to its owner before its first instruction. An after-spawn
/// assignment leaves an orphan window if the service host crashes in between.
/// Existing detached callers keep spawn_logged; they must not acquire this lifetime.
pub fn spawn_owned_logged(
    command: &Command,
    stdout: &File,
    stderr: &File,
    flags: u32,
) -> io::Result<OwnedProcessTree> {
    let raw_job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if raw_job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let jobs = [job.as_raw_handle()];
    let mut reader = ptr::null_mut();
    let mut writer = ptr::null_mut();
    if unsafe {
        windows_sys::Win32::System::Pipes::CreatePipe(&mut reader, &mut writer, ptr::null(), 0)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // Originals are not inheritable; only the explicit stdin duplicate crosses
    // CreateProcess. No child can retain the host's write end and mask EOF.
    let input = unsafe { File::from_raw_handle(reader) };
    let stop_input = unsafe { File::from_raw_handle(writer) };
    let child = spawn_logged_in_jobs(command, stdout, stderr, flags, &jobs, Some(&input))?;
    Ok(OwnedProcessTree {
        child,
        _job: job,
        stop_input: Some(stop_input),
    })
}

/// Spawn this CLI's already-configured core/script command. Its executable is
/// resolved by the existing locator; arguments are regular `Command::arg`s and
/// the environment inherits this process plus `env`/`env_remove` assignments.
/// This is deliberately not a general replacement for Command (no raw args,
/// env_clear, shell/batch dispatch, or implicit PATH executable resolution).
pub fn spawn_logged(
    command: &Command,
    stdout: &File,
    stderr: &File,
    flags: u32,
) -> io::Result<LaunchedCore> {
    spawn_logged_in_jobs(command, stdout, stderr, flags, &[], None)
}

// Resolve identity first, then use ordinary Win32 drive/UNC spelling at the
// process boundary. .NET Framework PowerShell fails during ServicePointManager
// initialization when its executable path uses canonicalize's extended prefix.
fn process_image_path(program: &OsStr) -> io::Result<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(program)?;
    let encoded: Vec<u16> = canonical.as_os_str().encode_wide().collect();
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
    let ordinary = if encoded.starts_with(&unc) {
        let mut result: Vec<u16> = r"\\".encode_utf16().collect();
        result.extend_from_slice(&encoded[unc.len()..]);
        std::path::PathBuf::from(OsString::from_wide(&result))
    } else if encoded.starts_with(&prefix)
        && encoded.get(5) == Some(&(b':' as u16))
        && encoded.get(6) == Some(&0x5c)
    {
        std::path::PathBuf::from(OsString::from_wide(&encoded[prefix.len()..]))
    } else {
        return Ok(canonical);
    };
    // Ordinary spelling must not redirect unusual extended-path names.
    if !same_file::is_same_file(&canonical, &ordinary)? {
        return Err(io::Error::other(
            "process image spelling changed file identity",
        ));
    }
    Ok(ordinary)
}

fn spawn_logged_in_jobs(
    command: &Command,
    stdout: &File,
    stderr: &File,
    flags: u32,
    jobs: &[HANDLE],
    stdin: Option<&File>,
) -> io::Result<LaunchedCore> {
    let executable = process_image_path(command.get_program())?;
    let mut application = wide(executable.as_os_str())?;
    application.push(0);
    let mut arguments = Vec::new();
    append_argument(&mut arguments, executable.as_os_str())?;
    for arg in command.get_args() {
        arguments.push(b' ' as u16);
        append_argument(&mut arguments, arg)?;
    }
    arguments.push(0);
    let environment = environment_block(command)?;
    let cwd = command
        .get_current_dir()
        .map(|path| {
            let mut path = wide(path.as_os_str())?;
            path.push(0);
            Ok::<_, io::Error>(path)
        })
        .transpose()?;

    let null_input;
    let input = match stdin {
        Some(input) => input,
        None => {
            null_input = File::open("NUL")?;
            &null_input
        }
    };
    let handles = [
        inheritable_duplicate(input.as_raw_handle())?,
        inheritable_duplicate(stdout.as_raw_handle())?,
        inheritable_duplicate(stderr.as_raw_handle())?,
    ];
    let raw = handles.each_ref().map(AsRawHandle::as_raw_handle);
    let attributes = HandleAttributes::new(&raw, jobs)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw[0];
    startup.StartupInfo.hStdOutput = raw[1];
    startup.StartupInfo.hStdError = raw[2];
    startup.lpAttributeList = attributes.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    // All pointers reference live owned buffers/handles until CreateProcessW
    // returns. TRUE is required with HANDLE_LIST; the list restricts inheritance
    // to these three handles rather than admitting every inheritable handle.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            arguments.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            flags | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            cwd.as_ref().map_or(ptr::null(), |path| path.as_ptr()),
            &startup.StartupInfo,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // A successful CreateProcess transfers two fresh handles to us. No thread
    // operations are needed; close that handle now and retain only the process.
    let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    drop(unsafe { OwnedHandle::from_raw_handle(process.hThread) });
    Ok(LaunchedCore {
        process: process_handle,
        pid: process.dwProcessId,
    })
}

fn inheritable_duplicate(handle: HANDLE) -> io::Result<OwnedHandle> {
    let mut duplicate = ptr::null_mut();
    // Only the NEW duplicate is inheritable. Never mutate flags on a caller's
    // handle: another task may be using/spawning with it concurrently.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(duplicate) })
}

struct HandleAttributes<'a> {
    storage: Vec<usize>,
    // UpdateProcThreadAttribute borrows this buffer until the spawn completes.
    _handles: &'a [HANDLE],
    _jobs: &'a [HANDLE],
}

impl<'a> HandleAttributes<'a> {
    fn new(handles: &'a [HANDLE], jobs: &'a [HANDLE]) -> io::Result<Self> {
        let count = if jobs.is_empty() { 1 } else { 2 };
        let mut bytes = 0;
        let result =
            unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut bytes) };
        let error = io::Error::last_os_error();
        if result != 0
            || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || bytes == 0
        {
            return Err(error);
        }
        // Pointer-aligned storage for the opaque native list.
        let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), count, 0, &mut bytes)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = Self {
            storage,
            _handles: handles,
            _jobs: jobs,
        };
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.as_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if !jobs.is_empty()
            && unsafe {
                UpdateProcThreadAttribute(
                    attributes.as_ptr(),
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    jobs.as_ptr().cast(),
                    size_of_val(jobs),
                    ptr::null_mut(),
                    ptr::null(),
                )
            } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(attributes)
    }

    fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_ptr().cast_mut().cast()
    }
}

impl Drop for HandleAttributes<'_> {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
    }
}

fn wide(text: &OsStr) -> io::Result<Vec<u16>> {
    let text: Vec<_> = text.encode_wide().collect();
    if text.contains(&0) || text.len() > i32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Windows launch text",
        ));
    }
    Ok(text)
}

/// Windows C-runtime argument quoting, also accepted by Git's bash. Quote every
/// argument, double backslashes before quotes/the closing delimiter, and leave
/// other backslashes alone. This preserves empty arguments and Unicode paths.
fn append_argument(line: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    let arg = wide(arg)?;
    line.push(b'"' as u16);
    let mut slashes = 0;
    for unit in arg {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
        } else {
            line.extend(std::iter::repeat_n(b'\\' as u16, slashes));
        }
        line.push(unit);
        slashes = 0;
    }
    line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    line.push(b'"' as u16);
    Ok(())
}

#[derive(Eq)]
struct EnvKey(Vec<u16>);

impl Ord for EnvKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // Windows environment keys use ordinal case-insensitive comparison,
        // not ASCII lowercase or locale-sensitive case conversion. Both lengths
        // were checked by wide(); no allocation occurs during comparison.
        let compared = unsafe {
            CompareStringOrdinal(
                self.0.as_ptr(),
                self.0.len() as i32,
                other.0.as_ptr(),
                other.0.len() as i32,
                1,
            )
        };
        compared.cmp(&2) // CSTR_LESS_THAN=1, CSTR_EQUAL=2, CSTR_GREATER_THAN=3
    }
}

impl PartialOrd for EnvKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for EnvKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

fn environment_block(command: &Command) -> io::Result<Vec<u16>> {
    let mut vars = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        vars.insert(EnvKey(wide(&key)?), wide(&value)?);
    }
    for (key, value) in command.get_envs() {
        let key = wide(key)?;
        if key.is_empty() || key.contains(&(b'=' as u16)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Windows environment key",
            ));
        }
        let key = EnvKey(key);
        match value {
            Some(value) => {
                vars.insert(key, wide(value)?);
            }
            None => {
                vars.remove(&key);
            }
        }
    }
    let mut block = Vec::new();
    for (key, value) in vars {
        block.extend(key.0);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
        PROCESS_SYNCHRONIZE,
    };

    const FIXTURE_ENV: &str = "CONTINUUM_DETACHED_LAUNCH_FIXTURE";
    const FIXTURE_TEST: &str = "windows_launch::tests::detached_child_fixture";
    const TREE_ROLE_ENV: &str = "CONTINUUM_OWNED_TREE_FIXTURE_ROLE";
    /// Set in the fresh process the pipe-leak check runs alone in (card 64ca188f).
    const ISOLATED_ENV: &str = "CONTINUUM_PIPE_LEAK_CHECK_ISOLATED";
    const ISOLATED_TEST: &str =
        "windows_launch::tests::detached_child_closes_unrelated_pipe_and_preserves_launch_contract";
    const QUOTED_ARGS: &[&str] = &[
        "space and λ",
        "embedded\"quote",
        "trailing slash\\",
        "two\\\\\"quote",
    ];

    // This is a real child entry through the same test executable, not a second
    // daemon or an installed shell dependency. Normal test runs leave it inert.
    #[test]
    fn detached_child_fixture() {
        let Some(root) = std::env::var_os(FIXTURE_ENV) else {
            return;
        };
        let root = PathBuf::from(root);
        if let Ok(role) = std::env::var(TREE_ROLE_ENV) {
            let _descendant = if role == "parent" {
                Some(
                    Command::new(std::env::current_exe().unwrap())
                        .args(["--exact", FIXTURE_TEST, "--nocapture", "--test-threads=1"])
                        .env(TREE_ROLE_ENV, "descendant")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .unwrap(),
                )
            } else {
                None
            };
            let pending = root.join(format!("{role}.pending"));
            std::fs::write(&pending, std::process::id().to_string()).unwrap();
            std::fs::rename(pending, root.join(&role)).unwrap();
            if role == "lifeline" {
                let mut bytes = Vec::new();
                std::io::stdin().read_to_end(&mut bytes).unwrap();
                return;
            }
            // Bounded fail-safe if the owner under test fails to close its job.
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        assert_eq!(
            std::env::current_dir().unwrap().canonicalize().unwrap(),
            root
        );
        assert_eq!(
            std::env::var("continuum_launch_case").unwrap(),
            "configured λ"
        );
        assert!(
            std::env::var_os("USERPROFILE").is_none(),
            "explicit removal must reach child"
        );
        assert_eq!(
            std::env::vars_os()
                .filter(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("systemroot"))
                .count(),
            1
        );
        let args: Vec<_> = std::env::args().collect();
        let skipped: Vec<_> = args
            .windows(2)
            .filter(|pair| pair[0] == "--skip")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(skipped, QUOTED_ARGS);
        assert_eq!(
            std::io::stdin().read(&mut [0u8; 1]).unwrap(),
            0,
            "stdin is null"
        );
        println!("detached fixture stdout");
        eprintln!("detached fixture stderr");
        std::io::stdout().flush().unwrap();
        std::io::stderr().flush().unwrap();
        std::fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !root.join("release").exists() {
            assert!(
                Instant::now() < deadline,
                "fixture parent did not release child"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    struct FixtureChild(LaunchedCore);

    // Catches killing only Node while leaving its browser descendants behind.
    // Uses the existing real-process fixture, with a second generation inheriting
    // the job; stable process handles prevent PID reuse from satisfying the check.
    #[test]
    fn owned_tree_drop_ends_worker_and_descendant() {
        let root = tempfile::tempdir().unwrap();
        // Actual public supervisor failure: resolving PowerShell to an extended
        // path caused exit -65536 before its script or our core could start.
        let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let shell_log_path = root.path().join("powershell.log");
        let shell_log = File::create(&shell_log_path).unwrap();
        let mut shell_command = Command::new(shell.canonicalize().unwrap());
        shell_command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Net.ServicePointManager]::SecurityProtocol; exit 0",
        ]);
        let mut shell_tree =
            spawn_owned_logged(&shell_command, &shell_log, &shell_log, CREATE_NO_WINDOW).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = shell_tree.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "PowerShell initialization timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "PowerShell initialization failed: {}",
            std::fs::read_to_string(&shell_log_path).unwrap()
        );
        drop(shell_tree);
        let log = File::create(root.path().join("tree.log")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", FIXTURE_TEST, "--nocapture", "--test-threads=1"])
            .env(FIXTURE_ENV, root.path())
            .env(TREE_ROLE_ENV, "parent");
        let mut tree = spawn_owned_logged(&command, &log, &log, CREATE_NO_WINDOW).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root.path().join("descendant").exists() {
            assert!(
                tree.try_wait().unwrap().is_none(),
                "worker exited before descendant started"
            );
            assert!(Instant::now() < deadline, "descendant did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        let descendant_pid: u32 = std::fs::read_to_string(root.path().join("descendant"))
            .unwrap()
            .parse()
            .unwrap();
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, descendant_pid) };
        assert!(
            !raw.is_null(),
            "open descendant: {}",
            io::Error::last_os_error()
        );
        let descendant = unsafe { OwnedHandle::from_raw_handle(raw) };
        let worker = tree.child.process.try_clone().unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
        // An unexpected worker exit must wake the service observer even while
        // descendants survive. Observing it must not surrender tree ownership.
        assert_ne!(unsafe { TerminateProcess(worker.as_raw_handle(), 23) }, 0);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let status = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), tree.wait())
                .await
                .expect("worker exit observer did not wake")
                .unwrap()
        });
        assert_eq!(status.code(), Some(23));
        drop(tree);
        for handle in [&worker, &descendant] {
            assert_eq!(
                unsafe { WaitForSingleObject(handle.as_raw_handle(), 5_000) },
                WAIT_OBJECT_0
            );
        }
        // EOF must reach a cooperating worker while its job is still owned,
        // proving that no inherited writer masks the graceful stop request.
        command.env(TREE_ROLE_ENV, "lifeline");
        let mut graceful = spawn_owned_logged(&command, &log, &log, CREATE_NO_WINDOW).unwrap();
        graceful.request_stop();
        let status = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), graceful.wait())
                .await
                .unwrap()
                .unwrap()
        });
        assert!(status.success());
    }

    impl Drop for FixtureChild {
        fn drop(&mut self) {
            // A failed assertion must not leave even a test child behind. This
            // owns only the exact handle returned by this test's spawn.
            unsafe {
                TerminateProcess(self.0.process.as_raw_handle(), 1);
                WaitForSingleObject(self.0.process.as_raw_handle(), 5_000);
            }
        }
    }

    // what this catches (card 9bc0fc5e): redirected stdio alone leaked a launch
    // shell's pipe writers. The actual child must leave an unrelated reader at
    // EOF WHILE alive, and still receive its intended I/O, cwd, args and env.
    #[test]
    fn detached_child_closes_unrelated_pipe_and_preserves_launch_contract() {
        // RUN ALONE (card 64ca188f). The check makes an INHERITABLE pipe writer on purpose, to
        // prove the child does not receive it. But std spawns every process with handle
        // inheritance on, so any sibling test in this binary that spawned a process in that
        // window inherited the writer and held the pipe open: the check then failed on a
        // child that was not the one under test (four heads on 2026-09-28, the canary tip
        // included, where a red tip blocks every deploy). So the check re-runs itself in a
        // fresh process where it is the only test, and nothing else can spawn meanwhile.
        if std::env::var_os(ISOLATED_ENV).is_none() {
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", ISOLATED_TEST, "--nocapture", "--test-threads=1"])
                .env(ISOLATED_ENV, "1")
                .status()
                .unwrap();
            assert!(
                status.success(),
                "the isolated pipe-leak check failed: {status} (its output is above)"
            );
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("core launch λ");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let log_path = root.join("child.log");
        let log = File::create(&log_path).unwrap();
        let log_error = log.try_clone().unwrap();
        let (reader, writer) = std::io::pipe().unwrap();
        let inherited_writer = inheritable_duplicate(writer.as_raw_handle()).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", FIXTURE_TEST, "--nocapture", "--test-threads=1"]);
        for arg in QUOTED_ARGS {
            command.arg("--skip").arg(arg);
        }
        command
            .current_dir(&root)
            .env(FIXTURE_ENV, &root)
            .env("CONTINUUM_LAUNCH_CASE", "old")
            .env("continuum_launch_case", "configured λ")
            .env("systemroot", std::env::var_os("SystemRoot").unwrap())
            .env_remove("USERPROFILE");
        // Exercise the same native boundary used on the job-denied fallback.
        // The test runner's own job policy need not permit breakaway.
        let mut child = FixtureChild(
            spawn_logged(
                &command,
                &log,
                &log_error,
                CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            )
            .unwrap(),
        );
        drop(writer);
        drop(inherited_writer);

        let deadline = Instant::now() + Duration::from_secs(30);
        while !root.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "fixture exited early: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "fixture did not become ready; child log: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "EOF must precede child exit"
        );
        let result = unsafe {
            PeekNamedPipe(
                reader.as_raw_handle(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        let error = io::Error::last_os_error();
        assert_eq!(
            result, 0,
            "the detached child retained the unrelated pipe writer"
        );
        assert_eq!(error.raw_os_error(), Some(ERROR_BROKEN_PIPE as i32));
        let output = std::fs::read_to_string(&log_path).unwrap();
        assert!(output.contains("detached fixture stdout"));
        assert!(output.contains("detached fixture stderr"));
        std::fs::write(root.join("release"), b"release").unwrap();
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "fixture status: {status}");
                break;
            }
            assert!(Instant::now() < deadline, "fixture did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
