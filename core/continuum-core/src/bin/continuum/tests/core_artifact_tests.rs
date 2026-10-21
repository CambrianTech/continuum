use super::*;

// what this catches: the direct-exec launch path losing the manifest's
// runtime library dirs. On a CUDA Windows node that loss is fatal BEFORE
// main() — STATUS_DLL_NOT_FOUND (0xC0000135), no output, empty start log,
// a core that "just doesn't come up" with nothing to read. Measured on the
// 5090 2026-09-04 with a positive control both ways: the same freshly
// built binary exits 0xC0000135 without the CUDA bin dir on PATH and runs
// with it.
//
// Asserts the SELECTION RULE rather than spawning a process: only
// existing dirs are contributed (a node without CUDA is unaffected), ONE
// CUDA tree's runtime dir is contributed (the one the build links), and
// unrelated directories are never picked up.
#[test]
fn runtime_library_dirs_take_only_real_toolchain_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let touch = |rel: &str| {
        let f = root.join(rel);
        std::fs::create_dir_all(f.parent().expect("parent")).expect("dir");
        std::fs::write(f, b"").expect("file");
    };
    std::fs::create_dir_all(root.join("tools/cmake/bin")).expect("cmake bin");
    std::fs::create_dir_all(root.join("tools/poppler/Library/bin")).expect("PDF tools");
    // The 5090's trees (2026-10-04): two complete CUDA 12 trees and one complete 13.
    for tree in ["cuda-toolkit", "cuda-env"] {
        let (lib, bin) = if tree == "cuda-toolkit" { ("lib/x64", "bin") } else { ("Library/lib/x64", "Library/bin") };
        touch(&format!("{tree}/{lib}/cuda.lib"));
        touch(&format!("{tree}/{lib}/curand.lib"));
        touch(&format!("{tree}/{bin}/cublas64_12.dll"));
    }
    touch("cuda-13.2/Library/lib/x64/cuda.lib");
    touch("cuda-13.2/Library/lib/x64/curand.lib");
    touch("cuda-13.2/Library/bin/cublas64_13.dll");
    // A newer major with a runtime but no import libs: a build could not link it.
    touch("cuda-14.0/bin/cublas64_14.dll");
    std::fs::create_dir_all(root.join("cuda-build-venv")).expect("empty tree");
    std::fs::create_dir_all(root.join("models")).expect("models");

    let dirs = super::runtime_library_dirs(root);
    assert!(dirs.contains(&root.join("tools/cmake/bin")), "a provisioned tool's bin/ must be contributed: {dirs:?}");
    assert!(dirs.contains(&root.join("tools/poppler/Library/bin")), "{dirs:?}");
    // what this catches: the multi-tree PATH #4653 re-introduced. Exactly one CUDA dir
    // reaches PATH, and it is the tree the build links (complete, highest major), so the
    // warm build's "CUDA MAJOR MISMATCH" check and the core's loader agree.
    let cuda: Vec<_> = dirs.iter().filter(|d| d.starts_with(root) && d.strip_prefix(root).is_ok_and(|r| r.to_string_lossy().starts_with("cuda-"))).collect();
    assert_eq!(cuda, vec![&root.join("cuda-13.2/Library/bin")], "{dirs:?}");
    assert!(!dirs.iter().any(|d| d.ends_with("models")), "unrelated continuum-root dirs are never runtime library paths: {dirs:?}");

    // A runtime-only node (prebuilt install, nothing to link) still gets its runtime.
    let only = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(only.path().join("cuda-toolkit/bin")).expect("bin");
    std::fs::write(only.path().join("cuda-toolkit/bin/cublas64_12.dll"), b"").expect("dll");
    assert_eq!(super::declared_cuda_runtime_dir(only.path()), Some(only.path().join("cuda-toolkit/bin")));
}

