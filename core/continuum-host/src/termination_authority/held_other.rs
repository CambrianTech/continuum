// This consent capability requires an OS handle binding inspection and termination.
// A Unix pid plus kill is not such a handle; refuse instead of emulating it unsafely.
pub(super) enum Handle {}

impl Handle {
    pub(super) fn take(_pid: crate::process_id::ProcessId) -> std::io::Result<Self> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported,
            "this host does not provide a held process termination capability"))
    }

    pub(super) fn image(&self) -> std::io::Result<std::path::PathBuf> {
        match *self {}
    }

    pub(super) fn terminate(self, _exit_code: u32) -> std::io::Result<()> {
        match self {}
    }
}
