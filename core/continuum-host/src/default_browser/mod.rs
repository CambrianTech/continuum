//! Open a URL in the user's default browser: their browser of choice, never a hardcoded one.
//! The logic (spawn the opener, name it when it cannot run) is here once; each adapter holds only
//! which program opens a URL on its OS. Three callers each carried their own copy of this choice
//! (the CLI's `desktop`, `commands/desktop.rs`, `modules/auth.rs`); this is its one owner.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(all(unix, not(target_os = "macos")), path = "xdg.rs")]
mod os;

/// The opener could not be started.
#[derive(Debug, thiserror::Error)]
#[error("cannot open the browser with `{opener}`: {source}")]
pub struct OpenError {
    pub opener: &'static str,
    #[source]
    pub source: std::io::Error,
}

/// Hand `url` to this OS's default-browser opener. Returns once the opener has started; the
/// browser itself is the user's process, not ours to wait on.
pub fn open(url: &str) -> Result<&'static str, OpenError> {
    let (opener, args) = os::opener(url);
    std::process::Command::new(opener)
        .args(args)
        .spawn()
        .map(|_| opener)
        .map_err(|source| OpenError { opener, source })
}
