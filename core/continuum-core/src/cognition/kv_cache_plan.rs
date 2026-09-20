//! **The KV cache type is a DECISION this substrate makes, not a variable a human
//! once exported.**
//!
//! # The defect this module exists to kill
//!
//! Until 2026-09-20 the KV cache type was read straight out of `config.env` at two
//! places that never met:
//!
//! * `inference/llama_server.rs` — `SERVING_KV_CACHE_TYPE` unset → no `--cache-type-k`
//!   / `--cache-type-v` flag at all, so the engine ran its f16 default
//!   (65,536 bytes/token for the 27B coder row);
//! * `modules/serving_daemon.rs` — the plan's resident-KV divisor came from the SAME
//!   key, so the fit math was *self-consistent* and simply planned half the capacity.
//!
//! Self-consistent and half-size is the worst shape a defect can take: nothing ever
//! disagreed, so nothing ever complained. The M5 had the key set by hand (its engine
//! runs `--cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on`, two lanes at
//! 67,072 tokens each). The 5090 never had it — and therefore planned AND served
//! half-size lanes its entire life; Joel measured a **26,880-token lane** on that box
//! on 2026-09-20 where the same model on the same-size budget serves ~2× that.
//!
//! A load-bearing decision that only works where a human once exported a variable is
//! not a decision (CLAUDE.md: *reliable out of the box, on everyone's machine*; no
//! env-var-tuned substrate thresholds). So the substrate decides, from the backend it
//! actually runs on, and the env keys become an operator **override** that is honored
//! and said out loud.
//!
//! # The decision
//!
//! **Ask the engine first.** The serving binary's own `--help` names the values its
//! build accepts for `--cache-type-k` ([`parse_engine_kv_support`]); when it answers,
//! THAT is the capability — a table in our source can only ever be a stale guess about
//! someone else's build. The launcher probes it once per process, bounded, with a
//! named outcome, and records it here.
//!
//! **The table is the fallback** for an engine that does not advertise (an older help
//! text, a probe that did not answer):
//!
//! | backend | cache type | flash attention | divisor |
//! |---|---|---|---|
//! | metal, cuda, **cpu** | `q8_0` | on | 2 |
//! | rocm, vulkan, directml, unknown | `f16` | off | 1 |
//!
//! CPU is in the quantized column deliberately. llama.cpp's quantized-KV path is not
//! GPU-only, and the third box in this fleet — Cormac's IntelMac, which serves on the
//! CPU by plan (#3729, `--n-gpu-layers 0`) — was ALSO running the f16 default with no
//! key in its `config.env`: a 1.5B at `-c 32768 --parallel 1` on ~15 GB usable, i.e.
//! half of the 1 × 64k (or 2 × 32k) it affords. Leaving the weakest box in the fleet
//! at half capacity is the exact opposite of this module's point.
//!
//! Quantized KV **requires** fused attention — llama.cpp's quantized-KV kernels ride
//! the flash-attention path. So the two are ONE choice ([`KvCachePlan`]), never two
//! independent reads: choosing `q8_0` without `--flash-attn on` is a misconfiguration,
//! not a half-fix.
//!
//! rocm / vulkan / directml stay conservative until someone MEASURES them there
//! (`[[verify-real-device-numbers-not-a-clamp-premise]]`) — and an engine that
//! advertises q8_0 lifts them without a source change. f16 on such a box is a DECISION
//! and it is probed as one (`serving.kv_cache.decided`, `backend=vulkan`,
//! `engine_support=unknown`), never a silence.
//!
//! # One resolved value, two consumers
//!
//! [`resolve`] is the single chokepoint. `llama_server` takes its launcher flags from
//! it and `serving_daemon` takes the plan's resident-KV divisor from the SAME struct,
//! so the flag and the fit math cannot disagree by construction — which is exactly the
//! property the two independent `config_env::read` calls did not have.

use std::sync::OnceLock;

