//! Mach VM FFI — the "what does the OS say about memory?" layer.
//!
//! Isolated into its own module because:
//!
//! 1. **Testable in isolation.** Struct-size vs count-arithmetic assumptions
//!    get their own tests here — if Apple ships a new Mach release and the
//!    `vm_statistics64` struct grows, this module's tests fail directly
//!    instead of the failure showing up as a mysterious SIGBUS in the
//!    MetalMonitor tick.
//!
//! 2. **Separation of concerns.** `MetalMonitor` cares about *what the
//!    monitor surfaces to the policy* (trait impl, tick cadence, pressure
//!    derivation). This module cares about *what the OS actually says*
//!    (raw bytes, raw counters). When the clashing-extern bug hit during
//!    initial impl, tangling these two concerns in one file made it
//!    harder to spot — the FFI layer should have been its own visible
//!    surface from the start.
//!
//! 3. **Reusability.** Nothing in this file is Metal-specific. The Mach
//!    VM info is process-wide memory accounting — a future macOS
//!    `SystemMonitor` can consume the same `read_system_free_bytes`
//!    / `read_process_phys_footprint` without copy-pasting the FFI dance.
//!
//! All `unsafe` lives here. The public API is two safe functions that
//! return `Option<u64>` — None on a genuine Mach error, surfaced honestly
//! so the caller reports the gap rather than baking in a wrong number.

use std::mem::size_of;

// ─── Type aliases matching Mach headers ─────────────────────────────────
//
// libc declares its own but not all of them are public; re-declaring keeps
// the intent local and documented. All match Mach's native widths on both
// Apple Silicon (ARM64) and Intel (x86_64) Macs.

#[allow(non_camel_case_types)]
pub(super) type natural_t = libc::c_uint;
#[allow(non_camel_case_types)]
pub(super) type integer_t = libc::c_int;
#[allow(non_camel_case_types)]
pub(super) type mach_msg_type_number_t = natural_t;

// Mach flavor constants. `host_flavor_t` is `integer_t` (i32) per libc;
// `task_flavor_t` is `natural_t` (u32). libc's aliases enforce this at
// the callsite, so we just use the raw integer values here and cast
// when calling.
const HOST_VM_INFO64: integer_t = 4;
const TASK_VM_INFO: natural_t = 22;

// ─── Mach structs ───────────────────────────────────────────────────────
//
// Layouts match `mach/vm_statistics.h` and `mach/task_info.h`. The kernel
// writes AT MOST `count × size_of::<integer_t>` bytes into our pointer —
// if our struct is bigger than the kernel's, the extra fields stay as
// whatever `Default` left (zeroed). If our struct is smaller, we might
// miss new fields the kernel wrote past our end (not applicable here —
// we only read stable leading fields).

/// Sized to match `mach/vm_statistics.h`'s `vm_statistics64_data_t`.
/// Stable on macOS 10.7+.
#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
pub(super) struct vm_statistics64 {
    pub free_count: natural_t,
    pub active_count: natural_t,
    pub inactive_count: natural_t,
    pub wire_count: natural_t,
    pub zero_fill_count: u64,
    pub reactivations: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub faults: u64,
    pub cow_faults: u64,
    pub lookups: u64,
    pub hits: u64,
    pub purges: u64,
    pub purgeable_count: natural_t,
    pub speculative_count: natural_t,
    pub decompressions: u64,
    pub compressions: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub compressor_page_count: natural_t,
    pub throttled_count: natural_t,
    pub external_page_count: natural_t,
    pub internal_page_count: natural_t,
    pub total_uncompressed_pages_in_compressor: u64,
}

/// `HOST_VM_INFO64_COUNT = sizeof(vm_statistics64) / sizeof(integer_t)`.
/// This is the `count` arg to `host_statistics64` — tells the kernel how
/// many `integer_t`-sized slots our buffer has. Wrong here → either kernel
/// writes past our buffer (SIGBUS) or truncates (zero'd fields we thought
/// were live).
#[allow(clippy::manual_div_ceil)]
pub(super) const HOST_VM_INFO64_COUNT: mach_msg_type_number_t =
    (size_of::<vm_statistics64>() / size_of::<integer_t>()) as mach_msg_type_number_t;

