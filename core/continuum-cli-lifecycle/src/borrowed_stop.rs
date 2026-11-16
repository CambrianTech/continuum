//! Borrowed teardown authority, composed with the existing registered-task owner.
//! The caller supplies its drain; this owner acquires authority before invoking it.
use std::{future::Future, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("this platform has no borrowed teardown authority")]
    Unsupported,
    #[error("registered core task: {0}")]
    Task(String),
    #[error("ContinuumCore is not registered")]
    NotRegistered,
    #[error("registered core descriptor: {0}")]
    Descriptor(#[source] serde_json::Error),
    #[error("registered core artifact has no parent: {0}")]
    NoDirectory(std::path::PathBuf),
    #[error(transparent)]
    Consent(#[from] crate::elevated_teardown::ElevationError),
    #[error(transparent)]
    Plan(#[from] crate::elevated_teardown::TeardownPlanError),
}

pub async fn run_child<F, Fut>(plan: &Path, sha: &str, drain: F) -> Result<(), Error>
where F: FnOnce() -> Fut, Fut: Future<Output = String> {
    #[cfg(windows)]
    { crate::windows_elevated_teardown::teardown_elevated(plan, sha, drain).await?; Ok(()) }
    #[cfg(not(windows))]
    { let _ = (plan, sha, drain); Err(Error::Unsupported) }
}

pub async fn borrow_authority(
    pid: i32,
    home: &Path,
    payload_root: impl FnOnce(&Path) -> Result<std::path::PathBuf, String>,
) -> Result<(), Error> {
    #[cfg(windows)]
    {
        let task = crate::registered_core_task::query_optional(home, payload_root).await
            .map_err(Error::Task)?.ok_or(Error::NotRegistered)?;
        let description: crate::installed_release::Release =
            serde_json::from_str(&task.description).map_err(Error::Descriptor)?;
        let artifact = std::path::PathBuf::from(description.artifact);
        let directory = artifact.parent().ok_or_else(|| Error::NoDirectory(artifact.clone()))?;
        crate::windows_elevated_teardown::request_elevated_teardown(pid, directory).await?;
        Ok(())
    }
    #[cfg(not(windows))]
    { let _ = (pid, home, payload_root); Err(Error::Unsupported) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn rejected_child_never_invokes_the_server_drain() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let drained = Cell::new(false);
        let result = run_child(&dir.path().join("missing-plan.json"), "invalid", || async {
            drained.set(true);
            String::new()
        }).await;
        #[cfg(windows)]
        assert!(matches!(result, Err(Error::Plan(crate::elevated_teardown::TeardownPlanError::Read { .. }))));
        #[cfg(not(windows))]
        assert!(matches!(result, Err(Error::Unsupported)));
        assert!(!drained.get());
        Ok(())
    }
}
