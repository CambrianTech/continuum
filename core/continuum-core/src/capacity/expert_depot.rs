//! expert_depot — Slices 0–1 of the Grid Expert Share arc (#315,
//! docs/serving/GRID-EXPERT-SHARE.md).
//!
//! The depot is the continuum-core side of the depot/GridFetcher split: the
//! fork stays THIN (one more `ExpertFetcher` that GETs bytes from
//! `GGML_MOE_DEPOT_URL`), and ALL grid intelligence — manifest publication,
//! peer resolution, trust, verification, disk-tier caching — lives here.
//! This module is the outlier-A slice: serve OUR OWN container's expert
//! records over localhost HTTP, plus the resident-bank manifest that later
//! slices publish to peers. Peer resolve (slice 2) plugs in BEHIND the same
//! two routes; the fork never changes again.
//!
//! Contract with the fork's GridFetcher (mirrors `DirContainerFetcher`):
//! - `GET /manifest`                     → [`DepotManifest`] JSON
//! - `GET /expert/{layer}/{expert}?tier=N` → the record's bytes
//!   (`application/octet-stream`), with `x-expert-sha256` carrying the hex
//!   digest of the body — the per-record verify seam slice 2's remote path
//!   already needs, priced in from day one.
//!
//! Miss semantics are the load-bearing choice: a bank this node does not
//! hold (the partial-shard case — a grid node holding 2 of 60 layer banks)
//! and an out-of-range key both answer **404**, which the fork fetcher
//! treats as "fall back to the current source". Geometry violations
//! (truncated bank, identity mismatch) answer **500** — that is a corrupt
//! artifact, an operator defect, never a routine miss. The depot can
//! DEGRADE serving; it can never break it.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::expert_container::{ContainerError, ExpertContainer, TierSpec, RECORD_ALIGN};
use super::expert_ecache::ExpertKey;

/// Depot manifest schema version — the grid advertisement contract.
/// Bumped independently of the CONTAINER manifest version: peers consume
/// this over the wire, the container manifest never leaves the node.
pub const DEPOT_MANIFEST_VERSION: u32 = 1;

/// One resident bank — the grid shard unit. A peer that sees
/// `{layer: 7, tier: 0}` here can GET every expert of layer 7 at tier 0
/// from this depot and nothing else needs to be true about this node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepotBank {
    pub layer: u16,
    pub tier: u16,
    /// On-disk size of the bank file, as advertised. Peers cross-check
    /// `experts_per_layer × tier.record_bytes` before trusting a shard.
    pub bytes: u64,
}

/// What this depot holds and how to read it — served at `GET /manifest`
/// and (slice 0) published on airc via the `depot.manifest` probe class.
/// Only RESIDENT banks are listed: advertising shards we don't hold would
/// turn every peer miss into a 404 storm instead of a clean fallback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepotManifest {
    pub version: u32,
    /// Model identity (from the container manifest) — grid advertisement
    /// key; never parsed for behavior.
    pub model: String,
    pub record_align: u64,
    pub n_layers: u16,
    pub experts_per_layer: u16,
    /// Tier table verbatim from the container — peers size their buffers
    /// (and their cliff arithmetic) from `tiers[t].record_bytes`.
    pub tiers: Vec<TierSpec>,
    /// Resident banks only, in (layer, tier) order.
    pub banks: Vec<DepotBank>,
}

impl DepotManifest {
    /// True when this depot holds the (layer, tier) bank — the routing
    /// predicate slice 2's peer resolve runs against every peer manifest.
    pub fn holds(&self, layer: u16, tier: u16) -> bool {
        self.banks
            .iter()
            .any(|b| b.layer == layer && b.tier == tier)
    }
}

/// A peer depot this node can fetch missing banks FROM — its base URL plus the
/// manifest it published. Injected by the control plane (airc manifest exchange, a
/// later slice); the depot routes a local miss only to peers whose manifest actually
/// HOLDS the bank, so a partial-shard node never storms a peer that also lacks it.
#[derive(Debug, Clone)]
pub struct PeerDepot {
    /// e.g. `http://100.1.2.3:58200` (a peer's depot over Tailscale) — the base the
    /// fork's `GGML_MOE_DEPOT_URL` would point at, one hop out on the grid.
    pub base_url: String,
    /// What that peer advertised it holds — the routing predicate.
    pub manifest: DepotManifest,
}

