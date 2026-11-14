//! The edges that ask the process to stop. Unix: SIGTERM (a stop script, `kill`) and SIGINT
//! (Ctrl+C). Windows, which has neither: Ctrl+C, console close (what `taskkill` and a stop script
//! deliver) and system shutdown or logoff. The core runs ONE stop on whichever arrives first
//! (moved from main.rs's two OS blocks for #5842's os_branches; the stop itself is unchanged).

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Which edge asked the process to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// SIGTERM.
    Terminate,
    /// SIGINT, or Ctrl+C on Windows.
    Interrupt,
    /// Windows: the console window closed.
    ConsoleClose,
    /// Windows: system shutdown or logoff.
    SystemShutdown,
}

impl Edge {
    /// The edge's name, for the stop's log line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Terminate => "SIGTERM",
            Self::Interrupt => "interrupt (SIGINT / Ctrl+C)",
            Self::ConsoleClose => "console close",
            Self::SystemShutdown => "system shutdown",
        }
    }
}

/// The first stop edge to arrive. An edge that cannot be installed waits forever, so the others
/// still stop the process; `Err` only when none could be installed, and then the process stops
/// only by being killed.
pub async fn first_edge() -> std::io::Result<Edge> {
    os::first_edge().await
}
