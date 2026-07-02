//! Link configuration for the LiveKit bridge binary.
//!
//! webrtc-sys (pulled in transitively by the `livekit` crate) ships a prebuilt
//! static `libwebrtc.a` that contains Objective-C **categories** — e.g. the
//! `NSString (absl)` category defining `+stringForAbslStringView:`. The Apple
//! linker dead-strips Objective-C category symbols that aren't referenced from a
//! Rust/C++ call site, because categories register themselves at load time via
//! metadata the linker can't see as "used". The symbol then vanishes from the
//! final binary, and the first time webrtc touches it (during `Room::connect`)
//! the ObjC runtime throws:
//!
//!     *** Terminating app due to uncaught exception 'NSInvalidArgumentException',
//!         reason: '+[NSString stringForAbslStringView:]: unrecognized selector
//!         sent to class ...'
//!
//! `-ObjC` tells the linker to load every Objective-C class and category from
//! static archives on the link line (including `libwebrtc.a`), preserving the
//! categories webrtc registers at runtime. This is the canonical fix for ObjC
//! static libraries; it is macOS-only (no effect / not understood elsewhere).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-ObjC");
    }
}
