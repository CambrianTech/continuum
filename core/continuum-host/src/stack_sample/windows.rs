//! Windows: no sampler ships with the OS, so the stall is named without one.

use std::path::Path;

pub(super) fn sampler(_pid: u32, _seconds: u32, _out: &Path) -> Option<super::Sampler> {
    None
}
