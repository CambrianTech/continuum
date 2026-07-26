fn main() {
    // Deploy-verification (#194). Embed the git commit this binary was built from as
    // `CONTINUUM_BUILD_GIT_SHA`, so the running service can PROVE which source it is and
    // `continuum reboot` can fail loud when a build silently ran stale (a reboot that reports
    // success while running an old binary is a lie that turns every test into a ghost hunt).
    // Watch `.git/logs/HEAD` (the reflog — updated on every commit/checkout) so the SHA
    // refreshes exactly when HEAD moves, without forcing a rebuild on every `cargo build`.
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=CONTINUUM_BUILD_GIT_SHA={sha}");
    // Relative to this crate (repo/core/continuum-core) the repo `.git` is two levels up.
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

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
