//! `private_probes`: a resource measured anywhere but the authority's probes. Five VRAM
//! probes, seven private RAM reads and a sentinel with its own `sysinfo` poll disagreed
//! with each other and with the authority (the inventory on canary 365f70fe4); each
//! consumer then sized itself from its own number.

use super::{is_vendored, owned_by};
use crate::{sites, SourceFile, SourceRule, Violation};

/// The authority's probes: the only code that measures a resource.
const OWNERS: &[&str] = &[
    "core/continuum-kernel/src/system_resources/memory_pressure",
    "core/continuum-kernel/src/system_resources/disk_pressure",
    "core/continuum-kernel/src/gpu/monitor.rs",
    "core/continuum-kernel/src/gpu/backends/",
];

/// Calls and paths that read memory, VRAM or disk capacity or usage.
const MEASUREMENTS: &[&str] = &[
    "sysinfo::",
    "System::new",
    ".refresh_memory(",
    ".available_memory(",
    ".total_memory(",
    "mem_get_info",
    "nvidia-smi",
    "recommendedMaxWorkingSetSize",
    "statvfs",
    "task_info(",
    "phys_footprint",
    "/proc/meminfo",
    "/proc/self/statm",
];

pub struct PrivateProbes;

impl SourceRule for PrivateProbes {
    fn name(&self) -> &'static str {
        "private_probes"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if is_vendored(file) || owned_by(file, OWNERS) {
            return Vec::new();
        }
        sites(file, self.name(), |code| MEASUREMENTS.iter().find(|m| code.contains(**m)).map(|m| format!("measures with `{m}`")))
    }
}
