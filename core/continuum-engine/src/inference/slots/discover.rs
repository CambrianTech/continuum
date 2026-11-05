//! What pool the running engine at a root has, and whether its ledger is this very process's: the
//! directory's question, asked by every caller that needs the pool before any request has run.
//!
//! The OpenAI adapter owned it, so only a request discovered a pool. On the M5 2026-11-04 14:56Z a
//! core-only restart adopted the engine, pool growth asked to replace it 5 s later before any
//! request, and the one growth relaunch was refused ("adopted checkpoint requires a known pool"):
//! every resident's window stayed ~24k and Benchy's view turned over on 5 of 10 acts.

use super::*;

/// Why discovery came back without a pool (card 394017ed, f3ba3b56 item 5). A request refuses
/// generation on a miss rather than letting the engine select an unowned slot; an adopted
/// checkpoint refuses as [`AdoptedCheckpointRefusal::UnknownPool`] carrying it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolDiscoveryMiss {
    /// An earlier probe proved `/props` absent; affinity stays off for this server.
    #[error("props endpoint was proven absent earlier")]
    LatchedUnsupported,
    /// `/props` answered a status that proves the endpoint does not exist (latched now).
    #[error("props endpoint absent (status {status})")]
    EndpointAbsent { status: u16 },
    /// The request to `/props` failed in transport (engine restarting, socket refused); its
    /// detail is display-only.
    #[error("props request failed: {error}")]
    Transport { error: String },
    /// `/props` did not answer within the caller's bound (a lifecycle pre-flight never waits on
    /// a wedged engine).
    #[error("props did not answer within the probe bound")]
    TimedOut,
    /// `/props` answered a transient status, such as 503 while the model loads.
    #[error("props not ready (status {status})")]
    NotReady { status: u16 },
    /// `/props` answered 200 with a body that did not parse.
    #[error("props body did not parse")]
    BodyUnparsed,
    /// The body carried no `total_slots`.
    #[error("props carried no slot geometry")]
    NoGeometry,
    /// The engine was replaced while the probe was in flight.
    #[error("engine replaced during discovery")]
    Superseded,
    /// The pool already installed for this server disagrees with what `/props` reports.
    #[error("installed pool has {installed} slots, props reports {props}")]
    GeometryMismatch { installed: u32, props: u32 },
    /// The page-admission negotiation refused this pool.
    #[error("page admission refused this pool")]
    PageAdmissionRefused,
}

impl PoolDiscoveryMiss {
    /// Probe field value for the cause.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::LatchedUnsupported => "latched_unsupported",
            Self::EndpointAbsent { .. } => "endpoint_absent",
            Self::Transport { .. } => "transport",
            Self::TimedOut => "timed_out",
            Self::NotReady { .. } => "not_ready",
            Self::BodyUnparsed => "body_unparsed",
            Self::NoGeometry => "no_geometry",
            Self::Superseded => "superseded",
            Self::GeometryMismatch { .. } => "geometry_mismatch",
            Self::PageAdmissionRefused => "page_admission_refused",
        }
    }

    /// Say the miss once, where it is decided. A latched server was announced when it
    /// latched (`EndpointAbsent`), so it is not repeated on every request after.
    pub(crate) fn probe(&self, root: &str) {
        let detail = match self {
            Self::EndpointAbsent { status } | Self::NotReady { status } => status.to_string(),
            Self::Transport { error } => error.clone(),
            Self::GeometryMismatch { installed, props } => format!("installed {installed} slots, props {props}"),
            _ => String::new(),
        };
        match self {
            Self::LatchedUnsupported => {}
            Self::EndpointAbsent { .. } => continuum_base::probe!(
                class = "inference.slot_affinity.unsupported",
                root = root,
                cause = self.as_str(),
                detail = %detail,
                "props endpoint absent — generation refused; this server cannot supply slot admission"
            ),
            _ => continuum_base::probe!(
                class = "inference.slot_affinity.deferred",
                root = root,
                cause = self.as_str(),
                detail = %detail,
                "no slot pool for this request — generation refused; re-probed on the next request"
            ),
        }
    }
}

