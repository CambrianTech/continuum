//! Readiness: the bring-up budgets (live and ephemeral), the measured load-progress
//! allowance read off `/health` 503 bodies, `wait_ready` (poll `/health`, then prove
//! decode), and the stderr-log tail that explains a bring-up that died or hung.

use super::*;

/// How long we wait for a freshly-spawned server to answer `/health` with 200
/// before declaring the launch degraded. Model load (mmap + Metal warm) can take
/// many seconds for a large GGUF; this is generous but bounded so a wedged
/// launch can't hang the daemon's reconcile forever.
/// How long a lane may take to come ready after spawn. Also the anchor for the
/// serving daemon's sustained-delta re-home window: you must never relaunch more
/// often than a relaunch can finish, so the observation window that justifies one
/// is derived from THIS ([`crate::modules::serving_daemon`]).
pub const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Readiness budget for an EPHEMERAL lane ([`EphemeralServingLane`]) — the eval /
/// teacher measurement lanes. These cold-load a SECOND large GGUF (a 24B Devstral)
/// on a Metal GPU that ALREADY holds the live persona lane's copy, so the weight
/// mmap + the first-decode Metal graph/command-buffer warmup runs under GPU
/// co-residency and legitimately exceeds the live lane's 90s budget. Glass-boxed
/// 2026-07-21: a co-resident cold Devstral-24B teacher lane answered `/health` 200
/// but its decode smoke-probe was still warming at the 90s cap — a PREMATURE FALSE
/// FAILURE (the lane served completions immediately after), which then dropped the
/// teacher onto the live LoRA lane and yielded 0 corpus. Same doctrine as the
/// 30→90 live-lane bump (see [`DEFAULT_SERVING_WAIT`]): fail LOUD, not FAST — a
/// generous bound that covers the real physical event, still bounded so a truly
/// wedged lane fails loud. Scoped to ephemeral lanes via `is_live_lane` so the live
/// lane's tighter fail-loud budget is untouched.
pub(super) const EPHEMERAL_READY_TIMEOUT: Duration = Duration::from_secs(240);

/// The bring-up budget in force on this poll. A server answering 503 is UP and loading
/// (llama-server serves `/health` 503 "Loading model" from the moment it listens), and a
/// child of ours that is still running is not dead, so that pair earns the cold-load
/// ceiling ([`EPHEMERAL_READY_TIMEOUT`], the measured co-resident cold-load bound); any
/// other state (no answer, a connection error, a 200 whose decode probe fails, an
/// adopted process we cannot see) keeps the caller's tight budget, so a wedge still
/// fails fast. Measured on the 5090 2026-10-06: a verified new engine's FIRST load
/// listened at 92s (about 1 GB of fresh CUDA libraries off disk plus the driver's kernel
/// cache), the 90s live budget called it failed, and the slot rollback put the lanes
/// back on the old engine; ordinary loads on that node run 80 to 92s.
pub(super) fn ready_budget(base: Duration, loading_and_alive: bool) -> Duration {
    if loading_and_alive {
        base.max(EPHEMERAL_READY_TIMEOUT)
    } else {
        base
    }
}

/// Measured loading work for one owned child. Labels and repeated samples do not
/// renew the existing quiet allowance. After the last measured advance (including
/// a stage reaching 1.0), unmeasured context initialization and warmup must finish
/// within that same quiet allowance; a future shorter allowance must cover that tail.
#[derive(Default)]
pub(super) struct ReadyLoadProgress {
    pub(super) stages: Vec<String>,
    pub(super) high: Vec<f64>,
    pub(super) last_advanced: Duration,
}

/// Existing native load callback snapshot carried by the health response.
#[derive(serde::Deserialize)]
pub(super) struct LoadProgressSample {
    pub(super) pid: u32,
    pub(super) stages: Vec<String>,
    pub(super) current: String,
    pub(super) value: f64,
}

#[derive(serde::Deserialize)]
pub(super) struct LoadingHealth {
    pub(super) load_progress: Option<LoadProgressSample>,
}

