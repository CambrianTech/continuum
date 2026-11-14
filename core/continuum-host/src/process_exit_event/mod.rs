//! A process's EXIT as the event the kernel delivers, never a liveness poll (card b5b9bef3).
//!
//! A stop that asks "is it gone yet?" every 100-200 ms both wastes the wait and decides on a
//! timer: the CLI's stop and the engine's exit confirmation did exactly that. The kernel already
//! knows the moment a process ends and can say so: macOS through a kqueue `EVFILT_PROC` /
//! `NOTE_EXIT` filter, Linux through a pidfd that turns readable, Windows through the process
//! handle turning signalled. What differs by OS is that one call; the contract is here, once.
//!
//! A bound a caller passes is a WATCHDOG: it never advances anything, it only reports, typed,
//! that the exit never came ([`ExitWaitError::StillRunning`]).

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), path = "other.rs")]
mod os;

use std::time::Duration;

/// Why an exit was not observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitWaitError {
    /// This OS has no exit event this seam knows how to ask for.
    Unsupported,
    /// The kernel refused to watch the pid; its own words.
    Refused { pid: u32, error: String },
    /// The watchdog elapsed with the process still running. A report, never a decision that it
    /// exited.
    StillRunning { pid: u32, watched: Duration },
    /// The thread watching for the exit ended without an answer.
    WatcherLost { pid: u32 },
}

impl std::fmt::Display for ExitWaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("this OS offers no process-exit event to wait on"),
            Self::Refused { pid, error } => write!(f, "the kernel refused to watch pid {pid}'s exit: {error}"),
            Self::StillRunning { pid, watched } => {
                write!(f, "pid {pid} was still running when its {} ms watchdog elapsed", watched.as_millis())
            }
            Self::WatcherLost { pid } => write!(f, "the watcher of pid {pid}'s exit ended without an answer"),
        }
    }
}

impl std::error::Error for ExitWaitError {}

/// Block the calling thread until `pid` exits; `Ok` at once when it already has (including a
/// zombie awaiting its reaper). No bound: see [`exited`] and [`exited_blocking`] for a watched
/// wait. Call it only on a thread that may block.
pub fn wait_exit(pid: u32) -> Result<(), ExitWaitError> {
    os::wait_exit(pid)
}

/// Await `pid`'s exit event, watched by `watchdog`.
///
/// The wait runs on its own DETACHED thread, never `spawn_blocking`: a runtime waits for its
/// blocking tasks when it drops, so a watchdog that fired on a process that never exits would
/// then hang the caller's exit on that same process.
pub async fn exited(pid: u32, watchdog: Duration) -> Result<(), ExitWaitError> {
    // A watch, not a oneshot: publishing replaces the value and cannot fail, so a waiter that
    // already gave up at its watchdog leaves no error for the watcher to drop.
    let (tx, mut rx) = tokio::sync::watch::channel(None);
    spawn_watcher(pid, move |outcome| {
        tx.send_replace(Some(outcome));
    })?;
    match tokio::time::timeout(watchdog, rx.wait_for(Option::is_some)).await {
        Ok(Ok(answered)) => match (*answered).clone() {
            Some(outcome) => outcome,
            None => Err(ExitWaitError::WatcherLost { pid }), // `wait_for` only returns on Some
        },
        Ok(Err(_watcher_gone)) => Err(ExitWaitError::WatcherLost { pid }),
        Err(_elapsed) => Err(ExitWaitError::StillRunning { pid, watched: watchdog }),
    }
}

/// [`exited`] for a caller with no runtime: the same detached watcher, answered on a channel.
pub fn exited_blocking(pid: u32, watchdog: Duration) -> Result<(), ExitWaitError> {
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_watcher(pid, move |outcome| match tx.send(outcome) {
        Ok(()) => {}
        // The waiter gave up at its watchdog and already reported `StillRunning`.
        Err(std::sync::mpsc::SendError(_unheard)) => {}
    })?;
    match rx.recv_timeout(watchdog) {
        Ok(outcome) => outcome,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(ExitWaitError::StillRunning { pid, watched: watchdog }),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(ExitWaitError::WatcherLost { pid }),
    }
}

fn spawn_watcher(pid: u32, answer: impl FnOnce(Result<(), ExitWaitError>) + Send + 'static) -> Result<(), ExitWaitError> {
    std::thread::Builder::new()
        .name(format!("exit-watch-{pid}"))
        .spawn(move || answer(wait_exit(pid)))
        .map(drop)
        .map_err(|error| ExitWaitError::Refused { pid, error: format!("no thread to watch from: {error}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an exit read from a timer instead of the event. The watched wait must
    // stay unanswered while the child runs (the watchdog reports it, typed) and answer once the
    // child ends, long before a second watchdog would.
    #[cfg(unix)]
    #[test]
    fn the_exit_arrives_as_an_event_and_a_running_process_trips_only_the_watchdog() {
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
        let watched = Duration::from_millis(200);
        assert_eq!(exited_blocking(pid, watched), Err(ExitWaitError::StillRunning { pid, watched }));
        let started = std::time::Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(exited_blocking(pid, Duration::from_secs(30))));
        child.0.stdin.take().expect("test: piped stdin").write_all(b"done\n").expect("test: write");
        assert_eq!(rx.recv().expect("test: watcher answers"), Ok(()), "the exit is observed");
        assert!(started.elapsed() < Duration::from_secs(10), "observed by the event, not the 30 s watchdog");
    }

    // what this catches: a stop that waits on a process that is already gone. No process has this
    // pid, so the kernel answers at once.
    #[tokio::test]
    async fn an_absent_pid_has_already_exited() {
        assert_eq!(exited(u32::MAX >> 2, Duration::from_secs(30)).await, Ok(()));
    }
}