/// f16 — llama.cpp's own default. Passing it is byte-identical to passing nothing, so
/// the launcher omits the flag entirely (see [`KvCachePlan::launcher_cache_type`]).
pub const F16: &str = "f16";

/// q8_0 — ~half the resident KV bytes at near-lossless quality. The decided type on
/// every backend whose kernels support it.
pub const Q8_0: &str = "q8_0";

/// Which compute backend the serving engine loads on this host.
///
/// Named from the SAME strings [`crate::gpu::device_probe::GpuDeviceProbe::platform`]
/// returns and [`crate::inference::backend_receipt::BackendVerdict::backend_label`]
/// reports, so there is one vocabulary for "what is this box" across the substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingBackend {
    /// Apple Silicon / Metal.
    Metal,
    /// NVIDIA CUDA.
    Cuda,
    /// AMD ROCm / HIP.
    Rocm,
    /// Vulkan (AMD/Intel Linux, MoltenVK-in-container).
    Vulkan,
    /// Windows DirectML.
    DirectMl,
    /// The lane runs on the CPU — by plan (`CONTINUUM_SERVING_PLACEMENT=cpu`, the
    /// Intel-Mac arm of #3729) or because no GPU backend was ever recorded.
    Cpu,
    /// No backend fact has reached this process. Decides f16 — the pre-existing
    /// default — and is reported distinctly in the receipt, because "nobody told us"
    /// is not "we measured a CPU". An engine that advertises its own `--cache-type-k`
    /// values still lifts this case (the advertisement outranks the table).
    Unknown,
}

impl ServingBackend {
    /// Parse a platform string (`"metal"`, `"cuda"`, `"vulkan"`, `"mlx"`, …). Anything
    /// unrecognised is [`ServingBackend::Unknown`] — never a guess.
    pub fn from_platform(platform: &str) -> Self {
        match platform.trim().to_ascii_lowercase().as_str() {
            // MLX is Apple Silicon: the llama.cpp lane beside it is still Metal.
            "metal" | "mtl" | "mlx" => Self::Metal,
            "cuda" | "nvidia" => Self::Cuda,
            "rocm" | "hip" => Self::Rocm,
            "vulkan" => Self::Vulkan,
            "directml" | "dml" => Self::DirectMl,
            "cpu" => Self::Cpu,
            _ => Self::Unknown,
        }
    }

    /// The one-word label the receipt carries.
    pub fn label(self) -> &'static str {
        match self {
            Self::Metal => "metal",
            Self::Cuda => "cuda",
            Self::Rocm => "rocm",
            Self::Vulkan => "vulkan",
            Self::DirectMl => "directml",
            Self::Cpu => "cpu",
            Self::Unknown => "unknown",
        }
    }

    /// THE FALLBACK TABLE — used only when the engine did not advertise its own
    /// `--cache-type-k` values (see [`parse_engine_kv_support`]).
    ///
    /// Metal, CUDA and **CPU** ship llama.cpp's quantized-KV kernels and the fused
    /// attention path they ride on. CPU is in this list on purpose: the IntelMac serves
    /// on the CPU by plan and was running the f16 default at half its KV budget
    /// (2026-09-20, Cormac) — a fallback table that excluded it would have left the
    /// weakest box in the fleet exactly where the defect found it.
    ///
    /// rocm / vulkan / directml builds vary in whether they ship those kernels at all,
    /// so they stay f16 until the engine says otherwise or someone measures them
    /// (`[[verify-real-device-numbers-not-a-clamp-premise]]`). `Unknown` means no
    /// backend fact ever reached this process: that is not a licence to guess.
    pub fn supports_quantized_kv(self) -> bool {
        matches!(self, Self::Metal | Self::Cuda | Self::Cpu)
    }
}

/// Where the resolved plan came from — the receipt's `source` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvPlanSource {
    /// The substrate decided it from the backend. No operator involved.
    Decided,
    /// An operator key in `config.env` moved it off the decision. Honored, and named.
    Override,
}

