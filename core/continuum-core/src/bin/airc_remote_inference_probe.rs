//! `airc_remote_inference_probe` — the canonical "is the cross-grid
//! inference wire alive?" probe.
//!
//! ## Why this exists
//!
//! `AircRemoteInferenceAdapter` is the substrate's structural answer for
//! constrained-locally hosts (the Intel Mac + the future Docker node + any
//! peer that genuinely cannot serve its own inference). The unit + loopback
//! tests prove the adapter is functionally identical to a local one
//! (`tests/airc_remote_inference_roundtrip.rs`,
//! `tests/airc_remote_inference_end_to_end.rs`). What they do NOT prove
//! is that the wire actually round-trips between two REAL airc peers on
//! REAL heterogeneous hardware. This binary closes that gap.
//!
//! Run it from a Mac Intel (or any consumer) pointing at a live remote
//! peer that has registered a `ai/generate` handler. It will:
//!
//! 1. Attach to the local airc daemon at `$AIRC_HOME/runtime/...sock` as
//!    a transient agent named `continuum-remote-inference-probe`.
//! 2. Construct `AircLiveTransport` targeting the configured remote
//!    `peer_id` with a configurable deadline.
//! 3. Construct `AircRemoteInferenceAdapter` over that transport.
//! 4. Fire `generate_text` with a single-message prompt.
//! 5. Assert: response returned within the deadline, `text` is
//!    non-empty, and the adapter's `has_observed_success` flipped
//!    `Unknown -> Healthy` per the honesty contract.
//! 6. Print a structured one-line summary with the wall-clock latency.
//!
//! ## Environment contract
//!
//! - `CONTINUUM_REMOTE_PEER_ID` (REQUIRED): UUID of the remote airc peer
//!   that has the `ai/generate` handler registered (typically a persona
//!   running on the provider host).
//! - `CONTINUUM_REMOTE_MODEL` (optional, default `"qwen2.5-0.5b-instruct"`):
//!   value passed as the `model` field of the `TextGenerationRequest`.
//!   The provider may map this to whatever local model serves it.
//! - `CONTINUUM_REMOTE_PROMPT` (optional, default short greeting): the
//!   user-role content of the single chat message in the request.
//! - `CONTINUUM_REMOTE_DEADLINE_MS` (optional, default `30000`): per-request
//!   deadline in milliseconds, fed into `AircLiveTransport::with_deadline`.
//! - `AIRC_HOME` (optional, default `$HOME/.airc`): airc home dir for the
//!   probe's transient attach.
//! - `AIRC_DAEMON_SOCKET` (optional): override for the daemon socket
//!   path. When unset, falls back to `{AIRC_HOME}/runtime/daemon.sock`.
//!   If the fallback misses the canonical resolution (e.g. machine-account
//!   scoping), set this explicitly.
//!
//! ## Why a binary rather than a `#[test]`
//!
//! The existing loopback tests live in `tests/` because they're
//! deterministic. This probe by design talks to a non-local peer at a
//! UUID supplied via env — that's outside the closed loop a unit test
//! can guarantee. As a binary it stays out of `cargo test` (no
//! flakes from a missing remote peer in CI) but remains in the build
//! graph, so any refactor that breaks the adapter / transport surface
//! breaks the probe at `cargo build` time.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use uuid::Uuid;

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::types::{
    ChatMessage, HealthState, MessageContent, TextGenerationRequest,
};
use continuum_core::inference::airc_remote::{
    AircLiveTransport, AircRemoteInferenceAdapter,
};

/// Agent name the probe attaches as. Stable so a peer can recognize it
/// in `airc network` / `airc peers` output as a known-transient probe
/// rather than a stray client.
const PROBE_AGENT_NAME: &str = "continuum-remote-inference-probe";

/// Default prompt — short, structured, with an obvious correctness
/// signal (a real model answers with the word "yes" or similar; a stub
/// produces something the operator can inspect).
const DEFAULT_PROMPT: &str =
    "Reply with exactly one short sentence: confirm you received this message.";

/// Default model identifier. Matches the floor we're targeting for L1
/// (qwen2.5-0.5b is the substrate's LCD default and the model M5's
/// serving daemon currently advertises).
const DEFAULT_MODEL: &str = "qwen2.5-0.5b-instruct";

