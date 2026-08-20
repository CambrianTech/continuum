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
    // Auto-incrementing BUILD NUMBER (Joel, 2026-08-08: "versions must always
    // increment and display along with sha in every repo … stale binaries ruin
    // you"). Commit count is monotonic per branch, deterministic, and needs no
    // state file — two builds can be ORDERED at a glance where bare SHAs
    // cannot, which is what turns "is this node stale?" into arithmetic.
    let build_num = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".to_string());
    println!("cargo:rustc-env=CONTINUUM_BUILD_NUMBER={build_num}");
    // Third leg of the version trio: WHEN this binary was compiled. Number
    // orders source, sha names source, built-at catches the case both miss —
    // a binary rebuilt from OLD source after a fix landed (number and sha look
    // plausible; the timestamp says the binary predates the fix). Refreshes
    // when the build script reruns (HEAD moved or clean build), which is
    // exactly the granularity a staleness question needs.
    let built_at = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=CONTINUUM_BUILD_AT={built_at}");
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