/// Served PER-SLOT context window by server root, captured from the same
/// `/props` probe that discovers slots (`default_generation_settings.n_ctx` —
/// llama-server reports the per-slot share there, already divided by
/// `--parallel`). This is the ground truth the registry row can only claim:
/// a consumer budgeting against the model's trained window while the server
/// slices `-c` across N slots silently overshoots (#139). Keyed per SERVER
/// like the slot directory, shared across adapter instances.
pub(crate) fn served_ctx_by_root() -> &'static dashmap::DashMap<String, u32> {
    static CTX: std::sync::OnceLock<dashmap::DashMap<String, u32>> = std::sync::OnceLock::new();
    CTX.get_or_init(dashmap::DashMap::new)
}

/// Does this `/props` status PROVE the endpoint does not exist — the only verdict
/// allowed to latch the slot directory Unsupported for the process's life? 404/501
/// are the server saying "no such surface"; everything else (above all the 503 of a
/// model still loading) is a statement about NOW, and a permanent conclusion drawn
/// from a transient state is the [[unknown-is-not-a-quantity]] error with a cache
/// bolted on.
pub(crate) fn props_status_proves_endpoint_absent(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::NOT_IMPLEMENTED
}

impl SlotDirectory {
    /// The KV slot pool of the engine at `root`: probe `/props` once, install via
    /// [`Self::ensure_pool`], adopt the ledger of a running engine this core did not launch,
    /// and negotiate page admission. DISCOVERY only: the lease + pin + KV paging live in
    /// [`crate::inference::turn_admission::admit_turn`], which takes this pool.
    pub(crate) async fn discover_pool(
        &self,
        client: &reqwest::Client,
        root: &str,
    ) -> Result<Arc<KvSlotPool>, PoolDiscoveryMiss> {
        use PoolDiscoveryMiss as Miss;
        // Fast path: this server's state already known (process-global directory —
        // every caller talking to the same server shares ONE assignment).
        let existing_pool = match self.get(root) {
            Some(Some(pool)) if self.page_admission_known(root) => return Ok(pool),
            Some(Some(pool)) => Some(pool),
            Some(None) => return Err(Miss::LatchedUnsupported),
            None => None, // never probed — probe below
        };
        let discovery_generation = self.page_discovery_generation(root);
        // Probe /props once. Lock is NOT held across the await.
        let url = format!("{}/props", root.trim_end_matches('/'));
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(Miss::Transport { error: e.to_string() }),
        };
        if !resp.status().is_success() {
            let status = resp.status();
            // ONLY "this endpoint does not exist" may latch Unsupported. Any other
            // status — above all the 503 a llama-server answers while the model is
            // still LOADING — is transient, and latching on it killed slot affinity
            // on effectively EVERY boot: personas start deliberating before the lane
            // is warm (`inference.lane_relaunch_retry` ×93, reason=503_loading, same
            // ledger), so the first probe raced the load window, latched Unsupported
            // for the process's life, and prefix-similarity slot theft quietly
            // replaced pinning — measured as `cached: 0` mid-conversation on
            // 2026-08-21. A verdict about what a server IS must never be reached
            // while the server is mid-transition (#442's rule, one layer down).
            if props_status_proves_endpoint_absent(status) {
                self.latch_unsupported(root);
                return Err(Miss::EndpointAbsent { status: status.as_u16() });
            }
            return Err(Miss::NotReady { status: status.as_u16() });
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            // A malformed body from a live server is also not proof of absence —
            // mid-load llama-server can answer partial/HTML bodies. Defer, don't latch.
            Err(_) => return Err(Miss::BodyUnparsed),
        };
        // Capture the served PER-SLOT window while we hold the props body —
        // the ONE authoritative source for what a request can actually carry
        // (#139). Recorded even for single-slot servers (the window truth is
        // independent of whether affinity is useful).
        if let Some(n_ctx) = body
            .pointer("/default_generation_settings/n_ctx")
            .and_then(|v| v.as_u64())
        {
            served_ctx_by_root().insert(root.to_owned(), n_ctx as u32);
        }
        // Missing geometry is not proof that activity paging is unsupported.
        let n_slots = match body.get("total_slots").and_then(|v| v.as_u64()) {
            Some(n) if n > 0 => n as u32,
            _ => return Err(Miss::NoGeometry),
        };
        if self.page_discovery_generation(root) != discovery_generation {
            return Err(Miss::Superseded); // props belongs to the superseded process, not its replacement
        }
        // The directory arbitrates the probe race: only the first writer installs
        // the pool, once per SERVER, not once per caller.
        let pool = match &existing_pool {
            Some(pool) => Arc::clone(pool),
            None => self.ensure_pool(root, n_slots),
        };
        if pool.n_slots() != n_slots {
            return Err(Miss::GeometryMismatch { installed: pool.n_slots(), props: n_slots }); // they must agree
        }
        // A running engine this core ADOPTED (a deploy kept it) has no launch contract here,
        // so its confirmed saves come from the ledger it left, when that ledger names this
        // very process. A spawned engine already attached its own at readiness;
        // its first capability probe must not pretend it adopted another ledger.
        let adoption = if existing_pool.is_some() { Err("pool_already_installed") } else { match crate::inference::lane_registry::live_lane() {
            None => Err("no_live_lane_record"),
            Some(lane) if lane.root_url() != root.trim_end_matches('/') => Err("live_lane_is_another_root"),
            Some(lane) if crate::inference::engine_residency::process_start_s(lane.pid) != Some(lane.started_s) => {
                Err("live_lane_is_another_process")
            }
            Some(lane) => {
                // The live lane IS this process (pid and start time just verified): name it on the
                // pool, with or without a page dir, so every admission on it is bound to it.
                pool.mark_incarnation((lane.pid, lane.started_s));
                match lane.page_dir.as_deref() {
                None => Err("live_lane_has_no_page_dir"),
                Some(page_dir) => {
                    // the adopted engine's page dir is this core's to trim and measure too
                    // (a spawn records it at launch; an adoption never did)
                    note_page_dir(page_dir);
                    adopt_engine_ledger(&pool, page_dir, lane.pid, lane.started_s)
                        .map_err(AdoptRefusal::as_str)
                }
            }},
        }};
        continuum_base::probe!(
            class = "inference.kv_page.ledger_adopted",
            adopted = adoption.is_ok(),
            loaded = adoption.unwrap_or(0) as u64, // probe field: 0 when nothing was adopted, `why` names the reason
            why = adoption.err().unwrap_or("adopted"), // probe field: the refusal, or "adopted" when the ledger was the engine's
            "an adopted engine's confirmed-save ledger: its saved pages are restorable only when the ledger names this same process"
        );
        if !self.negotiate_page_admission(root, &pool, discovery_generation, body.get("slot_save_size_version")).await {
            return Err(Miss::PageAdmissionRefused);
        }
        tracing::info!(
            n_slots,
            "slot affinity enabled — activities lease llama-server slots (props-discovered)"
        );
        Ok(pool)
    }

    /// An adopted engine's checkpoint pool ([`EndpointSlots::adopted_checkpoint_pool`]), discovered
    /// first when the endpoint was never probed: a replacement decided before any request (pool
    /// growth 5 s after adoption, the M5 2026-11-04 14:56Z) checkpoints the pool the running engine
    /// has instead of refusing. A discovery miss is the refusal's typed source.
    pub(crate) async fn adopted_checkpoint_pool(
        &self,
        client: &reqwest::Client,
        root: &str,
        incarnation: crate::inference::engine_residency::EngineIncarnation,
    ) -> Result<Arc<KvSlotPool>, AdoptedCheckpointRefusal> {
        let endpoint = self.endpoint(root);
        match endpoint.adopted_checkpoint_pool(incarnation) {
            Err(AdoptedCheckpointRefusal::Unprobed) => {
                self.discover_pool(client, root).await.map_err(AdoptedCheckpointRefusal::UnknownPool)?;
                endpoint.adopted_checkpoint_pool(incarnation)
            }
            checked => checked,
        }
    }
}
