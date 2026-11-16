use super::StopPlatformError;
use super::super::{CoreServiceDescription, MayDrain, PreparedCoreService, request_graceful_stop};
use continuum_cli_lifecycle::{supervisor_install, windows_elevated_teardown};
use std::path::Path;

pub(crate) async fn run_child(plan: &Path, sha: &str) -> Result<(), StopPlatformError> {
    windows_elevated_teardown::teardown_elevated(plan, sha, || async {
        // The lifecycle owner invokes this callback only while holding its verified
        // QUERY + TERMINATE handle. No caller may drain before that capability exists.
        format!("{:?}", request_graceful_stop(&MayDrain { borrow_authority_for: None }).await)
    }).await.map_err(|error| StopPlatformError(error.to_string()))
}

pub(crate) async fn borrow_authority(pid: i32) -> Result<(), StopPlatformError> {
    let task = PreparedCoreService::query().await.map_err(|error| StopPlatformError(error.to_string()))?;
    let description: CoreServiceDescription = serde_json::from_str(&task.description)
        .map_err(|error| StopPlatformError(format!("the {} task's descriptor is unreadable ({error}); rerun the installer before escalating", supervisor_install::CORE_TASK)))?;
    // Query the existing descriptor owner once. The held handle later proves that
    // the target image belongs to this installation, even during a slot swap.
    let directory = Path::new(&description.artifact).parent()
        .ok_or_else(|| StopPlatformError(format!("the installed artifact {} has no directory", description.artifact)))?;
    windows_elevated_teardown::request_elevated_teardown(pid, &directory.display().to_string())
        .await.map_err(|error| StopPlatformError(error.to_string()))
}
