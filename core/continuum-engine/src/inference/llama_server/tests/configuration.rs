//! Configuration regression fixtures.
#![cfg(test)]
use super::*;
// what this catches: the slice-1 K3 -ot builder — the LAYER-granular buft-override that
// physically pages MoE experts. Cold = (0..n_layers) minus hot; the value keys on the
// real blk.N with `\.` anchors (so blk.3 never matches blk.31), covers all four stacked
// ffn_*_exps projections, and targets CPU. None when nothing is cold (empty -ot is
// rejected by llama-server). A regression here = the wrong experts offloaded (a stale
// plan degrading throughput) or a spawn that 500s on a malformed override.
#[test]
fn cold_expert_offload_ot_emits_inverse_of_hot_layers() {
    // Mixed hot/cold: layers 0,2,4 fit GPU → 1,3,5 offload to CPU.
    let v = cold_expert_offload_ot(&[0, 2, 4], 6).expect("some cold → an override");
    assert_eq!(v, r"blk\.(1|3|5)\.ffn.*_exps=CPU");
    // Anchoring intent: the block index is fenced by `\.` on both sides.
    assert!(
        v.contains(r"blk\.(1|3|5)\.ffn"),
        "block index must be dot-anchored"
    );

    // Everything hot → no override at all (NOT an empty string, which llama-server rejects).
    assert_eq!(cold_expert_offload_ot(&[0, 1, 2, 3], 4), None);
    // Zero layers → nothing to place.
    assert_eq!(cold_expert_offload_ot(&[], 0), None);
    // Empty hot set → offload every block (the all-cold extreme).
    assert_eq!(
        cold_expert_offload_ot(&[], 3),
        Some(r"blk\.(0|1|2)\.ffn.*_exps=CPU".to_string())
    );
    // A hot index outside the block range is ignored, never forcing a bogus pattern.
    assert_eq!(
        cold_expert_offload_ot(&[99], 2),
        Some(r"blk\.(0|1)\.ffn.*_exps=CPU".to_string())
    );
}

// what this catches: host:port parsing for the launch args, including the
// /v1-stripped root and a missing port falling back to the default.
#[test]
fn split_host_port_parses_root() {
    assert_eq!(
        split_host_port("http://127.0.0.1:58057"),
        ("127.0.0.1".into(), 58057)
    );
    assert_eq!(
        split_host_port("http://0.0.0.0:9090"),
        ("0.0.0.0".into(), 9090)
    );
    assert_eq!(
        split_host_port("http://localhost"),
        ("localhost".into(), DEFAULT_PORT)
    );
}

// what this catches: serving_root normalizes a configured `.../v1` back to
// the bare root so /health and /v1 are derived from one source of truth.
// (No env set in test → default; the strip logic is exercised by the unit
// below feeding the same normalization the reader uses.)
#[test]
fn root_normalization_strips_v1() {
    // Mirror the normalization serving_root applies to a configured value.
    let normalize = |raw: &str| {
        let trimmed = raw.trim().trim_end_matches('/');
        trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
    };
    assert_eq!(normalize("http://h:1/v1"), "http://h:1");
    assert_eq!(normalize("http://h:1/v1/"), "http://h:1");
    assert_eq!(normalize("http://h:1"), "http://h:1");
}

// what this catches: first_free_port must SKIP a port already bound and
// return a different, bindable one — the whole point of "scan, never assume
// free". A regression to "return base unconditionally" would wedge the bind
// whenever the preferred port is taken (the Studio-squats-58057 case).
#[test]
fn first_free_port_skips_a_taken_port() {
    // Bind an ephemeral port so we KNOW one is taken, then scan from it.
    let taken = std::net::TcpListener::bind((DEFAULT_HOST, 0)).expect("bind ephemeral");
    let taken_port = taken.local_addr().unwrap().port();
    let chosen = first_free_port(taken_port);
    assert_ne!(chosen, taken_port, "must not return the held port");
    // And the returned port must itself be bindable right now.
    assert!(
        std::net::TcpListener::bind((DEFAULT_HOST, chosen)).is_ok(),
        "scanned port {chosen} should be free"
    );
}

// what this catches: an ephemeral lane must derive its `/v1` url from the
// EXPLICIT root it was handed (not the global serving_root), and must not
// double up the scheme or the `/v1` segment — the same `/v1/v1` class of bug
// that muted Asha. `with_root` is the seam EphemeralServingLane builds on.
#[test]
fn with_root_derives_v1_url_from_explicit_root() {
    let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string());
    assert_eq!(proc.root, "http://127.0.0.1:59123");
    assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
    // a trailing slash on the root must not yield `//v1`.
    let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123/".to_string());
    assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
}

