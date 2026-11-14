use super::ExitWaitError;

/// No exit event is known on this OS.
pub(super) fn wait_exit(_pid: u32) -> Result<(), ExitWaitError> {
    Err(ExitWaitError::Unsupported)
}
