//! Original engine application inputs captured by the Windows build owner.
//! This is not a model receipt or hermetic restore authority: Windows system and
//! NVIDIA driver inputs remain an explicit platform contract.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallReceipt {
    schema: u32,
    source_revision: String,
    backend: String,
    build_contract: String,
    runtime_origin: String,
    platform_contract: String,
    files: BTreeMap<String, String>,
}

/// Private launch evidence. Never serializes inherited environment values.
#[derive(Clone)]
pub(crate) struct EngineInstallReceipt {
    program: PathBuf,
    directory: PathBuf,
    system_directory: PathBuf,
    receipt: InstallReceipt,
    managed_paths: Vec<(String, std::ffi::OsString)>,
    // Windows share-read-only handles pin original bytes through actual child exit.
    _files: Vec<std::sync::Arc<std::fs::File>>,
}

fn hash(
    path: &Path,
    check: &dyn Fn() -> Result<(), String>,
    remaining: &mut u64,
) -> Result<(String, std::sync::Arc<std::fs::File>), String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: writes/deletes cannot replace pinned inputs.
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("engine input open: {e}"))?;
    let mut digest = Sha256::new();
    let mut buf = [0_u8; 65536];
    loop {
        check()?;
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("engine input read: {e}"))?;
        if n == 0 {
            break;
        }
        *remaining = remaining
            .checked_sub(n as u64)
            .ok_or("engine application inputs exceed 4 GiB verification bound")?;
        digest.update(&buf[..n]);
    }
    Ok((
        format!("{:x}", digest.finalize()),
        std::sync::Arc::new(file),
    ))
}

fn unredirected(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let meta = std::fs::symlink_metadata(ancestor).map_err(|e| format!("engine path: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err("redirected engine input".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("redirected engine input".into());
            }
        }
    }
    Ok(())
}

fn platform_system_directory() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut buffer = vec![0_u16; 32768];
        // SAFETY: the writable buffer has the exact capacity passed to Windows.
        let count = unsafe {
            windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        } as usize;
        if count == 0 || count >= buffer.len() {
            return Err("Windows system directory unavailable".into());
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..count],
        )))
    }
    #[cfg(not(windows))]
    {
        Err("Windows engine contract is unavailable on this platform".into())
    }
}

