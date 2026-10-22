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
/// transition (a relaunch after failed generations, a quarantine) and flipped its snapshot
/// not-ready. Everything in flight on it will be cut by that relaunch, and the relaunch's
/// drain (`transition_if`, a write on the endpoint gate) waits for every admitted turn to
/// drop its guard, with new admissions barred behind it. So a wait that keeps renewing on a
/// condemned engine because it is still busy keeps the whole node dark: the IntelMac on
/// 2026-09-27 went 25 min (15:44-16:09Z) and then 46 min (16:33-17:19Z) with no lane while
/// header waits rode a prefilling, condemned engine (card 682a5abf's sibling, Cormac).
/// `false` until the daemon installs its state: nothing is condemned that was never served.
pub fn engine_condemned() -> bool {
    SERVING_STATE.get().is_some_and(|rx| !rx.borrow().ready)
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


/// Await `work`, treating each `bound` as a checkpoint rather than a verdict. At each
/// checkpoint `progress()` reads the engine:
/// - its work fingerprint moved: the queue is advancing, keep waiting;
/// - nothing moved and NO slot is processing: the engine is working its queue head,
///   keep waiting — up to `quiet_checkpoints` in a row (a page switch passes 8: a
///   save/restore is that queue head and moves no counter; a generation's header wait
///   passes 1: a quiet engine is not working on a request it has not picked up);
/// - nothing moved while a slot claims to be processing: stuck, `None`;
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
    bound: std::time::Duration,
    quiet_checkpoints: u32,
    mut progress: P,
    mut on_busy: impl FnMut(u64),
) -> Option<T>
where
    W: std::future::Future<Output = T>,
    P: FnMut() -> PF,
    PF: std::future::Future<Output = EngineProbe>,
{
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
                        } else if !now.any_processing && quiet_in_a_row < quiet_checkpoints {
                            quiet_in_a_row += 1;
                        } else {
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
