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
