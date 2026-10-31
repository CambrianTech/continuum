//! macOS: `sample <pid> <seconds> -file <out>` (ships with the OS).

use std::path::Path;

pub(super) fn sampler(pid: u32, seconds: u32, out: &Path) -> Option<super::Sampler> {
    Some(super::Sampler {
        program: "sample",
        args: vec![pid.to_string(), seconds.to_string(), "-file".into(), out.to_string_lossy().into_owned()],
        writes_out: true,
    })
}
