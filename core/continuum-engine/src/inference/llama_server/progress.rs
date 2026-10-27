//! Progress responsibility of the llama-server process owner.
use super::*;
/// What one `/slots` read says about the engine's work: a fingerprint of everything its
/// slots have done (per slot: task id + prompt tokens processed + tokens decoded,
/// summed), and whether ANY slot is processing a request right now. The fingerprint
/// moves when a slot prefills, decodes or picks up a task. A slot save/restore is a
/// queue task that moves none of those counters, so "nothing moved while no slot is
/// processing" means the engine is working its queue head — possibly our own switch —
/// not that it is stuck. Pure; `None` if the body is not the `/slots` array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EngineProgress {
    pub fingerprint: u64,
    pub any_processing: bool,
}


pub(crate) fn slots_progress(slots: &serde_json::Value) -> Option<EngineProgress> {
    let slots = slots.as_array()?;
    let n = |v: &serde_json::Value| v.as_u64().unwrap_or(0); // JUSTIFIED unwrap_or: an absent counter contributes nothing to the sum; progress is a CHANGE, never a level
    let fingerprint = slots.iter().fold(0u64, |acc, slot| {
        acc.wrapping_add(n(&slot["id_task"]))
            .wrapping_add(n(&slot["n_prompt_tokens_processed"]))
            .wrapping_add(n(&slot["next_token"][0]["n_decoded"]))
    });
    let any_processing = slots.iter().any(|s| s["is_processing"].as_bool() == Some(true));
    Some(EngineProgress { fingerprint, any_processing })
}


/// Read the engine's progress from `GET /slots` (bounded like every probe here). `None`
/// when the engine does not answer — which a caller treats as not progressing. Asked by a
/// slow KV page switch before it may call the engine stuck
/// ([`crate::inference::turn_admission::kv_page_action`]).
/// What one read of the engine's `/slots` proved — typed by what it can and cannot say.
///
/// Why three outcomes and not `Option` (BigMama, 2026-09-26 14:24:55Z): a restore of a
/// 71,680-token page ran to its 14.3 s checkpoint, the checkpoint's `/slots` read timed
/// out 4.0 s later, and `None` was read as "the engine stopped answering" — the switch
/// was abandoned as uncertain, the endpoint quarantined, the engine retired and
/// relaunched: 135 s with every admission refused, 36 of them Kimi's. But llama-server
/// runs slot save/restore on the same loop that answers `/slots`, so an engine
/// mid-restore of a large page CANNOT answer the probe: the operation being waited on
/// is what times the probe out. A timeout is the engine busy; only a socket that is
/// not there is the engine gone. Busy is not dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineProbe {
    /// The engine answered: its work fingerprint and whether any slot is processing.
    Progress(EngineProgress),
    /// No answer inside the probe's timeout. The engine is working — most likely on the
    /// very switch the caller is waiting for.
    Busy,
    /// The socket refused, reset or closed, or the body was not the `/slots` array:
    /// nothing that is a llama-server answers on this root. The outcome that ends a wait.
    Unreachable,
    /// The serving daemon is replacing this engine ([`engine_condemned`]). Busy is not
    /// dead, but a condemned engine is not worth waiting on: its relaunch cuts the work
    /// anyway, and the relaunch's drain waits for every admitted turn to let go.
    Condemned,
    /// The exact observed paging operation failed or acknowledged cancellation.
    /// This ends observation; it is never a successful HTTP completion receipt.
    OperationEnded,
}


/// The engine this node serves is being replaced: the serving daemon admitted a lifecycle
/// transition (a relaunch, a quarantine, a convergence) and no replacement has published ready
/// since. Everything in flight on it will be cut by that relaunch, and the relaunch's drain
/// (`transition_if`, a write on the endpoint gate) waits for every admitted turn to drop its
/// guard, with new admissions barred behind it. So a wait that keeps renewing on a condemned
/// engine because it is still busy keeps the whole node dark: the IntelMac on 2026-09-27 went
/// 25 min (15:44-16:09Z) and then 46 min (16:33-17:19Z) with no lane while header waits rode a
/// prefilling, condemned engine (card 682a5abf's sibling, Cormac). The condemnation itself,
/// never the readiness bit: a transient not-ready (a 0 `/props` window under AlreadyServing)
/// condemns nothing (card 9c67e5fe, after Fable on #5500).
pub fn engine_condemned() -> bool {
    CONDEMNATIONS.pending()
}

