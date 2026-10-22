//! Public submission/review commands on the existing work owner. Artifact
//! references carry content identity; publication does not imply blob verification.

use continuum_base::time::EpochMs;
use super::*;
use crate::persona::training_producer::reviewed::{self, SubmissionSelection};
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

use crate::persona::work_board::submissions::*;

mod latest_submission;
pub use latest_submission::*;
mod next_after_submission;
pub use next_after_submission::*;
mod work_submission;

#[cfg(test)]
mod tests;