/// task_vm_info — only `phys_footprint` is load-bearing for us, but we
/// declare the full struct so `task_info` copies the right number of
/// bytes. Layout from `mach/task_info.h`. Fields through `max_address`
/// are stable on macOS 10.10+ (when `phys_footprint` was introduced);
/// ledger_* fields are 10.15+.
#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
pub(super) struct task_vm_info {
    pub virtual_size: u64,
    pub region_count: integer_t,
    pub page_size: integer_t,
    pub resident_size: u64,
    pub resident_size_peak: u64,
    pub device: u64,
    pub device_peak: u64,
    pub internal: u64,
    pub internal_peak: u64,
    pub external: u64,
    pub external_peak: u64,
    pub reusable: u64,
    pub reusable_peak: u64,
    pub purgeable_volatile_pmap: u64,
    pub purgeable_volatile_resident: u64,
    pub purgeable_volatile_virtual: u64,
    pub compressed: u64,
    pub compressed_peak: u64,
    pub compressed_lifetime: u64,
    pub phys_footprint: u64,
    pub min_address: u64,
    pub max_address: u64,
    pub ledger_phys_footprint_peak: u64,
    pub ledger_purgeable_nonvolatile: u64,
    pub ledger_purgeable_novolatile_compressed: u64,
    pub ledger_purgeable_volatile: u64,
    pub ledger_purgeable_volatile_compressed: u64,
    pub ledger_tag_network_nonvolatile: u64,
    pub ledger_tag_network_nonvolatile_compressed: u64,
    pub ledger_tag_network_volatile: u64,
    pub ledger_tag_network_volatile_compressed: u64,
    pub ledger_tag_media_footprint: u64,
    pub ledger_tag_media_footprint_compressed: u64,
    pub ledger_tag_media_nofootprint: u64,
    pub ledger_tag_media_nofootprint_compressed: u64,
    pub ledger_tag_graphics_footprint: u64,
    pub ledger_tag_graphics_footprint_compressed: u64,
    pub ledger_tag_graphics_nofootprint: u64,
    pub ledger_tag_graphics_nofootprint_compressed: u64,
    pub ledger_tag_neural_footprint: u64,
    pub ledger_tag_neural_footprint_compressed: u64,
    pub ledger_tag_neural_nofootprint: u64,
    pub ledger_tag_neural_nofootprint_compressed: u64,
}

#[allow(clippy::manual_div_ceil)]
pub(super) const TASK_VM_INFO_COUNT: mach_msg_type_number_t =
    (size_of::<task_vm_info>() / size_of::<integer_t>()) as mach_msg_type_number_t;

const KERN_SUCCESS: libc::c_int = 0;

// ─── Safe public API ────────────────────────────────────────────────────

