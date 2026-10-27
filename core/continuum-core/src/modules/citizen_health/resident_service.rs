//! Each resident's hour, judged on her own service stream (card 1c157668).
//!
//! On 2026-10-26 Kimi went 17 hours without a completed act (every turn refused before
//! inference: required 26,101 > budget 17,829 at 49,152) and Sahar stalled from 20:05Z, and
//! nobody noticed until Joel asked. The hour's tick closes every resident's window
//! (`cognition::resident_service`, fed at the turn's settle), judges it with
//! `continuum_service::resident::judge`, probes `resident.stalled` / `resident.regressed`, and
//! publishes the alarms on a watch the capacity beacon reads, so `grid/nodes` on any node
//! names another node's stuck residents. Runs inside this module's own tick: no task of its own.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, OnceLock};

use continuum_service::detect::{ServicePolicy, ServiceState};
use continuum_service::resident::{self, AlarmState, ResidentAlarm, StreamFacts};
use continuum_service::sample::{Builds, EngineIncarnation, ServingConfig, Window};
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use super::HEALTH_WINDOW;

/// The detector's bounds on one resident's hourly stream.
// derived-or-floor: a day of her own healthy hours is her reference (`history`), three of them
// before any hour is judged slow (`min_evidence`), two slow hours in a row before it is a
// regression (`sustained`); the window itself is HEALTH_WINDOW, this module's tick. A stall
// needs none of these: one window with demand and no completion is enough.
const RESIDENT_POLICY: ServicePolicy = ServicePolicy { history: 24, min_evidence: 3, sustained: 2 };

/// Every resident's stream, and where the last window closed.
#[derive(Default)]
struct Streams {
    states: HashMap<Uuid, ServiceState>,
    last_close_ms: Option<u64>,
}

static STREAMS: LazyLock<Mutex<Streams>> = LazyLock::new(|| Mutex::new(Streams::default()));

/// The alarms of the last closed window, read by the capacity beacon on every publish.
static ALARMS: LazyLock<watch::Sender<Arc<Vec<ResidentAlarm>>>> =
    LazyLock::new(|| watch::channel(Arc::new(Vec::new())).0);

/// The residents this node judged stalled or regressed over its last window.
pub fn current_resident_alarms() -> Vec<ResidentAlarm> {
    ALARMS.borrow().as_ref().clone()
}

/// This core's incarnation: a resident's stream lives and dies with the process.
fn incarnation() -> EngineIncarnation {
    static STARTED_MS: OnceLock<u64> = OnceLock::new();
    EngineIncarnation {
        pid: std::process::id(),
        started_ms: *STARTED_MS.get_or_init(|| continuum_base::time::EpochMs::now().millis()),
    }
}

fn stream_facts(residents_active: u32) -> StreamFacts {
    let serving = crate::inference::llama_server::current_serving();
    StreamFacts {
        node: crate::capacity::gossip::this_process_origin().to_string(),
        // The serving snapshot does not report the engine build, KV type, backend or expert
        // residency; empty = unreported, and a resident's baseline splits on what IS reported.
        builds: Builds { core: env!("CONTINUUM_BUILD_GIT_SHA").to_string(), engine: String::new() },
        config: ServingConfig {
            lanes: serving.lanes,
            ctx_per_lane: serving.served_context_window,
            cache_ram_mib: serving.host_prompt_cache_mib,
            kv_type: String::new(),
            backend: String::new(),
            expert_residency: String::new(),
        },
        incarnation: incarnation(),
        residents_active,
        memory_ceiling_bytes: 0,
    }
}

/// Close and judge every live resident's window ending at `now_ms`; probe and publish the
/// alarms. Returns them for the hour's line.
pub(super) async fn judge_residents(now_ms: u64) -> Vec<ResidentAlarm> {
    let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global();
    let live: HashSet<Uuid> = registry.as_ref().map(|r| r.live_personas().into_iter().collect()).unwrap_or_default(); // unwrap_or_default: no registry = no residents to judge
    let facts = stream_facts(live.len() as u32);
    let mut streams = STREAMS.lock().await;
    let window_ms = HEALTH_WINDOW.as_millis() as u64;
    let span = Window { start_ms: streams.last_close_ms.unwrap_or(now_ms.saturating_sub(window_ms)), end_ms: now_ms }; // unwrap_or: the first window is the tick's own interval
    streams.last_close_ms = Some(now_ms);
    let windows = crate::cognition::resident_service::close_windows(span);
    streams.states.retain(|id, _| live.contains(id));
    let mut alarms = Vec::new();
    for (persona, window) in windows.into_iter().filter(|(id, _)| live.contains(id)) {
        let name = registry.as_ref().and_then(|r| r.get(persona)).map(|rt| rt.agent_name().to_string());
        let Some(name) = name else { continue };
        let state = streams.states.entry(persona).or_default();
        let verdict = resident::judge(state, &RESIDENT_POLICY, &window, span, &facts);
        let Some(alarm) = ResidentAlarm::of(persona, &name, &window, span, &verdict) else { continue };
        probe(&alarm);
        alarms.push(alarm);
    }
    ALARMS.send_replace(Arc::new(alarms.clone()));
    alarms
}

fn probe(alarm: &ResidentAlarm) {
    let (required_tokens, budget_tokens, context_window) = match alarm.refusal {
        Some(resident::Refusal::PromptCapacity { required_tokens, budget_tokens, context_window }) => {
            (required_tokens, budget_tokens, context_window.unwrap_or(0)) // unwrap_or: 0 = no readiness owner observed a window
        }
        _ => (0, 0, 0),
    };
    let refusal = match alarm.refusal {
        Some(resident::Refusal::PromptCapacity { .. }) => "prompt_capacity",
        Some(resident::Refusal::InferenceFailed) => "inference_failed",
        Some(resident::Refusal::LaneStarved) => "lane_starved",
        None => "none",
    };
    match alarm.state {
        AlarmState::Stalled => continuum_base::probe!(
            class = "resident.stalled",
            persona = %alarm.name,
            persona_id = %alarm.persona_id,
            window_start_ms = alarm.window_start_ms,
            window_end_ms = alarm.window_end_ms,
            demand = alarm.demand,
            completions = alarm.completions,
            refusals = alarm.refusals,
            parked = alarm.parked,
            in_flight_since_ms = alarm.in_flight_since_ms.unwrap_or(0), // unwrap_or: 0 = no turn in flight across the window
            refusal = refusal,
            required_tokens = required_tokens,
            budget_tokens = budget_tokens,
            context_window = context_window,
            "a resident was asked for work and completed none this window"
        ),
        AlarmState::Regressed { observed_p50_ms, slowest_healthy_p50_ms, streak } => continuum_base::probe!(
            class = "resident.regressed",
            persona = %alarm.name,
            persona_id = %alarm.persona_id,
            window_start_ms = alarm.window_start_ms,
            window_end_ms = alarm.window_end_ms,
            demand = alarm.demand,
            completions = alarm.completions,
            observed_p50_ms = observed_p50_ms,
            slowest_healthy_p50_ms = slowest_healthy_p50_ms,
            streak = streak,
            refusal = refusal,
            required_tokens = required_tokens,
            budget_tokens = budget_tokens,
            context_window = context_window,
            "a resident's turns ran slower than her own healthy hours, sustained"
        ),
    }
}
