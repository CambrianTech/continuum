//! Regression test for the no-CPU-fallback alpha contract (#1262 → #1275 → #1280).
//!
//! Continuum's documented contract per `project_continuum_alpha_product_bar_sensory_personas.md`
//! and `docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md` is **NO silent CPU fallback**:
//! standard personas use `SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly` and the model
//! resolver is supposed to refuse rather than fall through to CPU.
//!
//! Pre-#1280 this contract was enforced (in part) by an explicit `panic!` inside
//! `inference::model::select_best_device`. That function lived in the dead Candle
//! chain (CandleAdapter → ContinuumModel → select_best_device), unreachable from
//! `AIProviderModule::register_adapters`. #1280 deleted the chain and moved the
//! contract assertion to its actually-load-bearing site:
//!
//!   `LlamaCppConfig::default()` sets `n_gpu_layers: -1` (= "all layers on GPU").
//!   When no GPU is available, llama.cpp's own model loader hard-fails — this is
//!   the runtime mechanism that prevents CPU fallback on the production hot path.
//!
//! This test asserts the `n_gpu_layers: -1` invariant by source inspection plus the
//! ort_providers + LlamaCppAdapter assertions that survived #1280 unchanged.
//!
//! Pattern: forbidden-strings ratchet (same shape as lane F PR-2 #1129 — TS persona
//! forbidden-strings ratchet) applied to the Rust inference layer.
//!
//! Audit context:
//!   https://github.com/CambrianTech/continuum/issues/1262#issuecomment-4461757997
//!   https://github.com/CambrianTech/continuum/issues/1280#issuecomment-4462181316

const LLAMACPP_BACKEND_SOURCE: &str = include_str!("../src/inference/backends/llamacpp.rs");

const ORT_PROVIDERS_SOURCE: &str = include_str!("../src/inference/ort_providers.rs");

const LLAMACPP_ADAPTER_SOURCE: &str = include_str!("../src/inference/llamacpp_adapter.rs");

// Candle-side sources surfaced by #1316 ALPHA-GAP finding #5: the
// no_cpu_fallback contract test originally covered only llama.cpp +
// ORT. The Candle / inference-grpc / orpheus / residency-gate paths
// shipped their own no-CPU-fallback guarantees in PRs #1312, #1314,
// #1331, #1335, #1338 — but the contract test didn't enforce them,
// so a future regression could silently re-add a CPU fallback to any
// of those paths without breaking this gate. The constants below close
// that hole.

const INFERENCE_GRPC_MODEL_SOURCE: &str = include_str!("../../inference-grpc/src/model.rs");

const ORPHEUS_TTS_SOURCE: &str = include_str!("../src/live/audio/tts/orpheus.rs");

const RESIDENCY_GATE_SOURCE: &str = include_str!("../src/inference_capability/residency.rs");

const ENFORCEMENT_SOURCE: &str = include_str!("../src/inference_capability/enforcement.rs");

const HW_PROBE_SOURCE: &str = include_str!("../src/inference_capability/hw_probe.rs");

// Cross-node (offload) GPU-first guardrail — the 9th path. "GPU-first
// ALWAYS" promoted from per-node to ACROSS-node: a weak node offloading a
// heavy job must land on a GPU peer, never a CPU-only peer; if only CPU
// peers are reachable the routing mechanism must REFUSE, not silently
// degrade a remote peer to CPU. These constants pin the eligibility
// predicate so a future PR can't drop it (or route heavy offload through
// the GPU-agnostic `find_capable`) without breaking this gate.
const CAPABILITY_TYPES_SOURCE: &str = include_str!("../src/inference_capability/types.rs");

const CAPABILITY_REGISTRY_SOURCE: &str = include_str!("../src/inference_capability/registry.rs");

#[test]
fn llamacpp_default_config_requires_full_gpu_offload() {
    // The production load path is `LlamaCppConfig::default()` →
    // `LlamaCppBackend::load(config)` → llama.cpp `Model::load_from_file`.
    // `n_gpu_layers: -1` means "put ALL layers on the GPU" — when no GPU
    // is available, llama.cpp's loader returns an error rather than
    // silently running on CPU.
    //
    // If a future PR changes the default to a positive integer (partial
    // offload) or to 0 (CPU-only), the no-CPU-fallback alpha contract is
    // broken on the production hot path. This assertion stops that from
    // shipping.

    assert!(
        LLAMACPP_BACKEND_SOURCE.contains("n_gpu_layers: -1"),
        "LlamaCppConfig::default() must set n_gpu_layers: -1 (all layers on GPU) so llama.cpp \
         loud-fails on no-GPU hosts rather than silently running on CPU. If you changed it, \
         update both this test and docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md. \
         A partial-offload or CPU-only default was the bug #1262 was filed for."
    );
}