// what this catches: THE seam that binds the orphan-reclaim machinery to
// exactly one process. The live constructors (`new`/`with_client`) must mark
// `is_live_lane` so they pin the canonical port + own the reclaim pidfile;
// `with_root` (the EphemeralServingLane seam) must NOT — an ephemeral lane that
// wrote the canonical pidfile or reaped the live port would kill the living
// persona's own server. A regression flipping either default reintroduces the
// two-servers-on-one-GPU OOM (live not pinning) or self-reaping (ephemeral
// claiming the live role).
#[test]
fn live_constructors_are_live_lane_ephemeral_is_not() {
    assert!(
        LlamaServerProcess::new().is_live_lane,
        "new() is THE live lane — must pin the canonical port + own the pidfile"
    );
    assert!(
        LlamaServerProcess::with_client(reqwest::Client::new()).is_live_lane,
        "with_client() is a live constructor"
    );
    assert!(
        !LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string()).is_live_lane,
        "with_root() is ephemeral — must never write the canonical pidfile or reclaim the live port"
    );
}

// what this catches: the live lane PINS the canonical port — it must never
// scan away from DEFAULT_PORT. Scanning past a held canonical port was the bug
// (spawned a GPU competitor AND blinded the daemon's own probe). The pin is
// what makes a crashed predecessor's orphan reclaimable on a known port. The
// ephemeral scan (first_free_port) is tested separately above and stays.
#[test]
fn live_lane_pins_the_canonical_port() {
    assert_eq!(
        chosen_port(),
        DEFAULT_PORT,
        "the live lane must bind the canonical port, never a scanned one"
    );
}

// what this catches: the readiness gate `await_ready_serving` waits on must
// require BOTH `ready` AND an `active_model` — a regression to `ready` alone
// (or `ready || ...`) would resolve a lease against a `None` model and bind
// a persona to nothing. Exercised on a LOCAL watch so it doesn't mutate the
// process-global SERVING_STATE (set-once → would race other tests).
#[tokio::test]
async fn ready_predicate_requires_ready_and_a_model() {
    let pred = |s: &ServingSnapshot| s.ready && s.active_model.is_some();
    let (tx, mut rx) = watch::channel(ServingSnapshot::empty());

    // empty: not ready, no model → unsatisfied.
    assert!(!pred(&rx.borrow()));
    // ready but no model → STILL unsatisfied (the bug we guard against).
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: None,
        ready: true,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    assert!(!pred(&rx.borrow()));
    // not-ready but has a model → unsatisfied.
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder".into()),
        ready: false,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    assert!(!pred(&rx.borrow()));
    // ready AND a model → satisfied, and wait_for resolves to it at once.
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder".into()),
        ready: true,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    let got = tokio::time::timeout(Duration::from_millis(100), rx.wait_for(pred))
        .await
        .expect("should not time out")
        .expect("sender live");
    assert_eq!(got.active_model.as_deref(), Some("coder"));
}

// what this catches (2026-09-18, the 5090 sightless with Qwen3.8-27B's projector on
// disk): the sidecar is a prosthesis for text-only minds. A row that prefers a
// sidecar withholds its projector ONLY when another VL candidate is on the node;
// with none, the main lane wears its own eyes — never blind because of a cache
// knob. Opt-in rows and the sidecar lane itself always wear; nothing resolved is
// nothing to decide; a row that prefers a sidecar but was not consulted errs toward
// sight.
#[test]
fn the_only_eyes_on_the_node_go_on_the_main_lane() {
    use MainLaneMmproj::*;
    assert_eq!(main_lane_mmproj_decision(false, false, true, false, Some(true)), Withhold);
    assert_eq!(main_lane_mmproj_decision(false, false, true, false, Some(false)), WearAsOnlyEyes);
    assert_eq!(main_lane_mmproj_decision(false, false, true, false, None), WearAsOnlyEyes);
    assert_eq!(main_lane_mmproj_decision(true, false, true, false, Some(true)), Wear);
    assert_eq!(main_lane_mmproj_decision(false, true, true, false, Some(true)), Wear);
    assert_eq!(main_lane_mmproj_decision(false, false, false, false, Some(false)), NoneResolved);
    // A hybrid (recurrent memory cannot shift) wears its eyes even with a sidecar on
    // the node: cache_reuse is inert there, so the withhold would buy only blindness
    // (2026-10-25, the M5 on Qwen3.6: withheld, then the sidecar yielded — blind).
    assert_eq!(main_lane_mmproj_decision(false, false, true, true, Some(true)), WearReuseInert);
    assert_eq!(main_lane_mmproj_decision(false, false, true, true, None), WearReuseInert);
    assert!(WearReuseInert.wears());
    // And the readiness check names a withheld/absent projector as what it is —
    // never "--mmproj was passed" about a lane that was given none.
    let err = vision_lane_ready(true, false, None).unwrap_err();
    assert!(err.contains("carries no mmproj"), "{err}");
    assert!(!err.contains("was passed"), "{err}");
}