/// Cap on the cross-grid record cache — bounded so a partial-shard node fetching many
/// banks from peers cannot grow unbounded (disk/RAM is governed). The ecache LFRU
/// integration is the follow-up; correctness (and a hard ceiling) first.
const REMOTE_CACHE_MAX: usize = 512;

/// The local expert depot: one opened container + the derived resident manifest,
/// servable over localhost HTTP — AND (slice 2, #315) able to fetch a bank it does not
/// hold from a grid peer that does. Fetches of resident records run on the blocking pool
/// (positioned file reads) behind a `std::sync::Mutex` never held across an await;
/// cross-grid fetches are async HTTP. This is where two nodes' compute combines on ONE
/// model: node A serves an expert it lacks, streamed from node B that holds it.
pub struct ExpertDepot {
    container: Mutex<ExpertContainer>,
    manifest: DepotManifest,
    /// Peer depots to try on a local miss. Empty = local-only (the Slice-0/1 behavior,
    /// unchanged). The control plane owns membership; the depot owns fetch + verify.
    peers: Mutex<Vec<PeerDepot>>,
    /// Records already pulled across the grid, verified — so a bank served remotely is
    /// not re-fetched every token. Bounded by [`REMOTE_CACHE_MAX`].
    remote_cache: Mutex<std::collections::HashMap<ExpertKey, Vec<u8>>>,
    /// One client for peer fetches. `no_proxy`: a grid peer (Tailscale / localhost) must
    /// never route through an ambient corporate proxy.
    http: reqwest::Client,
}

impl ExpertDepot {
    /// Open the container at `root` and derive the resident manifest by
    /// scanning bank presence. Lazily-opened banks mean this scan is pure
    /// `stat` — no bank IO, no failure on shards we don't hold.
    pub fn open(root: &Path) -> Result<Self, ContainerError> {
        let container = ExpertContainer::open(root)?;
        let cm = container.manifest().clone();
        let tiers = container.tiers().to_vec();
        let mut banks = Vec::new();
        for layer in 0..cm.n_layers {
            for tier in &tiers {
                let path = container.bank_path(layer, tier.id);
                if let Ok(meta) = std::fs::metadata(&path) {
                    banks.push(DepotBank {
                        layer,
                        tier: tier.id,
                        bytes: meta.len(),
                    });
                }
            }
        }
        let manifest = DepotManifest {
            version: DEPOT_MANIFEST_VERSION,
            model: cm.model.clone(),
            record_align: RECORD_ALIGN,
            n_layers: cm.n_layers,
            experts_per_layer: cm.experts_per_layer,
            tiers,
            banks,
        };
        crate::probe!(
            class = "depot.manifest",
            model = %manifest.model,
            n_layers = manifest.n_layers,
            resident_banks = manifest.banks.len(),
            "expert depot opened — resident-bank manifest derived"
        );
        Ok(Self {
            container: Mutex::new(container),
            manifest,
            peers: Mutex::new(Vec::new()),
            remote_cache: Mutex::new(std::collections::HashMap::new()),
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_default(),
        })
    }

    pub fn manifest(&self) -> &DepotManifest {
        &self.manifest
    }

    /// Inject the peer depots the control plane discovered (base URL + published
    /// manifest). Replaces the set — the control plane owns membership; the depot owns
    /// the fetch/verify. Empty restores local-only behavior.
    pub fn set_peers(&self, peers: Vec<PeerDepot>) {
        *self.peers.lock().unwrap_or_else(|p| p.into_inner()) = peers;
    }