/// The daemon published a ready snapshot (`emit_serving`): every condemnation so far is answered.
pub fn note_serving_ready() {
    CONDEMNATIONS.recovered();
}

/// How many times the serving daemon has DECIDED to tear the engine down while work may hold it,
/// and the generation its last ready snapshot answered. Not the readiness bit (Fable on #5500).
pub(crate) struct Condemnations {
    generation: watch::Sender<u64>,
    recovered: std::sync::atomic::AtomicU64,
}

impl Condemnations {
    pub(crate) fn new() -> Self {
        Self { generation: watch::channel(0).0, recovered: std::sync::atomic::AtomicU64::new(0) }
    }

    /// One more condemnation; its generation.
    pub(crate) fn condemn(&self) -> u64 {
        self.generation.send_modify(|generation| *generation += 1);
        *self.generation.borrow()
    }

    /// A ready engine answers every condemnation made so far.
    pub(crate) fn recovered(&self) {
        self.recovered.store(*self.generation.borrow(), std::sync::atomic::Ordering::Release);
    }

    /// A condemnation no ready engine has answered yet.
    pub(crate) fn pending(&self) -> bool {
        *self.generation.borrow() > self.recovered.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.generation.subscribe()
    }
}

static CONDEMNATIONS: std::sync::LazyLock<Condemnations> = std::sync::LazyLock::new(Condemnations::new);

/// The daemon decided to replace the engine and its relaunch will drain every admitted turn:
/// every act in flight ends now instead of holding that drain to its next checkpoint.
pub fn condemn(reason: &str) {
    let generation = CONDEMNATIONS.condemn();
    continuum_base::probe!(
        class = "serving.engine.condemned",
        generation = generation,
        reason = reason,
        "the engine is being replaced: acts in flight on it end now, so its drain is not held"
    );
}

/// Resolves at the next condemnation after the call: enrico, 2026-10-26 (core 839f70216), a
/// quarantine at 21:10:02Z, then `ensure_serving` wedged 23 min until an act's next checkpoint
/// at 21:33:18 let its guard go (card 4d2a64a2).
pub async fn until_condemned() {
    until_condemned_on(condemnations()).await
}

/// A receiver of the condemnation generation, marked seen: it changes at the next one.
pub fn condemnations() -> watch::Receiver<u64> {
    CONDEMNATIONS.subscribe()
}

/// [`until_condemned`] on one receiver: the next generation after it was subscribed.
pub(crate) async fn until_condemned_on(mut rx: watch::Receiver<u64>) {
    if rx.changed().await.is_err() {
        std::future::pending::<()>().await;
    }
}


/// Capability discovery uses the same bounded props read as the managed lane.
pub(crate) async fn engine_props(root: &str, client: &reqwest::Client) -> Result<serde_json::Value, LlamaServerError> {
    let response = client.get(format!("{}/props", root.trim_end_matches('/')))
        .timeout(PROBE_TIMEOUT).send().await
        .map_err(|error| LlamaServerError::Unreachable(error.to_string()))?;
    if !response.status().is_success() {
        return Err(LlamaServerError::Unreachable(format!("status {}", response.status())));
    }
    response.json().await.map_err(|error| LlamaServerError::Unreachable(error.to_string()))
}


pub(crate) async fn engine_probe(root: &str, client: &reqwest::Client) -> EngineProbe {
    if engine_condemned() {
        return EngineProbe::Condemned;
    }
    engine_probe_within(root, client, PROBE_TIMEOUT).await
}


/// Interpret only the requested operation. A different restore on the same slot
/// cannot keep this waiter alive, and a completed snapshot is not an HTTP receipt.
pub(super) fn restore_progress(body: &serde_json::Value, operation: uuid::Uuid, slot: u32) -> Option<EngineProbe> {
    if body["operation_id"].as_str()?.parse::<uuid::Uuid>().ok()? != operation
        || body["id_slot"].as_u64()? != u64::from(slot)
        || body["action"].as_str()? != "restore"
    {
        return None;
    }
    let phase = body["phase"].as_u64()?;
    if phase == 4 || phase == 5 {
        return Some(EngineProbe::OperationEnded);
    }
    if phase > 3 { return None; }
    let fingerprint = body["state_bytes"].as_u64()?
        .saturating_add(body["sidecar_bytes"].as_u64()?)
        .saturating_add(body["checkpoints"].as_u64()?)
        .saturating_add(phase);
    Some(EngineProbe::Progress(EngineProgress { fingerprint, any_processing: false }))
}


