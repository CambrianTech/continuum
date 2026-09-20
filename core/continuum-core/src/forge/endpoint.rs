//! `forge::endpoint` — the **routable forge endpoint** (Contract C, Pass 5a).
//!
//! One row the model-endpoint fabric (`MODEL-ENDPOINT-FABRIC.md`) scores and
//! routes a forge need against: *which custodians can turn this checkpoint into a
//! gene, how to reach them, are they healthy, how much spare capacity, and may a
//! given job cross to them*. A node DISCOVERS this by probing — capability is
//! observed, never declared by config (Joel: "smart self determination, to the
//! needs of the grid").
//!
//! ## Generalizes `ForgeCapability` onto the CLEAN surface
//! The seed the contract doc (§5) names — `inference::unsloth_forge::ForgeCapability`
//! (`reachable/busy/phase/held_genes/outputs_dir`) — is bound to the *retiring*
//! unsloth `ForgeCustodian` trait (`train_status`/`list_loras`, the stateful
//! `/api/*` surface excised by `[[unsloth-universal-model-gateway]]` / task #52).
//! Building Pass 5 on it would build on dead code. Instead `ForgeEndpoint` is
//! derived from the Contract C [`HealthResponse`](super::protocol::HealthResponse),
//! which ALREADY carries the honest router inputs (R4): `contract_version`,
//! `ready`, `slots_total`, `slots_available`, `capability`. The Pass 3/4 health
//! handshake IS the probe; this type is its routing-shaped projection.
//!
//! ## Why a distinct `ForgeLocator` (not `GeneLocator`, not `HandleRef`)
//! [`GeneLocator`](super::gene_handle::GeneLocator) answers *where the bytes live*
//! (custody, has a `path`); a `ForgeEndpoint` locator answers *how to reach the
//! service* (a base URL, or a grid peer) — a different question, so a different
//! type rather than overloading one to mean both. `HandleRef` is a
//! state-correlation envelope, wrong for either. (This is the same `#17`
//! reconciliation discipline applied at the endpoint tier.)
//!
//! ## Scope of this pass
//! 5a delivers the TYPE + the probe-to-endpoint constructor and proves it against
//! fake custodians. It does NOT yet announce over the grid bus (5b wires
//! `NodeCapability` + `GridTransport::announce` into the live grid module) or
//! route work to a remote endpoint (Pass 6). Those CONSUME this row.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::custodian_client::{
    custodian_base_url, ForgeCustodian, ForgeCustodianError, ForgeCustodianHttp,
};
use super::protocol::{HealthResponse, CAPABILITY_GGUF_LORA};
use crate::identity::PeerId;
use crate::modules::grid::node::TrustLevel;

/// How to reach a forge custodian *service* (distinct from gene byte-custody —
/// see [`GeneLocator`](super::gene_handle::GeneLocator)).
///
/// `Local` is the degenerate single-node case: the custodian is an HTTP service
/// at `base_url` on this machine (where Pass 2's `ForgeCustodianHttp` connects).
/// `Node` is a custodian reachable over the grid transport at `node` — Pass 6
/// resolves it to GRID-ADDRESSING-AND-ROUTING.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/ForgeLocator.ts"
)]
#[serde(tag = "where", rename_all = "lowercase")]
pub enum ForgeLocator {
    /// Reach the custodian over HTTP at this base URL (this machine).
    Local { base_url: String },
    /// Reach the custodian over the grid transport to this peer.
    Node {
        // Canonical airc PeerId (serde-transparent Uuid, no ts-rs derive) → string.
        #[ts(type = "string")]
        node: PeerId,
    },
}

/// The custodian's routable health, as the fabric's scorer reads it. Derived from
/// the Contract C [`HealthResponse`] + reachability — NOT self-declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/ForgeHealth.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum ForgeHealth {
    /// Reachable, ready, and has spare capacity — route here.
    Healthy,
    /// Reachable + ready but saturated (`slots_available == 0`) — route elsewhere
    /// for now; it will free up. A transient, not a fault.
    Busy,
    /// Unreachable, OR reachable but `ready == false` (its converter tooling does
    /// not resolve) — it cannot do work; do not route here.
    Down,
}

