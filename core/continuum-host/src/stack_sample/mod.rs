//! A stack sample of another process: what each of its threads is doing now (card 3913621a).
//! The engine that stopped answering a KV page restore on enrico twice on 2026-10-27 was
//! condemned with nothing captured, so neither hang could say whether the engine was blocked
//! writing its log, waiting on a lock, or busy. What differs by OS is only the sampler and how
//! it is asked; running it (bounded, through the kernel's one spawn path) is the caller's.

use std::path::Path;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(all(unix, not(target_os = "macos")), path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// How to sample a process on this host: the program, its arguments, and whether it writes the
/// sample to the output path itself (otherwise its standard output is the sample).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sampler {
    pub program: &'static str,
    pub args: Vec<String>,
    pub writes_out: bool,
}

/// The sampler for `pid` over `seconds`, writing to `out`. `None` on a host with no sampler.
pub fn sampler(pid: u32, seconds: u32, out: &Path) -> Option<Sampler> {
    os::sampler(pid, seconds, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a sampler that names no process, or that would write nowhere: the
    // pid is always an argument, and a sampler either writes `out` or answers on stdout.
    #[test]
    fn a_sampler_names_its_process_and_where_the_sample_goes() {
        let out = Path::new("/tmp/engine-stall.txt");
        if let Some(s) = sampler(4242, 2, out) {
            assert!(s.args.iter().any(|a| a == "4242"), "{s:?}");
            assert_eq!(s.writes_out, s.args.iter().any(|a| a == &out.to_string_lossy()), "{s:?}");
        }
    }
}
