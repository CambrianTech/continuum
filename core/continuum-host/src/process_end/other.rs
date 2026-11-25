use super::{Incarnation, Refusal};

/// No kill is known on this OS: nothing is signalled.
pub(super) fn kill(_pid: u32) {}

pub(super) fn end(incarnation: Incarnation, _still_present: impl FnOnce(Incarnation) -> bool) -> Result<bool, Refusal> {
    Err(Refusal::Os { call: "end", incarnation, source: std::io::Error::from(std::io::ErrorKind::Unsupported) })
}
