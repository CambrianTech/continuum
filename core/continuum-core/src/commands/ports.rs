//! `ports/lease`, `ports/release`, `ports/list`: a citizen's services get ports that never
//! collide with another citizen's or with Continuum's own (card 0c42c0bf). See
//! `modules::ports` for the range, the bound and the store.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::identity::PeerId;
use crate::modules::ports::{PortLease, PortLeaseError, PortLeases, CITIZEN_PORT_RANGE};
use crate::runtime::LateBound;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx, DynCommand};

/// One leased port, as the verbs report it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortLeaseView.ts")]
pub struct PortLeaseView {
    pub service: String,
    pub port: u16,
    #[ts(type = "string")]
    pub holder: PeerId,
    #[ts(type = "number")]
    pub leased_at_ms: u64,
}

impl From<PortLease> for PortLeaseView {
    fn from(l: PortLease) -> Self {
        Self { service: l.service, port: l.port, holder: l.holder, leased_at_ms: l.leased_at_ms }
    }
}

/// Wire shape for `ports/lease` and `ports/release`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortsServiceParams.ts")]
pub struct PortsServiceParams {
    /// Her name for the service, e.g. `cw-web`: 1 to 48 characters of a-z, 0-9, `-`, `_`, `.`.
    pub service: String,
}

/// What `ports/lease` answers.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortsLeaseResult.ts")]
pub struct PortsLeaseResult {
    pub lease: PortLeaseView,
    /// False when she already held this service's lease: the same port as before.
    pub new: bool,
}

/// What `ports/release` answers.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortsReleaseResult.ts")]
pub struct PortsReleaseResult {
    /// The lease that ended; absent when she held none for that service.
    #[ts(optional)]
    pub released: Option<PortLeaseView>,
}

/// Wire shape for `ports/list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortsListParams.ts")]
pub struct PortsListParams {}

/// What `ports/list` answers.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ports/PortsListResult.ts")]
pub struct PortsListResult {
    /// Her leases; every lease on the node for the node's owner.
    pub leases: Vec<PortLeaseView>,
    pub range_first: u16,
    pub range_last: u16,
}

impl From<PortLeaseError> for CommandError {
    fn from(e: PortLeaseError) -> Self {
        match e {
            PortLeaseError::Store(_) | PortLeaseError::NodeUnknown => CommandError::Internal(e.to_string()),
            PortLeaseError::InvalidService(_) | PortLeaseError::AtCap { .. } | PortLeaseError::RangeExhausted { .. } => {
                CommandError::Invalid(e.to_string())
            }
        }
    }
}

/// The citizen a lease belongs to: the caller. A lease is hers, so a call with no caller
/// identity (a bare local invocation) names nobody to hold it.
fn holder(ctx: &Ctx, verb: &str) -> Result<PeerId, CommandError> {
    ctx.caller.as_ref().map(|c| c.peer_id).ok_or_else(|| {
        CommandError::Invalid(format!("{verb} acts for the citizen calling it, and this call carries no caller identity"))
    })
}

pub struct PortsLease {
    leases: Arc<LateBound<PortLeases>>,
}

#[async_trait]
impl ActionCommand for PortsLease {
    const NAME: &'static str = "ports/lease";
    const DESCRIPTION: &'static str = "Lease a local port for one of your services (a dev server, a database, a preview), so it never collides with another citizen's or with Continuum's own ports. Asking again for the same service returns the same port, including after a restart. Example: ports/lease --service cw-web, then start the server on the port it returns.";
    type Params = PortsServiceParams;
    type Output = PortsLeaseResult;

    async fn run(&self, ctx: &Ctx, p: Self::Params) -> Result<Self::Output, CommandError> {
        let holder = holder(ctx, Self::NAME)?;
        let leases = self.leases.require().map_err(CommandError::Internal)?;
        let (lease, new) = leases.lease(holder, &p.service).await?;
        Ok(PortsLeaseResult { lease: lease.into(), new })
    }
}

pub struct PortsRelease {
    leases: Arc<LateBound<PortLeases>>,
}

#[async_trait]
impl ActionCommand for PortsRelease {
    const NAME: &'static str = "ports/release";
    const DESCRIPTION: &'static str = "End your lease on a service's port once the service is gone for good. The port stays unavailable to others for ten minutes, in case the service is still stopping, and leasing the same service again gives it back. Example: ports/release --service cw-web.";
    type Params = PortsServiceParams;
    type Output = PortsReleaseResult;

    async fn run(&self, ctx: &Ctx, p: Self::Params) -> Result<Self::Output, CommandError> {
        let holder = holder(ctx, Self::NAME)?;
        let leases = self.leases.require().map_err(CommandError::Internal)?;
        let released = leases.release(holder, &p.service).await?;
        Ok(PortsReleaseResult { released: released.map(Into::into) })
    }
}

pub struct PortsList {
    leases: Arc<LateBound<PortLeases>>,
}

#[async_trait]
impl ActionCommand for PortsList {
    const NAME: &'static str = "ports/list";
    const DESCRIPTION: &'static str = "List your leased ports (service -> port) and the range they come from. The node's owner sees every citizen's leases.";
    type Params = PortsListParams;
    type Output = PortsListResult;

    async fn run(&self, ctx: &Ctx, _p: Self::Params) -> Result<Self::Output, CommandError> {
        let leases = self.leases.require().map_err(CommandError::Internal)?;
        let owner = crate::routing::grid_trust_policy::caller_trust(ctx.caller.as_ref())
            == crate::modules::grid::node::TrustLevel::Owner;
        let shown = match (&ctx.caller, owner) {
            (Some(caller), false) => leases.held_by(caller.peer_id),
            _ => leases.all(),
        };
        Ok(PortsListResult {
            leases: shown.into_iter().map(Into::into).collect(),
            range_first: *CITIZEN_PORT_RANGE.start(),
            range_last: *CITIZEN_PORT_RANGE.end(),
        })
    }
}

/// The `ports/*` command objects over the node's leases.
pub fn command_objects(leases: Arc<LateBound<PortLeases>>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(PortsLease { leases: Arc::clone(&leases) }),
        Arc::new(PortsRelease { leases: Arc::clone(&leases) }),
        Arc::new(PortsList { leases }),
    ]
}