impl KvPlanSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decided => "decided",
            Self::Override => "override",
        }
    }
}

/// ONE coherent KV choice: the cache type, the attention path it requires, and the
/// resident-KV divisor the serving plan sizes its window with.
///
/// The three travel together because they are one decision. The launcher reads
/// [`Self::launcher_cache_type`] + [`Self::flash_attn`]; the plan reads
/// [`Self::bytes_per_token_divisor`]. Both from this struct, so a lane can never be
/// launched f16 while the plan budgets q8_0 (or the 5090's inverse: planned AND served
/// at half the capacity the box affords, invisibly, for months).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvCachePlan {
    /// `"f16"` | `"q8_0"` | whatever an operator override named.
    pub cache_type: String,
    /// `--flash-attn on`. Required whenever `cache_type` is quantized.
    pub flash_attn: bool,
    /// Divide the artifact's declared f16 bytes/token by this to get what the lane
    /// will ACTUALLY hold per token.
    pub bytes_per_token_divisor: u64,
    /// Decided by the substrate, or moved by an operator key.
    pub source: KvPlanSource,
    /// The backend the decision was made against.
    pub backend: ServingBackend,
}

impl KvCachePlan {
    /// The `--cache-type-k` / `--cache-type-v` value to pass, or `None` for f16.
    ///
    /// f16 IS llama.cpp's default, so omitting the flag is byte-identical to passing
    /// it — and omitting keeps the invocation identical to every pre-decision launch
    /// on an unsupported backend.
    pub fn launcher_cache_type(&self) -> Option<&str> {
        if self.cache_type == F16 {
            None
        } else {
            Some(self.cache_type.as_str())
        }
    }

    /// What one token of KV costs this lane, given the artifact's declared f16 rate.
    pub fn bytes_per_token(&self, f16_rate: u64) -> u64 {
        (f16_rate / self.bytes_per_token_divisor.max(1)).max(1)
    }
}

/// Is this cache type quantized (i.e. does it need the fused-attention path)?
pub fn is_quantized(cache_type: &str) -> bool {
    let ct = cache_type.trim().to_ascii_lowercase();
    !ct.is_empty() && !matches!(ct.as_str(), "f16" | "f32" | "bf16")
}

/// Pure KV-rate divisor for a cache-type string. CONSERVATIVE by design: q8_0 ≈ half
/// of f16 → 2; q4_0/q4_1 ≈ a third → 3 (under the ideal ~3.5×, so the plan never
/// over-grows the window past the real KV and OOMs). Anything else / f16 → 1.
/// Over-reserve is a smaller window (safe); under-reserve is an OOM (fatal).
pub fn kv_divisor_for(cache_type: Option<&str>) -> u64 {
    match cache_type.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("q8_0") => 2,
        Some("q4_0") | Some("q4_1") => 3,
        _ => 1,
    }
}

/// `&s[start..start+len]`, trimmed back to a char boundary. Help text is not
/// guaranteed ASCII and a raw byte slice would panic on a multi-byte character.
fn clamped(s: &str, start: usize, len: usize) -> &str {
    let mut end = start.saturating_add(len).min(s.len());
    while end > start && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[start..end]
}

/// What the SERVING BINARY says about its own quantized-KV support, parsed from its
/// `--help`. `Some(true)` = it lists `q8_0` among `--cache-type-k`'s allowed values AND
/// carries `--flash-attn`; `Some(false)` = the build has no `--cache-type-k` flag at
/// all; `None` = it did not say (an older help text, a probe that did not answer) —
/// and `None` is never read as "no", it falls through to the backend table.
///
/// Pure, so the real help transcripts are assertable without spawning anything.
pub fn parse_engine_kv_support(help: &str) -> Option<bool> {
    let lower = help.to_ascii_lowercase();
    let Some(at) = lower.find("--cache-type-k") else {
        // The flag does not exist in this build. That is an answer, not a silence.
        return Some(false);
    };
    // The allowed values follow the flag, on the flag's line or the next one:
    //   -ctk, --cache-type-k TYPE   KV cache data type for K
    //                               allowed values: f32, f16, bf16, q8_0, q4_0, ...
    let window = clamped(&lower, at, 600);
    let Some(values_at) = window.find("allowed values:") else {
        // The flag is there but the build does not enumerate its types — do NOT guess
        // either way; the backend table decides.
        return None;
    };
    let values = clamped(window, values_at, 240);
    // Quantized KV rides the fused-attention path; a build without `--flash-attn`
    // cannot run the pair coherently, so it does not count as advertising support.
    Some(values.contains(Q8_0) && lower.contains("--flash-attn"))
}