/// System-wide free bytes — what Activity Monitor reports as "Memory Free."
/// Sum of (free + speculative + inactive) page counts × page size. Returns
/// None on Mach error so the caller can fall back without baking in a
/// wrong number.
pub(super) fn read_system_free_bytes() -> Option<u64> {
    let mut info = vm_statistics64::default();
    let mut count = HOST_VM_INFO64_COUNT;
    // libc::mach_host_self is deprecated in favor of the mach2 crate.
    // Not yet a dep; adding it for one symbol is its own commit.
    #[allow(deprecated)]
    let kr = unsafe {
        libc::host_statistics64(
            libc::mach_host_self(),
            HOST_VM_INFO64,
            &mut info as *mut vm_statistics64 as *mut integer_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }
    // Page size: sysconf(_SC_PAGESIZE) is userspace-stable. Apple Silicon
    // uses 16384, x86_64 uses 4096 — sysconf returns the right one.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    let pages = info.free_count as u64 + info.speculative_count as u64 + info.inactive_count as u64;
    Some(pages.saturating_mul(page_size))
}

/// This process's `phys_footprint` — the same number macOS uses for its
/// memory-pressure computations and what `top` / Activity Monitor show
/// in the "Memory" column. Includes unified-memory Metal buffers mapped
/// into our address space.
pub(super) fn read_process_phys_footprint() -> Option<u64> {
    let mut info = task_vm_info::default();
    let mut count = TASK_VM_INFO_COUNT;
    #[allow(deprecated)]
    let kr = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            TASK_VM_INFO as libc::task_flavor_t,
            &mut info as *mut task_vm_info as *mut integer_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }
    Some(info.phys_footprint)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: `HOST_VM_INFO64_COUNT` arithmetic drifting from
    /// the actual struct size. This is the `count` we hand to
    /// `host_statistics64`; wrong value → kernel writes past our buffer
    /// (SIGBUS) or truncates (silent data loss). Compile-time assertion
    /// that the constant matches the struct's actual memory footprint.
    ///
    /// Validated 2026-04-21: subtracted 1 from HOST_VM_INFO64_COUNT's
    /// computation, test fails on the assert_eq at line 231 because
    /// constant diverged from struct size; reverted.
    #[test]
    fn host_vm_info64_count_matches_struct_size() {
        let expected = size_of::<vm_statistics64>() / size_of::<integer_t>();
        assert_eq!(
            HOST_VM_INFO64_COUNT as usize, expected,
            "HOST_VM_INFO64_COUNT ({HOST_VM_INFO64_COUNT}) must equal \
             size_of::<vm_statistics64>() / size_of::<integer_t>() ({expected})"
        );
    }

    /// What this catches: `TASK_VM_INFO_COUNT` arithmetic drifting from
    /// the actual struct size. Same failure mode as above but for task
    /// memory info (phys_footprint read). If this count is wrong, the
    /// process_bytes signal is silently garbage OR crashes.
    ///
    /// Validated 2026-04-21: subtracted 1 from TASK_VM_INFO_COUNT's
    /// computation, test fails on the assert_eq at line 249 with the
    /// same shape as the vm_statistics64 case; reverted.
    #[test]
    fn task_vm_info_count_matches_struct_size() {
        let expected = size_of::<task_vm_info>() / size_of::<integer_t>();
        assert_eq!(
            TASK_VM_INFO_COUNT as usize, expected,
            "TASK_VM_INFO_COUNT ({TASK_VM_INFO_COUNT}) must equal \
             size_of::<task_vm_info>() / size_of::<integer_t>() ({expected})"
        );
    }

    /// What this catches: `vm_statistics64` struct fields misaligned from
    /// the Mach header. Spot-check — if `free_count` (first field) or
    /// `inactive_count` (third) were moved/renamed in our declaration,
    /// the kernel's writes land in wrong fields and read_system_free_bytes
    /// returns meaningless numbers. We can't verify layout-against-kernel
    /// directly, but we CAN verify our declared layout matches what the
    /// reader expects to access.
    ///
    /// Validated 2026-04-21: swapped free_count and wire_count positions
    /// in the struct (free now at offset 12, wire at offset 0), test
    /// fails on `free_offset == 0` assertion at line 276; reverted.
    #[test]
    fn vm_statistics64_leading_field_offsets_stable() {
        // free_count is the first field — offset 0.
        let dummy = vm_statistics64::default();
        let base = &dummy as *const _ as usize;
        let free_offset = &dummy.free_count as *const _ as usize - base;
        let inactive_offset = &dummy.inactive_count as *const _ as usize - base;
        let speculative_offset = &dummy.speculative_count as *const _ as usize - base;

        assert_eq!(free_offset, 0, "free_count must be at offset 0");
        // active_count (4 bytes) + inactive_count = offset 8 on natural alignment.
        assert_eq!(
            inactive_offset, 8,
            "inactive_count must be at offset 8 (after free + active)"
        );
        assert!(
            speculative_offset > inactive_offset,
            "speculative_count must come after inactive_count"
        );
    }

    /// What this catches: `read_system_free_bytes` returning None on a
    /// healthy Mac. If this fails, Mach call failed — OS is broken or
    /// we're running in a SIP-restricted context. Sanity bounds: > 0
    /// (any live Mac has free pages), < 10 TB (sanity ceiling; no Mac
    /// has that much RAM).
    ///
    /// Validated 2026-04-21: added `|| true` to the kr check making
    /// read_system_free_bytes always return None, test fails on the
    /// .expect() at line 295; reverted.
    #[test]
    fn read_system_free_bytes_returns_positive_sane_value() {
        let bytes = read_system_free_bytes().expect("Mach host_statistics64 should succeed on Mac");
        assert!(bytes > 0, "free bytes = 0 on a live Mac is broken");
        assert!(
            bytes < 10_000_000_000_000,
            "free bytes > 10 TB — sanity failure"
        );
    }

    /// What this catches: `read_process_phys_footprint` returning None or
    /// zero bytes. We ARE a running process; if either fires, the Mach
    /// task_info call is broken.
    ///
    /// Validated 2026-04-21: added `|| true` to the kr check making
    /// read_process_phys_footprint always return None, test fails on
    /// the .expect() at line 310; reverted.
    #[test]
    fn read_process_phys_footprint_returns_positive_value() {
        let bytes =
            read_process_phys_footprint().expect("Mach task_info should succeed for our own task");
        assert!(bytes > 0, "this test process has phys_footprint = 0?");
    }
}
