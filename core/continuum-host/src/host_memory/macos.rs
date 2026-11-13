//! macOS: Mach `host_statistics64(HOST_VM_INFO64)` for the page counts, the
//! `vm.vm_page_free_target` sysctl for the pageout daemon's target and the
//! `kern.memorystatus_vm_pressure_level` sysctl for the kernel's own pressure verdict, converted
//! by `MachVmCounts`.
//! The struct layout is `mach/vm_statistics.h`'s `vm_statistics64_data_t` (stable since 10.7).

use std::mem::size_of;

use super::{HostMemoryReading, MachVmCounts};

/// Fields this reader does not use are `_`-named: they hold the kernel's layout.
#[repr(C)]
#[derive(Default)]
struct VmStatistics64 {
    free_count: libc::c_uint,
    _active_count: libc::c_uint,
    _inactive_count: libc::c_uint,
    _wire_count: libc::c_uint,
    _zero_fill_count: u64,
    _reactivations: u64,
    _pageins: u64,
    _pageouts: u64,
    _faults: u64,
    _cow_faults: u64,
    _lookups: u64,
    _hits: u64,
    _purges: u64,
    purgeable_count: libc::c_uint,
    speculative_count: libc::c_uint,
    _decompressions: u64,
    _compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: libc::c_uint,
    _throttled_count: libc::c_uint,
    external_page_count: libc::c_uint,
    _internal_page_count: libc::c_uint,
    _total_uncompressed_pages_in_compressor: u64,
}

const HOST_VM_INFO64: libc::c_int = 4;
/// `HOST_VM_INFO64_COUNT`: the buffer's size in `integer_t` slots, so the kernel writes exactly it.
const HOST_VM_INFO64_COUNT: libc::c_uint = (size_of::<VmStatistics64>() / size_of::<libc::c_int>()) as libc::c_uint;
const KERN_SUCCESS: libc::c_int = 0;

pub(super) fn read() -> Option<HostMemoryReading> {
    let mut info = VmStatistics64::default();
    let mut count = HOST_VM_INFO64_COUNT;
    // SAFETY: `info` is a writable vm_statistics64 and `count` its size in integer_t slots; the
    // kernel writes at most `count` slots. `mach_host_self` is deprecated in libc for mach2,
    // which is not a dependency.
    #[allow(deprecated)]
    let kr = unsafe {
        libc::host_statistics64(
            libc::mach_host_self(),
            HOST_VM_INFO64,
            &mut info as *mut VmStatistics64 as *mut libc::c_int,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }
    // SAFETY: sysconf reads a configuration value; it has no memory effects.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok().filter(|p| *p > 0)?;
    Some(
        MachVmCounts {
            page_size,
            free_count: u64::from(info.free_count),
            speculative_count: u64::from(info.speculative_count),
            external_page_count: u64::from(info.external_page_count),
            purgeable_count: u64::from(info.purgeable_count),
            compressor_page_count: u64::from(info.compressor_page_count),
            swapins: info.swapins,
            swapouts: info.swapouts,
            free_target_pages: u64::from(sysctl_u32(c"vm.vm_page_free_target")?),
            // Unreadable: no verdict, the counters still stand.
            memorystatus_pressure_level: sysctl_u32(c"kern.memorystatus_vm_pressure_level"),
        }
        .reading(),
    )
}

/// A 32-bit integer sysctl by name (`vm.vm_page_free_target` pages, the memorystatus level).
fn sysctl_u32(name: &std::ffi::CStr) -> Option<u32> {
    let mut value: libc::c_uint = 0;
    let mut len = size_of::<libc::c_uint>();
    // SAFETY: the name is NUL-terminated and `value`/`len` describe a writable c_uint; sysctl
    // writes at most `len` bytes and updates `len`.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut value as *mut libc::c_uint as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && len == size_of::<libc::c_uint>()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the count handed to host_statistics64 drifting from the struct (the
    // kernel writes past the buffer or truncates it), and the call or a sysctl failing on a live
    // Mac, which would read every Mac as unknown (or with no pressure verdict).
    #[test]
    fn a_live_mac_reads_and_the_count_matches_the_struct() {
        assert_eq!(HOST_VM_INFO64_COUNT as usize, size_of::<VmStatistics64>() / size_of::<libc::c_int>());
        let reading = read().expect("test: host_statistics64 and the sysctl succeed on a Mac");
        assert!(reading.free_target_bytes > 0, "the pageout daemon always has a target");
        assert!(reading.available_bytes() > 0, "a live Mac has free or cached pages");
        assert!(reading.pressure.is_some(), "the kernel always has a memorystatus level");
    }
}