    /// A LOCAL miss — try the grid. Find a peer whose manifest HOLDS this bank, GET the
    /// record from its depot, VERIFY the body against the `x-expert-sha256` header, cache
    /// it (bounded), and return the bytes. `None` = no peer holds it, or every candidate
    /// failed to reach/verify → the caller answers 404 and the fork falls back to its
    /// current source (degrade, never break). This IS the combine: node A serves an
    /// expert it does not hold, from node B that does. #315, [[misfit-grid-is-a-distributed-moe]]
    async fn resolve_from_peer(&self, key: ExpertKey) -> Option<Vec<u8>> {
        // Already pulled across the grid this session → don't re-fetch per token.
        if let Some(bytes) = self
            .remote_cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key)
            .cloned()
        {
            return Some(bytes);
        }
        // Only peers that actually advertise this (layer, tier) — never a 404 storm.
        let candidates: Vec<PeerDepot> = self
            .peers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|p| p.manifest.holds(key.layer, key.tier))
            .cloned()
            .collect();
        for peer in candidates {
            let url = format!(
                "{}/expert/{}/{}?tier={}",
                peer.base_url.trim_end_matches('/'),
                key.layer,
                key.expert,
                key.tier
            );
            let resp = match self.http.get(&url).send().await {
                Ok(r) if r.status().is_success() => r,
                Ok(r) => {
                    crate::probe!(
                        class = "depot.peer.miss",
                        layer = key.layer,
                        expert = key.expert,
                        tier = key.tier,
                        peer = %peer.base_url,
                        status = %r.status(),
                        "peer did not serve the bank it advertised — trying the next holder"
                    );
                    continue;
                }
                Err(error) => {
                    crate::probe!(
                        class = "depot.peer.unreachable",
                        peer = %peer.base_url,
                        error = %error,
                        "grid peer depot unreachable — trying the next holder"
                    );
                    continue;
                }
            };
            let advertised = resp
                .headers()
                .get(EXPERT_SHA256_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = match resp.bytes().await {
                Ok(b) => b.to_vec(),
                Err(error) => {
                    crate::probe!(
                        class = "depot.peer.unreachable",
                        peer = %peer.base_url,
                        error = %error,
                        "grid peer stream broke mid-record — trying the next holder"
                    );
                    continue;
                }
            };
            // VERIFY: a peer's bytes are trusted only if they hash to what it advertised.
            // A mismatch is a corrupt or lying peer — skip it, NEVER serve wrong weights.
            let digest = hex_sha256(&body);
            if advertised.as_deref() != Some(digest.as_str()) {
                crate::probe!(
                    class = "depot.peer.corrupt",
                    layer = key.layer,
                    expert = key.expert,
                    tier = key.tier,
                    peer = %peer.base_url,
                    "peer record failed sha256 verify — refused, never served as trusted weights"
                );
                continue;
            }
            {
                let mut cache = self.remote_cache.lock().unwrap_or_else(|p| p.into_inner());
                if cache.len() < REMOTE_CACHE_MAX {
                    cache.insert(key, body.clone());
                }
            }
            crate::probe!(
                class = "depot.peer.served",
                layer = key.layer,
                expert = key.expert,
                tier = key.tier,
                peer = %peer.base_url,
                bytes = body.len(),
                "expert fetched + verified from a grid peer that holds the bank — two nodes combined on one model (#315)"
            );
            return Some(body);
        }
        None
    }

    /// Fetch one resident record (blocking file IO — callers on the async
    /// side go through `spawn_blocking`, which is exactly what the HTTP
    /// handler does).
    pub fn fetch_blocking(&self, key: ExpertKey) -> Result<Vec<u8>, ContainerError> {
        // Poisoning: a panic in another fetch left no partial state we care
        // about (bank reads are stateless positioned reads), so recover the
        // guard rather than wedging the depot forever.
        let mut container = self
            .container
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let record_bytes = container
            .tiers()
            .get(key.tier as usize)
            .map(|t| t.record_bytes)
            .ok_or(ContainerError::TierOutOfRange {
                tier: key.tier,
                n_tiers: container.tiers().len() as u16,
            })?;
        let mut buf = vec![0u8; record_bytes as usize];
        container.fetch(key, &mut buf)?;
        Ok(buf)
    }

    /// Serve this depot on a localhost socket. `port` 0 binds an ephemeral
    /// port; the BOUND port comes back so the caller can hand the fork its
    /// `GGML_MOE_DEPOT_URL`. The server runs as its own tokio task; dropping
    /// the returned handle aborts it (RAII, same as every serving lane).
    pub async fn serve_localhost(
        self: Arc<Self>,
        port: u16,
    ) -> std::io::Result<(u16, tokio::task::JoinHandle<()>)> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let bound = listener.local_addr()?.port();
        let router = Router::new()
            .route("/manifest", get(get_manifest))
            .route("/expert/{layer}/{expert}", get(get_expert))
            .with_state(self.clone());
        crate::probe!(
            class = "depot.listen",
            port = bound,
            model = %self.manifest.model,
            "expert depot serving on localhost"
        );
        let handle = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                // A dead depot degrades serving (fork falls back to its
                // current source) but must never be silent about it.
                crate::probe!(
                    class = "depot.died",
                    error = %error,
                    "expert depot HTTP server exited with error — grid share degraded to local-only"
                );
            }
        });
        Ok((bound, handle))
    }
}