/// One row in the forge tier of the endpoint fabric — what a daemon scores and
/// routes a forge need against. Discovered by probing ([`ForgeEndpoint::probe`]),
/// never configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/ForgeEndpoint.ts"
)]
pub struct ForgeEndpoint {
    /// How to reach this custodian (local HTTP | grid peer).
    pub locator: ForgeLocator,
    /// What it can do — `CAPABILITY_*` tags (e.g. [`CAPABILITY_GGUF_LORA`]).
    /// A custodian that grows formats appends; tags are never renamed.
    pub capabilities: Vec<String>,
    /// The wire-contract version it speaks (from `/health`). The fabric refuses to
    /// route to a version it cannot speak — kept here so the scorer, not this
    /// type, makes that call.
    pub contract_version: u32,
    /// Routable health, derived from the probe (never self-declared).
    pub health: ForgeHealth,
    /// Spare conversion slots RIGHT NOW (R3 bound) — `0` when `Busy`/`Down`.
    /// Honest because the custodian's slot count is bounded by a semaphore.
    pub capacity: u32,
    /// The GridTrustAuthPolicy boundary a job must satisfy to be dispatched here
    /// (the hard trust gate, not a score term). A local custodian is `Owner`.
    pub trust_scope: TrustLevel,
}

impl ForgeEndpoint {
    /// Project a [`HealthResponse`] (Contract C probe) + the chosen locator/trust
    /// into a routable row. Health derivation: `ready == false` ⇒ `Down` (live but
    /// cannot work); ready + no free slots ⇒ `Busy`; ready + spare ⇒ `Healthy`.
    pub fn from_health(locator: ForgeLocator, trust_scope: TrustLevel, h: &HealthResponse) -> Self {
        let health = if !h.ready {
            ForgeHealth::Down
        } else if h.slots_available == 0 {
            ForgeHealth::Busy
        } else {
            ForgeHealth::Healthy
        };
        Self {
            locator,
            capabilities: vec![h.capability.clone()],
            contract_version: h.contract_version,
            health,
            capacity: h.slots_available,
            trust_scope,
        }
    }

    /// The row for a custodian that did not answer the probe — `Down`, no capacity.
    /// An unreachable custodian is an honest sensor reading the daemon routes on
    /// (route elsewhere, or fail loud if it's the last one), NEVER a silent
    /// fallback — `probe` reports state, the daemon decides.
    pub fn unreachable(locator: ForgeLocator, trust_scope: TrustLevel) -> Self {
        Self {
            locator,
            capabilities: Vec::new(),
            contract_version: 0,
            health: ForgeHealth::Down,
            capacity: 0,
            trust_scope,
        }
    }

    /// DISCOVER a custodian's routable state by probing its `/health` over the
    /// Contract C trait. A reachable custodian yields a [`Self::from_health`] row;
    /// an [`ForgeCustodianError::Unreachable`] yields [`Self::unreachable`] (`Down`)
    /// — the truthful reading, not a fallback. An [`ForgeCustodianError::Api`]
    /// (reached but the handshake failed) is surfaced LOUD, because a malformed
    /// `/health` from a custodian that DID answer is a real fault to see, not a
    /// silently-`Down` row that hides a broken endpoint.
    pub async fn probe(
        custodian: &dyn ForgeCustodian,
        locator: ForgeLocator,
        trust_scope: TrustLevel,
    ) -> Result<Self, ForgeCustodianError> {
        match custodian.health().await {
            Ok(h) => Ok(Self::from_health(locator, trust_scope, &h)),
            Err(ForgeCustodianError::Unreachable(_)) => Ok(Self::unreachable(locator, trust_scope)),
            Err(api) => Err(api),
        }
    }

    /// Does this endpoint advertise the given capability tag?
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// Is it routable for new work right now (healthy with spare capacity)?
    pub fn is_routable(&self) -> bool {
        matches!(self.health, ForgeHealth::Healthy) && self.capacity > 0
    }