pub(super) fn restore_probe_response(status: reqwest::StatusCode, body: &serde_json::Value, operation: uuid::Uuid, slot: u32) -> EngineProbe {
    if !status.is_success() { return EngineProbe::Busy; }
    // Older engines ignore the query and return the ordinary slots array. Only
    // that capability shape permits legacy progress; an unknown/replaced UUID
    // must never borrow progress from an unrelated operation.
    if body.is_array() {
        return slots_progress(body).map_or(EngineProbe::Busy, EngineProbe::Progress);
    }
    restore_progress(body, operation, slot).unwrap_or(EngineProbe::Busy)
}


pub(crate) async fn restore_probe(root: &str, client: &reqwest::Client, operation: uuid::Uuid, slot: u32) -> EngineProbe {
    if engine_condemned() { return EngineProbe::Condemned; }
    let response = client.get(format!("{root}/slots"))
        .query(&[("operation_id", operation.to_string())])
        .timeout(PROBE_TIMEOUT).send().await;
    match response {
        Ok(response) => {
            let status = response.status();
            match response.json::<serde_json::Value>().await {
                Ok(body) => restore_probe_response(status, &body, operation, slot),
                Err(_) => EngineProbe::Busy,
            }
        }
        Err(error) if error.is_connect() && !error.is_timeout() => EngineProbe::Unreachable,
        Err(_) => EngineProbe::Busy,
    }
}


/// [`engine_probe`] with the timeout as a parameter, so a test can hold a socket open
/// for 200 ms instead of 4 s.
pub(crate) async fn engine_probe_within(
    root: &str,
    client: &reqwest::Client,
    timeout: Duration,
) -> EngineProbe {
    // Two phases, because one timeout cannot tell them apart: a socket nothing listens
    // on can be slow to refuse, and the request's own timeout then fires first and reads
    // as "busy". So reachability is asked of the SOCKET (a bare connect, bounded), and
    // only a connected engine that does not answer in time is busy.
    let addr = root
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();
    let connect = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;
    match connect {
        Ok(Ok(stream)) => drop(stream),
        Ok(Err(_)) | Err(_) => return EngineProbe::Unreachable,
    }
    let url = format!("{root}/slots");
    let response = match client.get(&url).timeout(timeout).send().await {
        Ok(response) => response,
        Err(error) if error.is_timeout() => return EngineProbe::Busy,
        Err(_) => return EngineProbe::Unreachable,
    };
    let body: serde_json::Value = match response.json().await {
        Ok(body) => body,
        Err(error) if error.is_timeout() => return EngineProbe::Busy,
        Err(_) => return EngineProbe::Unreachable,
    };
    slots_progress(&body).map_or(EngineProbe::Unreachable, EngineProbe::Progress)
}


/// The two bounds of a wait on the engine: how often it checks (`checkpoint`), and how long a
/// PROCESSING engine may show no `/slots` counter movement before it is called stuck
/// (`unmoved`). A plain `Duration` is both, the strictest reading: processing and unmoved
/// at the first checkpoint is stuck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WaitBounds {
    pub checkpoint: std::time::Duration,
    pub unmoved: std::time::Duration,
    /// The prefill rate `unmoved` was sized from, and its source (`per_request`,
    /// `server_total`, `none`), so a refusal names the number behind its window.
    pub unmoved_rate: Option<f64>,
    pub unmoved_rate_source: &'static str,
}

impl From<std::time::Duration> for WaitBounds {
    fn from(bound: std::time::Duration) -> Self {
        Self { checkpoint: bound, unmoved: bound, unmoved_rate: None, unmoved_rate_source: "none" }
    }
}

impl WaitBounds {
    /// Checks every `checkpoint`, and lets a processing engine stay unmoved for the
    /// [`unmoved_window`] of `model` (never less than one checkpoint).
    pub(crate) fn for_model(checkpoint: std::time::Duration, model: Option<&str>) -> Self {
        let (unmoved, unmoved_rate, unmoved_rate_source) = unmoved_window_and_rate(checkpoint, model);
        Self { checkpoint, unmoved, unmoved_rate, unmoved_rate_source }
    }
}

/// The longest any wait lets a processing engine stay unmoved: the act's own ceiling, so a
/// caller without an act bound above it (a probe, an eval, a page switch) still ends.
pub(crate) const UNMOVED_CEILING: std::time::Duration = crate::inference::turn_bound::ACT_CEILING;

