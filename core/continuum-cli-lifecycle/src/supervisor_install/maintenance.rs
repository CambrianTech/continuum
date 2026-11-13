//! Recoverable scheduler exclusion under the existing installation lease.
//! This does not stop running processes or exclude manual/legacy launchers.
//! It is deliberately not an expert-container migration authorization.

use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub use windows::{
    LegacyImageCustody, LegacyRestoration, LegacySchedulerWindow, SchedulerCustody, StartupPermit,
};

mod task_snapshot;
pub use task_snapshot::*;
mod launch_gate;
pub use launch_gate::*;