/// The engine's own answer, recorded once per process by the launcher.
static ENGINE_QUANTIZED_KV: OnceLock<bool> = OnceLock::new();

/// Record what the serving binary advertised. First write wins; later calls are no-ops
/// (the binary does not change under a running core — a swap is a reboot).
pub fn record_engine_quantized_kv_support(supported: bool) {
    let _ = ENGINE_QUANTIZED_KV.set(supported);
}

/// The engine's advertisement, or `None` when it has not been probed / did not answer.
pub fn engine_quantized_kv_support() -> Option<bool> {
    ENGINE_QUANTIZED_KV.get().copied()
}

/// THE DECISION — pure. Given the backend and whatever the engine advertised, what does
/// the substrate choose?
///
/// The engine's own answer OUTRANKS the backend table: a table in our source can only
/// be a stale guess about someone else's build, whereas `--help` is that build speaking.
/// `engine_support: None` = it did not say, so the table decides.
pub fn decide_with_engine(backend: ServingBackend, engine_support: Option<bool>) -> KvCachePlan {
    let quantized = engine_support.unwrap_or_else(|| backend.supports_quantized_kv());
    let cache_type = if quantized { Q8_0 } else { F16 };
    KvCachePlan {
        cache_type: cache_type.to_string(),
        // Quantized KV rides the fused-attention path. One choice, never two.
        flash_attn: quantized,
        bytes_per_token_divisor: kv_divisor_for(Some(cache_type)),
        source: KvPlanSource::Decided,
        backend,
    }
}

/// [`decide_with_engine`] with no engine advertisement — the backend table alone.
pub fn decide(backend: ServingBackend) -> KvCachePlan {
    decide_with_engine(backend, None)
}

/// Parse an operator flash-attention value the way the launcher always has.
fn flash_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// THE RESOLUTION — pure. The decision, then the operator's overrides folded on top.
///
/// * `cache_type_env` / `flash_env` are the raw `SERVING_KV_CACHE_TYPE` /
///   `SERVING_FLASH_ATTN` values, `None` when the key is absent.
/// * An absent flash key follows the RESOLVED cache type, not the old blanket
///   `false`: an operator who writes `SERVING_KV_CACHE_TYPE=q8_0` and nothing else
///   must not get quantized KV with the fused path off — that is the misconfiguration
///   this struct exists to make unrepresentable.
/// * `source` is [`KvPlanSource::Override`] only when the resolved pair actually
///   DIFFERS from the decision. A key that restates the decision is not an override,
///   it is a no-op, and the receipt should not cry wolf about it.
pub fn resolve_from(
    backend: ServingBackend,
    engine_support: Option<bool>,
    cache_type_env: Option<&str>,
    flash_env: Option<&str>,
) -> KvCachePlan {
    let decided = decide_with_engine(backend, engine_support);
    let ct_override = cache_type_env
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let fa_override = flash_env
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| flash_flag(&s));

    let cache_type = ct_override.unwrap_or_else(|| decided.cache_type.clone());
    let flash_attn = fa_override.unwrap_or_else(|| is_quantized(&cache_type));
    let source = if cache_type == decided.cache_type && flash_attn == decided.flash_attn {
        KvPlanSource::Decided
    } else {
        KvPlanSource::Override
    };
    KvCachePlan {
        bytes_per_token_divisor: kv_divisor_for(Some(&cache_type)),
        cache_type,
        flash_attn,
        source,
        backend,
    }
}

