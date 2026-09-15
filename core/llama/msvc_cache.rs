//! Cargo and CMake must agree when a Windows native toolchain changes.
use std::{env, fs, io, path::Path};

const STAMP: &str = "msvc-toolchain.identity";

pub fn environment_identity() -> String {
    let mut names: Vec<String> = [
        "VSINSTALLDIR",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
        "VCToolsVersion",
        "VisualStudioVersion",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "UCRTVersion",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "PATH",
        "CMAKE_GENERATOR_INSTANCE",
        "CMAKE_GENERATOR_PLATFORM",
        "CMAKE_GENERATOR_TOOLSET",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let target = env::var("TARGET").expect("TARGET must be set");
    for base in [
        "CMAKE",
        "CMAKE_GENERATOR",
        "CMAKE_TOOLCHAIN_FILE",
        "CC",
        "CXX",
        "CFLAGS",
        "CXXFLAGS",
    ] {
        names.extend([
            base.to_owned(),
            format!("HOST_{base}"),
            format!("TARGET_{base}"),
            format!("{base}_{target}"),
            format!("{base}_{}", target.replace('-', "_")),
        ]);
    }
    names.sort();
    names.dedup();
    let mut identity = String::from("msvc-native-cache-v1\n");
    let explicit_msvc = env::var_os("VCToolsInstallDir").is_some_and(|value| !value.is_empty());
    for name in names {
        println!("cargo:rerun-if-env-changed={name}");
        // vcvars pins the compiler and runtime through the tracked MSVC/SDK
        // variables. An unrelated PATH addition need not discard its objects.
        if name == "PATH" && explicit_msvc {
            continue;
        }
        identity.push_str(&format!("{name}={:?}\n", env::var_os(&name)));
    }
    identity
}

fn owned_directory(root: &Path, name: &str) -> io::Result<Option<std::path::PathBuf>> {
    if !matches!(name, "build" | "lib") {
        return Err(io::Error::other("not an owned native cache directory"));
    }
    let path = root.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let resolved = path.canonicalize()?;
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    };
    #[cfg(not(windows))]
    let redirected = metadata.file_type().is_symlink();
    if !metadata.is_dir() || redirected || resolved == root || !resolved.starts_with(root) {
        return Err(io::Error::other(format!(
            "native cache escapes owned output: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

pub fn prepare(out: &Path, identity: &str) -> io::Result<()> {
    let root = out.canonicalize()?;
    let previous = match fs::read_to_string(root.join(STAMP)) {
        Ok(previous) => Some(previous),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if previous.as_deref() == Some(identity) {
        return Ok(());
    }
    // Validate BOTH destinations before any mutation. Never remove OUT_DIR or
    // the shared Cargo cache, and reject redirected directories/junctions.
    let directories = [
        owned_directory(&root, "build")?,
        owned_directory(&root, "lib")?,
    ];
    // A failed rebuild must not leave an old identity certifying partially
    // rebuilt objects if the next invocation switches back to that toolchain.
    if previous.is_some() {
        fs::remove_file(root.join(STAMP))?;
    }
    for directory in directories.into_iter().flatten() {
        fs::remove_dir_all(directory)?;
    }
    Ok(())
}

pub fn record(out: &Path, identity: &str) -> io::Result<()> {
    fs::write(out.join(STAMP), identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression for #4056: compiler changes must evict native objects and
    // installed archives together, while preserving unrelated Cargo outputs.
    #[test]
    fn toolchain_changes_reset_only_owned_native_cache() {
        let root = env::temp_dir().join(format!(
            "continuum-msvc-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("bindings.rs"), "keep").unwrap();
        for identity in [None, Some("old-toolchain"), Some("current-toolchain")] {
            for name in ["build", "lib"] {
                fs::create_dir_all(root.join(name)).unwrap();
                fs::write(root.join(name).join("native.obj"), "cached").unwrap();
            }
            if let Some(identity) = identity {
                record(&root, identity).unwrap();
            }
            prepare(&root, "current-toolchain").unwrap();
            for name in ["build", "lib"] {
                assert_eq!(
                    root.join(name).join("native.obj").exists(),
                    identity == Some("current-toolchain")
                );
            }
            assert_eq!(
                fs::read_to_string(root.join("bindings.rs")).unwrap(),
                "keep"
            );
            // A failed build must not record the candidate identity.
            assert_eq!(
                fs::read_to_string(root.join(STAMP)).ok().as_deref(),
                identity.filter(|identity| *identity == "current-toolchain")
            );
        }
        assert!(owned_directory(&root, "../other-cache").is_err());
        assert!(owned_directory(&root, ".").is_err());
        // A junction/symlink must not redirect invalidation into another cache.
        let scoped = root.join("scoped");
        let outside = root.join("outside");
        fs::create_dir_all(scoped.join("build")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("valuable.lib"), "keep").unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("cmd")
                .arg("/c")
                .raw_arg(format!(
                    "mklink /J \"{}\" \"{}\"",
                    scoped
                        .join("lib")
                        .display()
                        .to_string()
                        .trim_start_matches(r"\\?\"),
                    outside.display().to_string().trim_start_matches(r"\\?\")
                ))
                .output()
                .unwrap();
            assert!(output.status.success(), "junction fixture: {output:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, scoped.join("lib")).unwrap();
        #[cfg(any(windows, unix))]
        {
            assert!(prepare(&scoped, "new-toolchain").is_err());
            assert!(
                scoped.join("build").is_dir(),
                "validate containment before deleting anything"
            );
            assert_eq!(
                fs::read_to_string(outside.join("valuable.lib")).unwrap(),
                "keep"
            );
            // Remove the link itself before fixture cleanup.
            #[cfg(windows)]
            fs::remove_dir(scoped.join("lib")).unwrap();
            #[cfg(unix)]
            fs::remove_file(scoped.join("lib")).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
}
