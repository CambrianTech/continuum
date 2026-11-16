//! No adapter yet (Windows would read `GetPerformanceInfo` and the page file): no reading, which
//! callers hold as unknown, never as calm.

pub(super) fn read() -> Option<super::HostMemoryReading> {
    None
}