/// Default per-request deadline. 30s comfortably covers a cold-start
/// inference on a small model + the cross-grid hop; production callers
/// will tighten this once the floor is established.
const DEFAULT_DEADLINE_MS: u64 = 30_000;

#[tokio::main]
async fn main() -> ExitCode {
    // tracing-subscriber isn't initialized — keep the probe's output
    // human-readable on stdout. Substrate-side `tracing::info!` from
    // the adapter / transport is suppressed unless the caller wires
    // a subscriber, which is fine for this one-shot tool.
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("probe: FAIL — {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let cfg = ProbeConfig::from_env()?;
    eprintln!("probe: config — {cfg}");

    // Bootstrap airc against the live daemon. attach_as opens the home,
    // mints/loads identity, then attaches to the running daemon at
    // `socket`. Fails loudly if the daemon isn't up.
    let airc = airc_lib::Airc::attach_as(
        cfg.airc_home.clone(),
        PROBE_AGENT_NAME.to_string(),
        cfg.daemon_socket.clone(),
    )
    .await
    .map_err(|e| {
        format!(
            "airc attach_as(home={:?}, socket={:?}) — is the daemon running? \
             Try `airc status` to confirm. underlying: {e}",
            cfg.airc_home, cfg.daemon_socket
        )
    })?;
    // Per airc#1222 spawn contract: publish identity card AFTER attach
    // so the probe is grounded by NAME on the wire. Fatal on failure —
    // an ungrounded probe defeats the entire point of this binary
    // (asymmetric with the persona bootstrap, which warn-and-continues
    // because a deaf persona is worse than an anonymous one).
    airc.publish_identity().await.map_err(|e| {
        format!(
            "airc publish_identity failed — probe cannot ground itself by name: {e}"
        )
    })?;
    let airc = Arc::new(airc);
    eprintln!(
        "probe: airc attached + identity published as {PROBE_AGENT_NAME} \
         (local peer_id={})",
        airc.peer_id().as_uuid()
    );

    // Construct the transport + adapter. Deadline lands on the
    // transport, not the adapter — the adapter is dumb about timing.
    // `AircLiveTransport::new` returns an `Arc<Self>` already; to apply
    // the configured deadline (via `with_deadline`, which consumes
    // `self`), unwrap the freshly-minted Arc (refcount = 1 here, so the
    // unwrap is infallible).
    let transport_arc = AircLiveTransport::new(airc.clone(), cfg.remote_peer_id);
    let transport = Arc::try_unwrap(transport_arc)
        .map_err(|_| "AircLiveTransport::new returned a shared Arc; cannot unwrap to apply deadline".to_string())?
        .with_deadline(Duration::from_millis(cfg.deadline_ms));
    let adapter = AircRemoteInferenceAdapter::new(transport);

    // Pre-call health: per the #1560 honesty fix in
    // `AircRemoteInferenceAdapter`, status is pessimistic `Unhealthy`
    // until the first successful round-trip flips it to `Healthy`. We
    // print the full HealthStatus for visibility but only assert on the
    // `status` field.
    let pre_health = adapter.health_check().await;
    eprintln!(
        "probe: pre-call adapter health = {:?} \
         (Unhealthy is correct pre-observation per #1560)",
        pre_health.status
    );

    // Build the request. Single user message + minimal config; the
    // remote peer's `ai/generate` handler decides everything else.
    // Field set matches `TextGenerationRequest` exactly — no
    // `..Default::default()` because the type doesn't derive Default,
    // which is intentional (forces every caller to make every choice
    // explicit per the no-silent-fallback rule).
    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(cfg.prompt.clone()),
            name: None,
        }],
        system_prompt: None,
        model: Some(cfg.model.clone()),
        provider: None,
        temperature: None,
        max_tokens: Some(128),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: None,
    };

    // Fire + time the round-trip. Wall-clock latency is the honest
    // measurement; the transport's deadline is what trips an error.
    let started = Instant::now();
    let result = adapter.generate_text(request).await;
    let elapsed = started.elapsed();

    match result {
        Ok(response) => {
            let text = response.text.trim().to_string();
            if text.is_empty() {
                return Err(format!(
                    "probe: round-trip OK in {:?} BUT response.text is empty — \
                     remote peer's adapter returned a non-error empty body",
                    elapsed
                ));
            }

            // Post-call health: per the contract, the first successful
            // round-trip must flip status from Unhealthy -> Healthy. If
            // it doesn't, the #1560 honesty fix regressed.
            let post_health = adapter.health_check().await;
            let health_flipped = matches!(post_health.status, HealthState::Healthy);

            println!(
                "probe: OK\n  \
                 round_trip_ms = {}\n  \
                 response_len = {} chars\n  \
                 response_preview = {:?}\n  \
                 pre_health_status = {:?}\n  \
                 post_health_status = {:?}\n  \
                 health_flipped_to_healthy = {}\n  \
                 remote_peer = {}\n  \
                 model_requested = {:?}",
                elapsed.as_millis(),
                text.chars().count(),
                truncate_for_preview(&text, 200),
                pre_health.status,
                post_health.status,
                health_flipped,
                cfg.remote_peer_id,
                cfg.model,
            );

            if !health_flipped {
                return Err(format!(
                    "probe: round-trip succeeded but adapter health stayed \
                     {:?} — #1560 honesty contract regressed",
                    post_health.status
                ));
            }
            Ok(())
        }
        Err(e) => Err(format!(
            "probe: generate_text failed after {} ms — {e:?}",
            elapsed.as_millis()
        )),
    }
}

