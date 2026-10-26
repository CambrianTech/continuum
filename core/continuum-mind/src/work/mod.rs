//! Native airc work/kanban tools — a persona claims / creates / releases cards
//! as ITS OWN airc identity, by DELEGATING to airc's own work API
//! (`Airc::claim_work_card` / `create_work_card` / `release_work_claim`). This is
//! encapsulation, not reinvention: the board, the projection, the events, the
//! storage are all airc's; continuum just lets a persona drive them as a peer.
//!
//! ## Why these exist alongside `airc work` over `code/shell`
//!
//! A persona can already READ/run the board via the `airc` CLI through its
//! `code/shell` — but that acts as the MACHINE-SCOPE identity, not the persona's
//! own key. These tools resolve the caller's `PersonaAircRuntime` and call its
//! `Airc` handle, so a claim is recorded as ASHA, not the operator. Identity-
//! correct kanban participation: the persona is a first-class work peer on the
//! same board as everyone else ([[personas-are-peers-in-your-mesh]]).
//!
//! Access tier: `Privileged` → `Trusted`. Coordinating on the shared board is for
//! trusted local citizens (a local persona / a trusted node), not an arbitrary
//! remote `Provisional` peer (who could otherwise spam claims). Board READS can
//! open up later; writes stay trusted.

use continuum_base::time::EpochMs;
use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use airc_lib::{Airc, CardState, ClaimId, ClaimWorkCard, CreateWorkCard, HeartbeatWorkClaim, Priority, ReleaseWorkClaim, RepoId, WorkCard, WorkCardId};

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

pub mod card_staging;
pub mod submission;
pub use submission::BenchmarkSubmissionAdapter;

use crate::persona::work_board::advance::*;
use crate::persona::work_board::events::*;
use crate::cognition::card_facts::*;
use crate::persona::work_board::horizon::*;

mod work_card_state_changed;
pub use work_card_state_changed::*;
mod work_claim;
mod claim_workspace;
pub use claim_workspace::ClaimWorkspace;
mod work_create_command;
pub use work_create_command::*;
mod work_create;
pub use work_create::*;
mod work_note;
pub use work_note::*;
mod work_heartbeat;
pub use work_heartbeat::*;
mod work_list;
pub use work_list::*;
mod work_similar;
pub use work_similar::*;

#[cfg(test)]
mod tests;
