//! Published release staging uses the configured storage tier and the existing
//! projected-capacity policy. This is admission, not a reservation or an evictor.
use continuum_cli_lifecycle::prebuilt_artifact::{
    PublishedCoreError, ResourceCondition, ResourceDeferral,
};
use std::path::{Path, PathBuf};

pub fn root(home: &Path) -> Result<PathBuf, String> {
    let storage = super::configured("CONTINUUM_STORAGE_PATH");
    resolve_root(storage, home)
}

/// Installed payload placement is independent of downloadable artifact storage.
/// An unavailable or corrupt placement record is operational uncertainty, never
/// evidence that the selected published artifact is invalid.
pub fn service_destination(home: &Path) -> Result<PathBuf, PublishedCoreError> {
    super::payload_root(home)
        .map(|root| root.join("bin"))
        .map_err(PublishedCoreError::OperationalBlocked)
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

/// One measured volume snapshot for a single sequential archive write. Per-chunk
/// accounting uses the shared policy in memory; observation is refreshed only
/// when the measured allowance is exhausted. Other writers are not reserved out.
pub struct WriteAdmission {
    path: PathBuf,
    drive: crate::system_resources::drives::DriveInfo,
    written: u64,
}

impl WriteAdmission {
    pub async fn observe(path: &Path) -> Result<Self, PublishedCoreError> {
        let path = path.to_path_buf();
        let observed = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            if let Some(storage) = super::configured("CONTINUUM_STORAGE_PATH") {
                if observed == storage.join("artifacts") && !storage.is_dir() {
                    return Err("Configured artifact storage is unavailable; refusing a home-drive fallback".into());
                }
            }
            let physical = super::cargo::physical_root(&observed)?;
            let drives = crate::system_resources::drives::detect_drives();
            crate::system_resources::drives::drive_holding(&drives, &physical).cloned()
                .ok_or_else(|| format!("artifact volume is unavailable for {}", physical.display()))
        }).await.map_err(|error| deferred(&path, 0, format!("artifact volume observation failed: {error}")))?;
        let drive = result.map_err(|reason| deferred(&path, 0, reason))?;
        Ok(Self {
            path,
            drive,
            written: 0,
        })
    }

    pub fn check(&self, additional: u64) -> Result<(), PublishedCoreError> {
        let projected = self.written.saturating_add(additional);
        check_volume(&self.drive, projected)
            .map_err(|reason| deferred(&self.path, additional, reason))
    }

    pub async fn before_write(&mut self, bytes: u64) -> Result<(), PublishedCoreError> {
        if self.check(bytes).is_err() {
            // The refreshed used count already includes this stream's prior writes.
            *self = Self::observe(&self.path).await?;
            self.check(bytes)?;
        }
        self.written = self.written.saturating_add(bytes);
        Ok(())
    }
}

fn deferred(path: &Path, projected_bytes: u64, reason: String) -> PublishedCoreError {
    PublishedCoreError::ResourceDeferred(ResourceDeferral {
        path: path.into(),
        projected_bytes,
        reason,
        condition: ResourceCondition::VolumeCapacity,
    })
}

pub async fn check_capacity(path: &Path, projected_bytes: u64) -> Result<(), PublishedCoreError> {
    WriteAdmission::observe(path)
        .await
        .map_err(|error| match error {
            PublishedCoreError::ResourceDeferred(mut condition) => {
                condition.projected_bytes = projected_bytes;
                PublishedCoreError::ResourceDeferred(condition)
            }
            error => error,
        })?
        .check(projected_bytes)
}

/// Preserve a filesystem's own full/quota signal even when global volume free
/// space still looks sufficient. A later volume observation cannot erase it.
pub async fn write_failure(
    path: &Path,
    projected_bytes: u64,
    error: std::io::Error,
) -> PublishedCoreError {
    classify_write_failure(
        path,
        projected_bytes,
        error,
        check_capacity(path, projected_bytes).await,
    )
}