/// THE window in which a processing engine may show no `/slots` counter movement and still
/// be working: `floor`, or TWO batches at `model`'s measured prefill rate, whichever is
/// longer. [`UNMOVED_CEILING`] caps the RATE-derived part only; a caller's floor above it
/// stands (the window never shortens a bound the caller already chose), so the result is
/// `max(floor, min(two batches, ceiling))`. The counters advance a whole batch at a time
/// (`lane_args::UBATCH_TOKENS`), so a lane prefilling one 2,048-token chunk every ~176 s (the
/// IntelMac at 11.6 tok/s; measured chunk gaps 124 / 168 / 214 / 171 s, Cormac on #4506)
/// shows no move for most of any shorter window. One owner for every wait that reads
/// `/slots`: the stream's starvation check, a generation's header wait and a page switch
/// (enrico 2026-10-26 07:59-08:59Z: 3 of 3 page saves refused 15-17 s into another slot's
/// 2,048-token batch at 10.4-15.4 tok/s, each mind cold on her next turn, because the page
/// switch judged "unmoved" at its 16.8 s checkpoint; card 0b52b957).
///
/// The rate is the SLOWER of the two measurements ([`window_rate`]): the server-total one
/// (`serving_rates`, over busy windows) and the per-request one (`prefill_rate`, fresh or
/// stale). The two errors are not symmetric: a window too long only delays calling a truly
/// stuck engine, and [`UNMOVED_CEILING`] still bounds it; a window too short refuses a working
/// engine and the evicted mind loses her page. Measured on enrico 2026-10-26 13:15-14:15Z
/// (0eeceda84): the per-request rate read 9.7-16.2 tok/s, the windows used implied 64-174 tok/s
/// (server-total), and 3 saves were still refused 23-64 s into a ~125-210 s batch, the window being two of them, 250-420 s (card
/// e1a5a21a; this reverses the server-total preference from #4506). No rate measured: the floor.
pub(crate) fn unmoved_window(floor: std::time::Duration, model: Option<&str>) -> std::time::Duration {
    unmoved_window_and_rate(floor, model).0
}

/// [`unmoved_window`] with the rate it used and where that rate came from, for the probes.
pub(crate) fn unmoved_window_and_rate(
    floor: std::time::Duration,
    model: Option<&str>,
) -> (std::time::Duration, Option<f64>, &'static str) {
    let (rate, source) = model.map_or((None, "none"), |m| {
        window_rate(
            crate::inference::serving_rates::rates_for(m).prefill_tps,
            crate::inference::prefill_rate::measured_rate_for(m).tps,
        )
    });
    (window_at_rate(floor, rate), rate, source)
}

/// PURE: the rate a no-movement window is sized from: the slower of the server-total and the
/// per-request measurement, each only when finite and positive, with which one it was.
pub(crate) fn window_rate(server_total: Option<f64>, per_request: Option<f64>) -> (Option<f64>, &'static str) {
    let usable = |r: Option<f64>| r.filter(|r| r.is_finite() && *r > 0.0);
    match (usable(server_total), usable(per_request)) {
        (Some(s), Some(p)) if p <= s => (Some(p), "per_request"),
        (Some(s), Some(_)) => (Some(s), "server_total"),
        (Some(s), None) => (Some(s), "server_total"),
        (None, Some(p)) => (Some(p), "per_request"),
        (None, None) => (None, "none"),
    }
}

/// PURE: [`unmoved_window`] at a known rate: `max(floor, min(two batches, UNMOVED_CEILING))`.
pub(crate) fn window_at_rate(floor: std::time::Duration, prefill_tps: Option<f64>) -> std::time::Duration {
    prefill_tps
        .filter(|r| r.is_finite() && *r > 0.0)
        // try_: a vanishing rate would overflow Duration and panic (Codex on #4506); anything
        // past the ceiling is the ceiling
        .map(|r| {
            std::time::Duration::try_from_secs_f64(2.0 * f64::from(crate::inference::lane_args::UBATCH_TOKENS) / r)
                .map_or(UNMOVED_CEILING, |d| d.min(UNMOVED_CEILING))
        })
        .map_or(floor, |chunk| floor.max(chunk))
}