#[test]
fn ort_providers_documents_no_cpu_fallback_contract() {
    // ort_providers.rs carries the same contract for the ORT consumer
    // (embedding / TTS / STT / vision via ONNX Runtime). The doc string
    // must remain present so the architectural rule is discoverable from
    // source alone.

    assert!(
        ORT_PROVIDERS_SOURCE.contains("CPU fallback is forbidden"),
        "ort_providers.rs must document 'CPU fallback is forbidden' for the ORT consumer. \
         If you removed the comment, the no-CPU-fallback rule is no longer self-documenting \
         from source — surface the rule in another way before removing the comment."
    );
}

#[test]
fn llamacpp_adapter_uses_loud_fail_for_no_local_model() {
    // The production adapter must use the typed `NoLocalModelLoadable` error
    // (shipped in #1093 / lane A PR-2) rather than a silent fallthrough when
    // no local GGUF is on disk.

    assert!(
        LLAMACPP_ADAPTER_SOURCE.contains("NoLocalModelLoadable"),
        "LlamaCppAdapter must use the typed NoLocalModelLoadable error for missing-model cases. \
         If you replaced it with a silent skip / Result::Ok-with-None / log-and-continue, \
         the no-fallback alpha contract is violated and the user gets 1 tok/sec CPU instead \
         of a clear 'install missing artifact' error."
    );
}

// ─── Candle-side / inference-grpc / orpheus / residency gate ─────────
//
// All assertions below close gaps surfaced by #1316 ALPHA-GAP finding
// #5. Each pins a load-bearing guarantee that's already shipped (PRs
// cited in each assertion). They aren't new behavior — they're the
// canary in the coal mine that catches a future PR re-introducing a
// CPU fallback in any of these layers.

#[test]
fn inference_grpc_select_best_device_hard_fails_on_no_gpu() {
    // Shipped in #1314 (post-canary by codex). The function previously
    // returned `Device::Cpu` silently with a friendly "no GPU
    // acceleration" log when neither CUDA nor Metal could open. That's
    // the exact pattern Joel + vhsm-d1f4 audit pass 1 flagged. The fix:
    // return `Err` with "GPU required, no CPU fallback" in the message.

    assert!(
        INFERENCE_GRPC_MODEL_SOURCE.contains("GPU required, no CPU fallback")
            || INFERENCE_GRPC_MODEL_SOURCE.contains("no CPU fallback"),
        "inference-grpc/src/model.rs must hard-fail on no-GPU with the 'no CPU fallback' \
         contract phrase in the error message. If you removed the message, the only-CPU \
         host now silently runs at ~1 tok/s — the exact bug #1314 fixed."
    );
    // Additionally pin the return-type shape: select_best_device must
    // return Result, not Device. A return-type regression would let
    // someone silently re-add Device::Cpu as the "Ok" fallback.
    assert!(
        INFERENCE_GRPC_MODEL_SOURCE.contains("fn select_best_device")
            && (INFERENCE_GRPC_MODEL_SOURCE.contains("fn select_best_device() -> Result<Device")
                || INFERENCE_GRPC_MODEL_SOURCE
                    .contains("fn select_best_device() -> Result <Device")),
        "select_best_device must return Result<Device, ...>. If you changed the signature \
         back to -> Device, the function can silently return Device::Cpu and the no-CPU-fallback \
         contract is broken at the type level."
    );
}

#[test]
fn orpheus_tts_select_device_hard_fails_on_no_metal() {
    // Shipped in #1312 (codex's orpheus follow-on to #1314's pattern).
    // The TTS path silently fell back to CPU when Metal was
    // unavailable; now it returns TTSError::ModelNotLoaded so the
    // caller sees the broken state instead of getting choppy CPU TTS.

    assert!(
        ORPHEUS_TTS_SOURCE.contains("fn select_device") && ORPHEUS_TTS_SOURCE.contains("TTSError"),
        "orpheus.rs select_device must return Result<Device, TTSError> and refuse to fall \
         back to CPU. If you removed the Result return type or the TTSError variant, \
         the TTS path silently CPU-degrades — the exact bug #1312 fixed."
    );
}

#[test]
fn residency_gate_emits_no_gpu_block_reason() {
    // Shipped in #1331 (CBAR-PIECE-5 PR-1). The pure gate defines a
    // typed BlockReason variant NoGpuBackendOnNode that fires when no
    // GPU is detected. The gate's job is to refuse the turn rather
    // than let llama.cpp silently split layers to CPU — same
    // architectural rule, one layer up from the llamacpp_default
    // contract.

    assert!(
        RESIDENCY_GATE_SOURCE.contains("NoGpuBackendOnNode"),
        "residency.rs must define BlockReason::NoGpuBackendOnNode so the gate has a typed \
         way to surface 'no GPU, refuse the turn' to callers. If you removed the variant, \
         the gate has no way to express the alpha-contract failure mode."
    );

    // PartialGpuSplit is the OTHER half — when there IS a GPU but it
    // doesn't have enough VRAM for the model. llama.cpp would split
    // layers to CPU; the gate must refuse instead.
    assert!(
        RESIDENCY_GATE_SOURCE.contains("PartialGpuSplit"),
        "residency.rs must define BlockReason::PartialGpuSplit so the gate refuses turns \
         where the model would partially spill to CPU. Removal would let llama.cpp silently \
         split — the exact CBAR-SUBSTRATE §336 piece #5 anti-pattern."
    );
}