// what this catches: card d2698cdc — ONNX selection must work without any
// CUDA/toolchain directory and must select the host platform's installed ABI.
#[test]
fn native_launch_selects_platform_ort_without_cuda() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir(root.join("lib")).expect("lib dir");
    for (os, name) in [
        ("windows", "onnxruntime.dll"),
        ("linux", "libonnxruntime.so"),
        ("macos", "libonnxruntime.dylib"),
    ] {
        let library = root.join("lib").join(name);
        std::fs::write(&library, []).expect("installed library fixture");
        let mut cmd = std::process::Command::new("unused");
        cmd.env("ORT_DYLIB_PATH", "");
        cmd.env("PATH", root.join("operator-bin"));
        super::apply_runtime_library_env_in(&mut cmd, root, os);
        assert_eq!(
            super::command_env(&cmd, "ORT_DYLIB_PATH"),
            Some(library.into_os_string())
        );
        assert_eq!(
            super::command_env(&cmd, "PATH"),
            Some(root.join("operator-bin").into_os_string()),
            "ONNX discovery must not require or rewrite a toolchain PATH"
        );
    }
}

// what this catches: config.env overrides live on Command, not in the CLI's
// environment. Preserve both an explicit ORT selection and configured PATH.
#[test]
fn native_launch_preserves_runtime_overrides_and_configured_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let cuda = root.join("cuda-13.2/Library/bin");
    std::fs::create_dir_all(&cuda).expect("cuda bin");
    let explicit_ort = root.join("operator-ort.dll");
    let explicit_path = root.join("operator-bin");
    let mut cmd = std::process::Command::new("unused");
    // Windows preserves spelling but compares environment keys without case.
    // config.env may legally use either spelling; Unix remains case-sensitive.
    let ort_key = if cfg!(windows) {
        "ort_dylib_path"
    } else {
        "ORT_DYLIB_PATH"
    };
    let path_key = if cfg!(windows) { "Path" } else { "PATH" };
    cmd.env(ort_key, &explicit_ort);
    cmd.env(path_key, &explicit_path);
    super::apply_runtime_library_env_in(&mut cmd, root, "windows");
    assert_eq!(
        super::command_env(&cmd, "ORT_DYLIB_PATH"),
        Some(explicit_ort.into_os_string()),
        "an explicit override remains authoritative even if the path is absent"
    );
    let actual = super::command_env(&cmd, "PATH").expect("child PATH");
    assert_eq!(
        std::env::split_paths(&actual).collect::<Vec<_>>(),
        vec![cuda, explicit_path],
        "prepend runtime dirs to the configured child PATH, not the parent's PATH"
    );
    cmd.env_remove(path_key);
    assert!(
        super::command_env(&cmd, "PATH").is_none(),
        "an explicit removal must not fall through to the parent's environment"
    );
}

// what this catches (#194): the artifact resolution ORDER is a shared contract with
// tools/scripts/install-service.sh::resolve_core_bin — installed locations before the
// cargo target dir (release before debug), default target dir under ~/.continuum/cache.
// A drift here means the CLI verifies a different binary than the service installer
// deploys — and NOTHING may resolve "next to the CLI exe" (the guess that printed
// "could not locate continuum-core-server" while a 2-day-old core kept serving).
#[test]
fn artifact_resolution_order_matches_install_service() {
    let c = core_artifact_candidates(Path::new("/home/u/.continuum/cache/cargo-target"), Path::new("/home/u/.continuum"));
    let shown: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
    #[cfg(not(windows))]
    assert_eq!(
        shown,
        vec![
            "/usr/local/bin/continuum-core-server",
            "/home/u/.continuum/bin/continuum-core-server",
            "/home/u/.continuum/cache/cargo-target/release/continuum-core-server",
            "/home/u/.continuum/cache/cargo-target/debug/continuum-core-server",
        ],
        "order is the install-service.sh contract"
    );
    #[cfg(windows)]
    {
        assert!(shown
            .iter()
            .all(|p| p.ends_with("continuum-core-server.exe")));
        assert!(
            !shown.iter().any(|p| p.starts_with("/usr/local/bin")),
            "no unix-only install location on Windows"
        );
    }
    // explicit CARGO_TARGET_DIR overrides the default cache location
    let c = core_artifact_candidates(Path::new("/tgt"), Path::new("/cold/payload"));
    assert!(c.iter().any(|p| p.starts_with("/cold/payload/bin")));
    assert!(!c.iter().any(|p| p.starts_with("/home/u/.continuum/bin")));
    assert!(
        c.iter().any(|p| p.starts_with("/tgt/release"))
            && c.iter().any(|p| p.starts_with("/tgt/debug")),
        "CARGO_TARGET_DIR is honored: {c:?}"
    );
}