    /// The ADVERTISE policy (Pass 5b): turn a probe result into an optional row to
    /// announce on the grid bus. A node only claims the forge capability when a
    /// custodian actually answered:
    /// - `Ok(health)` ⇒ `Some` — a reachable custodian is advertised at whatever
    ///   health it reported (the fabric re-probes for the live reading; this is a
    ///   discovery snapshot, exactly like `NodeCapability::Compute`'s VRAM).
    /// - `Unreachable` ⇒ `None` — no custodian here means no capability to claim.
    ///   This is NOT a fallback: the node genuinely lacks the capability, so the
    ///   honest move is to not advertise it (claiming a `Down` row you can't serve
    ///   would be the lie).
    /// - `Api` (reached but `/health` is malformed) ⇒ `None` **and logged LOUD** —
    ///   a broken custodian is a real fault to SEE, but forge is optional infra and
    ///   must not block grid bringup, so we name it and decline to advertise.
    pub fn advertise_from_probe(
        locator: ForgeLocator,
        trust_scope: TrustLevel,
        probe: Result<HealthResponse, ForgeCustodianError>,
    ) -> Option<Self> {
        match probe {
            Ok(h) => Some(Self::from_health(locator, trust_scope, &h)),
            Err(ForgeCustodianError::Unreachable(_)) => None,
            Err(ForgeCustodianError::Api(msg)) => {
                eprintln!(
                    "[forge] local custodian answered but /health is broken: {msg} \
                     — not advertising forge capability"
                );
                None
            }
        }
    }

    /// Probe THIS machine's forge custodian and, if it answered, produce the row to
    /// announce. `Owner`-scoped (it's our own node). Returns `None` when no local
    /// custodian is running — the grid then simply does not advertise forge. The
    /// thin I/O glue over [`Self::advertise_from_probe`] (the testable policy).
    pub async fn probe_local() -> Option<Self> {
        let client = ForgeCustodianHttp::from_config();
        let locator = ForgeLocator::Local {
            base_url: custodian_base_url(),
        };
        Self::advertise_from_probe(locator, TrustLevel::Owner, client.health().await)
    }
}

