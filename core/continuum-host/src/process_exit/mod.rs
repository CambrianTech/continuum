//! Whether a process that still answers by pid has in fact EXITED and only waits for its reaper
//! (a zombie). Its pid answers `kill(pid, 0)`, so a liveness probe that stops there reads it as
//! running, and a resource bound to it is held forever (card e4ef7780). What differs by OS is the
//! call that can tell: macOS lists no zombie in its process table and only `proc_pidinfo` can
//! tell (measured 2026-10-28: running -> 136 bytes; zombie -> 0 with ESRCH); Linux keeps zombies
//! in /proc, where a process table reader already sees their status, so it has nothing to add.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod os;

/// True only on positive evidence that `pid` exited and awaits its reaper. Anything short of
/// that is false: never proof of exit.
pub fn exited_unreaped(pid: u32) -> bool {
    os::exited_unreaped(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card e4ef7780): a zombie read as running, so its engine's residency held
    // serving forever. A real child: not exited while it runs, exited once it ends and before it
    // is reaped (macOS: proc_pidinfo's ESRCH; elsewhere the process table reader's job, so false).
    #[cfg(unix)]
    #[test]
    fn a_running_child_is_not_exited_and_an_unreaped_one_is_on_macos() {
        use std::io::Write;
        struct Reaped(std::process::Child);
        impl Drop for Reaped {
            fn drop(&mut self) {
                let _ = self.0.kill(); // best effort: an exited child refuses the kill
                let _ = self.0.wait(); // the reap; nothing to report from a drop
            }
        }
        let mut child = Reaped(
            std::process::Command::new("sh")
                .args(["-c", "read line"])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .expect("test: sh spawns"),
        );
        let pid = child.0.id();
        assert!(!exited_unreaped(pid), "a running child has not exited");
        child.0.stdin.take().expect("test: piped stdin").write_all(b"done\n").expect("test: write");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while exited_unreaped(pid) != cfg!(target_os = "macos") {
            assert!(std::time::Instant::now() < deadline, "test: the exited child never read as exited on macOS");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // a pid that never existed is not proof of anything
        assert!(!exited_unreaped(u32::MAX >> 1), "an absent pid is not an unreaped zombie");
    }
}
