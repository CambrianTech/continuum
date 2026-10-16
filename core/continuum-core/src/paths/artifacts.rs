//! Published release staging uses the configured storage tier and the existing
//! projected-capacity policy. This is admission, not a reservation or an evictor.
use continuum_cli_lifecycle::prebuilt_artifact::{PublishedCoreError, ResourceDeferral};
use std::path::{Path, PathBuf};

pub fn root(home: &Path) -> Result<PathBuf, String> {
    let storage = super::configured("CONTINUUM_STORAGE_PATH");
    resolve_root(storage, home)
}

fn resolve_root(storage: Option<PathBuf>, home: &Path) -> Result<PathBuf, String> {
    match storage {
        Some(root) if !root.is_absolute() => {
            Err("Configured storage root must be absolute for shared artifact placement".into())
        }
        Some(root) => Ok(root.join("artifacts")),
        None => Ok(home.join(".continuum/cache/artifacts")),
    }
}

pub async fn check_capacity(path: &Path, projected_bytes: u64) -> Result<(), PublishedCoreError> {
    let path = path.to_path_buf();
    let observed = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        if let Some(storage) = super::configured("CONTINUUM_STORAGE_PATH") {
            if observed == storage.join("artifacts") && !storage.is_dir() {
                return Err(
                    "Configured artifact storage is unavailable; refusing a home-drive fallback"
                        .into(),
                );
            }
        }
        let physical = super::cargo::physical_root(&observed)?;
        let drives = crate::capacity::system_profile::detect_drives();
        check_on_drives(&physical, projected_bytes, &drives)
    })
    .await
    .map_err(|error| {
        PublishedCoreError::ResourceDeferred(ResourceDeferral {
            path: path.clone(),
            projected_bytes,
            reason: format!("artifact volume observation failed: {error}"),
        })
    })?;
    result.map_err(|reason| {
        PublishedCoreError::ResourceDeferred(ResourceDeferral {
            path,
            projected_bytes,
            reason,
        })
    })
}

fn check_on_drives(
    path: &Path,
    projected_bytes: u64,
    drives: &[crate::capacity::system_profile::DriveInfo],
) -> Result<(), String> {
    let drive = crate::capacity::system_profile::drive_holding(drives, path)
        .ok_or_else(|| format!("artifact volume is unavailable for {}", path.display()))?;
    crate::paging::pool::ensure_projected_disk_capacity(
        "core-artifacts",
        drive.total_bytes.saturating_sub(drive.available_bytes),
        drive.total_bytes,
        projected_bytes,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::system_profile::{DriveInfo, DriveRole};

    // Published b475 repeatedly filled the home drive although configured storage
    // was available. Check placement and admission on the selected volume, including
    // compressed+expanded coexistence, without touching any operator cache.
    #[test]
    fn configured_artifacts_use_their_own_volume_and_count_coexistence() {
        let home = tempfile::tempdir().unwrap();
        let storage = tempfile::tempdir().unwrap();
        let root = resolve_root(Some(storage.path().into()), home.path()).unwrap();
        assert_eq!(root, storage.path().join("artifacts"));
        assert_eq!(
            resolve_root(None, home.path()).unwrap(),
            home.path().join(".continuum/cache/artifacts")
        );
        assert!(resolve_root(Some("relative".into()), home.path()).is_err());
        let drives = vec![
            DriveInfo {
                mount: home.path().into(),
                total_bytes: 1000,
                available_bytes: 1,
                role: DriveRole::System,
            },
            DriveInfo {
                mount: storage.path().into(),
                total_bytes: 1000,
                available_bytes: 300,
                role: DriveRole::Cold,
            },
        ];
        assert!(check_on_drives(&root, 100 + 150, &drives).is_ok());
        assert!(check_on_drives(&root, 100 + 151, &drives).is_err());
        assert!(check_on_drives(&root, 1, &[]).is_err());
    }
}
