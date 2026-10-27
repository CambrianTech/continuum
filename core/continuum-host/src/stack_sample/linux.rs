//! Linux and other Unix: `eu-stack -p <pid>` (elfutils) prints every thread's stack once.

use std::path::Path;

pub(super) fn sampler(pid: u32, _seconds: u32, _out: &Path) -> Option<super::Sampler> {
    Some(super::Sampler { program: "eu-stack", args: vec!["-p".into(), pid.to_string()], writes_out: false })
}
