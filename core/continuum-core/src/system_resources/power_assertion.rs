//! A hosting node does not sleep — the core holds the assertion itself.
//!
//! Card 94a95a98 (2026-09-07, the M5): a hosting Mac idle-slept on battery, then cycled
//! Maintenance Sleep on AC for three hours while hosting five citizens; the core ran only
//! inside 180 s dark-wake windows and every hourly number that evening was taken on a
//! suspended machine. The fix then was the start script wrapping the exec in
//! `caffeinate -s -i`. A supervised launch (launchd, the binary as the command — #4228)
//! has no script, so the assertion moves to the layer that cannot forget it: the core
//! takes it at boot and holds it for its own lifetime. macOS releases an IOPM assertion
//! when its owning process exits, exactly the lifetime `caffeinate`'s wrap gave it.
//!
//! Two assertions, the same pair `caffeinate -s -i` takes: `PreventSystemSleep` (`-s`,
//! the machine stays up on AC) and `PreventUserIdleSystemSleep` (`-i`, idle does not
//! sleep it). On battery macOS may still sleep; the core says so itself (boot.absent,
//! `absence_watch`). Not a monitor: no task, no tick — one call, two handles, a probe.
//! Other platforms: a no-op (the Windows task and systemd unit own their own policy).

/// Take the process-lifetime sleep assertions. Idempotent: a second call is a no-op.
/// Never fails the boot — a refused assertion is a `power.assertion.refused` probe with
/// the IOReturn code, and the node runs (the absence watch will name the sleeps).
pub fn hold_for_process() {
    #[cfg(target_os = "macos")]
    macos::hold();
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void, CString};
    use std::sync::OnceLock;

    type CFStringRef = *const c_void;
    type IOReturn = i32;
    type IOPMAssertionID = u32;
    const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            level: u32,
            name: CFStringRef,
            id: *mut IOPMAssertionID,
        ) -> IOReturn;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    /// The pair `caffeinate -s -i` holds, by their IOKit names.
    const ASSERTIONS: [(&str, &str); 2] = [
        ("PreventSystemSleep", "continuum-core: hosting node (caffeinate -s)"),
        ("PreventUserIdleSystemSleep", "continuum-core: hosting node (caffeinate -i)"),
    ];

    /// Held for the process lifetime; the ids are kept only so the take is idempotent
    /// and the probe can name them.
    static HELD: OnceLock<Vec<(&'static str, Result<IOPMAssertionID, IOReturn>)>> = OnceLock::new();

    fn cf(s: &str) -> Option<CFStringRef> {
        let c = CString::new(s).ok()?;
        // SAFETY: a valid NUL-terminated C string and the UTF-8 encoding constant;
        // CoreFoundation copies the bytes. Null on allocation failure, checked below.
        let r = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) };
        (!r.is_null()).then_some(r)
    }

    fn take(kind: &str, name: &str) -> Result<IOPMAssertionID, IOReturn> {
        let (Some(k), Some(n)) = (cf(kind), cf(name)) else {
            return Err(-1);
        };
        let mut id: IOPMAssertionID = 0;
        // SAFETY: both CFStrings are live for the call and released after; `id` is ours
        // to be written. The assertion is owned by this process until it exits.
        let rc = unsafe { IOPMAssertionCreateWithName(k, K_IOPM_ASSERTION_LEVEL_ON, n, &mut id) };
        // SAFETY: created above by this module, released exactly once.
        unsafe {
            CFRelease(n);
            CFRelease(k);
        }
        if rc == 0 { Ok(id) } else { Err(rc) }
    }

    pub fn hold() {
        let held = HELD.get_or_init(|| ASSERTIONS.iter().map(|(kind, name)| (*kind, take(kind, name))).collect());
        for (kind, outcome) in held {
            match outcome {
                Ok(id) => crate::probe!(
                    class = "power.assertion.held",
                    kind = *kind,
                    assertion_id = *id as u64,
                    "the core holds this sleep assertion for its lifetime — the seat stays awake without a caffeinate wrapper"
                ),
                Err(rc) => crate::probe!(
                    class = "power.assertion.refused",
                    kind = *kind,
                    io_return = *rc as i64,
                    "IOKit refused the sleep assertion; the node may sleep unattended — the absence watch will say when"
                ),
            }
        }
    }
}
