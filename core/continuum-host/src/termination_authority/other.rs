use super::TerminationAuthority;

/// No terminate query is known on this OS: nothing can be proven either way.
pub(super) fn may_terminate(_pid: crate::process_id::ProcessId) -> TerminationAuthority {
    TerminationAuthority::Unknown(std::io::Error::from(std::io::ErrorKind::Unsupported))
}
