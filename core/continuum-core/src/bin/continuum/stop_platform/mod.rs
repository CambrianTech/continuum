//! Server-specific drain callback for the lifecycle-owned consent boundary.
use std::path::Path;
use super::{MayDrain, request_graceful_stop};
use continuum_cli_lifecycle::borrowed_stop;

pub(crate) async fn run_child(plan: &Path, sha: &str) -> Result<(), borrowed_stop::Error> {
    borrowed_stop::run_child(plan, sha, || async {
        format!("{:?}", request_graceful_stop(&MayDrain { borrow_authority_for: None }).await)
    }).await
}

pub(crate) async fn borrow_authority(pid: i32) -> Result<(), borrowed_stop::Error> {
    let home = super::continuum_root().map_err(borrowed_stop::Error::Task)?;
    borrowed_stop::borrow_authority(pid, &home, continuum_core::paths::payload_root).await
}