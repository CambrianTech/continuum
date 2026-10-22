use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSnapshot {
    pub name: String,
    /// Actual scheduler XML, retained for recovery/audit, never regenerated.
    pub xml: String,
    /// The same XML with Settings/Enabled normalized by the scheduler reader.
    pub identity_xml: String,
    pub sddl: String,
    pub enabled: bool,
    /// Observation only: disabling a task does not terminate this state.
    pub state: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoverySnapshot {
    pub version: u32,
    pub tasks: Vec<TaskSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<LegacyPurpose>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyPurpose {
    pub(super) operation_digest: String,
    pub(super) source: String,
    pub(super) serving_expectation: serde_json::Value,
    #[serde(default)]
    pub(super) phase: LegacyPhase,
    pub(super) payload: String,
    pub(super) bootstrap: String,
    pub(super) bootstrap_manifest: String,
    pub(super) active: String,
    pub(super) previous: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum LegacyPhase {
    #[default]
    Excluded,
    Restoring,
}

impl RecoverySnapshot {
    pub fn validate(&self) -> Result<(), String> {
        // Boundary: the entire existing recovery journal, including caller-owned
        // serving identity, uses the lifecycle receipt limit before publication.
        let encoded = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if encoded.len() as u64 > crate::installed_release::MAX_RECEIPT_BYTES {
            return Err("oversized maintenance recovery snapshot".into());
        }
        if let Some(legacy) = &self.legacy {
            if !legacy.serving_expectation.is_object()
                || legacy
                    .serving_expectation
                    .get("schema")
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
            {
                return Err("invalid caller serving expectation schema".into());
            }
            if !std::path::Path::new(&legacy.source).is_absolute() {
                return Err("invalid legacy maintenance source binding".into());
            }
            if legacy.operation_digest.len() != 64
                || !legacy
                    .operation_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("invalid legacy maintenance operation binding".into());
            }
        }
        if self.version != 1
            || self.tasks.len() != 2
            || self.tasks[0].name != super::super::CORE_TASK
            || self.tasks[1].name != super::super::DEPLOY_TASK
            || self
                .tasks
                .iter()
                .any(|t| t.xml.is_empty() || t.identity_xml.is_empty() || t.sddl.is_empty())
        {
            return Err(
                "invalid scheduler maintenance recovery snapshot; evidence retained".into(),
            );
        }
        Ok(())
    }

    pub fn check_operation(&self, expected: Option<&str>) -> Result<(), String> {
        self.validate()?;
        if self.legacy.as_ref().map(|p| p.operation_digest.as_str()) != expected {
            return Err("maintenance operation differs; recovery evidence retained".into());
        }
        Ok(())
    }

    /// Enabled may differ after an interrupted disable/restore; no other
    /// registration or security change is ours to overwrite.
    pub fn check_identity(&self, observed: &[TaskSnapshot]) -> Result<(), String> {
        self.validate()?;
        if observed.len() != self.tasks.len()
            || self.tasks.iter().zip(observed).any(|(saved, now)| {
                saved.name != now.name
                    || saved.identity_xml != now.identity_xml
                    || saved.sddl != now.sddl
            })
        {
            return Err(
                "scheduled task identity/security changed; recovery refuses to overwrite it".into(),
            );
        }
        Ok(())
    }

    pub fn check_restored(&self, observed: &[TaskSnapshot]) -> Result<(), String> {
        self.check_identity(observed)?;
        if self
            .tasks
            .iter()
            .zip(observed)
            .any(|(saved, now)| saved.enabled != now.enabled)
        {
            return Err("original scheduled task enabled state is not restored".into());
        }
        Ok(())
    }

    pub fn check_disabled(&self, observed: &[TaskSnapshot]) -> Result<(), String> {
        self.check_identity(observed)?;
        if observed.iter().any(|t| t.enabled) {
            return Err("maintenance task was re-enabled".into());
        }
        Ok(())
    }
}

/// The same FileShare::None lease held by the installer/reboot handoff.
#[cfg(windows)]
pub fn take_install_lease(root: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::create_dir_all(root)
        .map_err(|e| format!("cannot create install lease directory: {e}"))?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(root.join("install.lock"))
        .map_err(|e| {
            format!(
                "another installer/reboot holds the install lease, or it cannot be acquired: {e}"
            )
        })
}
