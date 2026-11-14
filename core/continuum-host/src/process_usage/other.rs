//! No adapter yet (Windows would read `GetProcessTimes`): no reading, which callers hold as
//! unknown, never as zero.

pub(super) fn cpu_time(_pid: u32) -> Option<std::time::Duration> {
    None
}

pub(super) fn anon_footprint(_pid: u32) -> Option<u64> {
    None
}

pub(super) fn args(_pid: u32) -> Option<Vec<String>> {
    None
}
