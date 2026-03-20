fn main() {
    // macOS: LiveKit's native WebRTC library uses Objective-C categories (via abseil).
    // Without -ObjC, category methods like +[NSString stringForAbslStringView:] are
    // not loaded from static libraries, causing runtime crashes:
    //   "unrecognized selector sent to class" in RTCVideoEncoderVP9 / RTCDefaultVideoEncoderFactory
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-ObjC");

    // Linux: webrtc-sys bundles protozero_plugin.o which contains a `main` symbol
    // that conflicts with our binary crate `main` functions. Allow multiple definitions
    // so the linker picks our main over the bundled plugin's.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
}
