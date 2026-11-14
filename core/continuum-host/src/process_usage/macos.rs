//! macOS: `proc_pid_rusage` (libproc, no entitlement for our own user's processes). The same
//! `rusage_info_v4` layout `continuum-engine`'s `lane_footprint` reads for the phys footprint:
//! `ri_uuid[16]`, then 35 u64, of which v0's first two are `ri_user_time` and `ri_system_time`.
//! Those two are in Mach absolute time units (nanoseconds on Intel, not on Apple silicon), so
//! they are converted through `mach_timebase_info`.

use std::time::Duration;

#[repr(C)]
struct RusageInfoV4 {
    uuid: [u8; 16],
    fields: [u64; 35],
}

#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

extern "C" {
    fn proc_pid_rusage(pid: libc::c_int, flavor: libc::c_int, buffer: *mut RusageInfoV4) -> libc::c_int;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
}

const RUSAGE_INFO_V4: libc::c_int = 4;
const USER_TIME: usize = 0;
const SYSTEM_TIME: usize = 1;

pub(super) fn cpu_time(pid: u32) -> Option<Duration> {
    let mut info = RusageInfoV4 { uuid: [0; 16], fields: [0; 35] };
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v4; libproc only writes
    // within it for flavor V4.
    if unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V4, &mut info) } != 0 {
        return None;
    }
    let mut base = MachTimebaseInfo { numer: 0, denom: 0 };
    // SAFETY: writes exactly one mach_timebase_info_data_t into the struct given.
    if unsafe { mach_timebase_info(&mut base) } != 0 || base.denom == 0 {
        return None;
    }
    let ticks = u128::from(info.fields[USER_TIME]) + u128::from(info.fields[SYSTEM_TIME]);
    let nanos = ticks * u128::from(base.numer) / u128::from(base.denom);
    u64::try_from(nanos).ok().map(Duration::from_nanos)
}

/// `ri_phys_footprint`, index 7 of `rusage_info_v4` after the uuid (v0: user_time, system_time,
/// pkg_idle_wkups, interrupt_wkups, pageins, wired_size, resident_size, PHYS_FOOTPRINT). The
/// CURRENT footprint, never the lifetime maximum (index 28), which holds a process's load and
/// read 19.02 GB against a running 11 GB engine on the M5 (2026-10-10).
pub(super) fn anon_footprint(pid: u32) -> Option<u64> {
    const PHYS_FOOTPRINT: usize = 7;
    let mut info = RusageInfoV4 { uuid: [0; 16], fields: [0; 35] };
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v4; libproc only writes
    // within it for flavor V4.
    let rc = unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V4, &mut info) };
    (rc == 0).then(|| info.fields[PHYS_FOOTPRINT])
}

/// `KERN_PROCARGS2` through `sysctl`, parsed by [`super::parse_procargs2`].
pub(super) fn args(pid: u32) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = 0;
    // SAFETY: a size query (null buffer) on a valid mib.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: the buffer is `size` writable bytes, as the query returned.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    buf.truncate(size);
    super::parse_procargs2(&buf)
}
