//! `forge::grid_custodian` — the **grid-transport** impl of the Contract C
//! [`ForgeCustodian`](super::custodian_client::ForgeCustodian) trait (Pass 6).
//!
//! Pass 2's [`ForgeCustodianHttp`](super::custodian_client::ForgeCustodianHttp) is
//! the LOCAL adapter (custodian on this machine over loopback HTTP). This is the
//! REMOTE adapter — the SAME trait, the SAME [`protocol`](super::protocol) types,
//! routed to a custodian on another grid node. Because `modules/forge.rs` depends
//! only on the trait, a forge lease can cross the mesh with the consumer
//! unchanged: that is the "outlier B" that proves the trait is grid-ready (the
//! methodical-process step 4 — build the maximally-different second impl and
//! confirm the interface fits both without forcing).
//!
//! ## Handles, not bytes
//! Only the request (a checkpoint locator + base id) and the result envelope cross
//! the wire. The gene bytes stay node-local under the remote custodian's
//! `save_directory`; the requester gets a [`GeneHandle`](super::gene_handle) back,
//! never the safetensors. The grid hop carries control, not payload.
//!
//! ## The `GridDispatch` seam
//! Routing a Continuum command to a remote node is `modules::grid`'s job
//! ([`dispatch_to_node`](crate::modules::grid) — connect → send frame → recv
//! frame). Rather than couple this custodian to the whole `GridState`, it depends
//! on a thin [`GridDispatch`] trait: "send this command to the node I target, give
//! me JSON back, with the [`Unreachable`](GridDispatchError::Unreachable) vs
//! [`Remote`](GridDispatchError::Remote) distinction preserved." The production
//! impl (wrapping `Arc<GridState>` + the resolved [`GridNode`] over
//! `dispatch_to_node`) lands with the two-node integration fixture; this slice
//! delivers the custodian + the seam + a fake-dispatch unit proof, so the routing
//! shape is validated end-to-end without a live second node.
//!
//! ## Server side already exists
//! The receiving node serves the lease through the EXISTING `forge/*` commands
//! (`modules/forge.rs`): `forge/health` returns the local custodian's Contract C
//! [`HealthResponse`], and `forge/export` with `format: "gguf-lora"` routes to its
//! local custodian's `export_gguf_lora`. So Pass 6 adds no new server surface
//! beyond `forge/health` — it reuses the one dispatch point.
//!
//! ## Fail loud, the R2 heal distinction preserved across the hop
//! [`GridDispatchError::Unreachable`] (the grid hop itself failed — connect /
//! send / recv / timeout) maps to [`ForgeCustodianError::Unreachable`]: a router
//! may re-route this idempotent job to an equivalent endpoint.
//! [`GridDispatchError::Remote`] (the node was reached but its command failed —
//! including a non-success custodian export, which the server surfaces as a
//! command error) maps to [`ForgeCustodianError::Api`]: the same job fails the
//! same way against that node until it recovers, so this custodian does not
//! silently retry — re-routing to a DIFFERENT node is the scorer's call against
//! the endpoint table, never a hidden fallback here.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::custodian_client::{ForgeCustodian, ForgeCustodianError};
use super::endpoint::{can_accept_gguf_lora, ForgeEndpoint};
use super::protocol::{ExportResult, GgufLoraRequest, HealthResponse, CONTRACT_VERSION};
use crate::modules::grid::node::TrustLevel;

/// The remote forge command names — the SAME strings `modules/forge.rs` matches on
/// (one source so a renamed command can't silently 404 the dispatch).
const CMD_FORGE_HEALTH: &str = "forge/health";
const CMD_FORGE_EXPORT: &str = "forge/export";

/// Typed failure of a grid dispatch, preserving the R2 heal distinction across the
/// hop (see module docs). Distinct from [`ForgeCustodianError`] so the seam stays
/// transport-shaped (it knows nothing about forge); the custodian maps one to the
/// other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridDispatchError {
    /// The remote node could not be reached at all — connect / send / recv /
    /// timeout. The job never ran; a router may re-route it.
    Unreachable(String),
    /// The node was reached but the command returned an error (or an unexpected
    /// frame). The same command will fail the same way against this node.
    Remote(String),
}

/// Route a single Continuum command to ONE pre-bound remote node and return its
/// JSON result. The implementor holds whatever it needs to reach that node (the
/// production impl: `Arc<GridState>` + the resolved [`GridNode`], over
/// `dispatch_to_node`). Kept minimal and forge-agnostic so it is trivially fakeable
/// in tests and reusable by any future remote capability.
#[async_trait]
pub trait GridDispatch: Send + Sync {
    async fn dispatch(&self, command: &str, params: Value) -> Result<Value, GridDispatchError>;
}