impl EngineInstallReceipt {
    pub(crate) async fn prepare(program: String) -> Result<Option<Self>, String> {
        let (cancel, _cancellation_guard) = tokio::sync::oneshot::channel::<()>();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || Self::load(&program, &|| cancel.is_closed())),
        )
        .await
        .map_err(|_| "engine receipt verification exceeded 30 s".to_string())?
        .map_err(|e| format!("engine receipt worker: {e}"))?
    }

    /// Called off the async lane, before lifecycle admission. Absence preserves
    /// ordinary legacy behavior and grants no receipt to an existing child.
    fn load(program: &str, cancelled: &dyn Fn() -> bool) -> Result<Option<Self>, String> {
        if !cfg!(windows) {
            return Ok(None);
        }
        let program = PathBuf::from(program);
        // PATH spellings cannot identify the neighboring build receipt.
        if !program.is_absolute() {
            return Ok(None);
        }
        let directory = program
            .parent()
            .ok_or("engine program has no directory")?
            .to_owned();
        match std::fs::symlink_metadata(directory.join("engine-install.pending")) {
            Ok(_) => return Err("engine publication is incomplete".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("engine publication state: {e}")),
        }
        let path = directory.join("engine-install.json");
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("engine receipt metadata: {e}")),
            Ok(meta) if meta.len() > 1048576 => return Err("oversized engine receipt".into()),
            Ok(_) => {}
        }
        if std::env::var_os("GGML_BACKEND_PATH").is_some() {
            return Err("receipted engine does not support ambient GGML_BACKEND_PATH".into());
        }
        let system_directory = platform_system_directory()?;
        Self::read(program, directory, system_directory, &path, cancelled).map(Some)
    }

    fn read(
        program: PathBuf,
        directory: PathBuf,
        system_directory: PathBuf,
        path: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        unredirected(path)?;
        let bytes = std::fs::read(path).map_err(|e| format!("engine receipt read: {e}"))?;
        let receipt: InstallReceipt =
            serde_json::from_slice(&bytes).map_err(|e| format!("engine receipt shape: {e}"))?;
        let mut managed_paths = Vec::new();
        for key in [
            "GGML_MOE_CAPTURE_FILE",
            "GGML_MOE_PLAN_FILE",
            "GGML_MOE_TRACE_FILE",
            "LLAMA_RESIDENT_OVERRIDE",
        ] {
            if let Some(value) = std::env::var_os(key) {
                if !Path::new(&value).is_absolute() {
                    return Err(format!("receipted engine requires absolute {key}"));
                }
                managed_paths.push((key.to_string(), value));
            }
        }
        let mut result = Self {
            program,
            directory,
            system_directory,
            receipt,
            managed_paths,
            _files: Vec::new(),
        };
        result._files = result.verify(cancelled)?;
        Ok(result)
    }

    fn namespace(&self, cancelled: &dyn Fn() -> bool) -> Result<BTreeMap<String, PathBuf>, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let check = || -> Result<(), String> {
            if cancelled() {
                Err("engine receipt verification cancelled".into())
            } else if std::time::Instant::now() >= deadline {
                Err("engine receipt verification exceeded 30 s".into())
            } else {
                Ok(())
            }
        };
        check()?;

        let r = &self.receipt;
        if r.schema != 1
            || r.source_revision.len() != 40
            || !r
                .source_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || !matches!(r.backend.as_str(), "cpu" | "cuda")
            || r.build_contract != "static-local-backends-v1"
            || r.runtime_origin != "installed-toolkit-bin-snapshot"
            || r.platform_contract != "windows-system32-nvidia-driver-v1"
            || self.program.file_name().and_then(|n| n.to_str()) != Some("llama-server.exe")
        {
            return Err("unsupported engine install contract".into());
        }
        match std::fs::symlink_metadata(self.directory.join("engine-install.pending")) {
            Ok(_) => return Err("engine publication is incomplete".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("engine publication state: {e}")),
        }
        unredirected(&self.directory)?;
        let mut actual = BTreeMap::new();
        let mut entries = 0_usize;
        for entry in
            std::fs::read_dir(&self.directory).map_err(|e| format!("engine namespace: {e}"))?
        {
            check()?;
            entries += 1;
            if entries > 4096 {
                return Err("engine directory exceeds entry bound".into());
            }
            let entry = entry.map_err(|e| format!("engine namespace entry: {e}"))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 engine candidate")?
                .to_ascii_lowercase();
            if name == "llama-server.exe" || name.ends_with(".dll") {
                unredirected(&entry.path())?;
                if !entry
                    .file_type()
                    .map_err(|e| format!("engine candidate type: {e}"))?
                    .is_file()
                {
                    return Err("non-file engine candidate".into());
                }
                if actual.insert(name, entry.path()).is_some() {
                    return Err("ambiguous engine candidate".into());
                }
            }
        }
        if !actual.contains_key("llama-server.exe")
            || actual.len() > 256
            || !actual.keys().eq(r.files.keys())
        {
            return Err("engine application bytes or candidate membership changed".into());
        }
        Ok(actual)
    }

    fn verify(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<std::sync::Arc<std::fs::File>>, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let check = || -> Result<(), String> {
            if cancelled() {
                Err("engine receipt verification cancelled".into())
            } else if std::time::Instant::now() >= deadline {
                Err("engine receipt verification exceeded 30 s".into())
            } else {
                Ok(())
            }
        };
        let actual = self.namespace(cancelled)?;
        let mut remaining = 4_u64 * 1024 * 1024 * 1024;
        let mut files = Vec::new();
        for (name, path) in actual {
            let (digest, file) = hash(&path, &check, &mut remaining)?;
            if digest != self.receipt.files[&name] {
                return Err("engine application bytes changed".into());
            }
            files.push(file);
        }
        Ok(files)
    }

    /// Cooperative installer namespace checks bracket launch. Held input handles
    /// prevent replacement; additions/pending state are checked independently.
    pub(crate) async fn confirm(&self) -> Result<(), String> {
        let original = self.clone();
        let (cancel, _cancellation_guard) = tokio::sync::oneshot::channel::<()>();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                original.namespace(&|| cancel.is_closed()).map(|_| ())
            }),
        )
        .await
        .map_err(|_| "engine namespace confirmation exceeded 30 s".to_string())?
        .map_err(|e| format!("engine namespace worker: {e}"))?
    }

    pub(crate) fn require_absolute(&self, path: &Path) -> Result<(), String> {
        if path.is_absolute() {
            Ok(())
        } else {
            Err("receipted engine requires absolute model/config paths before transition".into())
        }
    }

    /// All probes and the real child use the same controlled application loader
    /// roots. Other environment remains inherited; this is not hermetic execution.
    pub(crate) fn command(&self) -> Result<tokio::process::Command, String> {
        let mut command = tokio::process::Command::new(&self.program);
        let path = std::env::join_paths([&self.directory, &self.system_directory])
            .map_err(|e| format!("engine loader path: {e}"))?;
        command
            .envs(self.managed_paths.iter().map(|(k, v)| (k, v)))
            .current_dir(&self.directory)
            .env("PATH", path)
            .env_remove("GGML_BACKEND_PATH");
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: changing candidate membership or same-size bytes cannot
    // reuse a launch receipt; the real consumer also fixes cwd/loader PATH.
    #[test]
    fn installed_engine_receipt_binds_bytes_and_candidate_membership() {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let directory = canonical.join("engine-a");
        let system = canonical.join("System32");
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(&system).unwrap();
        let program = directory.join("llama-server.exe");
        let dll = directory.join("cublas64_12.dll");
        std::fs::write(&program, b"engine").unwrap();
        std::fs::write(&dll, b"before").unwrap();
        assert!(
            EngineInstallReceipt::load(program.to_str().unwrap(), &|| false)
                .unwrap()
                .is_none(),
            "legacy engine is not retroactively certified"
        );
        let path = directory.join("engine-install.json");
        let receipt = serde_json::json!({
            "schema": 1, "source_revision": "0123456789012345678901234567890123456789",
            "backend": "cuda", "build_contract": "static-local-backends-v1",
            "runtime_origin": "installed-toolkit-bin-snapshot",
            "platform_contract": "windows-system32-nvidia-driver-v1",
            "files": {"llama-server.exe": hash(&program, &|| Ok(()), &mut 1024).unwrap().0, "cublas64_12.dll": hash(&dll, &|| Ok(()), &mut 1024).unwrap().0}
        });
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let captured = EngineInstallReceipt::read(
            program.clone(),
            directory.clone(),
            system.clone(),
            &path,
            &|| false,
        )
        .unwrap();
        assert!(captured
            .require_absolute(Path::new("relative/adapter.gguf"))
            .is_err());
        assert!(captured.require_absolute(&dll).is_ok());
        assert!(captured.verify(&|| true).unwrap_err().contains("cancelled"));
        let command = captured.command().unwrap();
        assert_eq!(command.as_std().get_program(), program.as_os_str());
        assert_eq!(
            command.as_std().get_current_dir(),
            Some(directory.as_path())
        );
        let loader = std::env::join_paths([&directory, &system]).unwrap();
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == "PATH" && value == Some(loader.as_os_str())));
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == "GGML_BACKEND_PATH" && value.is_none()));
        let pending = directory.join("engine-install.pending");
        std::fs::write(&pending, b"copy in progress").unwrap();
        assert!(captured
            .namespace(&|| false)
            .unwrap_err()
            .contains("incomplete"));
        std::fs::remove_file(&pending).unwrap();
        let added = directory.join("ggml-cuda-new.dll");
        std::fs::write(&added, b"new candidate").unwrap();
        assert!(captured
            .verify(&|| false)
            .unwrap_err()
            .contains("membership"));
        std::fs::remove_file(&added).unwrap();
        std::fs::create_dir(&added).unwrap();
        assert!(
            captured.verify(&|| false).is_err(),
            "candidate directories are not ignored"
        );
        std::fs::remove_dir(&added).unwrap();
        #[cfg(windows)]
        {
            assert!(
                std::fs::write(&dll, b"after!").is_err(),
                "live receipt pins original bytes"
            );
            assert!(
                std::fs::remove_file(&dll).is_err(),
                "live receipt pins input identity"
            );
        }
        // Receipt clones carry the same pins, as the retained child owner does.
        let mut captured = captured;
        let retained = captured.clone();
        captured._files.clear();
        #[cfg(windows)]
        assert!(
            std::fs::write(&dll, b"after!").is_err(),
            "retained owner keeps input pinned"
        );
        drop(retained);
        // The original content identity remains useful after ownership ends.
        std::fs::write(&dll, b"after!").unwrap();
        assert!(
            captured.verify(&|| false).is_err(),
            "same size is not content identity"
        );
        // Changing the on-disk receipt cannot recertify the ORIGINAL launch.
        let mut rewritten = receipt;
        rewritten["files"]["cublas64_12.dll"] = hash(&dll, &|| Ok(()), &mut 1024).unwrap().0.into();
        std::fs::write(&path, serde_json::to_vec(&rewritten).unwrap()).unwrap();
        assert!(captured.verify(&|| false).is_err());
        std::fs::remove_file(&dll).unwrap();
        assert!(EngineInstallReceipt::read(program, directory, system, &path, &|| false).is_err());
    }
}