// what this catches: the #106 vision-readiness verdict must only claim sight when
// ALL THREE facts line up — the row declares Vision, an mmproj resolved at spawn,
// AND the running server's own /props confirms modalities.vision. A regression that
// returns Ok(true) from a weaker combination publishes `vision_ready: true` for a
// lane that silently drops image parts — the exact capability lie the observe path
// gates on ([[fallbacks-are-illegal-fail-loud]]).
#[test]
fn vision_lane_ready_requires_declaration_projector_and_props_confirmation() {
    let sees = MultimodalSupport {
        vision: true,
        audio: false,
    };
    let blind = MultimodalSupport {
        vision: false,
        audio: true,
    };
    // Text lane: never declared Vision → not vision-ready, and NOT an error.
    assert_eq!(vision_lane_ready(false, false, None), Ok(false));
    assert_eq!(vision_lane_ready(false, true, Some(sees)), Ok(false));
    // Declared Vision but no projector resolved → loud error naming the gap.
    let err = vision_lane_ready(true, false, None).unwrap_err();
    assert!(err.contains("carries no mmproj projector"), "{err}");
    // Projector passed, server confirms vision → the ONLY Ok(true) path.
    assert_eq!(vision_lane_ready(true, true, Some(sees)), Ok(true));
    // Projector passed but the server says it can't see (wrong/failed mmproj).
    let err = vision_lane_ready(true, true, Some(blind)).unwrap_err();
    assert!(err.contains("modalities.vision=false"), "{err}");
    // Server build reports no modalities block: UNVERIFIED must not read as sight.
    let err = vision_lane_ready(true, true, None).unwrap_err();
    assert!(err.contains("cannot VERIFY"), "{err}");
}

// what this catches: a WEDGED server (port held open, never answers HTTP — a
// GPU hang, OOM-thrash, deadlock, or a SIGSTOP'd orphan from a crashed
// predecessor) must not hang `active_model` forever. Without the per-request
// PROBE_TIMEOUT the probe blocks indefinitely: the TCP connect succeeds against
// the held socket, then the read never returns, so `ensure_model_serving` never
// returns and the reconcile that would reclaim the sick lane never runs — the
// exact silent-hang that wedged a successor core. The bound converts "wedged"
// into a fast `Unreachable`, which serve()+reclaim acts on. We assert it
// returns (Err) well under an unbounded "forever" — if the timeout regressed to
// unbounded this test would hang and the runner would kill it. Regression for
// the unbounded control-plane probe (#90 reliability follow-up).
#[tokio::test]
async fn wedged_server_probe_times_out_instead_of_hanging() {
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    // A listener that ACCEPTS then holds the socket silent — the precise wedge a
    // frozen llama-server produces (connect ok, read never answered).
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let _holder = tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            // Drain quietly and never reply; hold the conn open past the probe.
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    let proc = LlamaServerProcess::with_root(format!("http://127.0.0.1:{port}"));
    let started = Instant::now();
    let result = proc.active_model().await;
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(LlamaServerError::Unreachable(_))),
        "wedged server must surface as Unreachable, got {result:?}"
    );
    assert!(
        elapsed < PROBE_TIMEOUT + Duration::from_secs(4),
        "probe must be bounded by PROBE_TIMEOUT, took {elapsed:?}"
    );
}
// what this catches: the host's CPU placement decision must be one config key, read
// once, defaulting to the GPU — never a raw env var and never a spelling guess.
#[test]
fn main_lane_placement_reads_the_config_decision() {
    assert_eq!(placement_from_config(Some("cpu")), LanePlacement::Cpu);
    assert_eq!(placement_from_config(Some(" CPU ")), LanePlacement::Cpu);
    assert_eq!(placement_from_config(Some("gpu")), LanePlacement::Gpu);
    assert_eq!(placement_from_config(None), LanePlacement::Gpu);
}
