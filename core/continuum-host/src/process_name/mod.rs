//! The command name of a pid, read from the kernel: one call, never a spawned `ps` or `tasklist` (the
//! serving daemon's tick spawned `ps` to read one name, measured on the M5 2026-11-17). What differs by
//! OS is the call: macOS asks `proc_pidpath`, Linux reads `/proc/<pid>/comm` (the field `ps -o comm=`
//! reads), Windows asks `QueryFullProcessImageNameW` through a query-only handle.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), path = "other.rs")]
mod os;

/// The basename of `pid`'s executable or command, so a match is on the binary's name, never its
/// install directory. `None` when the pid is gone or the kernel will not say: a caller reads that as
/// unverifiable and refuses to act on the process.
pub fn command_name(pid: u32) -> Option<String> {
    basename(&os::name_of(pid)?)
}

fn basename(name: &str) -> Option<String> {
    let name = name.trim();
    let base = name.rsplit(['/', '\\']).next().filter(|base| !base.is_empty())?;
    Some(base.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a name read that returns nothing for a live process (the whole reap and
    // verify path goes inert, as Windows did before 2026-09-05), one that names a dead pid, and a
    // name that carries its install directory.
    #[test]
    fn our_own_pid_is_named_by_its_binary_and_a_gone_pid_is_not() {
        let named = command_name(std::process::id()).expect("test: our own pid is named"); // expect: the test's own precondition
        assert!(!named.contains('/') && !named.contains('\\'), "a basename: {named}");
        assert!(!named.is_empty());
        assert_eq!(command_name(i32::MAX as u32), None);
        assert_eq!(basename("/usr/local/bin/llama-server\n").as_deref(), Some("llama-server"));
        assert_eq!(basename(r"C:\continuum\bin\llama-server.exe").as_deref(), Some("llama-server.exe"));
        assert_eq!(basename("  "), None);
    }
}
