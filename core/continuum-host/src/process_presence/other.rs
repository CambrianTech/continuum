use super::Presence;

/// No kernel query is known on this OS: nothing can be proven either way.
pub(super) fn kernel_says(_pid: u32) -> Presence {
    Presence::Unreadable
}
