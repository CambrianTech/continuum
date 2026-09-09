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
    // WATCH ONLY WHAT EXISTS (card 58aa1a6a). Two lines used to live here hardcoding
    // `../../.git/logs/HEAD` and `../../.git/HEAD`, on the reasoning that "the repo
    // `.git` is two levels up". That is true in a CLONE and FALSE in a git WORKTREE,
    // where `.git` is a ~66-byte pointer FILE and neither path can resolve.
    //
    // Cargo treats a MISSING `rerun-if-changed` path as permanently stale, so this was
    // not a no-op — it was a guaranteed rebuild forever. Cargo's own words:
    //
    //   build-script-build: dirty: FsStatusOutdated(StaleItem(MissingFile {
    //       path: ".../core/continuum-core/../../.git/logs/HEAD" }))
    //   continuum_core:     dirty: FsStatusOutdated(StaleDepFingerprint { .. })
    //
    // The script reran every invocation, `built_at` minted a fresh CONTINUUM_BUILD_AT,
    // the rustc env changed, and the whole crate recompiled: measured at 2m53s to run
    // 70 tests that execute in 5.08s, with an unchanged HEAD and a clean tree. It never
    // reproduced in a plain clone, which is why it survived — and a worktree is exactly
    // how `airc work` lays out every claimed card, so every citizen paid it on every
    // cargo invocation.
    //
    // THE INVARIANT IS THE FIX, NOT THE PATHS — the paths are what drifted. Never emit
    // a watch for a path that is absent right now; see `git_watch_paths`.
    for path in git_watch_paths() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

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

/// The files whose modification means HEAD moved — filtered to those that EXIST.
///
/// **Every path returned here is `exists()`-checked, and that is the whole point.**
/// Cargo treats a missing `rerun-if-changed` target as permanently stale, so naming a
/// path that is absent does not "watch nothing", it rebuilds the crate on every single
/// invocation forever. Asking git where things are — rather than assuming a layout —
/// is what keeps this correct across the three layouts that actually occur:
///
/// * **clone** — `.git` is a directory; everything is where the old hardcoded paths
///   expected it, which is why this bug was invisible for so long.
/// * **worktree** — `.git` is a pointer FILE. `--absolute-git-dir` resolves to
///   `<main>/.git/worktrees/<name>`, whose `HEAD` is the WORKTREE-PRIVATE one. That is
///   the correct file to watch: a checkout in this worktree moves it and leaves the
///   shared HEAD untouched, so watching the common dir would miss it.
/// * **packed refs** — after a `gc`, `refs/heads/<branch>` has no loose file at all and
///   the tip lives in `packed-refs`. Watching the loose path would reintroduce this
///   exact defect through a third door.
///
/// The reflog is deliberately NOT watched. `core.logAllRefUpdates=false`, or a pruned
/// reflog, makes `logs/HEAD` absent — and an absent watch path is the defect itself, not
/// a degraded case of it.
fn git_watch_paths() -> Vec<std::path::PathBuf> {
    // No git at all (a source tarball, a vendored or packaged build) means there is
    // nothing to watch. Returning empty is correct; inventing a path we cannot verify
    // is precisely the failure being fixed.
    let Some(git_dir) = git_output(&["rev-parse", "--absolute-git-dir"]).map(std::path::PathBuf::from)
    else {
        return Vec::new();
    };
    // Worktrees share refs with the main repository; `--git-common-dir` is where the
    // branch tips actually live. It can come back relative to the invocation cwd, so it
    // is canonicalized, and falls back to the git dir when that fails.
    let common_dir = git_output(&["rev-parse", "--git-common-dir"])
        .map(std::path::PathBuf::from)
        .and_then(|p| std::fs::canonicalize(p).ok())
        .unwrap_or_else(|| git_dir.clone());

    let mut watched = Vec::new();
    // HEAD itself: moves on checkout, and on commit while detached.
    let head = git_dir.join("HEAD");
    if head.exists() {
        watched.push(head);
    }
    // Where the CURRENT branch's tip is stored. A commit moves the ref, not HEAD, so
    // watching HEAD alone would miss new commits on the same branch. A detached HEAD has
    // no symbolic ref — `--quiet` makes that a clean `None`, and HEAD above already
    // covers that case.
    if let Some(reference) = git_output(&["symbolic-ref", "--quiet", "HEAD"]) {
        let loose = common_dir.join(&reference);
        if loose.exists() {
            watched.push(loose);
        } else {
            // Packed: the loose file legitimately does not exist.
            let packed = common_dir.join("packed-refs");
            if packed.exists() {
                watched.push(packed);
            }
        }
    }
    watched
}

/// Run `git` and return trimmed stdout, or `None` if git is absent, the command failed,
/// or the output was empty. Never panics: a build script that dies because git is
/// missing would make the crate unbuildable from a tarball.
fn git_output(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}