#[derive(Debug, Deserialize)]
struct TierQuery {
    tier: Option<u16>,
}

async fn get_manifest(State(depot): State<Arc<ExpertDepot>>) -> Json<DepotManifest> {
    Json(depot.manifest.clone())
}

async fn get_expert(
    State(depot): State<Arc<ExpertDepot>>,
    UrlPath((layer, expert)): UrlPath<(u16, u16)>,
    Query(query): Query<TierQuery>,
) -> Response {
    let tier = query.tier.unwrap_or(0);
    let key = ExpertKey {
        layer,
        expert,
        tier,
    };
    // Not resident locally → try the GRID (#315 slice 2): fetch this bank from a peer
    // that holds it, verify, serve. Only if no peer holds it is this a clean 404 the
    // fork falls back on. A shard we never held is a miss, never an error.
    if !depot.manifest.holds(layer, tier) {
        if let Some(bytes) = depot.resolve_from_peer(key).await {
            return expert_ok_response(layer, expert, tier, bytes);
        }
        crate::probe!(
            class = "depot.miss",
            layer,
            expert,
            tier,
            "expert not held here and no grid peer holds it — clean miss, fork falls back"
        );
        return StatusCode::NOT_FOUND.into_response();
    }
    let fetched = tokio::task::spawn_blocking({
        let depot = depot.clone();
        move || depot.fetch_blocking(key)
    })
    .await;
    match fetched {
        Ok(Ok(bytes)) => expert_ok_response(layer, expert, tier, bytes),
        Ok(Err(error)) => {
            let status = miss_or_defect(&error);
            crate::probe!(
                class = "depot.miss",
                layer,
                expert,
                tier,
                status = %status,
                error = %error,
                "expert fetch refused"
            );
            (status, error.to_string()).into_response()
        }
        Err(join_error) => {
            crate::probe!(
                class = "depot.miss",
                layer,
                expert,
                tier,
                error = %join_error,
                "expert fetch task died"
            );
            (StatusCode::INTERNAL_SERVER_ERROR, join_error.to_string()).into_response()
        }
    }
}

/// Response header carrying the SHA-256 hex digest of the record body —
/// slice 2's remote-fetch verify reads THIS, so it is part of the contract
/// from the first localhost byte served.
pub const EXPERT_SHA256_HEADER: header::HeaderName =
    header::HeaderName::from_static("x-expert-sha256");

