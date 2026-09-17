//! THE FLOOR FETCHER — a node with no serveable coder model pulls the ladder floor,
//! instead of stranding on whatever junk is on disk.
//!
//! Joel, 2026-09-17: "add the 1.5B to the downloads manifest, find it, the 0.5 is
//! useless" — and the close on deleting the 0.5B. #4154 put the 1.5B
//! (`bartowski/Qwen2.5-Coder-1.5B-Instruct-GGUF`) in the coder ladder, and
//! `provision_model` can download it, but NOTHING ever called it: at boot the core only
//! REPORTS the artifact cache, and when no model is serveable the serving daemon just
//! goes `degraded` and sits. So a weak node that had its 0.5B removed fell back to an
//! on-disk 4B whose window could not hold a turn (the Intel tier, 2026-09-17, down).
//!
//! This module is the missing wire. It watches the serving snapshot; when serving reports
//! NO SERVEABLE MODEL ([`reason_is_no_candidate`]) for a debounce of ticks, it climbs the
//! coder ladder top-down via [`provision_model`] (idempotent — a present file is a
//! cache-hit) and downloads the largest rung that fits, which on a tiny box is the 1.5B
//! floor. The fetch runs OFF the tick in a spawned task (models are GB, minutes) with an
//! in-flight guard; once a model lands the serving daemon adopts it on its next reconcile,
//! no restart.
//!
//! The host's own rate-limit headers are honored through `provisioning::rate_limit` (the
//! one primitive): a `CatalogError::RateLimited` carries HF's `Retry-After`, the module
//! records it, and [`should_fetch_floor`] refuses to hit the network again until it
//! elapses — never a retry loop that earns a ban (Joel: "they give these for your own
//! good"). Between attempts a cooldown keeps a genuinely-cannot-host node (even the 1.5B
//! refused) from hammering HF every tick — it stays degraded, which is the honest state
//! (a weak node hosts orchestration, not a coder — [[weak-nodes-host-orchestration-phenotypes]]).

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::watch;

use crate::inference::llama_server::ServingSnapshot;
use crate::provisioning::model_catalog::{ModelFamily, PowerMode, ProvisionModelError};
use crate::provisioning::rate_limit::RateLimitTracker;
use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;

/// How often the module checks whether the node has a serveable model.
pub const FLOOR_FETCH_TICK: Duration = Duration::from_secs(120);
/// The floor-serving state must persist this many consecutive ticks before a fetch — a
/// boot transient (the daemon momentarily degraded while the first plan settles) is not
/// a missing model.
pub const NO_CANDIDATE_DEBOUNCE: u32 = 2;
/// At most one fetch ATTEMPT per this long — so a node that genuinely cannot host even the
/// 1.5B (fetched, still refused) does not hit HF every tick. The rate-limit tracker is the
/// host-driven bound ON TOP of this steady floor.
pub const FLOOR_FETCH_COOLDOWN: Duration = Duration::from_secs(15 * 60);

/// Does a serving `degraded_reason` mean "this node has no serveable model"? Keyed on the
/// substrate sentences `no_candidate_reason` emits (serving_daemon.rs) — an unfit or empty
/// disk, never a transient. A reason we do not recognise is NOT treated as no-candidate
/// (we do not fetch on an unknown degrade).
pub fn reason_is_no_candidate(reason: &str) -> bool {
    reason.contains("no servable model") || reason.contains("no candidate to host")
}

/// The pure gate: fetch only when the no-candidate state has held the debounce, no fetch
/// is already running, the host has not asked us to wait, and the steady cooldown has
/// elapsed.
pub fn should_fetch_floor(
    no_candidate_streak: u32,
    in_flight: bool,
    rate_limit_waiting: bool,
    cooldown_elapsed: bool,
) -> bool {
    no_candidate_streak >= NO_CANDIDATE_DEBOUNCE && !in_flight && !rate_limit_waiting && cooldown_elapsed
}

/// Whether the live snapshot says the node has no serveable model.
fn snapshot_has_no_candidate(snap: &ServingSnapshot) -> bool {
    !snap.ready
        && snap
            .degraded_reason
            .as_deref()
            .map(reason_is_no_candidate)
            .unwrap_or(false)
}