/// Await `work`, treating each `bound` as a checkpoint rather than a verdict. At each
/// checkpoint `progress()` reads the engine:
/// - its work fingerprint moved: the queue is advancing, keep waiting;
/// - nothing moved and NO slot is processing: the engine is working its queue head,
///   keep waiting — up to `quiet_checkpoints` in a row (a page switch passes 8: a
///   save/restore is that queue head and moves no counter; a generation's header wait
///   passes 1: a quiet engine is not working on a request it has not picked up);
/// - nothing moved while a slot claims to be processing: working, IF the last move is
///   younger than the bounds' `unmoved` patience (an engine's `/slots` counters advance a
///   whole batch at a time, so a slow prefill is unmoved for one batch: see
///   [`unmoved_window`]); past it, stuck, `None`;
/// - the engine did not answer inside the probe's own timeout ([`EngineProbe::Busy`]):
///   it is working, most likely on this very switch — keep waiting, bounded like a quiet
///   engine (card 8c06f778: a 71,680-token restore's checkpoint probe timed out and was
///   read as death; 135 s closed, one engine replaced);
/// - nothing that is a llama-server answers on this root ([`EngineProbe::Unreachable`]):
///   `None`, the one silence that IS the engine gone.
///
/// Why both signals (M5, 2026-09-26 10:2xZ): a save ran 12.4 s against a 12.2 s bound on
/// an engine with idle slots; the fingerprint alone could not see the save and called it a
/// stall — one quarantine, one engine replacement, 11 turns failed. And why not liveness
/// alone (Cormac's review of #4387): `/health` answers off the slot queue, so it stays
/// healthy through a genuinely stuck switch.
pub(crate) async fn wait_while_engine_progresses<T, W, P, PF>(
    work: W,
    bounds: impl Into<WaitBounds>,
    quiet_checkpoints: u32,
    mut progress: P,
    mut on_busy: impl FnMut(u64),
) -> Option<T>
where
    W: std::future::Future<Output = T>,
    P: FnMut() -> PF,
    PF: std::future::Future<Output = EngineProbe>,
{
    let WaitBounds { checkpoint: bound, unmoved, .. } = bounds.into();
    tokio::pin!(work);
    // A probe is observation, never a suspension of the request it observes.
    // In particular a terminal HTTP receipt wins over a simultaneous stale
    // probe verdict; otherwise paging can quarantine work that already finished.
    let initial = tokio::select! {
        biased;
        done = &mut work => return Some(done),
        observed = progress() => observed,
    };
    let mut last = match initial {
        EngineProbe::Progress(p) => Some(p.fingerprint),
        EngineProbe::OperationEnded => return None,
        EngineProbe::Busy | EngineProbe::Unreachable | EngineProbe::Condemned => None,
    };
    let mut busy_checkpoints: u64 = 0;
    let mut quiet_in_a_row: u32 = 0;
    // When the work fingerprint last moved (the wait's start until it is seen to move).
    let mut moved_at = tokio::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            done = &mut work => return Some(done),
            _ = tokio::time::sleep(bound) => {
                let observed = tokio::select! {
                    biased;
                    done = &mut work => return Some(done),
                    observed = progress() => observed,
                };
                match observed {
                    EngineProbe::Unreachable | EngineProbe::OperationEnded => return None,
                    EngineProbe::Condemned => {
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.wait.engine_condemned",
                            busy_checkpoints,
                            "a wait reached its checkpoint on an engine the daemon is replacing — ending it so the relaunch's drain can finish"
                        );
                        return None;
                    }
                    // Working, and unable to say so: bounded exactly like a quiet engine.
                    EngineProbe::Busy => {
                        if quiet_in_a_row < quiet_checkpoints {
                            quiet_in_a_row += 1;
                        } else {
                            return None;
                        }
                    }
                    EngineProbe::Progress(now) => {
                        let moved = last.map_or(true, |before| before != now.fingerprint);
                        if moved {
                            quiet_in_a_row = 0;
                            moved_at = tokio::time::Instant::now();
                        } else if !now.any_processing {
                            if quiet_in_a_row < quiet_checkpoints {
                                quiet_in_a_row += 1;
                            } else {
                                return None;
                            }
                        } else if moved_at.elapsed() >= unmoved {
                            // processing, and unmoved for longer than one batch can take
                            return None;
                        }
                        last = Some(now.fingerprint);
                    }
                }
                busy_checkpoints += 1;
                on_busy(busy_checkpoints);
            }
        }
    }
}