fn truncate_for_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("…");
        out
    }
}

/// Parsed env contract. Constructed once; immutable thereafter so the
/// startup log is the canonical record of what the probe ran with.
struct ProbeConfig {
    remote_peer_id: Uuid,
    model: String,
    prompt: String,
    deadline_ms: u64,
    airc_home: PathBuf,
    daemon_socket: PathBuf,
}

impl ProbeConfig {
    fn from_env() -> Result<Self, String> {
        let remote_peer_id = match env::var("CONTINUUM_REMOTE_PEER_ID") {
            Ok(s) => Uuid::parse_str(s.trim()).map_err(|e| {
                format!(
                    "CONTINUUM_REMOTE_PEER_ID must be a UUID; got {s:?}: {e}"
                )
            })?,
            Err(_) => {
                return Err(
                    "CONTINUUM_REMOTE_PEER_ID is required \
                     (UUID of the remote peer hosting ai/generate)"
                        .to_string(),
                );
            }
        };

        let model = env::var("CONTINUUM_REMOTE_MODEL")
            .unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        let prompt = env::var("CONTINUUM_REMOTE_PROMPT")
            .unwrap_or_else(|_| DEFAULT_PROMPT.to_string());

        let deadline_ms = match env::var("CONTINUUM_REMOTE_DEADLINE_MS") {
            Ok(s) => s.trim().parse::<u64>().map_err(|e| {
                format!(
                    "CONTINUUM_REMOTE_DEADLINE_MS must be a u64 (ms); got {s:?}: {e}"
                )
            })?,
            Err(_) => DEFAULT_DEADLINE_MS,
        };

        // Home: $AIRC_HOME or $HOME/.airc. The probe attaches as a
        // transient agent in the same scope as the user's `airc` so
        // identity / trust / room context all match.
        let airc_home = match env::var_os("AIRC_HOME") {
            Some(s) => PathBuf::from(s),
            None => {
                let user_home = env::var_os("HOME")
                    .ok_or_else(|| "HOME is unset and AIRC_HOME is unset".to_string())?;
                PathBuf::from(user_home).join(".airc")
            }
        };

        // Daemon socket: explicit override, else the standard
        // `{home}/runtime/daemon.sock`. The latter matches what
        // `airc-cli`'s `default_socket_path_in` produces in the common
        // (non-deep-home) case; pathological cases require the env
        // override per the no-silent-fallback rule.
        let daemon_socket = match env::var_os("AIRC_DAEMON_SOCKET") {
            Some(s) => PathBuf::from(s),
            None => airc_home.join("runtime").join("daemon.sock"),
        };

        Ok(Self {
            remote_peer_id,
            model,
            prompt,
            deadline_ms,
            airc_home,
            daemon_socket,
        })
    }
}

impl std::fmt::Display for ProbeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "remote_peer={} model={:?} deadline_ms={} home={:?} socket={:?}",
            self.remote_peer_id,
            self.model,
            self.deadline_ms,
            self.airc_home,
            self.daemon_socket,
        )
    }
}
