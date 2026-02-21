fn main() {
    // macOS: LiveKit's native WebRTC library uses Objective-C categories (via abseil).
    // Without -ObjC, category methods like +[NSString stringForAbslStringView:] are
    // not loaded from static libraries, causing runtime crashes:
    //   "unrecognized selector sent to class" in RTCVideoEncoderVP9 / RTCDefaultVideoEncoderFactory
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-ObjC");
}
