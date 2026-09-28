//! Port leases for citizen services (card 0c42c0bf).
//!
//! A citizen building a website runs a dev server, a database, a preview. Each needs a port,
//! and before this nothing kept one clear of another citizen's server or of Continuum's own
//! fixed ports (the lanes at 58057 and up, 58091 and up, 58200 and up, the websocket at 8974,
//! the grid at 7117/7118, LiveKit at 7880/7443, the call server at 50053). A lease names ONE
//! port in [`CITIZEN_PORT_RANGE`] for `(node, citizen, service)`, durably, so the same service
//! gets the same port after a deploy (the respawn of declared services, card 2effe50e, reads it).
//!
//! - The lease is the authority AMONG CITIZENS, not an OS reservation: a port is taken only if
//!   nobody holds a lease on it AND it binds right now (the one probe,
//!   [`crate::utils::ports::first_bindable`]). A foreign process that later sits on a leased
//!   port shows up as her server's bind error; the lease stays hers.
//! - A release does not free the port at once (Codex on the plan): her service may still be
//!   running, or restarting between listens. The row is kept as released, the port stays
//!   unavailable to anyone else for [`RELEASE_QUARANTINE_MS`], and re-leasing the same service
//!   gives her the same port back.
//! - Keyed by node (Fable on the plan): a port is a fact about one machine, and `main` may be
//!   a database several nodes share (`DATABASE_URL`).
//! - Bounded: the range is 1,000 ports and a citizen holds at most
//!   [`MAX_LEASES_PER_CITIZEN`]. There is no TTL, since a TTL would hand a running server's
//!   port to someone else.
//! - Stored through the ORM ([`PortLease`], `port_leases`), with unique(node, port) and
//!   unique(node, holder, service) as the backstop. The ORM has no transactions, so choosing
//!   and saving is serialized in-process: one core owns its node's ports.
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

/// How long a released port stays unavailable to anyone else: long enough for a service that
/// was still shutting down, or restarting, to be gone.
pub const RELEASE_QUARANTINE_MS: u64 = 10 * 60 * 1000;

/// Where a leased port must bind to be free: citizen services listen on loopback.
const LEASE_HOST: &str = "127.0.0.1";

/// One leased port: `service` (her name for it) of `holder` listens on `port` of `node`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "port_leases")]
#[entity(index(name = "idx_port_leases_node_port", fields = ["node", "port"], unique = true))]
#[entity(index(name = "idx_port_leases_node_holder_service", fields = ["node", "holder", "service"], unique = true))]
pub struct PortLease {
    #[serde(flatten)]
    pub base: BaseEntity,
    /// The node the port is on: its core's airc identity.
    #[entity(indexed)]
    pub node: PeerId,
    #[entity(indexed)]
    pub holder: PeerId,
    pub service: String,
    pub port: u16,
    pub leased_at_ms: u64,
    /// When she released it; the port stays hers to re-lease, and nobody else's, until
    /// [`RELEASE_QUARANTINE_MS`] has passed.
    pub released_at_ms: Option<u64>,
}

impl PortLease {
    fn active(&self) -> bool {
        self.released_at_ms.is_none()
    }

    /// Released, but not long enough ago for the port to go to anyone else.
    fn quarantined(&self, now_ms: u64) -> bool {
        self.released_at_ms.is_some_and(|at| now_ms.saturating_sub(at) < RELEASE_QUARANTINE_MS)
    }
}

/// This node's identity, once its core's airc handle is attached (`None` before).
pub type NodeIdentity = Arc<dyn Fn() -> Option<PeerId> + Send + Sync>;

/// Why a lease could not be given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortLeaseError {
    #[error("a service name is 1 to 48 characters of a-z, 0-9, '-', '_' or '.'; got {0:?}")]
    InvalidService(String),
    #[error("you hold {} port leases, the most one citizen may: {}. Release one with ports/release first", .held.len(), listed(.held))]
    AtCap { held: Vec<(String, u16)> },
    #[error("every port in {}-{} is leased, recently released or will not bind on this node{}", CITIZEN_PORT_RANGE.start(), CITIZEN_PORT_RANGE.end(), unbindable_note(.unbindable))]
    RangeExhausted {
        /// The free-by-lease ports that would not bind here, as `(first, last)` runs, so she
        /// is not left guessing (Fable on the plan: Windows' Hyper-V and WinNAT reserve blocks).
        unbindable: Vec<(u16, u16)>,
    },
    #[error("port leases could not be stored: {0}")]
    Store(String),
    #[error("this node's identity is not known yet (its core has not attached to airc), so a port on it cannot be leased; try again shortly")]
    NodeUnknown,
}