/// The host's serving backend, recorded ONCE at the one place that probes it.
static HOST_BACKEND: OnceLock<ServingBackend> = OnceLock::new();

/// Record what the live GPU monitor said this host runs. Called from the single
/// `gpu::monitor::detect()` site in `ipc/mod.rs`, with the same monitor that feeds the
/// resource governor — never a second probe (which would spawn a second nvidia-smi
/// sampling daemon). First write wins; later calls are no-ops.
pub fn record_host_backend(platform: &str) {
    let backend = ServingBackend::from_platform(platform);
    let _ = HOST_BACKEND.set(backend);
    crate::probe!(
        class = "serving.kv_cache.backend",
        platform = platform,
        backend = backend.label(),
        quantized_kv = backend.supports_quantized_kv(),
        "the host's serving backend, recorded once — the KV cache decision reads this"
    );
}

/// The backend the KV decision is made against.
///
/// `CONTINUUM_SERVING_PLACEMENT=cpu` WINS over the detected device: that key is how
/// the #3729 Intel-Mac installer records "this host's llama-server is built with Metal
/// OFF", and such a box still reports a Metal device from `--list-devices`. The lane
/// runs on the CPU there, so the KV decision must be the CPU one.
fn host_backend(placement_cfg: Option<&str>) -> ServingBackend {
    if crate::inference::llama_server::placement_from_config(placement_cfg)
        == crate::inference::llama_server::LanePlacement::Cpu
    {
        return ServingBackend::Cpu;
    }
    HOST_BACKEND.get().copied().unwrap_or(ServingBackend::Unknown)
}

/// THE ONE RESOLVED VALUE both consumers read — the launcher's flags and the plan's
/// resident-KV divisor.
///
/// One `config.env` read serves all three keys (the file is parsed whole, last
/// assignment wins — the same `source` semantics every other reader gets), so this is
/// no more I/O than the single `config_env::read` it replaced.
pub fn resolve() -> KvCachePlan {
    let cfg = crate::config_env::read_all();
    resolve_from(
        host_backend(pick(&cfg, "CONTINUUM_SERVING_PLACEMENT")),
        engine_quantized_kv_support(),
        pick(&cfg, "SERVING_KV_CACHE_TYPE"),
        pick(&cfg, "SERVING_FLASH_ATTN"),
    )
}

/// One key out of an already-parsed `config.env`, last assignment winning — the same
/// shell `source` semantics [`crate::config_env::read`] gives a single-key read.
fn pick<'a>(cfg: &'a [(String, String)], key: &str) -> Option<&'a str> {
    cfg.iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real shape llama-server's `--help` prints for the KV cache flags.
    const REAL_HELP: &str = "\
-c,    --ctx-size N                     size of the prompt context (default: 4096)
-ctk,  --cache-type-k TYPE              KV cache data type for K
                                        allowed values: f32, f16, bf16, q8_0, q4_0, q4_1, iq4_nl, q5_0, q5_1
                                        (default: f16)
