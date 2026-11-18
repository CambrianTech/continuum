//! Whether a process that still answers by pid has in fact EXITED and only waits for its reaper
//! (a zombie). Its pid answers `kill(pid, 0)`, so a liveness probe that stops there reads it as
//! running, and a resource bound to it is held forever (card e4ef7780). What differs by OS is the
//! call that can tell: macOS lists no zombie in its process table and only `proc_pidinfo` can
//! tell (measured 2026-10-28: running -> 136 bytes; zombie -> 0 with ESRCH); Linux keeps zombies
//! in /proc, where a process table reader already sees their status, so it has nothing to add.
<<<<<<< HEAD
//!
//! And the EXIT ITSELF as the event the kernel delivers, never a liveness poll (card b5b9bef3):
//! every stop, reboot, kill and engine retirement polled `pid_alive` because nothing could await
//! another process's exit. [`exited`] is that one owner. What differs by OS is the call: macOS a
//! kqueue `EVFILT_PROC` / `NOTE_EXIT` filter, Linux a pidfd that turns readable, Windows the
//! process handle turning signalled. A child this process owns is awaited by `Child::wait`.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), path = "other.rs")]
mod os;

/// How awaiting a process's exit ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitObserved {
    /// The process ended (or had already ended: an absent pid, a zombie awaiting its reaper).
    Exited,
    /// The kernel could not deliver this exit; never read as an exit.
    Unobservable(Unobservable),
}

/// Why an exit could not be observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unobservable {
    /// This OS has no exit event this seam knows how to ask for.
    Unsupported,
    /// The kernel refused to watch the pid; its own words.
    Refused { pid: u32, error: String },
    /// The thread watching for the exit ended without an answer.
    WatcherLost { pid: u32 },
}

impl std::fmt::Display for Unobservable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("this OS offers no process-exit event to wait on"),
            Self::Refused { pid, error } => write!(f, "the kernel refused to watch pid {pid}'s exit: {error}"),
            Self::WatcherLost { pid } => write!(f, "the watcher of pid {pid}'s exit ended without an answer"),
        }
    }
}

impl std::error::Error for Unobservable {}

/// Why an OS adapter armed no watch.
pub(super) enum Armless {
    /// No process holds the pid: the process it named has exited.
    AlreadyExited,
    Unobservable(Unobservable),
}

/// An exit watch armed on ONE process: the kernel binding (macOS kqueue registration, Linux pidfd, Windows
/// process handle) was acquired before the caller's identity check, so it names the process that check proved,
/// and waiting never resolves the pid again.
pub struct ArmedExit {
    pid: u32,
    armed: os::Armed,
}

/// Why [`arm`] refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmRefusal {
    /// No process holds the pid: the expected one has already exited.
    AlreadyExited,
    /// A process holds the pid, but the identity check says it is not the expected one (a reused pid).
    NotThatProcess,
    /// The kernel could not bind a watch; never read as an exit.
    Unobservable(Unobservable),
}

/// Arm an exit watch on `pid` for the process `is_expected` identifies (its OS start time, say): the kernel
/// binding is acquired first, without blocking, THEN `is_expected` runs, so a true answer proves the binding is
/// that process even if the pid is reused the next instant (Codex on #6099).
pub fn arm(pid: u32, is_expected: impl FnOnce() -> bool) -> Result<ArmedExit, ArmRefusal> {
    let armed = os::arm(pid).map_err(|armless| match armless {
        Armless::AlreadyExited => ArmRefusal::AlreadyExited,
        Armless::Unobservable(why) => ArmRefusal::Unobservable(why),
    })?;
    if !is_expected() {
        return Err(ArmRefusal::NotThatProcess);
    }
    Ok(ArmedExit { pid, armed })
}

