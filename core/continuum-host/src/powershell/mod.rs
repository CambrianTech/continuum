//! Hidden Windows PowerShell execution. Callers own their protocol and diagnostics;
//! this capability retains native process output and typed launch failures.
#[derive(Debug, thiserror::Error)]
pub enum PowerShellError {
    #[error("SystemRoot is unset")]
    MissingSystemRoot,
    #[error("PowerShell execution timed out")]
    TimedOut,
    #[error("cannot launch PowerShell: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("PowerShell exited with {status}; stderr bytes: {stderr:?}")]
    Exit {
        status: std::process::ExitStatus,
        stderr: Vec<u8>,
    },
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::run;