-ctv,  --cache-type-v TYPE              KV cache data type for V
-fa,   --flash-attn [on|off|auto]       set Flash Attention use (default: auto)
";

    // what this catches: the 5090's 26,880-token lane. That box never had
    // SERVING_KV_CACHE_TYPE exported, so the launcher passed no --cache-type-* flag
    // (engine default f16, 65,536 B/token) AND the plan divided by 1 — planned and
    // served at half the capacity the card affords, self-consistently, invisibly.
    // A cuda host with NO operator key must now decide q8_0 + flash attention, and
    // the divisor must be the one that doubles the window.
    #[test]
    fn a_cuda_host_with_no_env_decides_q8_0_with_flash_attention() {
        let plan = resolve_from(ServingBackend::Cuda, None, None, None);
        assert_eq!(plan.cache_type, Q8_0);
        assert!(plan.flash_attn, "quantized KV without the fused path is a misconfiguration");
        assert_eq!(plan.bytes_per_token_divisor, 2);
        assert_eq!(plan.source, KvPlanSource::Decided);
        assert_eq!(plan.launcher_cache_type(), Some("q8_0"));
        // The 5090's measured f16 rate for the 27B coder row.
        assert_eq!(plan.bytes_per_token(65_536), 32_768);
    }

    // what this catches: the no-op requirement on the M5. That box already runs
    // `--cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on`; the decision must
    // reproduce EXACTLY that on metal, with or without the operator's existing keys,
    // so this change is a doubling on the 5090 and byte-identical on every Mac.
    #[test]
    fn metal_decides_exactly_what_the_m5_already_runs() {
        let decided = resolve_from(ServingBackend::Metal, None, None, None);
        assert_eq!(decided.cache_type, Q8_0);
        assert!(decided.flash_attn);
        assert_eq!(decided.bytes_per_token_divisor, 2);
        assert_eq!(decided.source, KvPlanSource::Decided);

        // The M5's existing config.env keys restate the decision → same flags, and
        // NOT reported as an override (a key that agrees is a no-op, not a wolf).
        let with_keys = resolve_from(ServingBackend::Metal, None, Some("q8_0"), Some("1"));
        assert_eq!(with_keys, decided);
    }

    // what this catches: the THIRD box. Cormac's IntelMac serves on the CPU by plan
    // (`--n-gpu-layers 0`, a 1.5B at -c 32768 --parallel 1 on ~15 GB usable) and had no
    // key in its config.env either — so it too planned and served at half its KV budget
    // (2026-09-20). A CPU backend is quantized-capable in llama.cpp; excluding it from
    // the table would have left the weakest box in the fleet exactly where the defect
    // found it.
    #[test]
    fn a_cpu_by_plan_host_gets_the_same_doubling_as_the_gpu_boxes() {
        let plan = resolve_from(ServingBackend::Cpu, None, None, None);
        assert_eq!(plan.cache_type, Q8_0);
        assert!(plan.flash_attn);
        assert_eq!(plan.bytes_per_token_divisor, 2);
        assert_eq!(plan.source, KvPlanSource::Decided);
        assert_eq!(plan.launcher_cache_type(), Some("q8_0"));
    }

    // what this catches: a backend whose build may not ship quantized-KV kernels must
    // stay on f16 — and must OMIT the flag, so its invocation is byte-identical to
    // every launch before the decision existed. f16 there is a DECISION (the receipt
    // names backend + source), never a silence. Over-reserve is a smaller window;
    // under-reserve is an OOM.
    #[test]
    fn unmeasured_backends_stay_on_f16_and_pass_no_flag() {
        for backend in [
            ServingBackend::Unknown,
            ServingBackend::Vulkan,
            ServingBackend::Rocm,
            ServingBackend::DirectMl,
        ] {
            let plan = resolve_from(backend, None, None, None);
            assert_eq!(plan.cache_type, F16, "{backend:?}");
            assert!(!plan.flash_attn, "{backend:?}");
            assert_eq!(plan.bytes_per_token_divisor, 1, "{backend:?}");
            assert_eq!(plan.launcher_cache_type(), None, "{backend:?}");
            assert_eq!(plan.source, KvPlanSource::Decided, "{backend:?}");
        }
    }

    // what this catches: the engine's own answer must OUTRANK the source table, in both
    // directions — a vulkan build that advertises q8_0 gets it without a source change,
    // and a cuda build whose `--help` has no --cache-type-k flag at all is NOT handed a
    // flag it will refuse to start with.
    #[test]
    fn the_engines_own_advertisement_outranks_the_backend_table() {
        let advertised = parse_engine_kv_support(REAL_HELP);
        assert_eq!(advertised, Some(true), "the real help text advertises q8_0");

        let lifted = resolve_from(ServingBackend::Vulkan, Some(true), None, None);
        assert_eq!(lifted.cache_type, Q8_0);
        assert!(lifted.flash_attn);
        assert_eq!(lifted.bytes_per_token_divisor, 2);

        let held_back = resolve_from(ServingBackend::Cuda, Some(false), None, None);
        assert_eq!(held_back.cache_type, F16);
        assert!(!held_back.flash_attn);
        assert_eq!(held_back.launcher_cache_type(), None);

        // A build with no --cache-type-k flag says "no" by its absence...
        assert_eq!(parse_engine_kv_support("-c, --ctx-size N\n"), Some(false));
        // ...but a build that has the flag and does not enumerate its types says
        // NOTHING, and the table decides. `None` is never read as "no".
        assert_eq!(
            parse_engine_kv_support("-ctk, --cache-type-k TYPE  KV cache data type for K\n-fa, --flash-attn\n"),
            None
        );
        // A build with the types but no fused-attention flag cannot run the pair.
        assert_eq!(
            parse_engine_kv_support("--cache-type-k TYPE\n   allowed values: f16, q8_0\n"),
            Some(false)
        );
    }

    // what this catches: an operator who has a reason to pin f16 on a cuda box (a
    // build without the kernels, a quality investigation) is HONORED — and the receipt
    // says `override`, so the half-size lane that results is never mistaken for the
    // substrate's own decision again.
    #[test]
    fn an_explicit_f16_override_on_cuda_is_honored_and_named_an_override() {
        let plan = resolve_from(ServingBackend::Cuda, None, Some("f16"), None);
        assert_eq!(plan.cache_type, F16);
        assert!(!plan.flash_attn, "f16 does not need the fused path");
        assert_eq!(plan.bytes_per_token_divisor, 1);
        assert_eq!(plan.source, KvPlanSource::Override);
        assert_eq!(plan.launcher_cache_type(), None);

        // Trimmed + case-insensitive, like every other config value.
        assert_eq!(resolve_from(ServingBackend::Cuda, None, Some("  F16 "), None), plan);
    }

    // what this catches: THE defect's shape — the launcher flag and the plan divisor
    // derived from two independent reads that could differ. They now come off ONE
    // struct, so "what the engine was told" and "what the plan budgeted" are the same
    // fact by construction. A box that served q8_0 while planning f16 would over-commit
    // its budget by 2×; a box that plans q8_0 while serving f16 is the 5090's
    // 26,880-token lane. Asserted on the shared value, never on two reads.
    #[test]
    fn the_launcher_flag_and_the_plan_divisor_cannot_disagree() {
        for (backend, engine, ct, fa) in [
            (ServingBackend::Cuda, None, None, None),
            (ServingBackend::Metal, None, None, None),
            (ServingBackend::Cpu, None, None, None),
            (ServingBackend::Vulkan, Some(true), None, None),
            (ServingBackend::Cuda, Some(false), None, None),
            (ServingBackend::Cuda, None, Some("f16"), None),
            (ServingBackend::Unknown, None, Some("q8_0"), None),
            (ServingBackend::Cuda, None, Some("q4_0"), None),
        ] {
            let plan = resolve_from(backend, engine, ct, fa);
            // The flag the launcher passes and the divisor the plan sizes with are
            // two projections of ONE cache_type.
            let launcher_says = plan.launcher_cache_type().unwrap_or(F16);
            assert_eq!(
                kv_divisor_for(Some(launcher_says)),
                plan.bytes_per_token_divisor,
                "{backend:?} {engine:?} {ct:?}: the engine's flag and the plan's divisor must be one fact",
            );
            // And quantized KV always carries the fused path it requires.
            assert_eq!(
                plan.flash_attn,
                is_quantized(&plan.cache_type),
                "{backend:?} {engine:?} {ct:?}: quantized KV and flash attention are one choice",
            );
        }
    }

    // what this catches: an operator pinning q8_0 on a backend the substrate would NOT
    // have chosen still gets the fused path it requires — the half-fix (q8_0, flash
    // off) that llama-server either refuses or runs slowly is unrepresentable.
    #[test]
    fn a_quantized_override_carries_flash_attention_even_where_it_was_not_decided() {
        let plan = resolve_from(ServingBackend::Vulkan, None, Some("q8_0"), None);
        assert_eq!(plan.cache_type, Q8_0);
        assert!(plan.flash_attn);
        assert_eq!(plan.bytes_per_token_divisor, 2);
        assert_eq!(plan.source, KvPlanSource::Override);

        // An explicit off is still honored — the operator gets what they asked for,
        // named as an override, not silently corrected.
        let forced_off = resolve_from(ServingBackend::Metal, None, None, Some("0"));
        assert_eq!(forced_off.cache_type, Q8_0);
        assert!(!forced_off.flash_attn);
        assert_eq!(forced_off.source, KvPlanSource::Override);
    }

    // what this catches: the divisor table itself — the #232 fit-math coupling. The
    // served window grows only when the lane actually runs quantized KV, and
    // CONSERVATIVELY so the plan never over-grows past the real KV and OOMs.
    #[test]
    fn kv_divisor_reflects_cache_type_conservatively() {
        assert_eq!(kv_divisor_for(None), 1, "unset never scales the window");
        assert_eq!(kv_divisor_for(Some("f16")), 1, "explicit f16 is the no-op default");
        assert_eq!(kv_divisor_for(Some("q8_0")), 2, "q8_0 ~ half of f16");
        assert_eq!(kv_divisor_for(Some("  Q8_0 ")), 2, "trimmed + case-insensitive");
        assert_eq!(kv_divisor_for(Some("q4_0")), 3, "q4_0 conservative, under the ideal ~3.5x");
        assert_eq!(kv_divisor_for(Some("garbage")), 1, "unknown type → no grow, never a bogus OOM");
    }

    // what this catches: the platform vocabulary. These are the exact strings
    // `GpuDeviceProbe::platform()` and `BackendVerdict::backend_label()` emit — if one
    // of them drifts, the decision silently degrades to f16 on a capable box, which is
    // the whole defect, rebuilt.
    #[test]
    fn the_platform_strings_the_substrate_actually_emits_all_parse() {
        assert_eq!(ServingBackend::from_platform("metal"), ServingBackend::Metal);
        assert_eq!(ServingBackend::from_platform("mlx"), ServingBackend::Metal);
        assert_eq!(ServingBackend::from_platform("cuda"), ServingBackend::Cuda);
        assert_eq!(ServingBackend::from_platform("CUDA"), ServingBackend::Cuda);
        assert_eq!(ServingBackend::from_platform("vulkan"), ServingBackend::Vulkan);
        assert_eq!(ServingBackend::from_platform("cpu"), ServingBackend::Cpu);
        assert_eq!(ServingBackend::from_platform("mock"), ServingBackend::Unknown);
        assert_eq!(ServingBackend::from_platform(""), ServingBackend::Unknown);
        assert_eq!(ServingBackend::Cuda.label(), "cuda");
        assert_eq!(ServingBackend::Unknown.label(), "unknown");
    }

    // what this catches: a CPU-by-plan host (the #3729 Intel Mac, whose llama-server is
    // built with Metal OFF but whose `--list-devices` still names a Metal device) is
    // decided as a CPU lane — the placement config WINS over the detected device, so
    // the KV choice matches where the weights actually run.
    #[test]
    fn cpu_by_plan_wins_over_a_detected_metal_device() {
        assert_eq!(host_backend(Some("cpu")), ServingBackend::Cpu);
        assert_eq!(host_backend(Some(" CPU ")), ServingBackend::Cpu);
        // Not pinned, nothing recorded (the `cargo test` process never boots the IPC
        // server) → Unknown, which decides f16: the pre-existing default, never a guess.
        if HOST_BACKEND.get().is_none() {
            assert_eq!(host_backend(None), ServingBackend::Unknown);
        }
    }
}
