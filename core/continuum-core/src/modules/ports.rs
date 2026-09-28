//! Port leases for citizen services (card 0c42c0bf).
//!
//! A citizen building a website runs a dev server, a database, a preview. Each needs a port,
//! and before this nothing kept one clear of another citizen's server or of Continuum's own
//! fixed ports (the lanes at 58057 and up, 58091 and up, 58200 and up, the websocket at 8974,
//! the grid at 7117/7118, LiveKit at 7880/7443, the call server at 50053). A lease names ONE
//! port in [`CITIZEN_PORT_RANGE`] for `(citizen, service)`, durably, so the same service gets
//! the same port after a deploy (the respawn of declared services, card 2effe50e, reads it).
//!
//! - The lease is the authority AMONG CITIZENS, not an OS reservation: a port is taken only if
//!   nobody holds a lease on it AND it binds right now (the one probe,
//!   [`crate::utils::ports::first_bindable`]). A foreign process that later sits on a leased
//!   port shows up as her server's bind error, with the lease named.
//! - Bounded: the range is 1,000 ports and a citizen holds at most
//!   [`MAX_LEASES_PER_CITIZEN`]. There is no TTL, since a TTL would hand a running server's
//!   port to someone else; a lease ends when she releases it.
//! - Stored through the ORM ([`PortLease`], `port_leases`), with unique(port) and
//!   unique(holder, service) as the backstop. The ORM has no transactions, so allocation is
//!   serialized in-process: one core owns its node's ports.
//! - She sees her leases in her grounding (`WorkspaceMapSource`), so she never guesses a port.

use std::any::Any;
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::identity::PeerId;
use crate::orm::entity::BaseEntity;
use crate::orm::{Entity, OrmStore};
use crate::runtime::{CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

/// The ports citizen services are leased from. Below every OS ephemeral range (Linux starts at
/// 32768, macOS and Windows at 49152), clear of every fixed port Continuum binds, and clear of
/// the common dev-server defaults (3000, 5173, 8080) a framework picks on its own.
pub const CITIZEN_PORT_RANGE: std::ops::RangeInclusive<u16> = 31_000..=31_999;

/// The most leases one citizen holds at once: enough for a web app's server, database,
/// preview and test runner several times over, and a bound on one citizen's share.
pub const MAX_LEASES_PER_CITIZEN: usize = 16;

/// Where a leased port must bind to be free: citizen services listen on loopback.
const LEASE_HOST: &str = "127.0.0.1";

/// One leased port: `service` (her name for it) of `holder` listens on `port`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "port_leases")]
#[entity(index(name = "idx_port_leases_holder_service", fields = ["holder", "service"], unique = true))]
pub struct PortLease {
    #[serde(flatten)]
    pub base: BaseEntity,
    #[entity(indexed)]
    pub holder: PeerId,
    pub service: String,
    #[entity(unique)]
    pub port: u16,
    pub leased_at_ms: u64,
}

/// Why a lease could not be given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortLeaseError {
    #[error("a service name is 1 to 48 characters of a-z, 0-9, '-', '_' or '.'; got {0:?}")]
    InvalidService(String),
    #[error("you hold {} port leases, the most one citizen may: {}. Release one with ports/release first", .held.len(), listed(.held))]
    AtCap { held: Vec<(String, u16)> },
    #[error("every port in {}-{} is leased or in use on this node", CITIZEN_PORT_RANGE.start(), CITIZEN_PORT_RANGE.end())]
    RangeExhausted,
    #[error("port leases could not be stored: {0}")]
    Store(String),
}

/// Her leases as `service -> port`, for a refusal that names them.
fn listed(held: &[(String, u16)]) -> String {
    held.iter().map(|(service, port)| format!("{service} -> {port}")).collect::<Vec<_>>().join(", ")
}

/// A service name: short, and safe to show in her grounding and in a log.
fn validate_service(service: &str) -> Result<(), PortLeaseError> {
    let ok = (1..=48).contains(&service.len())
        && service.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(PortLeaseError::InvalidService(service.to_string()))
    }
}

/// What a lease request resolves to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LeaseDecision {
    /// She already holds this service's lease: the same port, again.
    Held(PortLease),
    /// A new lease on this port.
    Take(u16),
}

/// PURE: the lease decision over every lease on the node. `pick` chooses a port given the set
/// already leased (production: the first in range that also binds right now).
pub(crate) fn decide(
    all: &[PortLease],
    holder: PeerId,
    service: &str,
    pick: impl FnOnce(&BTreeSet<u16>) -> Option<u16>,
) -> Result<LeaseDecision, PortLeaseError> {
    validate_service(service)?;
    if let Some(existing) = all.iter().find(|l| l.holder == holder && l.service == service) {
        return Ok(LeaseDecision::Held(existing.clone()));
    }
    let hers: Vec<(String, u16)> = all.iter().filter(|l| l.holder == holder).map(|l| (l.service.clone(), l.port)).collect();
    if hers.len() >= MAX_LEASES_PER_CITIZEN {
        return Err(PortLeaseError::AtCap { held: hers });
    }
    let leased: BTreeSet<u16> = all.iter().map(|l| l.port).collect();
    pick(&leased).map(LeaseDecision::Take).ok_or(PortLeaseError::RangeExhausted)
}

