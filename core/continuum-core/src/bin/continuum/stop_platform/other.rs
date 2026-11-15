use super::StopPlatformError;
use std::path::Path;

pub(crate) async fn run_child(_plan: &Path, _sha: &str) -> Result<(), StopPlatformError> {
    Err(StopPlatformError("stop --elevated is the Windows consent child; this platform has no borrowed-privilege boundary".into()))
}

pub(crate) async fn borrow_authority(pid: i32) -> Result<(), StopPlatformError> {
    Err(StopPlatformError(format!("no teardown authority over pid {pid}; this platform has no consent boundary to borrow it through")))
}