/// A forge custodian reached over the grid transport. Targets the node behind
/// `dispatch`; carries the announced [`ForgeEndpoint`] (Pass 5b) for the hard trust
/// + capability gate, and the `trust_floor` the dispatched job demands.
pub struct GridForgeCustodian<D: GridDispatch> {
    dispatch: D,
    /// The announced row for the remote custodian (locator + trust_scope + the
    /// capability/contract/health snapshot the gate reads).
    endpoint: ForgeEndpoint,
    /// The GridTrustAuthPolicy floor THIS job must clear before its checkpoint may
    /// cross to the remote node — the hard gate, enforced again here as
    /// defense-in-depth even though the scorer already gated (a private-data job
    /// must never leak past a trust boundary on a scorer bug).
    trust_floor: TrustLevel,
}

impl<D: GridDispatch> GridForgeCustodian<D> {
    /// Bind a remote custodian: the dispatch reaches the target node, the endpoint
    /// is its announced row, and `trust_floor` is the sensitivity of the work this
    /// custodian will be asked to do.
    pub fn new(dispatch: D, endpoint: ForgeEndpoint, trust_floor: TrustLevel) -> Self {
        Self {
            dispatch,
            endpoint,
            trust_floor,
        }
    }
}

/// Map the transport-shaped dispatch error onto the forge error, preserving the
/// heal-vs-don't-heal distinction (R2).
fn map_dispatch_err(e: GridDispatchError) -> ForgeCustodianError {
    match e {
        GridDispatchError::Unreachable(m) => ForgeCustodianError::Unreachable(m),
        GridDispatchError::Remote(m) => ForgeCustodianError::Api(m),
    }
}

