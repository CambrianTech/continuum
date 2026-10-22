use super::*;

pub(super) const LAUNCH_GATE: &str = "supervisor-maintenance-launch.lock";

pub(super) fn launch_gate(root: &Path, exclusive: bool) -> Result<File, String> {
    crate::installed_release::unredirected(root)?;
    let path = root.join(LAUNCH_GATE);
    if path.symlink_metadata().is_ok() {
        crate::installed_release::unredirected(&path)?;
    }
    // All cooperating openers deny deletion. Windows symmetric sharing
    // admits concurrent startup readers or one maintenance owner, never both.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(if exclusive { 0 } else { 3 })
        .open(&path)
        .map_err(|e| format!("cannot acquire maintenance launch custody: {e}"))?;
    crate::installed_release::unredirected(&path)?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("maintenance launch gate is not a regular file".into());
    }
    Ok(file)
}

/// Held only from startup admission through child creation, not child exit.
/// This is distinct from install.lock, which the installing parent retains
/// while waiting for the newly started core to become ready.
pub struct StartupPermit {
    _gate: File,
    _evidence: Option<File>,
}

impl StartupPermit {
    pub async fn acquire(root: &Path) -> Result<Self, String> {
        let gate = launch_gate(root, false)?;
        match root.join(JOURNAL).symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                _gate: gate,
                _evidence: None,
            }),
            Ok(_) => {
                let (file, snapshot) = read_snapshot(root)?;
                if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
                    return Err(
                        "scheduler maintenance recovery is pending; startup refused".into()
                    );
                }
                check_restoring_startup(&snapshot, &read_tasks().await?)?;
                let purpose = snapshot
                    .legacy
                    .as_ref()
                    .ok_or("legacy restoration binding absent")?;
                // Launch cannot redirect to a different selection while verification is pending.
                if crate::installed_release::read_receipt(&root.join("install-active.json"))?
                    != purpose.active
                    || previous_receipt(root)? != purpose.previous
                {
                    return Err("restoration launch selection changed".into());
                }
                Ok(Self {
                    _gate: gate,
                    _evidence: Some(file),
                })
            }
            Err(e) => Err(format!(
                "cannot establish maintenance state before startup: {e}"
            )),
        }
    }
}