impl ReadyLoadProgress {
    pub(super) fn observe(&mut self, sample: &LoadProgressSample, pid: u32, elapsed: Duration) -> bool {
        if sample.pid != pid {
            return false;
        }
        let stages = &sample.stages;
        let Some(stage) = stages.iter().position(|name| name == &sample.current) else {
            return false;
        };
        if stages.iter().any(|name| name.is_empty())
            || stages
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != stages.len()
        {
            return false;
        }
        if self.stages.is_empty() {
            self.stages = stages.clone();
            self.high = vec![0.0; stages.len()];
        } else if self.stages != *stages {
            return false;
        }
        let value = sample.value;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) || value <= self.high[stage] {
            return false;
        }
        self.high[stage] = value;
        self.last_advanced = elapsed;
        true
    }

    pub(super) fn stalled(&self, elapsed: Duration, allowance: Duration) -> bool {
        elapsed.saturating_sub(self.last_advanced) >= allowance
    }
}

/// Poll cadence while waiting for `/health`. 503 → still loading, keep waiting.
const READY_POLL: Duration = Duration::from_millis(500);

impl LlamaServerProcess {
    /// The tail of this lane's stderr log (`llama-server-<port>.log`, the file the
    /// spawn wires stderr to) — the llama.cpp load banner or ggml/Metal fault that
    /// explains a bring-up failure. Best-effort: an unreadable/absent log yields a
    /// marker, never blocks the error path. An EMPTY log is itself a signal — a
    /// child that produced no output before the deadline hung in early init (e.g.
    /// a co-resident second Metal context that never acquired the device), distinct
    /// from a crash (which prints a banner + fault first).
    pub(super) fn stderr_log_tail(&self) -> String {
        let Some(port) = self
            .root
            .rsplit(':')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
        else {
            return "<lane port unparseable>".to_string();
        };
        let Some(path) = dirs::home_dir().map(|h| {
            h.join(".continuum")
                .join("logs")
                .join(format!("llama-server-{port}.log"))
        }) else {
            return "<no home dir>".to_string();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => tail_or_hang_marker(&s),
            // The marker is the contract (never block the error path), but a bare
            // "unreadable" hides WHY — permissions vs absent vs mid-rotation.
            Err(e) => format!("<stderr log unreadable: {e}>"),
        }
    }

