use super::{Armless, Unobservable};

/// No zombie query is known on this OS: never proof of exit.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}

/// No exit event is known on this OS: nothing can be armed.
pub(super) enum Armed {}

pub(super) fn arm(_pid: u32) -> Result<Armed, Armless> {
    Err(Armless::Unobservable(Unobservable::Unsupported))
}

pub(super) fn wait(armed: Armed) -> Result<(), Unobservable> {
    match armed {}
}