/// The 200 response for one served record — bytes + the body-hashing `x-expert-sha256`
/// header — shared by the local-container path and the cross-grid peer-fetch path so both
/// carry the exact same verify seam the fork reads.
fn expert_ok_response(layer: u16, expert: u16, tier: u16, bytes: Vec<u8>) -> Response {
    let digest = hex_sha256(&bytes);
    crate::probe!(
        class = "depot.serve",
        layer,
        expert,
        tier,
        bytes = bytes.len(),
        "served expert record"
    );
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (EXPERT_SHA256_HEADER, digest),
        ],
        bytes,
    )
        .into_response()
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A missing/out-of-range record is a MISS (404, fork falls back); a
/// geometry or identity violation is a corrupt artifact (500, loud).
fn miss_or_defect(error: &ContainerError) -> StatusCode {
    match error {
        ContainerError::LayerOutOfRange { .. }
        | ContainerError::TierOutOfRange { .. }
        | ContainerError::ExpertOutOfRange { .. }
        | ContainerError::BankIo { .. } => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::super::expert_container::fixtures::{
        assert_v1_record_identity, write_container, write_tiered_container,
    };
    use super::*;

    fn client() -> reqwest::Client {
        // no_proxy: an ambient HTTP(S)_PROXY env var must never route a
        // 127.0.0.1 test through a corporate proxy.
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client")
    }

    #[test]
    fn manifest_lists_only_resident_banks() {
        // what this catches: advertising shards we don't hold — a node with
        // 2 of 3 layer banks must publish exactly those 2, or every peer
        // miss becomes a 404 storm instead of a clean fallback.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 3, 4);
        std::fs::remove_file(dir.path().join("experts-L1.bin")).expect("drop shard");
        let depot = ExpertDepot::open(dir.path()).expect("open");
        let layers: Vec<u16> = depot.manifest().banks.iter().map(|b| b.layer).collect();
        assert_eq!(layers, vec![0, 2], "only resident banks advertised");
        assert!(depot.manifest().holds(0, 0));
        assert!(!depot.manifest().holds(1, 0));
        assert_eq!(
            depot.manifest().banks[0].bytes,
            4 * RECORD_ALIGN,
            "advertised bank size is the on-disk truth peers cross-check"
        );
    }

    #[tokio::test]
    async fn http_serves_expert_bytes_with_verifiable_hash() {
        // what this catches: the GridFetcher seam contract end-to-end —
        // GET /expert/{layer}/{expert} must return THAT record's bytes with
        // a body-matching sha256 header (slice 2's remote verify), and
        // GET /manifest must describe the same container the bytes came from.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 2, 3);
        let depot = Arc::new(ExpertDepot::open(dir.path()).expect("open"));
        let (port, server) = depot.serve_localhost(0).await.expect("serve");

        let manifest: DepotManifest = client()
            .get(format!("http://127.0.0.1:{port}/manifest"))
            .send()
            .await
            .expect("manifest get")
            .json()
            .await
            .expect("manifest json");
        assert_eq!(manifest.model, "test-moe");
        assert_eq!(manifest.banks.len(), 2);

        let response = client()
            .get(format!("http://127.0.0.1:{port}/expert/1/2"))
            .send()
            .await
            .expect("expert get");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let advertised = response
            .headers()
            .get("x-expert-sha256")
            .expect("hash header")
            .to_str()
            .expect("ascii")
            .to_string();
        let body = response.bytes().await.expect("body");
        assert_eq!(body.len() as u64, RECORD_ALIGN);
        assert_v1_record_identity(&body, 1, 2);
        assert_eq!(hex_sha256(&body), advertised, "header must hash the body");
        server.abort();
    }

    #[tokio::test]
    async fn tier_query_selects_the_tiers_bank_and_bytes() {
        // what this catches: the tier axis dropping out of the HTTP seam —
        // ?tier=1 must come back at TIER 1's record size with tier-1 payload,
        // never silently defaulting to sharp.
        let dir = tempfile::tempdir().expect("tempdir");
        write_tiered_container(dir.path(), 1, 2);
        let depot = Arc::new(ExpertDepot::open(dir.path()).expect("open"));
        let (port, server) = depot.serve_localhost(0).await.expect("serve");

        let body = client()
            .get(format!("http://127.0.0.1:{port}/expert/0/1?tier=1"))
            .send()
            .await
            .expect("get")
            .bytes()
            .await
            .expect("body");
        assert_eq!(body.len() as u64, RECORD_ALIGN, "tier 1 record size");
        assert_eq!(body[8], 1u8 ^ 0x5A, "tier-1 payload signature");
        server.abort();
    }

    #[tokio::test]
    async fn miss_is_404_and_corruption_is_500() {
        // what this catches: the degrade-never-break contract — an absent
        // shard or out-of-range key is a 404 the fork falls back on, while a
        // truncated (corrupt) bank is a LOUD 500, never served as zeros.
        // Depot A: L1 dropped, L0 healthy — every miss shape must 404.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 2, 2);
        std::fs::remove_file(dir.path().join("experts-L1.bin")).expect("drop shard");
        let depot = Arc::new(ExpertDepot::open(dir.path()).expect("open"));
        let (port, server) = depot.serve_localhost(0).await.expect("serve");
        let base = format!("http://127.0.0.1:{port}");
        for miss in [
            "/expert/1/0",
            "/expert/0/9",
            "/expert/5/0",
            "/expert/0/0?tier=7",
        ] {
            let status = client()
                .get(format!("{base}{miss}"))
                .send()
                .await
                .expect("get")
                .status();
            assert_eq!(
                status,
                reqwest::StatusCode::NOT_FOUND,
                "{miss} must be a clean miss"
            );
        }
        server.abort();

        // Depot B: truncated L0 — corruption must be a LOUD 500.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        let bank = dir.path().join("experts-L0.bin");
        let full = std::fs::read(&bank).expect("read");
        std::fs::write(&bank, &full[..full.len() - RECORD_ALIGN as usize]).expect("truncate");
        let depot = Arc::new(ExpertDepot::open(dir.path()).expect("open"));
        let (port, server) = depot.serve_localhost(0).await.expect("serve");
        let status = client()
            .get(format!("http://127.0.0.1:{port}/expert/0/0"))
            .send()
            .await
            .expect("get")
            .status();
        assert_eq!(
            status,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "truncated bank is corruption, never a silent miss"
        );
        server.abort();
    }

    #[tokio::test]
    async fn a_node_serves_a_bank_it_lacks_by_fetching_from_a_peer_that_holds_it() {
        // what this catches: THE COMBINE (#315 slice 2) — two nodes' storage combined on ONE
        // model. Node B holds layer 1; node A does NOT. A gets B as a peer, and a request to
        // A for an L1 expert must be served by FETCHING it from B, verified against the
        // sha256 header. This is the whole reason to interlink: A serves compute it could not
        // hold alone. If A ever 404'd an expert a peer holds, the grid cannot combine.
        let dir_b = tempfile::tempdir().expect("tempdir b");
        write_container(dir_b.path(), 2, 3);
        let depot_b = Arc::new(ExpertDepot::open(dir_b.path()).expect("open b"));
        let manifest_b = depot_b.manifest().clone();
        let (port_b, server_b) = depot_b.serve_localhost(0).await.expect("serve b");

        // A is a PARTIAL-SHARD node: holds L0, genuinely lacks L1's bank.
        let dir_a = tempfile::tempdir().expect("tempdir a");
        write_container(dir_a.path(), 2, 3);
        std::fs::remove_file(dir_a.path().join("experts-L1.bin")).expect("drop A's L1");
        let depot_a = Arc::new(ExpertDepot::open(dir_a.path()).expect("open a"));
        assert!(!depot_a.manifest().holds(1, 0), "A genuinely lacks layer 1");
        depot_a.set_peers(vec![PeerDepot {
            base_url: format!("http://127.0.0.1:{port_b}"),
            manifest: manifest_b,
        }]);
        let (port_a, server_a) = depot_a.serve_localhost(0).await.expect("serve a");

        // Ask A for an expert in the bank it does NOT hold — it must combine with B.
        let response = client()
            .get(format!("http://127.0.0.1:{port_a}/expert/1/2"))
            .send()
            .await
            .expect("A expert get");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "A serves layer 1 by fetching it from the peer that holds it — the combine"
        );
        let advertised = response
            .headers()
            .get("x-expert-sha256")
            .expect("hash header")
            .to_str()
            .expect("ascii")
            .to_string();
        let body = response.bytes().await.expect("body");
        assert_eq!(body.len() as u64, RECORD_ALIGN);
        assert_v1_record_identity(&body, 1, 2); // it IS layer-1 expert-2, sourced from B
        assert_eq!(
            hex_sha256(&body),
            advertised,
            "the cross-grid record is verified end to end"
        );
        server_a.abort();
        server_b.abort();
    }

    #[tokio::test]
    async fn a_miss_no_peer_holds_still_404s_never_breaks() {
        // what this catches: degrade-never-break ACROSS the grid. A lacks L1, and its only
        // peer B ALSO lacks L1 — A must answer a clean 404 (the fork falls back), never 500
        // and never hang on a peer that cannot help. The grid can only ever ADD reach.
        let dir_b = tempfile::tempdir().expect("tempdir b");
        write_container(dir_b.path(), 2, 3);
        std::fs::remove_file(dir_b.path().join("experts-L1.bin")).expect("B also lacks L1");
        let depot_b = Arc::new(ExpertDepot::open(dir_b.path()).expect("open b"));
        let manifest_b = depot_b.manifest().clone();
        let (port_b, server_b) = depot_b.serve_localhost(0).await.expect("serve b");

        let dir_a = tempfile::tempdir().expect("tempdir a");
        write_container(dir_a.path(), 2, 3);
        std::fs::remove_file(dir_a.path().join("experts-L1.bin")).expect("A lacks L1");
        let depot_a = Arc::new(ExpertDepot::open(dir_a.path()).expect("open a"));
        depot_a.set_peers(vec![PeerDepot {
            base_url: format!("http://127.0.0.1:{port_b}"),
            manifest: manifest_b,
        }]);
        let (port_a, server_a) = depot_a.serve_localhost(0).await.expect("serve a");

        let status = client()
            .get(format!("http://127.0.0.1:{port_a}/expert/1/0"))
            .send()
            .await
            .expect("get")
            .status();
        assert_eq!(
            status,
            reqwest::StatusCode::NOT_FOUND,
            "no peer holds it → clean miss, the fork falls back; the grid never breaks serving"
        );
        server_a.abort();
        server_b.abort();
    }
}