    pub(super) async fn wait_ready(&self) -> Result<(), LlamaServerError> {
        // The live lane keeps the tight 90s fail-loud budget; an ephemeral
        // measurement lane gets the co-resident cold-large-model budget (it loads a
        // SECOND large GGUF beside the live one — the warmup legitimately runs
        // longer). Scoped by `is_live_lane` so a wedged LIVE lane still fails fast.
        let budget = if self.is_live_lane {
            READY_TIMEOUT
        } else {
            EPHEMERAL_READY_TIMEOUT
        };
        let health = format!("{}/health", self.root);
        let started = Instant::now();
        let owned_loading = {
            let child = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic mutating the owned child.
            child.as_ref().and_then(|owned| {
                owned.child.id().map(|pid| {
                    (
                        OwnedEngineIdentity {
                            owner: Arc::downgrade(&self.child),
                            generation: owned.generation.clone(),
                        },
                        pid,
                    )
                })
            })
        };
        let mut progress = ReadyLoadProgress::default();
        // Assigned by every arm below before the deadline check reads it.
        let mut last: String;
        // Sticky: once the server has said it is loading, the finish of that load (the 200
        // and the decode probe's first graph) belongs to the same allowance.
        let mut seen_loading = false;
        loop {
            match self.client.get(&health).timeout(PROBE_TIMEOUT).send().await {
                Ok(resp) if resp.status().is_success() => {
                    // HTTP listener is up and the model is loaded. Confirm the GPU
                    // decode path before declaring ready — a 200 /health does not
                    // prove `llama_decode` works (the wedged-orphan failure mode).
                    if self.decode_smoke_ok().await {
                        return Ok(());
                    }
                    last = String::from("/health 200 but decode smoke-probe failed");
                }
                Ok(resp) => {
                    let loading = resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE;
                    seen_loading |= loading;
                    last = format!("status {}", resp.status());
                    if loading {
                        if let Some((expected, pid)) = &owned_loading {
                            if let Ok(LoadingHealth {
                                load_progress: Some(sample),
                            }) = resp.json::<LoadingHealth>().await
                            {
                                if self.owned_engine().as_ref() == Some(expected)
                                    && progress.observe(&sample, *pid, started.elapsed())
                                {
                                    crate::probe!(
                                        class = "serving.load.progress",
                                        pid = *pid,
                                        elapsed_ms = started.elapsed().as_millis() as u64,
                                        stage = %sample.current,
                                        value = sample.value,
                                        "owned model loading made measured progress; retaining the existing stall allowance"
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => last = e.to_string(),
            }
            // #205 unmask: the instant our OWN child exits, fail LOUD with its exit
            // status + stderr tail — the real cause (bad args, model-load failure, an
            // aborted Metal init) — instead of polling a dead port for the whole budget
            // and reporting a bare "/health request failed" that discards the reason.
            // No-op for the adopt path (we own no child); benefits live + ephemeral alike.
            if let Some(status) = self.child_exit_status() {
                return Err(LlamaServerError::Spawn(format!(
                    "llama-server exited during bring-up ({status}) before it served on {} \
                     — last stderr:\n{}",
                    self.root,
                    self.stderr_log_tail()
                )));
            }
            let budget = ready_budget(budget, seen_loading && self.owns_child());
            if progress.stalled(started.elapsed(), budget) {
                // Even a non-exiting HANG now surfaces the stderr state: an empty log
                // fingerprints "child emitted nothing before it stopped" — the diagnostic
                // that was missing when this masked as a bare 240s /health timeout. The
                // exit-status arm above disambiguates the two empty-log causes: an OS
                // OOM/jetsam kill (SIGKILL/137 — the proven cause when a second large
                // model won't fit in free unified memory) vs. a genuine early-init hang
                // (no exit status). Glass-boxed 2026-07-28: a second 24B eval lane co-resident
                // with the live 24B lane was jetsam-killed with 9.6 GB free.
                return Err(LlamaServerError::NotReady(
                    started.elapsed(),
                    format!(
                        "{last}; measured load progress quiet for {:?} (allowance {budget:?}) — last stderr:\n{}",
                        started.elapsed().saturating_sub(progress.last_advanced),
                        self.stderr_log_tail()
                    ),
                ));
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }
}

/// The last lines of an llama-server stderr log, OR — when the log is empty — a
/// diagnostic marker naming what an empty log MEANS: the child produced NO output
/// before it stopped. Read alongside the exit-status arm in `wait_ready`, that
/// disambiguates the two causes: an OS OOM/jetsam kill (SIGKILL/137 — the proven
/// cause when a second large model can't fit in free unified memory; the child
/// dies before llama.cpp prints its banner) vs. a genuine early-init hang (child
/// still running at the deadline). A crash from bad args or a model-load fault
/// prints the banner + fault FIRST, so a non-empty tail carries that directly.
/// Pure so the load-bearing empty-vs-tail decision is unit-tested without touching
/// the real `~/.continuum/logs` path ([[self-test-via-command-feedback-surface-
/// never-blind]], and #72's env-dependent-test lesson).
pub(super) fn tail_or_hang_marker(contents: &str) -> String {
    let mut lines: Vec<&str> = contents.lines().rev().take(20).collect();
    lines.reverse();
    let tail = lines.join("\n");
    if tail.trim().is_empty() {
        "<stderr log empty — child produced no output before it stopped: with a SIGKILL/137 \
         exit this is an OS OOM/jetsam kill (a second large model that didn't fit in free \
         memory); with no exit status it is a genuine early-init hang>"
            .to_string()
    } else {
        tail
    }
}