/// Her leases as `service -> port`, for a refusal that names them.
fn listed(held: &[(String, u16)]) -> String {
    held.iter().map(|(service, port)| format!("{service} -> {port}")).collect::<Vec<_>>().join(", ")
}

/// Where a refusal names the blocks that would not bind, and how to see who reserved them.
fn unbindable_note(runs: &[(u16, u16)]) -> String {
    if runs.is_empty() {
        return String::new();
    }
    let named: Vec<String> = runs.iter().map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}-{b}") }).collect();
    format!(
        "; these would not bind here: {} (on Windows, Hyper-V and WinNAT reserve port blocks: \
         netsh int ipv4 show excludedportrange protocol=tcp)",
        named.join(", ")
    )
}

/// PURE: `ports` (ascending) that `bindable` refuses, as contiguous `(first, last)` runs.
fn unbindable_runs(ports: impl IntoIterator<Item = u16>, bindable: impl Fn(u16) -> bool) -> Vec<(u16, u16)> {
    let mut runs: Vec<(u16, u16)> = Vec::new();
    for port in ports.into_iter().filter(|&p| !bindable(p)) {
        match runs.last_mut() {
            Some((_, last)) if last.checked_add(1) == Some(port) => *last = port,
            _ => runs.push((port, port)),
        }
    }
    runs
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
    /// She holds this service's lease: the same port, again.
    Held(PortLease),
    /// She released this service's lease: hers again, on the same port.
    Reclaim(PortLease),
    /// A new lease on this port.
    Take(u16),
}

/// PURE: the lease decision over this node's leases (released ones included) at `now_ms`.
/// `pick` chooses a port given the set unavailable to a new lease (production: the first in
/// range that is not in the set and binds right now).
pub(crate) fn decide(
    on_node: &[PortLease],
    holder: PeerId,
    service: &str,
    now_ms: u64,
    pick: impl FnOnce(&BTreeSet<u16>) -> Option<u16>,
) -> Result<LeaseDecision, PortLeaseError> {
    validate_service(service)?;
    if let Some(existing) = on_node.iter().find(|l| l.holder == holder && l.service == service) {
        return Ok(if existing.active() {
            LeaseDecision::Held(existing.clone())
        } else {
            LeaseDecision::Reclaim(existing.clone())
        });
    }
    let hers: Vec<(String, u16)> = on_node.iter().filter(|l| l.holder == holder && l.active()).map(|l| (l.service.clone(), l.port)).collect();
    if hers.len() >= MAX_LEASES_PER_CITIZEN {
        return Err(PortLeaseError::AtCap { held: hers });
    }
    let unavailable: BTreeSet<u16> = on_node.iter().filter(|l| l.active() || l.quarantined(now_ms)).map(|l| l.port).collect();
    pick(&unavailable).map(LeaseDecision::Take).ok_or(PortLeaseError::RangeExhausted { unbindable: Vec::new() })
}

/// The first port in [`CITIZEN_PORT_RANGE`] not in `unavailable` that binds right now. The
/// probe is a non-blocking bind per candidate, microseconds each, so it runs inline.
fn first_free_citizen_port(unavailable: &BTreeSet<u16>) -> Option<u16> {
    crate::utils::ports::first_bindable(LEASE_HOST, CITIZEN_PORT_RANGE.filter(|p| !unavailable.contains(p)))
}

/// The node's port leases: the store, and every lease row held in memory for the synchronous
/// readers (her grounding), kept in step with the store under `write`.
pub struct PortLeases {
    store: OrmStore<PortLease>,
    /// Every row the store holds, for any node; each read filters to this one.
    leases: parking_lot::RwLock<Vec<PortLease>>,
    /// Serializes choose-then-save: the ORM has no transactions.
    write: tokio::sync::Mutex<()>,
    node: NodeIdentity,
}

