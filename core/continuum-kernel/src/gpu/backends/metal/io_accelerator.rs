//! The GPU's own busy share on Apple silicon, read from the IOKit registry: the first
//! `IOAccelerator` service's `PerformanceStatistics` dictionary, key `Device Utilization %`
//! (the number `ioreg -c IOAccelerator` prints; no root, no IOReport subscription).
//!
//! Joel, 2026-10-31: "Success is completed allocated and active gpu." The Metal backend
//! reported no utilization at all (`utilization: None`), so the core could not see the
//! number the node is judged by. Read by `MetalProbe::sample` on its existing tick: a
//! registry property read, no process spawned.
//!
//! Every miss is `None` (no service, no dictionary, no key, not a number): the monitor
//! keeps its last measured value, it never substitutes.

use std::ffi::{c_char, c_void, CString};

type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFNumberRef = *const c_void;
type IoObject = u32;

const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const KCF_NUMBER_SINT64_TYPE: i64 = 4;
/// `kIOMainPortDefault`: the default main port is the null port.
const KIO_MAIN_PORT_DEFAULT: u32 = 0;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(alloc: CFAllocatorRef, c_str: *const c_char, encoding: u32) -> CFStringRef;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: CFNumberRef, the_type: i64, value_ptr: *mut c_void) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFRelease(cf: CFTypeRef);
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFDictionaryRef;
    fn IOServiceGetMatchingService(main_port: u32, matching: CFDictionaryRef) -> IoObject;
    fn IORegistryEntryCreateCFProperty(entry: IoObject, key: CFStringRef, allocator: CFAllocatorRef, options: u32) -> CFTypeRef;
    fn IOObjectRelease(object: IoObject) -> i32;
}

/// A CF object this file created, released on drop.
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is a non-null CF object returned by a CF `Create` call above,
            // owned by this guard alone and released exactly once.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn cf_string(text: &str) -> Option<Owned> {
    let c_text = CString::new(text).ok()?;
    // SAFETY: `c_text` is a valid NUL-terminated string for the duration of the call; a null
    // allocator is `kCFAllocatorDefault`.
    let string = unsafe { CFStringCreateWithCString(std::ptr::null(), c_text.as_ptr(), KCF_STRING_ENCODING_UTF8) };
    (!string.is_null()).then_some(Owned(string))
}

/// The GPU's busy share now, 0.0..=1.0, or `None` when the registry does not say.
pub(super) fn read_device_utilization() -> Option<f32> {
    let class = CString::new("IOAccelerator").ok()?;
    // SAFETY: `IOServiceMatching` takes a NUL-terminated class name and returns a +1
    // dictionary that `IOServiceGetMatchingService` consumes (releases) whatever it returns.
    let service = unsafe { IOServiceGetMatchingService(KIO_MAIN_PORT_DEFAULT, IOServiceMatching(class.as_ptr())) };
    if service == 0 {
        return None;
    }
    let percent = utilization_of(service);
    // SAFETY: `service` is a non-zero io_object_t returned above, released once here.
    unsafe { IOObjectRelease(service) };
    // A share outside 0..=100 is not a reading: it is no reading, never clamped into one.
    percent.filter(|percent| (0..=100).contains(percent)).map(|percent| percent as f32 / 100.0)
}

fn utilization_of(service: IoObject) -> Option<i64> {
    let stats_key = cf_string("PerformanceStatistics")?;
    // SAFETY: `service` is a live io_object_t; the key is a live CFString; the result is a +1
    // CF object (or null) owned by the guard below.
    let stats = Owned(unsafe { IORegistryEntryCreateCFProperty(service, stats_key.0, std::ptr::null(), 0) });
    // SAFETY: `CFGetTypeID` reads the type of a live CF object; checked non-null first.
    if stats.0.is_null() || unsafe { CFGetTypeID(stats.0) != CFDictionaryGetTypeID() } {
        return None;
    }
    let busy_key = cf_string("Device Utilization %")?;
    // SAFETY: `stats` is a live CFDictionary (type checked above); the returned value is
    // borrowed from it (Get rule), valid while `stats` lives, and never released here.
    let value = unsafe { CFDictionaryGetValue(stats.0, busy_key.0) };
    // SAFETY: as above, `value` is checked non-null before its type is read.
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut percent: i64 = 0;
    // SAFETY: `value` is a live CFNumber; `percent` is a valid i64 out-pointer for SInt64.
    let converted = unsafe { CFNumberGetValue(value, KCF_NUMBER_SINT64_TYPE, (&mut percent as *mut i64).cast()) };
    (converted != 0).then_some(percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the registry read wired to a wrong key, type or ownership rule on a
    // Mac with a GPU (a `None` where `ioreg -c IOAccelerator` prints `Device Utilization %`,
    // or a value outside 0..=1). Read twice, so a release done twice or not at all shows as a
    // crash or a leak-checker failure on the second read.
    #[test]
    fn the_gpu_reports_its_busy_share_between_zero_and_one() {
        for _ in 0..2 {
            let share = read_device_utilization().expect("an Apple GPU publishes Device Utilization % on its IOAccelerator"); // expect: this test runs only on macOS, where every Metal device registers an IOAccelerator
            assert!((0.0..=1.0).contains(&share), "{share}");
        }
    }
}