fn classify_write_failure(
    path: &Path,
    projected_bytes: u64,
    error: std::io::Error,
    capacity: Result<(), PublishedCoreError>,
) -> PublishedCoreError {
    match capacity {
        Err(mut condition) => {
            if let PublishedCoreError::ResourceDeferred(blocked) = &mut condition {
                blocked.reason = format!("{error}; {}", blocked.reason);
            }
            condition
        }
        Ok(()) if matches!(error.kind(), std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded) => {
            PublishedCoreError::ResourceDeferred(ResourceDeferral {
                path: path.into(),
                projected_bytes,
                reason: format!("{error}; whole-volume capacity passes but cannot measure this filesystem refusal"),
                condition: ResourceCondition::UnmeasuredFilesystem,
            })
        }
        Ok(()) => PublishedCoreError::Artifact(error.to_string()),
    }
}

#[cfg(test)]
fn check_on_drives(
    path: &Path,
    projected_bytes: u64,
    drives: &[crate::system_resources::drives::DriveInfo],
) -> Result<(), String> {
    let drive = crate::system_resources::drives::drive_holding(drives, path)
        .ok_or_else(|| format!("artifact volume is unavailable for {}", path.display()))?;
    check_volume(drive, projected_bytes)
}

fn check_volume(
    drive: &crate::system_resources::drives::DriveInfo,
    projected_bytes: u64,
) -> Result<(), String> {
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
    use crate::system_resources::drives::{DriveInfo, DriveRole};

    // Published b475 repeatedly filled the home drive although configured storage
    // was available. Check placement and admission on the selected volume, including
    // compressed+expanded coexistence, without touching any operator cache.
    #[tokio::test]
    async fn configured_artifacts_use_their_own_volume_and_count_coexistence() {
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
        // Regression: three c7deff downloads were completed while the installed
        // service volume could not admit even zero additional bytes. Cold
        // artifact storage must not be mistaken for installed payload placement.
        let destination = service_destination(home.path()).unwrap();
        assert_eq!(destination, home.path().join("bin"));
        assert!(check_on_drives(&destination, 0, &drives).is_err());
        assert!(check_on_drives(&root, 0, &drives).is_ok());
        std::fs::write(
            home.path().join("payload-root"),
            storage.path().to_str().unwrap(),
        )
        .unwrap();
        let destination = service_destination(home.path()).unwrap();
        assert_eq!(destination, storage.path().join("bin"));
        assert!(check_on_drives(&destination, 0, &drives).is_ok());
        assert!(
            check_on_drives(&destination, 251, &drives).is_err(),
            "zero-byte preflight does not approve the later measured copy"
        );
        std::fs::write(home.path().join("payload-root"), "relative").unwrap();
        assert!(matches!(
            service_destination(home.path()),
            Err(PublishedCoreError::OperationalBlocked(_))
        ));
        assert!(check_on_drives(&root, 100 + 150, &drives).is_ok());
        assert!(check_on_drives(&root, 100 + 151, &drives).is_err());
        assert!(check_on_drives(&root, 1, &[]).is_err());
        let mut stream = WriteAdmission {
            path: root.clone(),
            drive: drives[1].clone(),
            written: 0,
        };
        stream.before_write(100).await.unwrap();
        stream.before_write(150).await.unwrap();
        assert_eq!(
            stream.written, 250,
            "chunk accounting accumulates against one measured snapshot"
        );
        assert!(
            stream.check(1).is_err(),
            "the shared policy detects exhaustion without another drive enumeration"
        );
        for kind in [
            std::io::ErrorKind::StorageFull,
            std::io::ErrorKind::QuotaExceeded,
        ] {
            let error = classify_write_failure(&root, 251, std::io::Error::from(kind), Ok(()));
            assert!(
                matches!(
                    &error,
                    PublishedCoreError::ResourceDeferred(ResourceDeferral {
                        condition: ResourceCondition::UnmeasuredFilesystem,
                        ..
                    })
                ),
                "whole-volume success cannot dismiss an unmeasured quota/full condition"
            );
            assert!(matches!(error, PublishedCoreError::ResourceDeferred(ResourceDeferral { projected_bytes: 251, .. })), "filesystem full/quota cannot become invalid artifact merely because volume reports free space");
        }
    }
}