/// The first port in [`CITIZEN_PORT_RANGE`] nobody leases that binds right now. The probe is
/// a non-blocking bind per candidate, microseconds each, so it runs inline.
fn first_free_citizen_port(leased: &BTreeSet<u16>) -> Option<u16> {
    crate::utils::ports::first_bindable(LEASE_HOST, CITIZEN_PORT_RANGE.filter(|p| !leased.contains(p)))
}

/// The node's port leases: the store, and every lease held in memory for the synchronous
/// readers (her grounding), kept in step with the store under `write`.
pub struct PortLeases {
    store: OrmStore<PortLease>,
    leases: parking_lot::RwLock<Vec<PortLease>>,
    /// Serializes choose-then-save: the ORM has no transactions.
    write: tokio::sync::Mutex<()>,
}

static NODE_LEASES: OnceLock<Arc<PortLeases>> = OnceLock::new();

impl PortLeases {
    /// Load every lease from `store`.
    pub async fn load(store: OrmStore<PortLease>) -> Result<Self, PortLeaseError> {
        let leases = store.find_all().await.map_err(|e| PortLeaseError::Store(e.to_string()))?.into_iter().map(|(_, l)| l).collect();
        Ok(Self { store, leases: parking_lot::RwLock::new(leases), write: tokio::sync::Mutex::new(()) })
    }

    /// Lease a port to `holder` for `service`; `true` when the lease is new. Asking again for a
    /// service she holds returns the same lease.
    pub async fn lease(&self, holder: PeerId, service: &str) -> Result<(PortLease, bool), PortLeaseError> {
        let _serialized = self.write.lock().await;
        let snapshot = self.leases.read().clone();
        match decide(&snapshot, holder, service, first_free_citizen_port)? {
            LeaseDecision::Held(lease) => Ok((lease, false)),
            LeaseDecision::Take(port) => {
                let lease = PortLease {
                    base: BaseEntity::for_new_record(),
                    holder,
                    service: service.to_string(),
                    port,
                    leased_at_ms: crate::persona::trace::now_ms(),
                };
                let id = Uuid::parse_str(&lease.base.id).map_err(|e| PortLeaseError::Store(e.to_string()))?;
                self.store.save(id, &lease).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
                self.leases.write().push(lease.clone());
                crate::probe!(
                    class = "ports.leased",
                    holder = %holder,
                    service,
                    port = port as u64,
                    "a port is leased to a citizen's service"
                );
                Ok((lease, true))
            }
        }
    }

    /// End `holder`'s lease for `service`; the lease that ended, if she held one.
    pub async fn release(&self, holder: PeerId, service: &str) -> Result<Option<PortLease>, PortLeaseError> {
        let _serialized = self.write.lock().await;
        let Some(lease) = self.leases.read().iter().find(|l| l.holder == holder && l.service == service).cloned() else {
            return Ok(None);
        };
        let id = Uuid::parse_str(&lease.base.id).map_err(|e| PortLeaseError::Store(e.to_string()))?;
        self.store.delete(id).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
        self.leases.write().retain(|l| l.base.id != lease.base.id);
        crate::probe!(
            class = "ports.released",
            holder = %holder,
            service,
            port = lease.port as u64,
            "a citizen released a port lease"
        );
        Ok(Some(lease))
    }

    /// `holder`'s leases, by service name.
    pub fn held_by(&self, holder: PeerId) -> Vec<PortLease> {
        let mut hers: Vec<PortLease> = self.leases.read().iter().filter(|l| l.holder == holder).cloned().collect();
        hers.sort_by(|a, b| a.service.cmp(&b.service));
        hers
    }

    /// Every lease on the node, by port.
    pub fn all(&self) -> Vec<PortLease> {
        let mut all = self.leases.read().clone();
        all.sort_by_key(|l| l.port);
        all
    }
}

/// `holder`'s leases on this node, for her grounding; empty before the ports module is up.
pub fn held_by(holder: PeerId) -> Vec<PortLease> {
    NODE_LEASES.get().map(|leases| leases.held_by(holder)).unwrap_or_default()
}

/// Owns the node's [`PortLeases`] and the `ports/*` verbs.
pub struct PortsModule {
    leases: Arc<LateBound<PortLeases>>,
}

impl PortsModule {
    pub fn new() -> Self {
        Self { leases: Arc::new(LateBound::new("port leases")) }
    }
}

