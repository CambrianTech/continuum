fn main() {
    // macOS: LiveKit's native WebRTC library uses Objective-C categories (via abseil).
    // Without -ObjC, category methods like +[NSString stringForAbslStringView:] are
    // not loaded from static libraries, causing runtime crashes:
    //   "unrecognized selector sent to class" in RTCVideoEncoderVP9 / RTCDefaultVideoEncoderFactory
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-ObjC");

    // Linux: webrtc-sys (LiveKit) and ort-sys (ONNX Runtime) both statically link
    // their own copy of protobuf, causing duplicate symbol errors at link time.
    // Allow multiple definitions so the linker picks one.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
}