static NODE_LEASES: OnceLock<Arc<PortLeases>> = OnceLock::new();

fn row_id(lease: &PortLease) -> Result<Uuid, PortLeaseError> {
    Uuid::parse_str(&lease.base.id).map_err(|e| PortLeaseError::Store(e.to_string()))
}

impl PortLeases {
    /// Load every lease from `store`; `node` names this node once it is known.
    pub async fn load(store: OrmStore<PortLease>, node: NodeIdentity) -> Result<Self, PortLeaseError> {
        let leases = store.find_all().await.map_err(|e| PortLeaseError::Store(e.to_string()))?.into_iter().map(|(_, l)| l).collect();
        Ok(Self { store, leases: parking_lot::RwLock::new(leases), write: tokio::sync::Mutex::new(()), node })
    }

    /// This node's rows, released ones included (empty until the node is known).
    fn on_this_node(&self) -> Vec<PortLease> {
        match (self.node)() {
            Some(node) => self.leases.read().iter().filter(|l| l.node == node).cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Replace (or add) `lease`'s row in memory, after the store took it.
    fn remember(&self, lease: &PortLease) {
        let mut rows = self.leases.write();
        rows.retain(|l| l.base.id != lease.base.id);
        rows.push(lease.clone());
    }

    /// Lease a port to `holder` for `service`; `true` when it is a new port. Asking again for
    /// a service she holds, or released, returns the same port.
    pub async fn lease(&self, holder: PeerId, service: &str) -> Result<(PortLease, bool), PortLeaseError> {
        let _serialized = self.write.lock().await;
        let node = (self.node)().ok_or(PortLeaseError::NodeUnknown)?;
        let now = crate::persona::trace::now_ms();
        let on_node = self.on_this_node();
        let decision = decide(&on_node, holder, service, now, first_free_citizen_port).map_err(|e| match e {
            // only on this refusal is the whole range probed, to name what would not bind
            PortLeaseError::RangeExhausted { .. } => {
                let unavailable: BTreeSet<u16> =
                    on_node.iter().filter(|l| l.active() || l.quarantined(now)).map(|l| l.port).collect();
                let candidates = CITIZEN_PORT_RANGE.filter(|p| !unavailable.contains(p));
                let bindable = |p: u16| crate::utils::ports::first_bindable(LEASE_HOST, [p]).is_some();
                PortLeaseError::RangeExhausted { unbindable: unbindable_runs(candidates, bindable) }
            }
            other => other,
        })?;
        match decision {
            LeaseDecision::Held(lease) => Ok((lease, false)),
            LeaseDecision::Reclaim(mut lease) => {
                lease.released_at_ms = None;
                lease.leased_at_ms = now;
                self.store.update(row_id(&lease)?, &lease).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
                self.remember(&lease);
                Ok((lease, false))
            }
            LeaseDecision::Take(port) => {
                // a row whose quarantine is over still holds unique(node, port): retire it first
                if let Some(stale) = on_node.iter().find(|l| l.port == port) {
                    self.store.delete(row_id(stale)?).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
                    self.leases.write().retain(|l| l.base.id != stale.base.id);
                }
                let lease = PortLease {
                    base: BaseEntity::for_new_record(),
                    node,
                    holder,
                    service: service.to_string(),
                    port,
                    leased_at_ms: now,
                    released_at_ms: None,
                };
                self.store.save(row_id(&lease)?, &lease).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
                self.remember(&lease);
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

    /// Release `holder`'s lease for `service`: the port stays unavailable to others for
    /// [`RELEASE_QUARANTINE_MS`]. The lease that ended, if she held one.
    pub async fn release(&self, holder: PeerId, service: &str) -> Result<Option<PortLease>, PortLeaseError> {
        let _serialized = self.write.lock().await;
        let Some(mut lease) = self.on_this_node().into_iter().find(|l| l.holder == holder && l.service == service && l.active()) else {
            return Ok(None);
        };
        lease.released_at_ms = Some(crate::persona::trace::now_ms());
        self.store.update(row_id(&lease)?, &lease).await.map_err(|e| PortLeaseError::Store(e.to_string()))?;
        self.remember(&lease);
        crate::probe!(
            class = "ports.released",
            holder = %holder,
            service,
            port = lease.port as u64,
            "a citizen released a port lease; the port is quarantined before anyone else may take it"
        );
        Ok(Some(lease))
    }

    /// `holder`'s active leases on this node, by service name.
    pub fn held_by(&self, holder: PeerId) -> Vec<PortLease> {
        let mut hers: Vec<PortLease> = self.on_this_node().into_iter().filter(|l| l.holder == holder && l.active()).collect();
        hers.sort_by(|a, b| a.service.cmp(&b.service));
        hers
    }

    /// Every active lease on this node, by port.
    pub fn all(&self) -> Vec<PortLease> {
        let mut all: Vec<PortLease> = self.on_this_node().into_iter().filter(PortLease::active).collect();
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
    node: NodeIdentity,
}

impl PortsModule {
    /// `node` names this node once its core's airc handle is attached.
    pub fn new(node: NodeIdentity) -> Self {
        Self { leases: Arc::new(LateBound::new("port leases")), node }
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
        let leases = Arc::new(PortLeases::load(store, Arc::clone(&self.node)).await.map_err(|e| e.to_string())?);
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
        PortLease {
            base: BaseEntity::for_new_record(),
            node: peer(100),
            holder,
            service: service.into(),
            port,
            leased_at_ms: 1,
            released_at_ms: None,
        }
    }

    fn node(n: u128) -> NodeIdentity {
        Arc::new(move || Some(peer(n)))
    }

    fn first_not_in(unavailable: &BTreeSet<u16>) -> Option<u16> {
        CITIZEN_PORT_RANGE.clone().find(|p| !unavailable.contains(p))
    }

    // what this catches: the lease contract. Asking again for a service she holds is the SAME
    // port (a respawned dev server gets its port back), a new lease never takes a port anyone
    // leases, a citizen at the cap is refused with her leases named, and a bad service name
    // is refused before anything is chosen.
    #[test]
    fn a_lease_is_stable_per_service_and_never_shares_a_port() {
        let (kimi, iris) = (peer(1), peer(2));
        let on_node = vec![lease(kimi, "cw-web", 31_000), lease(iris, "api", 31_001)];

        let again = decide(&on_node, kimi, "cw-web", 5, |_| panic!("a held service picks nothing")).expect("held");
        assert!(matches!(again, LeaseDecision::Held(l) if l.port == 31_000));
        assert_eq!(decide(&on_node, kimi, "cw-db", 5, first_not_in), Ok(LeaseDecision::Take(31_002)), "the first port nobody leases");

        let full: Vec<PortLease> = (0..MAX_LEASES_PER_CITIZEN as u16).map(|i| lease(kimi, &format!("s{i}"), 31_100 + i)).collect();
        match decide(&full, kimi, "one-more", 5, |_| Some(31_500)) {
            Err(PortLeaseError::AtCap { held }) => assert_eq!(held.len(), MAX_LEASES_PER_CITIZEN),
            other => panic!("at the cap she is refused, got {other:?}"),
        }
        assert!(decide(&full, iris, "one-more", 5, |_| Some(31_500)).is_ok(), "the cap is per citizen");

        assert_eq!(decide(&on_node, kimi, "Bad Name", 5, |_| Some(31_500)), Err(PortLeaseError::InvalidService("Bad Name".into())));
        assert_eq!(decide(&on_node, kimi, "x", 5, |_| None), Err(PortLeaseError::RangeExhausted { unbindable: Vec::new() }));
    }

    // what this catches (Codex on the plan): a released port handed to another citizen while
    // the old service may still be running or restarting. Within the quarantine it is
    // unavailable to everyone else, and hers again on re-leasing the same service; after it,
    // the port can go to someone new.
    #[test]
    fn a_released_port_is_quarantined_and_hers_to_reclaim() {
        let (kimi, iris) = (peer(1), peer(2));
        let released_at = 1_000_000;
        let mut old = lease(kimi, "cw-web", 31_000);
        old.released_at_ms = Some(released_at);
        let on_node = vec![old];

        let during = released_at + RELEASE_QUARANTINE_MS - 1;
        assert_eq!(decide(&on_node, iris, "api", during, first_not_in), Ok(LeaseDecision::Take(31_001)), "not the quarantined port");
        assert!(matches!(decide(&on_node, kimi, "cw-web", during, first_not_in), Ok(LeaseDecision::Reclaim(l)) if l.port == 31_000));
        let after = released_at + RELEASE_QUARANTINE_MS;
        assert_eq!(decide(&on_node, iris, "api", after, first_not_in), Ok(LeaseDecision::Take(31_000)), "free once the quarantine is over");
        assert!(decide(&on_node, kimi, "other", during, first_not_in).is_ok(), "a released lease does not count toward her cap");
    }

    // what this catches: the probe choosing a port that is in use on the host (bound by a
    // process with no lease), or one another citizen leases.
    #[test]
    fn a_new_lease_skips_leased_and_bound_ports() {
        let first = *CITIZEN_PORT_RANGE.start();
        let squatter = std::net::TcpListener::bind((LEASE_HOST, first + 1)).ok();
        let unavailable: BTreeSet<u16> = [first].into_iter().collect();
        let chosen = first_free_citizen_port(&unavailable).expect("a free port in range");
        assert!(CITIZEN_PORT_RANGE.contains(&chosen));
        assert_ne!(chosen, first, "a leased port is never chosen");
        if squatter.is_some() {
            assert_ne!(chosen, first + 1, "a port bound on the host is never chosen");
        }
    }

    // what this catches (Fable on the plan): a refusal on an exhausted range that leaves her
    // guessing why. The ports that would not bind are named as runs (Windows reserves whole
    // blocks), and the message says where to look.
    #[test]
    fn an_exhausted_range_names_the_blocks_that_would_not_bind() {
        let binds = |p: u16| !(31_100..=31_227).contains(&p) && p != 31_500;
        assert_eq!(unbindable_runs(31_000..=31_999, binds), vec![(31_100, 31_227), (31_500, 31_500)]);
        let refusal = PortLeaseError::RangeExhausted { unbindable: vec![(31_100, 31_227), (31_500, 31_500)] }.to_string();
        assert!(refusal.contains("31100-31227, 31500") && refusal.contains("excludedportrange"), "{refusal}");
        assert!(!PortLeaseError::RangeExhausted { unbindable: Vec::new() }.to_string().contains("netsh"));
    }

    // what this catches: leases that do not survive the store (a restart loses every port,
    // and every respawned server lands somewhere new), a release that frees the port at once,
    // two nodes sharing a store seeing each other's leases, and a node that leases before it
    // knows its own identity.
    #[tokio::test]
    async fn leases_survive_a_reload_and_stay_on_their_node() {
        let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
        let store = || async { OrmStore::<PortLease>::new(Arc::clone(&adapter)).await.expect("store") };
        let leases = PortLeases::load(store().await, node(100)).await.expect("load");
        let kimi = peer(7);
        let (first, new) = leases.lease(kimi, "cw-web").await.expect("lease");
        assert!(new);
        let (again, new_again) = leases.lease(kimi, "cw-web").await.expect("lease again");
        assert!(!new_again);
        assert_eq!(again.port, first.port, "the same port on asking again");

        let reloaded = PortLeases::load(store().await, node(100)).await.expect("reload");
        assert_eq!(reloaded.held_by(kimi).iter().map(|l| l.port).collect::<Vec<_>>(), vec![first.port], "durable across a restart");
        let other_node = PortLeases::load(store().await, node(200)).await.expect("other node");
        assert!(other_node.held_by(kimi).is_empty(), "a lease is a fact about one node");

        assert_eq!(reloaded.release(kimi, "cw-web").await.expect("release").map(|l| l.port), Some(first.port));
        assert!(reloaded.held_by(kimi).is_empty(), "released: no longer in her grounding");
        let (reclaimed, reclaimed_new) = reloaded.lease(kimi, "cw-web").await.expect("reclaim");
        assert_eq!((reclaimed.port, reclaimed_new), (first.port, false), "hers again, the same port");

        let unknown = PortLeases::load(store().await, Arc::new(|| None)).await.expect("load");
        assert_eq!(unknown.lease(kimi, "cw-web").await.map(|_| ()), Err(PortLeaseError::NodeUnknown));
    }
}