pub struct FloorFetchModule {
    serving_rx: watch::Receiver<ServingSnapshot>,
    system: Arc<SystemResourceMonitor>,
    client: reqwest::Client,
    tracker: Arc<parking_lot::Mutex<RateLimitTracker>>,
    in_flight: Arc<AtomicBool>,
    last_attempt_ms: Arc<AtomicU64>,
    streak: AtomicU32,
}

impl FloorFetchModule {
    pub fn new(
        serving_rx: watch::Receiver<ServingSnapshot>,
        system: Arc<SystemResourceMonitor>,
    ) -> Self {
        Self {
            serving_rx,
            system,
            client: reqwest::Client::new(),
            tracker: Arc::new(parking_lot::Mutex::new(RateLimitTracker::default())),
            in_flight: Arc::new(AtomicBool::new(false)),
            last_attempt_ms: Arc::new(AtomicU64::new(0)),
            streak: AtomicU32::new(0),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[async_trait]
impl ServiceModule for FloorFetchModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "floor_fetch",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(FLOOR_FETCH_TICK),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        let snap = self.serving_rx.borrow().clone();
        let no_candidate = snapshot_has_no_candidate(&snap);
        let streak = if no_candidate {
            self.streak.fetch_add(1, Ordering::Relaxed).saturating_add(1)
        } else {
            self.streak.store(0, Ordering::Relaxed);
            0
        };
        if !no_candidate {
            return Ok(()); // a serveable model is up (or degraded for another reason) — nothing to do
        }

        let now = Self::now_ms();
        let wait_ms = self.tracker.lock().wait_ms(now);
        let cooldown_elapsed =
            now.saturating_sub(self.last_attempt_ms.load(Ordering::Relaxed)) >= FLOOR_FETCH_COOLDOWN.as_millis() as u64;
        let in_flight = self.in_flight.load(Ordering::Relaxed);

        if !should_fetch_floor(streak, in_flight, wait_ms > 0, cooldown_elapsed) {
            crate::probe!(
                class = "provisioning.floor_fetch.holding",
                streak = streak as u64,
                in_flight,
                rate_limit_wait_ms = wait_ms,
                cooldown_elapsed,
                reason = snap.degraded_reason.as_deref().unwrap_or(""),
                "node has no serveable model but a fetch is not warranted this tick"
            );
            return Ok(());
        }

        // Fire the fetch OFF the tick — it is GB and minutes. The in-flight flag is the
        // one-at-a-time guard; the spawned task clears it in a drop guard.
        self.in_flight.store(true, Ordering::Relaxed);
        self.last_attempt_ms.store(now, Ordering::Relaxed);
        let total_memory = self.system.memory().total_bytes;
        let client = self.client.clone();
        let tracker = self.tracker.clone();
        let in_flight = self.in_flight.clone();

        tokio::spawn(async move {
            struct Clear(Arc<AtomicBool>);
            impl Drop for Clear {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Relaxed);
                }
            }
            let _clear = Clear(in_flight);
            fetch_floor(&client, total_memory, &tracker).await;
        });
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("floor-fetch has no command surface; '{command}' is unknown"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Climb the coder ladder top-down, downloading the largest rung that fits (the 1.5B floor
/// on a tiny box), honoring the host's rate-limit headers. `provision_model` is idempotent,
/// so a present file is a cache-hit. Standalone (not a method) so the spawned task owns no
/// `&self`.
async fn fetch_floor(
    client: &reqwest::Client,
    total_memory: u64,
    tracker: &Arc<parking_lot::Mutex<RateLimitTracker>>,
) {
    let Some(dest_dir) = dirs::home_dir().map(|h| h.join(".continuum").join("genome").join("models")) else {
        crate::probe!(class = "provisioning.floor_fetch.no_home", "no home dir — cannot place a fetched model");
        return;
    };
    let downloader = crate::provisioning::Downloader::new(client.clone());
    let progress = crate::provisioning::downloader::NoopProgress;
    let family = ModelFamily::coder();

    crate::probe!(
        class = "provisioning.floor_fetch.started",
        family = family.name,
        floor = family.ladder.first().copied().unwrap_or(""),
        total_gb = total_memory / 1_000_000_000,
        "no serveable model — climbing the coder ladder for the fitting floor"
    );

    // Largest first (the ladder is ascending); the first that FITS wins. `NoneFit` = too
    // big for this box, try the next-smaller. `RateLimited` = the host asked us to wait,
    // so stop and honor it (the next tick re-checks after the recorded wait).
    for repo in family.ladder.iter().rev() {
        match crate::provisioning::model_catalog::provision_model(
            client,
            &downloader,
            repo,
            total_memory,
            PowerMode::Comfort,
            &dest_dir,
            &progress,
        )
        .await
        {
            Ok(path) => {
                crate::probe!(
                    class = "provisioning.floor_fetch.placed",
                    repo = *repo,
                    path = %path.display(),
                    "floor model placed — serving adopts it on the next reconcile"
                );
                return;
            }
            Err(ProvisionModelError::Catalog(
                crate::provisioning::model_catalog::CatalogError::RateLimited { repo: r, wait_ms },
            )) => {
                // Honor the host's own signal — record it so `should_fetch_floor` sits out
                // the window and we never earn a ban.
                tracker
                    .lock()
                    .observe_wait_ms(wait_ms, FloorFetchModule::now_ms());
                crate::probe!(
                    class = "provisioning.floor_fetch.rate_limited",
                    repo = r.as_str(),
                    wait_ms,
                    "host rate-limited the fetch — honoring its Retry-After, will retry after it elapses"
                );
                return;
            }
            Err(ProvisionModelError::Catalog(
                crate::provisioning::model_catalog::CatalogError::NoneFit { .. },
            )) => {
                continue; // too big for this box — try the next-smaller rung
            }
            Err(e) => {
                crate::probe!(
                    class = "provisioning.floor_fetch.failed",
                    repo = *repo,
                    error = %e,
                    "a ladder rung failed to provision — trying the next"
                );
                continue;
            }
        }
    }
    crate::probe!(
        class = "provisioning.floor_fetch.none_fit",
        family = family.name,
        total_gb = total_memory / 1_000_000_000,
        "no coder rung fits this box — it cannot host a coder (orchestration tier); staying degraded"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Joel 2026-09-17, the Intel tier down): the module fetches only
    // when the node has held a no-serveable-model state past the debounce, nothing is
    // already fetching, the host has not asked us to wait, and the steady cooldown has
    // elapsed — so a boot transient, an in-flight fetch, an honored Retry-After, or a
    // just-attempted fetch all correctly hold.
    #[test]
    fn the_gate_fetches_only_when_the_node_truly_needs_it_and_the_host_permits() {
        assert!(should_fetch_floor(NO_CANDIDATE_DEBOUNCE, false, false, true));
        assert!(!should_fetch_floor(NO_CANDIDATE_DEBOUNCE - 1, false, false, true), "a boot transient waits out the debounce");
        assert!(!should_fetch_floor(NO_CANDIDATE_DEBOUNCE, true, false, true), "one fetch at a time");
        assert!(!should_fetch_floor(NO_CANDIDATE_DEBOUNCE, false, true, true), "an honored Retry-After holds");
        assert!(!should_fetch_floor(NO_CANDIDATE_DEBOUNCE, false, false, false), "the steady cooldown holds between attempts");
    }

    // what this catches: the fetch trigger is keyed on the substrate sentences the serving
    // daemon actually emits, and an unrelated degrade (or a healthy lane) never fetches.
    #[test]
    fn only_a_real_no_candidate_reason_triggers() {
        assert!(reason_is_no_candidate("no servable model on disk — the planner found no local weights"));
        assert!(reason_is_no_candidate("no candidate to host citizens — 1 model(s) on disk were refused: qwen (window)"));
        assert!(!reason_is_no_candidate("the lane returned an EMPTY completion"));
        assert!(!reason_is_no_candidate("cache_reuse is not supported"));
    }
}