impl Default for PortsModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for PortsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ports",
            priority: ModulePriority::Normal,
            command_prefixes: &["ports/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let module = ctx.registry.get_by_name("data").ok_or_else(|| "ports requires the data module".to_string())?;
        let data = module
            .as_any()
            .downcast_ref::<crate::modules::data::DataModule>()
            .ok_or_else(|| "ports data module type mismatch".to_string())?;
        let adapter = data.state.get_adapter("main").await?;
        let store = OrmStore::new(adapter).await.map_err(|e| e.to_string())?;
        let leases = Arc::new(PortLeases::load(store).await.map_err(|e| e.to_string())?);
        // one ports module per core: a second install would be a boot-order defect
        let _ = NODE_LEASES.set(Arc::clone(&leases));
        self.leases.install(leases);
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::ports::command_objects(Arc::clone(&self.leases))
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("ports commands are on the typed registry; '{command}' has no legacy handler"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(n))
    }

    fn lease(holder: PeerId, service: &str, port: u16) -> PortLease {
        PortLease { base: BaseEntity::for_new_record(), holder, service: service.into(), port, leased_at_ms: 1 }
    }

    // what this catches: the lease contract. Asking again for a service she holds is the SAME
    // port (a respawned dev server gets its port back), a new lease never takes a port anyone
    // leases, a citizen at the cap is refused with her leases named, and a bad service name
    // is refused before anything is chosen.
    #[test]
    fn a_lease_is_stable_per_service_and_never_shares_a_port() {
        let (kimi, iris) = (peer(1), peer(2));
        let all = vec![lease(kimi, "cw-web", 31_000), lease(iris, "api", 31_001)];

        let again = decide(&all, kimi, "cw-web", |_| panic!("a held service picks nothing")).expect("held");
        assert!(matches!(again, LeaseDecision::Held(l) if l.port == 31_000));

        let fresh = decide(&all, kimi, "cw-db", |leased| CITIZEN_PORT_RANGE.clone().find(|p| !leased.contains(p))).expect("new");
        assert_eq!(fresh, LeaseDecision::Take(31_002), "the first port nobody leases");

        let full: Vec<PortLease> = (0..MAX_LEASES_PER_CITIZEN as u16).map(|i| lease(kimi, &format!("s{i}"), 31_100 + i)).collect();
        match decide(&full, kimi, "one-more", |_| Some(31_500)) {
            Err(PortLeaseError::AtCap { held }) => assert_eq!(held.len(), MAX_LEASES_PER_CITIZEN),
            other => panic!("at the cap she is refused, got {other:?}"),
        }
        assert!(decide(&full, iris, "one-more", |_| Some(31_500)).is_ok(), "the cap is per citizen");

        assert_eq!(decide(&all, kimi, "Bad Name", |_| Some(31_500)), Err(PortLeaseError::InvalidService("Bad Name".into())));
        assert_eq!(decide(&all, kimi, "x", |_| None), Err(PortLeaseError::RangeExhausted));
    }

    // what this catches: the probe choosing a port that is in use on the host (bound by a
    // process with no lease), or one another citizen leases.
    #[test]
    fn a_new_lease_skips_leased_and_bound_ports() {
        let first = *CITIZEN_PORT_RANGE.start();
        let squatter = std::net::TcpListener::bind((LEASE_HOST, first + 1)).ok();
        let leased: BTreeSet<u16> = [first].into_iter().collect();
        let chosen = first_free_citizen_port(&leased).expect("a free port in range");
        assert!(CITIZEN_PORT_RANGE.contains(&chosen));
        assert_ne!(chosen, first, "a leased port is never chosen");
        if squatter.is_some() {
            assert_ne!(chosen, first + 1, "a port bound on the host is never chosen");
        }
    }

    // what this catches: leases that do not survive the store (a restart loses every port,
    // and every respawned server lands somewhere new), or a release that leaves the row.
    #[tokio::test]
    async fn leases_survive_a_reload_and_a_release_removes_the_row() {
        let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
        let leases = PortLeases::load(OrmStore::new(Arc::clone(&adapter)).await.expect("store")).await.expect("load");
        let kimi = peer(7);
        let (first, new) = leases.lease(kimi, "cw-web").await.expect("lease");
        assert!(new);
        let (again, new_again) = leases.lease(kimi, "cw-web").await.expect("lease again");
        assert!(!new_again);
        assert_eq!(again.port, first.port, "the same port on asking again");

        let reloaded = PortLeases::load(OrmStore::new(Arc::clone(&adapter)).await.expect("store")).await.expect("reload");
        assert_eq!(reloaded.held_by(kimi).iter().map(|l| l.port).collect::<Vec<_>>(), vec![first.port], "durable across a restart");

        assert_eq!(reloaded.release(kimi, "cw-web").await.expect("release").map(|l| l.port), Some(first.port));
        let after = PortLeases::load(OrmStore::new(adapter).await.expect("store")).await.expect("reload");
        assert!(after.held_by(kimi).is_empty(), "the released row is gone from the store");
    }
}
