//! Cargo's configured artifact root, shared by writers and resource ownership.
//! Relative overrides retain Cargo's working-directory semantics.

use std::path::{Path, PathBuf};

pub fn resolve_target(target: &Path, cwd: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        cwd.join(target)
    }
}

pub fn target_dir(cwd: &Path) -> Result<PathBuf, String> {
    super::shared_cargo_target_dir()
        .map(|target| resolve_target(&target, cwd))
        .ok_or_else(|| "Cargo target has neither configuration nor a native home".to_string())
}

/// Resolve existing ancestors too: a not-yet-created target under a directory alias
/// must join the same owner as its physical spelling. No directory is created here.
pub fn physical_root(path: &Path) -> Result<PathBuf, String> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| format!("cannot resolve Cargo target {}", path.display()))?;
            let mut resolved = physical_root(parent)?;
            match path.components().next_back() {
                Some(std::path::Component::Normal(name)) => resolved.push(name),
                Some(std::path::Component::ParentDir) => {
                    resolved.pop();
                }
                Some(std::path::Component::CurDir) => {}
                _ => return Err(format!("cannot resolve Cargo target {}", path.display())),
            }
            Ok(resolved)
        }
        Err(error) => Err(format!(
            "cannot resolve Cargo target {}: {error}",
            path.display()
        )),
    }
}

/// Native device identity, not a mount-path inference. An unavailable device is
/// an error; callers retain its ownership rather than treating it as empty.
pub fn volume_id(path: &Path) -> Result<u64, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.dev())
            .map_err(|error| error.to_string())
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        };
        let directory = std::fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|error| error.to_string())?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // The owned handle remains live, and info is the native output structure.
        if unsafe { GetFileInformationByHandle(directory.as_raw_handle() as _, &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(u64::from(info.dwVolumeSerialNumber))
    }
}

pub(crate) fn existing_volume_id(path: &Path) -> Result<u64, String> {
    match std::fs::metadata(path) {
        Ok(_) => volume_id(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            existing_volume_id(path.parent().ok_or_else(|| error.to_string())?)
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_relative_and_native_roots_share_one_resolution_contract() {
        let workspace = tempfile::tempdir().unwrap();
        let other_volume = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_target(other_volume.path(), workspace.path()),
            other_volume.path()
        );
        assert_eq!(
            resolve_target(Path::new("artifacts"), workspace.path()),
            workspace.path().join("artifacts")
        );
        let missing = workspace.path().join("new/target");
        assert_eq!(
            physical_root(&missing).unwrap(),
            workspace.path().canonicalize().unwrap().join("new/target")
        );
        assert_eq!(
            physical_root(&workspace.path().join("new/../target")).unwrap(),
            workspace.path().canonicalize().unwrap().join("target")
        );
    }
}