impl ArmedExit {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Resolves when the armed process exits: the kernel's event on the binding already held. The OS wait
    /// blocks, so it runs on its own DETACHED thread (see [`exited`]).
    pub async fn exited(self) -> ExitObserved {
        let pid = self.pid;
        // A watch, not a oneshot: publishing replaces the value and cannot fail.
        let (tx, mut rx) = tokio::sync::watch::channel(None);
        let armed = self.armed;
        let spawned = std::thread::Builder::new().name(format!("exit-watch-{pid}")).spawn(move || {
            tx.send_replace(Some(os::wait(armed)));
        });
        if let Err(error) = spawned {
            return ExitObserved::Unobservable(Unobservable::Refused { pid, error: format!("no thread to watch from: {error}") });
        }
        let answered = match rx.wait_for(Option::is_some).await {
            Ok(answered) => (*answered).clone(),
            Err(_watcher_gone) => None,
        };
        match answered {
            Some(Ok(())) => ExitObserved::Exited,
            Some(Err(unobservable)) => ExitObserved::Unobservable(unobservable),
            None => ExitObserved::Unobservable(Unobservable::WatcherLost { pid }),
        }
    }
}

/// Resolves when `pid` exits: the kernel's event, the moment it happens, with no timer of its own.
/// A caller that wants a bound wraps it in a watchdog that REPORTS a process that never went.
///
/// The OS wait blocks, so it runs on its own DETACHED thread, never `spawn_blocking`: a runtime
/// waits for its blocking tasks when it drops, so a watchdog that fired on a process that never
/// exits would then hang the caller's own exit on that process.
pub async fn exited(pid: u32) -> ExitObserved {
    match os::arm(pid) {
        Ok(armed) => ArmedExit { pid, armed }.exited().await,
        Err(Armless::AlreadyExited) => ExitObserved::Exited,
        Err(Armless::Unobservable(why)) => ExitObserved::Unobservable(why),
    }
}

=======

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod os;

>>>>>>> origin/main
/// True only on positive evidence that `pid` exited and awaits its reaper. Anything short of
/// that is false: never proof of exit.
pub fn exited_unreaped(pid: u32) -> bool {
    os::exited_unreaped(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

<<<<<<< HEAD
    // what this catches (card b5b9bef3): an exit read from a timer instead of the event. The wait
    // is still pending while the child runs, and resolves on its exit; nothing in the test sleeps.
    #[cfg(unix)]
    #[tokio::test]
    async fn exited_resolves_on_the_childs_exit_and_not_before() {
        use std::future::Future;
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "read line"])
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("test: sh spawns");
        let pid = child.id().expect("test: a running child has a pid");
        let exit = exited(pid);
        tokio::pin!(exit);
        std::future::poll_fn(|cx| {
            assert!(exit.as_mut().poll(cx).is_pending(), "a running process has not exited");
            std::task::Poll::Ready(())
        })
        .await;
        let mut stdin = child.stdin.take().expect("test: piped stdin");
        stdin.write_all(b"done\n").await.expect("test: write");
        drop(stdin);
        assert_eq!(exit.await, ExitObserved::Exited, "the exit arrives as the event");
        child.wait().await.expect("test: reap");
    }

    // what this catches (Codex on #6099): a watch trusted for a process the identity check did not prove, and a
    // wait that resolves the pid again. The binding is acquired first: a false identity check refuses it; a true
    // one holds the process, so its exit is seen even when it is reaped before the wait starts.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_armed_watch_is_bound_before_the_identity_check_and_survives_exit_before_the_wait() {
        let mut child = std::process::Command::new("sh").args(["-c", "sleep 30"]).spawn().expect("test: sh spawns");
        let pid = child.id();
        assert!(matches!(arm(pid, || false), Err(ArmRefusal::NotThatProcess)), "an unproven process is refused");
        let armed = arm(pid, || true).expect("a running, proven process arms");
        child.kill().expect("test: end it");
        child.wait().expect("test: reap it before the wait begins");
        assert_eq!(armed.exited().await, ExitObserved::Exited, "the held binding reports the exit, not the pid");
        assert!(matches!(arm(u32::MAX >> 2, || true), Err(ArmRefusal::AlreadyExited)), "no process: already exited");
    }

    // what this catches: a stop that waits on a process that is already gone. No process has this
    // pid, so the kernel answers at once.
    #[tokio::test]
    async fn an_absent_pid_has_already_exited() {
        assert_eq!(exited(u32::MAX >> 2).await, ExitObserved::Exited);
    }

=======
>>>>>>> origin/main
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