#[test]
fn enforcement_module_exists_and_composes_the_three_layers() {
    // Shipped in #1338 (CBAR-PIECE-5 PR-4). The enforcement helper
    // composes hw_probe + read_qwen_model_metadata + check_residency_gate
    // into one typed function. Removing it would leave callers to
    // re-compose by hand — every adapter would need to remember the
    // ordering, which is the path to silent regressions.

    assert!(
        ENFORCEMENT_SOURCE.contains("pub fn enforce_residency"),
        "inference_capability/enforcement.rs must export enforce_residency(model_path) \
         as the composed before-turn helper. If you removed it, callers can't reliably \
         enforce the gate without re-implementing the composition."
    );
    assert!(
        ENFORCEMENT_SOURCE.contains("probe_hardware_profile")
            && ENFORCEMENT_SOURCE.contains("read_qwen_model_metadata")
            && ENFORCEMENT_SOURCE.contains("check_residency_gate"),
        "enforcement.rs must compose probe_hardware_profile + read_qwen_model_metadata + \
         check_residency_gate. Any one of these missing means the gate fires with stale \
         or fabricated data."
    );
}

#[test]
fn llamacpp_adapter_wires_residency_gate_at_load_time() {
    // Shipped in #1338. The adapter calls enforce_residency BEFORE
    // LlamaCppBackend::load. Removing the call would let llama.cpp's
    // own loader try to put all layers on a non-existent GPU; while
    // llama.cpp's n_gpu_layers: -1 contract (asserted above) still
    // catches the catastrophic case, the typed enforce_residency
    // catches the subtler case where there IS a GPU but the model
    // won't fit — and surfaces a typed BlockReason for telemetry.

    assert!(
        LLAMACPP_ADAPTER_SOURCE.contains("enforce_residency"),
        "LlamaCppAdapter must call enforce_residency before LlamaCppBackend::load so the \
         typed ResidencyBlock fires for the 'GPU exists but model won't fit' case. \
         Removal would silently allow partial-spill turns that llama.cpp's n_gpu_layers: -1 \
         catches less gracefully."
    );
}

#[test]
fn hw_probe_does_not_introduce_cpu_fallback() {
    // Shipped in #1335 (CBAR-PIECE-5 PR-3). The hardware probe must
    // NEVER panic + must return all-flags-false when no GPU is
    // available — so the residency gate downstream surfaces
    // NoGpuBackendOnNode. A "fall back to CPU if no GPU" branch in
    // the probe would defeat the entire gate (it would lie about
    // what's available).

    assert!(
        HW_PROBE_SOURCE.contains("Probe NEVER panics")
            || HW_PROBE_SOURCE.contains("never panics")
            || HW_PROBE_SOURCE.contains("probe NEVER panics"),
        "hw_probe.rs must document its never-panic contract — the probe is called from \
         supervisor + adapter init code, panicking there crashes the process. Comment \
         is also the contract for reviewers: don't add a panic path here."
    );
    // Pure-functions test: build_hardware_profile must be a pub fn so
    // the gate composition can call it from tests / mocks without
    // needing to hit real hardware.
    assert!(
        HW_PROBE_SOURCE.contains("pub fn build_hardware_profile"),
        "hw_probe.rs must expose build_hardware_profile so the residency gate can be tested \
         with synthetic profiles. Privatizing it would force every test to hit real \
         hardware — flaky + slow + wrong shape."
    );
}

#[test]
fn cross_node_offload_has_gpu_first_guardrail() {
    // 9th no-CPU-fallback path: the OFFLOAD path. "GPU-first ALWAYS"
    // promoted from per-node to ACROSS-node. The per-node gates above stop
    // a single box from running CPU inference; this stops a weak box from
    // OFFLOADING a heavy job onto a CPU-only peer over the grid. The
    // guardrail is a GPU-class eligibility predicate that makes CPU-only
    // peers ineligible offload targets — so when no GPU peer is reachable
    // the routing mechanism must REFUSE rather than silently CPU-serve.
    // (Behavioral tests live in inference_capability/{types,registry}.rs;
    // these source assertions are the ratchet that stops a silent removal.)

    assert!(
        CAPABILITY_TYPES_SOURCE.contains("fn has_gpu"),
        "HardwareProfile must expose has_gpu() (Metal || CUDA || Vulkan) — the eligibility \
         predicate for the cross-node offload guardrail. Removing it (or narrowing it to \
         has_metal only) lets a heavy job offload onto a CPU-only or mis-classified peer: \
         the across-node 'GPU-first ALWAYS' violation."
    );
    assert!(
        CAPABILITY_REGISTRY_SOURCE.contains("fn find_gpu_capable"),
        "NodeCapabilityRegistry must expose find_gpu_capable() — the GPU-first offload \
         selector that filters out CPU-only peers. If you removed it, or routing started \
         using the GPU-agnostic find_capable() for heavy OFFLOAD selection, a weak node can \
         silently offload to a CPU peer. Re-add the guardrail (and its registry tests) \
         before removing this assertion."
    );
}