#[async_trait]
impl<D: GridDispatch> ForgeCustodian for GridForgeCustodian<D> {
    async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
        // Read-only handshake — not trust-gated (we only hold this endpoint because
        // a trusted announce gave it to us; confirming its live contract version is
        // the point of the call). The default `ensure_contract` rides on this.
        let v = self
            .dispatch
            .dispatch(CMD_FORGE_HEALTH, json!({}))
            .await
            .map_err(map_dispatch_err)?;
        serde_json::from_value(v)
            .map_err(|e| ForgeCustodianError::Api(format!("{CMD_FORGE_HEALTH}: decode {e}")))
    }

    async fn export_gguf_lora(
        &self,
        req: &GgufLoraRequest,
    ) -> Result<ExportResult, ForgeCustodianError> {
        // Hard trust + capability + contract gate BEFORE the checkpoint crosses the
        // boundary. One pure predicate (the same the scorer uses) — fail loud,
        // naming the floor, never silently dispatch to an endpoint that fails it.
        if !can_accept_gguf_lora(&self.endpoint, CONTRACT_VERSION, self.trust_floor) {
            return Err(ForgeCustodianError::Api(format!(
                "endpoint {:?} refused gguf-lora dispatch — routable/contract/capability/trust gate \
                 not satisfied (job trust_floor={:?}, endpoint trust_scope={:?}, health={:?})",
                self.endpoint.locator,
                self.trust_floor,
                self.endpoint.trust_scope,
                self.endpoint.health,
            )));
        }

        // Map the stateless Contract C request onto the remote `forge/export`
        // gguf-lora params (the one dispatch point — `modules/forge.rs` routes
        // `format: "gguf-lora"` to its LOCAL custodian's `export_gguf_lora`).
        let params = json!({
            "checkpoint": req.checkpoint,
            "save_directory": req.save_directory,
            "format": "gguf-lora",
            "base_model_id": req.base_model_id,
            "outtype": req.outtype,
        });
        let v = self
            .dispatch
            .dispatch(CMD_FORGE_EXPORT, params)
            .await
            .map_err(map_dispatch_err)?;

        // Dispatch success ⇒ the remote command returned Ok ⇒ the export succeeded.
        // (A non-success custodian export becomes a remote command ERROR on the
        // server, surfaced as `Remote` → `Api` above — never a success envelope with
        // success=false.) The remote envelope is
        // `{format, checkpoint, save_directory, message, details}`; project the
        // forge-relevant fields back onto the contract `ExportResult`.
        Ok(ExportResult {
            success: true,
            message: v
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            details: v.get("details").cloned().unwrap_or(Value::Null),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::endpoint::{ForgeHealth, ForgeLocator};
    use crate::forge::protocol::CAPABILITY_GGUF_LORA;
    use std::sync::Mutex;

    /// Records the (command, params) the custodian dispatched and returns a scripted
    /// result — so we assert the routing shape (which command, which params) and the
    /// error mapping without a live second node. Stands in for the production
    /// `GridState`-backed dispatch.
    struct FakeGridDispatch {
        calls: Mutex<Vec<(String, Value)>>,
        result: Result<Value, GridDispatchError>,
    }
    impl FakeGridDispatch {
        fn returning(result: Result<Value, GridDispatchError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                result,
            }
        }
    }
    #[async_trait]
    impl GridDispatch for FakeGridDispatch {
        async fn dispatch(&self, command: &str, params: Value) -> Result<Value, GridDispatchError> {
            self.calls
                .lock()
                .unwrap()
                .push((command.to_string(), params));
            self.result.clone()
        }
    }

    /// A routable, gguf-lora, current-contract endpoint at the given trust scope —
    /// every gate passes EXCEPT whatever a test deliberately mismatches.
    fn routable_endpoint(trust_scope: TrustLevel) -> ForgeEndpoint {
        ForgeEndpoint {
            locator: ForgeLocator::Node {
                node: crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(42)),
            },
            capabilities: vec![CAPABILITY_GGUF_LORA.to_string()],
            contract_version: CONTRACT_VERSION,
            health: ForgeHealth::Healthy,
            capacity: 2,
            trust_scope,
        }
    }

    fn sample_request() -> GgufLoraRequest {
        GgufLoraRequest {
            checkpoint: "/runs/coder-4b".into(),
            save_directory: "/genes/coder-4b".into(),
            base_model_id: "continuum-ai/qwen3-4b-GGUF".into(),
            outtype: "f16".into(),
        }
    }

    // what this catches: health() routes to the `forge/health` command and decodes
    // the remote HealthResponse — the handshake the default `ensure_contract` rides
    // on. A wrong command name would hit the wrong (or no) remote handler; a decode
    // miss would break the version check.
    #[tokio::test]
    async fn health_routes_to_forge_health_and_decodes() {
        let health_json = serde_json::to_value(HealthResponse::gguf_lora(true, 3, 2)).unwrap();
        let dispatch = FakeGridDispatch::returning(Ok(health_json));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Trusted),
            TrustLevel::Trusted,
        );

        let h = cust.health().await.expect("decodes the remote health");
        assert_eq!(h.contract_version, CONTRACT_VERSION);
        assert_eq!(h.slots_available, 2);

        let calls = cust.dispatch.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, CMD_FORGE_HEALTH, "must route to forge/health");
    }

    // what this catches: ensure_contract (the default trait method) works over the
    // grid seam too — a version-mismatched remote custodian is refused at the
    // handshake, BEFORE any checkpoint is dispatched. Proves the trait's default
    // logic is transport-agnostic.
    #[tokio::test]
    async fn ensure_contract_refuses_version_mismatch_over_grid() {
        let ahead = HealthResponse {
            contract_version: CONTRACT_VERSION + 1,
            ..HealthResponse::ok_gguf_lora()
        };
        let dispatch = FakeGridDispatch::returning(Ok(serde_json::to_value(ahead).unwrap()));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Trusted),
            TrustLevel::Trusted,
        );

        let err = cust
            .ensure_contract()
            .await
            .expect_err("version drift must refuse");
        match err {
            ForgeCustodianError::Api(m) => assert!(m.contains("version mismatch"), "got: {m}"),
            other => panic!("expected Api mismatch, got {other:?}"),
        }
    }

    // what this catches: export routes to `forge/export` carrying the gguf-lora
    // format tag and the stateless Contract C fields (checkpoint named in the body,
    // base + outtype threaded) — the exact params the remote `modules/forge.rs`
    // dispatch needs to reach its local custodian. A dropped/renamed param is how a
    // remote forge silently produces nothing.
    #[tokio::test]
    async fn export_routes_to_forge_export_with_gguf_lora_params() {
        let envelope = json!({
            "format": "gguf-lora",
            "checkpoint": "/runs/coder-4b",
            "save_directory": "/genes/coder-4b",
            "message": "converted 196 tensors",
            "details": {"tensors": 196},
        });
        let dispatch = FakeGridDispatch::returning(Ok(envelope));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Trusted),
            TrustLevel::Trusted,
        );

        let res = cust
            .export_gguf_lora(&sample_request())
            .await
            .expect("dispatch ok");
        assert!(res.success, "a successful dispatch ⇒ a successful export");
        assert_eq!(res.message, "converted 196 tensors");
        assert_eq!(res.details["tensors"], 196);

        let calls = cust.dispatch.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, CMD_FORGE_EXPORT, "must route to forge/export");
        let p = &calls[0].1;
        assert_eq!(
            p["format"], "gguf-lora",
            "the format tag routes to the custodian server-side"
        );
        assert_eq!(
            p["checkpoint"], "/runs/coder-4b",
            "checkpoint named in body (stateless)"
        );
        assert_eq!(p["base_model_id"], "continuum-ai/qwen3-4b-GGUF");
        assert_eq!(p["outtype"], "f16");
    }

    // what this catches: the HARD trust gate refuses an Owner-only job dispatched to
    // a merely-Trusted endpoint — and the custodian is NEVER contacted (no checkpoint
    // crosses the boundary). This is the defense-in-depth gate; a regression here
    // leaks private-data jobs across a trust boundary on any scorer bug.
    #[tokio::test]
    async fn export_trust_gate_refuses_below_floor_without_dispatching() {
        // endpoint sits at Trusted; the job demands Owner.
        let dispatch = FakeGridDispatch::returning(Ok(json!({})));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Trusted),
            TrustLevel::Owner,
        );

        let err = cust
            .export_gguf_lora(&sample_request())
            .await
            .expect_err("trust floor not met");
        match err {
            ForgeCustodianError::Api(m) => assert!(m.contains("trust"), "got: {m}"),
            other => panic!("expected Api gate refusal, got {other:?}"),
        }
        assert!(
            cust.dispatch.calls.lock().unwrap().is_empty(),
            "the gate must refuse BEFORE any dispatch — no checkpoint crosses"
        );
    }

    // what this catches: a Down (unreachable/not-ready) endpoint is refused by the
    // routable gate before dispatch — we never send work to a custodian the snapshot
    // says cannot take it.
    #[tokio::test]
    async fn export_refuses_unroutable_endpoint() {
        let mut ep = routable_endpoint(TrustLevel::Owner);
        ep.health = ForgeHealth::Down;
        ep.capacity = 0;
        let dispatch = FakeGridDispatch::returning(Ok(json!({})));
        let cust = GridForgeCustodian::new(dispatch, ep, TrustLevel::Owner);

        cust.export_gguf_lora(&sample_request())
            .await
            .expect_err("Down endpoint refused");
        assert!(
            cust.dispatch.calls.lock().unwrap().is_empty(),
            "no dispatch to a Down endpoint"
        );
    }

    // what this catches: an UNREACHABLE grid hop maps to ForgeCustodianError::Unreachable
    // — the heal-able class (R2) a router uses to re-route the idempotent job. If this
    // collapsed to Api, the fabric would give up instead of trying another node.
    #[tokio::test]
    async fn unreachable_dispatch_maps_to_unreachable() {
        let dispatch = FakeGridDispatch::returning(Err(GridDispatchError::Unreachable(
            "connect refused".into(),
        )));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Owner),
            TrustLevel::Owner,
        );

        let err = cust
            .export_gguf_lora(&sample_request())
            .await
            .expect_err("hop failed");
        assert!(
            matches!(err, ForgeCustodianError::Unreachable(_)),
            "grid-unreachable must stay heal-able, got: {err:?}"
        );
    }

    // what this catches: a REMOTE command failure (node reached, its custodian export
    // failed) maps to ForgeCustodianError::Api — the don't-auto-heal class. The job
    // fails the same way against that node, so this custodian surfaces it loud rather
    // than silently retrying; re-routing is the scorer's explicit call.
    #[tokio::test]
    async fn remote_command_failure_maps_to_api() {
        let dispatch = FakeGridDispatch::returning(Err(GridDispatchError::Remote(
            "Remote command failed: custodian export (gguf-lora) failed: convert exited 1".into(),
        )));
        let cust = GridForgeCustodian::new(
            dispatch,
            routable_endpoint(TrustLevel::Owner),
            TrustLevel::Owner,
        );

        let err = cust
            .export_gguf_lora(&sample_request())
            .await
            .expect_err("remote failed");
        match err {
            ForgeCustodianError::Api(m) => assert!(m.contains("custodian export"), "got: {m}"),
            other => panic!("expected Api (don't-heal), got {other:?}"),
        }
    }
}
