//! CLI composition at the platform boundary. The service query stays with its existing
//! owner; Windows handle custody and consent live in continuum-cli-lifecycle.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct StopPlatformError(String);

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(windows), path = "other.rs")]
mod platform;
pub(super) use platform::{borrow_authority, run_child};