/// True if `endpoint` may accept the gguf-lora job: routable, speaks a contract
/// version the caller understands, advertises gguf-lora, and clears the trust
/// floor. The two forge-specific gates (§5) — contract + capability — plus the
/// hard trust gate, expressed as one pure predicate the scorer calls before
/// ranking on capacity/latency.
pub fn can_accept_gguf_lora(
    endpoint: &ForgeEndpoint,
    client_contract_version: u32,
    trust_floor: TrustLevel,
) -> bool {
    endpoint.is_routable()
        && endpoint.contract_version == client_contract_version
        && endpoint.supports(CAPABILITY_GGUF_LORA)
        && endpoint.trust_scope >= trust_floor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::custodian_client::ForgeCustodian;
    use crate::forge::protocol::{ExportResult, GgufLoraRequest, CONTRACT_VERSION};
    use async_trait::async_trait;

    fn local() -> ForgeLocator {
        ForgeLocator::Local {
            base_url: "http://127.0.0.1:8899".into(),
        }
    }

    // A fake custodian whose health() the test controls, to drive each routable
    // state without standing up the real binary (the integration test covers that).
    struct FakeCustodian(Result<HealthResponse, ForgeCustodianError>);
    #[async_trait]
    impl ForgeCustodian for FakeCustodian {
        async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
            self.0.clone()
        }
        async fn export_gguf_lora(
            &self,
            _req: &GgufLoraRequest,
        ) -> Result<ExportResult, ForgeCustodianError> {
            unreachable!("endpoint probing never exports")
        }
    }

    // what this catches: the health derivation is honest across all three states —
    // a ready custodian with spare slots is Healthy + routable; the same custodian
    // saturated is Busy (transient, not a fault); a live-but-not-ready custodian
    // is Down (can't work). A wrong mapping would route work to a custodian that
    // can't take it (or starve a healthy one).
    #[test]
    fn health_derivation_covers_healthy_busy_down() {
        let healthy = ForgeEndpoint::from_health(
            local(),
            TrustLevel::Owner,
            &HealthResponse::gguf_lora(true, 3, 2),
        );
        assert_eq!(healthy.health, ForgeHealth::Healthy);
        assert_eq!(healthy.capacity, 2);
        assert!(healthy.is_routable());

        let busy = ForgeEndpoint::from_health(
            local(),
            TrustLevel::Owner,
            &HealthResponse::gguf_lora(true, 3, 0),
        );
        assert_eq!(busy.health, ForgeHealth::Busy);
        assert!(!busy.is_routable(), "saturated ⇒ not routable for new work");

        let not_ready = ForgeEndpoint::from_health(
            local(),
            TrustLevel::Owner,
            &HealthResponse::gguf_lora(false, 3, 3),
        );
        assert_eq!(
            not_ready.health,
            ForgeHealth::Down,
            "ready==false ⇒ Down even with free slots (tooling missing)"
        );
    }

    // what this catches: an Unreachable probe yields a Down row (honest sensor
    // reading), but an Api error (custodian answered with a broken handshake) is
    // surfaced LOUD — never silently Down, which would hide a real fault behind a
    // fallback. This is the no-fallback contract at the probe boundary.
    #[tokio::test]
    async fn probe_unreachable_is_down_but_api_error_is_loud() {
        let dead = FakeCustodian(Err(ForgeCustodianError::Unreachable("no listener".into())));
        let row = ForgeEndpoint::probe(&dead, local(), TrustLevel::Trusted)
            .await
            .expect("unreachable is a Down row, not an error");
        assert_eq!(row.health, ForgeHealth::Down);
        assert_eq!(row.capacity, 0);
        assert!(row.capabilities.is_empty());

        let broken = FakeCustodian(Err(ForgeCustodianError::Api("bad /health json".into())));
        let err = ForgeEndpoint::probe(&broken, local(), TrustLevel::Trusted)
            .await
            .expect_err("an Api fault must surface, not become a silent Down row");
        assert!(matches!(err, ForgeCustodianError::Api(_)));
    }

    // what this catches: the dispatch predicate enforces ALL gates — routable,
    // matching contract version, advertised capability, AND the trust floor. A
    // single relaxed gate is how private-data jobs leak past a trust boundary or a
    // version-mismatched custodian gets a body it can't parse.
    #[tokio::test]
    async fn can_accept_enforces_contract_capability_and_trust() {
        let healthy = FakeCustodian(Ok(HealthResponse::gguf_lora(true, 2, 1)));
        let ep = ForgeEndpoint::probe(&healthy, local(), TrustLevel::Trusted)
            .await
            .unwrap();

        // All gates pass: routable, same version, gguf-lora, trust >= Trusted.
        assert!(can_accept_gguf_lora(
            &ep,
            CONTRACT_VERSION,
            TrustLevel::Trusted
        ));
        // Version mismatch ⇒ refused (the handshake gate).
        assert!(!can_accept_gguf_lora(
            &ep,
            CONTRACT_VERSION + 1,
            TrustLevel::Trusted
        ));
        // Trust floor not met (Owner-only job, endpoint only Trusted) ⇒ refused.
        assert!(!can_accept_gguf_lora(
            &ep,
            CONTRACT_VERSION,
            TrustLevel::Owner
        ));
        // Wrong capability ⇒ refused.
        assert!(!ep.supports("train"));
    }

    // what this catches: the advertise policy (5b) is honest about WHEN a node
    // claims forge — a reachable custodian is advertised; an Unreachable one yields
    // NO row (no custodian = no capability, not a fallback Down-row lie); a broken
    // /health yields no row either (declined, not advertised). If Unreachable ever
    // produced Some(Down), every node would advertise forge it can't serve.
    #[test]
    fn advertise_only_when_custodian_answered() {
        let reachable = ForgeEndpoint::advertise_from_probe(
            local(),
            TrustLevel::Owner,
            Ok(HealthResponse::gguf_lora(true, 2, 1)),
        );
        assert!(reachable.is_some(), "a reachable custodian is advertised");
        assert_eq!(reachable.unwrap().health, ForgeHealth::Healthy);

        let absent = ForgeEndpoint::advertise_from_probe(
            local(),
            TrustLevel::Owner,
            Err(ForgeCustodianError::Unreachable("no listener".into())),
        );
        assert!(absent.is_none(), "no custodian ⇒ advertise nothing");

        let broken = ForgeEndpoint::advertise_from_probe(
            local(),
            TrustLevel::Owner,
            Err(ForgeCustodianError::Api("bad json".into())),
        );
        assert!(
            broken.is_none(),
            "a broken custodian is declined, not advertised"
        );
    }

    // what this catches: a ForgeEndpoint round-trips JSON — it crosses the grid bus
    // as an announcement (5b), so a field that failed to (de)serialize would drop
    // silently on the hop. Covers both locator variants.
    #[test]
    fn endpoint_round_trips_both_locators() {
        let remote = ForgeEndpoint {
            locator: ForgeLocator::Node {
                node: PeerId::from_uuid(uuid::Uuid::from_u128(7)),
            },
            capabilities: vec![CAPABILITY_GGUF_LORA.into()],
            contract_version: CONTRACT_VERSION,
            health: ForgeHealth::Healthy,
            capacity: 4,
            trust_scope: TrustLevel::Trusted,
        };
        let back: ForgeEndpoint =
            serde_json::from_value(serde_json::to_value(&remote).unwrap()).unwrap();
        assert_eq!(remote, back);
    }
}
