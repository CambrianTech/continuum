use super::Unobservable;

/// No zombie query is known on this OS: never proof of exit.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}

/// No exit event is known on this OS.
pub(super) fn wait_exit(_pid: u32) -> Result<(), Unobservable> {
    Err(Unobservable::Unsupported)
}
