//! The llama-server INVOCATION SURFACE — the flags we hand the upstream binary.
//!
//! # Why this is its own module
//!
//! `llama_server.rs` was 4,190 lines and every concern lived in it: flag
//! construction, readiness probing, request shaping, lifecycle, orphan reaping.
//! This is slice 1 of the decomposition (docs/architecture/KV-CACHE-ECONOMY.md §7).
//!
//! Flags earn the FIRST slice because they are one of exactly two places upstream
//! drift can reach us. We consume llama.cpp two independent ways, and only one of
//! them touches this file:
//!
//! - **as a BINARY** — `llama-server` spawned as a subprocess, spoken to over HTTP.
//!   Zero linkage. Upstream drift arrives as a changed CLI surface (here) or a
//!   changed JSON shape (the request/readiness modules). That is this module's
//!   whole exposure.
//! - **as a LIBRARY** — the `llama` crate links libllama in-process for the forge
//!   and conversion paths. That is where our fork's C++ lives (K3 KDA kernel,
//!   AttnRes, MoE gather). It never passes through here.
//!
//! So a fork rebase should touch this file and the wire shapes, and nothing else.
//! It could not, while the flags were welded into a `Command` chain inside a 4k-line
//! file with no way to assert them without launching a real GPU process.
//!
//! # The invocation is DATA, not a side effect
//!
//! [`base_invocation`] is pure: primitives in, [`LaneInvocation`] out. No `Command`,
//! no process, no I/O. That is what lets the arithmetic below be unit-tested on a
//! machine with no GPU and no model — and the arithmetic is exactly where the
//! expensive bug was (see `-c` / `--parallel`, below).
//!
//! It also matches the policy shape the rest of the serving stack already uses:
//! `TierPolicy::plan` returns a plan rather than applying one, so decisions can be
//! logged, replayed and diffed. An invocation is the same kind of object.
//!
//! # The comments are the incident record — they travel WITH their flag
//!
//! Every value here was set by a specific failure. The comment beside it is the only
//! surviving account of that failure, so moving a flag without its comment destroys
//! the reason and invites the reversion. They are copied verbatim from
//! `llama_server.rs`, not paraphrased.

use std::path::Path;

/// One conversion for anything that can be a CLI argument.
///
/// Joel, 2026-08-21: *"Templates are underutilized by you in both cpp and rust… it
/// reduces code size a lot sometimes."* Correct, and this is the place it pays. Without
/// it every value is hand-converted at its call site — `total_ctx.to_string()`,
/// `p.to_string_lossy().into_owned()`, a bare `"256".into()` — which is three different
/// spellings of one idea and, worse, three chances to write the VALUE without its FLAG.
/// That adjacency is exactly the class that broke serving on 2026-08-20.
///
/// A trait plus [`pair`] makes the flag-and-its-value a single indivisible call, so the
/// bug becomes unrepresentable rather than merely tested for. The Rust monomorphises it
/// the same way a C++ template would — one generic definition, zero runtime cost.
pub trait AsArg {
    fn as_arg(&self) -> String;
}
impl AsArg for &str {
    fn as_arg(&self) -> String { (*self).to_string() }
}
impl AsArg for String {
    fn as_arg(&self) -> String { self.clone() }
}
impl AsArg for &Path {
    fn as_arg(&self) -> String { self.to_string_lossy().into_owned() }
}
impl AsArg for u16 {
    fn as_arg(&self) -> String { self.to_string() }
}
impl AsArg for u32 {
    fn as_arg(&self) -> String { self.to_string() }
}

/// Push a flag and its value together — the only way this module emits a valued flag,
/// so a value can never be orphaned from its flag.
fn pair<V: AsArg>(args: &mut Vec<String>, flag: &str, value: V) {
    args.push(flag.to_string());
    args.push(value.as_arg());
}

/// A fully-specified llama-server invocation: what to pass, and what environment to
/// pass it in. Returned rather than applied, so it can be asserted in a test and
/// logged on a receipt.
///
/// `envs` exists because the invocation is genuinely not args-only —
/// `LLAMA_RESIDENT_OVERRIDE` (device-fit, #29) is an environment variable read by a
/// loader hook. Modelling it as "args plus a side effect someone remembers to apply"
/// is how it would get dropped by the next caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LaneInvocation {
    /// Positional CLI arguments, in order.
    pub args: Vec<String>,
    /// Environment variables the child must be spawned with, as `(key, value)`.
    pub envs: Vec<(String, String)>,
}

impl LaneInvocation {
    /// Value of the flag named `flag`, if present — i.e. the argument that follows
    /// it. Test-facing convenience so assertions read as intent
    /// (`inv.value_of("--parallel")`) rather than as index arithmetic, which is its
    /// own source of wrong-but-passing tests.
    pub fn value_of(&self, flag: &str) -> Option<&str> {
        let at = self.args.iter().position(|a| a == flag)?;
        self.args.get(at + 1).map(String::as_str)
    }

    /// Whether a valueless switch is present.
    pub fn has(&self, flag: &str) -> bool {
        self.args.iter().any(|a| a == flag)
    }
}

/// The flags EVERY generation lane carries, regardless of model or placement.
///
/// Conditional flags (KV quant, flash-attn, mmproj, MTP draft, resident override,
/// CPU placement, jinja template, LoRA set, MoE `-ot` overrides) are still assembled
/// by the caller and are slice 2 of this extraction. They are conditional on model
/// artifacts and operator config; this function is the unconditional spine, which is
/// also the part carrying the arithmetic worth pinning.
pub fn base_invocation(
    gguf: &Path,
    model_id: &str,
    host: &str,
    port: u16,
    lanes: u32,
    total_ctx: u32,
) -> LaneInvocation {
    let arg = |s: &str| s.to_string();
    LaneInvocation {
        args: vec![
            arg("-m"),
            gguf.to_string_lossy().into_owned(),
            arg("--alias"),
            model_id.to_string(),
            arg("--host"),
            host.to_string(),
            arg("--port"),
            port.to_string(),
            // llama-server's `-c` is the TOTAL KV cache, split evenly across the
            // `--parallel` slots; each request only sees `-c / n_parallel` tokens.
            // The plan's `context_window` is PER-LANE, so the total we must request
            // is `context_window * lanes` — then each of `lanes` slots holds exactly
            // one planned window. We pass `--parallel` EXPLICITLY (never inherit
            // llama.cpp's default, which is 4 and silently quartered the window in
            // the prior bug). See `served_total_ctx` / `parallel_lanes`.
            arg("-c"),
            total_ctx.to_string(),
            arg("--parallel"),
            lanes.to_string(),
            // KV PREFIX REUSE across a persona's turns. `cache_prompt:true` (sent
            // per-request) only reuses a slot's prior content when the *exact*
            // prefix still sits in that slot; with the volatile grounding tail
            // changing every turn and embedding requests sharing these same slots,
            // measured cross-turn reuse was ZERO (`cachedTokens: 0` over every
            // captured live turn, forcing a full re-prefill of the static
            // identity/doctrine/tool prefix each turn). `--cache-reuse` lets
            // llama.cpp reuse cached chunks >= N tokens via KV shifting even when a
            // later span differs — so the stable prefix is kept, not recomputed. 256
            // is the llama.cpp-recommended min chunk. This is a pure optimization
            // flag: absent it we just re-prefill (correct, slow); present it we reuse
            // (correct, fast) — no fallback, no behavior change.
            //
            // 2026-08-21 POSTSCRIPT, and it matters for anyone tempted to tune this
            // number: the flag was never the defect. It was set the whole time reuse
            // measured 0%, because WE were mutating our own system prompt at token
            // ~2,000 (salience re-ordering the stable tier) and invalidating the
            // 34k-token tail behind it. Fixed by canonical ordering in
            // `llm_deliberation_faculty`, not by touching this value.
            arg("--cache-reuse"),
            arg("256"),
            // PREFILL THROUGHPUT (#139). Live personas are prefill-bound: a real turn
            // re-prefills ~4k tokens of fresh RAG context at ~109 tok/s → 30-110s turns
            // (decode is tiny and fast; the mind is NOT slow, the re-read is). The
            // physical micro-batch (`--ubatch-size`, llama.cpp default 512) is how many
            // prompt tokens Metal processes per compute pass — bigger batch = more
            // parallel prefill = higher tok/s, traded against a larger per-slot compute
            // buffer. 1024 doubles prefill parallelism; the compute-buffer growth is the
            // same axis that OOMs (kIOGPUCommandBufferCallbackErrorOutOfMemory) so it is
            // sized WITH the 2-lane headroom, not blindly. Measured knob: watch prefill
            // tok/s in the captures and back off if the lane 500s "Compute error".
            arg("--ubatch-size"),
            arg("1024"),
            // Overflow must FAIL, never silently amputate. With context shift on
            // (the llama.cpp default), a prompt larger than the slot's window has
            // its MIDDLE evicted and generation proceeds on the mutilated prompt —
            // exam-corrupting amnesia no log line reports (#139: 44k-token prompts
            // observed riding ~13.4k slots with no error anywhere). Disabled, the
            // server 400s ("exceeds context size") and the caller's fail-loud path
            // surfaces the real defect: a RAG budget that overshot the served
            // window ([[fallbacks-are-illegal-fail-loud]]).
            arg("--no-context-shift"),
        ],
        envs: Vec::new(),
    }
}

/// The per-lane CONDITIONAL surface — flags that depend on the model's artifacts, the
/// governor's plan, or an operator opt-in. Slice 2 of the extraction.
///
/// Every field is an ALREADY-RESOLVED value, never a thing to look up. Resolution
/// (reading `config_env`, probing the registry for an mmproj or an MTP head, deciding a
/// placement) stays with the caller, which owns the I/O and the receipts; this module
/// only decides what that resolution MEANS on a command line. That split is what keeps
/// the function pure and the flags assertable — and it is the same split as
/// `TierPolicy`: gather inputs, then compute a plan.
#[derive(Debug, Clone, Copy, Default)]
pub struct LaneOptions<'a> {
    /// KV cache quantization (#232) — `Some("q8_0")` etc. `None` (or `f16` upstream)
    /// leaves llama.cpp's f16 default, byte-identical to passing nothing.
    pub kv_cache_type: Option<&'a str>,
    /// Flash attention (#232), operator opt-in.
    pub flash_attn: bool,
    /// Multimodal projector (#106) — the model actually SEES when present.
    pub mmproj: Option<&'a Path>,
    /// Native MTP speculative-decode draft head (#440).
    pub mtp_draft: Option<&'a Path>,
    /// Device-fit resident override (#29) — an ENV var, not a flag.
    pub resident_override: Option<&'a Path>,
    /// CPU-pinned lane: never contend for VRAM a living lane already holds.
    pub cpu_only: bool,
}

impl LaneInvocation {
    /// Fold the conditional surface onto a base invocation.
    pub fn with_options(mut self, opts: &LaneOptions<'_>) -> Self {
        let mut push = |s: String| self.args.push(s);
        // KV CACHE QUANTIZATION (#232, opt-in field-proven technique). f16 KV is the
        // default; q8_0 is ~half the resident KV footprint at near-lossless quality,
        // freeing memory the elastic window (#234) can spend on a BIGGER context or MORE
        // warm lanes. OFF by default: not every backend/build ships Metal KV-quant
        // kernels, so this is an operator opt-in, never a blind assumption
        // ([[verify-real-device-numbers-not-a-clamp-premise]]). NOTE: to have the plan
        // actually GROW the window on the freed memory (not just leave it as extra
        // headroom), the fit math must also scale kv_per_token — that footprint coupling
        // is the follow-up; this is the safe enablement.
        if let Some(kv) = opts.kv_cache_type {
            push("--cache-type-k".into());
            push(kv.to_string());
            push("--cache-type-v".into());
            push(kv.to_string());
        }
        // FLASH ATTENTION (#232, opt-in). Fused attention is faster on BOTH prefill and
        // decode and lowers peak memory — directly attacking prefill-bound turn latency
        // (#139). OFF by default: Metal/backend support + quality vary by build.
        //
        // MUST carry an explicit VALUE. Upstream changed this from a bare boolean switch
        // to `-fa, --flash-attn [on|off|auto]`. A bare `--flash-attn` now EATS THE NEXT
        // ARGUMENT as its value and the server refuses to start:
        //
        //   error while handling argument "--flash-attn": unknown value for
        //   --flash-attn: '--mmproj'
        //
        // Found 2026-08-20 the first time anyone ever set SERVING_FLASH_ATTN=1 — the flag
        // shipped, was never exercised because it was opt-in and nobody opted in, and
        // rotted against upstream in the meantime. The whole lane failed to spawn and
        // serving sat at 0 lanes. An opt-in that has never once been switched on is
        // untested code with a config-shaped trigger
        // ([[an-absence-is-an-unfinished-measurement]]). `every_flag_that_takes_a_value_
        // is_followed_by_one` is the regression test that class earned.
        if opts.flash_attn {
            push("--flash-attn".into());
            push("on".into());
        }
        // MULTIMODAL PROJECTOR (#106): a vision/audio-capable model needs its mmproj GGUF
        // so llama-server loads the encoder and can tokenize image/audio parts. Absent on
        // a Vision-capable row the server serves TEXT only and silently ignores images,
        // which is a capability LIE — the caller warns LOUD rather than fabricate sight
        // ([[fallbacks-are-illegal-fail-loud]]). Safe on a generation lane (unlike
        // `--embeddings`): the projector only adds the encoder, it does not switch the
        // server out of causal-generation mode.
        if let Some(p) = opts.mmproj {
            push("--mmproj".into());
            push(p.to_string_lossy().into_owned());
        }
        // NATIVE MTP SPECULATIVE DECODE (#440): an `mtp-*.gguf` draft head shipped beside
        // the main GGUF (the ggml-org Qwen3.8 layout) loads as the spec-decode draft. MTP
        // heads are trained WITH the model, so acceptance is high and there is no external
        // draft model to fit: field-measured on Qwen3.8-27B (RTX 4090, 2026-08-15) decode
        // went 40.7 → 60.1 t/s for ~0.1GB extra state. Artifact presence IS the capability
        // signal (the mmproj pattern): no draft file → no flags → byte-identical serving.
        // n-max 4 / p-min 0.7 are the upstream-recommended MTP operating point from that
        // same field benchmark — per-model tuning, if ever needed, belongs on the Model
        // row beside `sampling`, not here.
        if let Some(d) = opts.mtp_draft {
            push("--spec-type".into());
            push("draft-mtp".into());
            push("--spec-draft-model".into());
            push(d.to_string_lossy().into_owned());
            push("--spec-draft-n-max".into());
            push("4".into());
            push("--spec-draft-p-min".into());
            push("0.7".into());
        }
        // Placement: CPU lanes pin every layer to RAM so they never contend for the GPU
        // VRAM a living lane already holds (the Metal decode-time OOM that muted the
        // eval). GPU lanes omit the flag — llama-server offloads all it can by default.
        // [[ServingTarget::placement]] / #59 / #56.
        if opts.cpu_only {
            push("--n-gpu-layers".into());
            push("0".into());
        }
        // Device-fit resident-override (#29): source the RESIDENT (non-expert) tensors
        // from the precision-shrunk fit GGUF so the whole resident tier fits VRAM
        // offloaded to GPU, while the primary GGUF streams the experts. The loader hook
        // lazy-maps only the override's resident bytes (its experts are ignored). Set by
        // the governor's device_fit plan when as-shipped resident overflows the VRAM
        // budget; absent = resident fits as-shipped. [[device-fit-repeatable-primitive]].
        //
        // An ENV var, not a flag — which is precisely why `LaneInvocation` carries `envs`.
        if let Some(ov) = opts.resident_override {
            self.envs
                .push(("LLAMA_RESIDENT_OVERRIDE".to_string(), ov.to_string_lossy().into_owned()));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn inv(lanes: u32, total_ctx: u32) -> LaneInvocation {
        base_invocation(
            &PathBuf::from("/models/m.gguf"),
            "qwen3-coder",
            "127.0.0.1",
            58057,
            lanes,
            total_ctx,
        )
    }

    // what this catches: the window-quartering regression — `--parallel` inherited
    // llama.cpp's default of 4 while `-c` was passed as a per-lane window, so every
    // slot silently served a QUARTER of the planned context. Nothing failed; citizens
    // were just quietly given 1/4 of their window.
    //
    // This is the whole reason flags became data. The arithmetic was previously
    // welded into a `Command` chain, so asserting it meant launching a real
    // llama-server on a real GPU with a real model — which no unit test does, which
    // is why it went unnoticed. Now it is a pure function and this is three lines.
    #[test]
    fn parallel_is_explicit_and_total_ctx_is_the_per_lane_window_times_lanes() {
        let i = inv(2, 32_768);
        assert_eq!(
            i.value_of("--parallel"),
            Some("2"),
            "--parallel must be passed EXPLICITLY — inheriting llama.cpp's default of \
             4 is the bug that quartered every citizen's window\n{:?}",
            i.args
        );
        assert_eq!(
            i.value_of("-c"),
            Some("32768"),
            "-c is the TOTAL KV across slots (per-lane window x lanes), not the \
             per-lane window\n{:?}",
            i.args
        );
    }

    // what this catches: a "cheap" future edit re-enabling context shift, which does
    // not fail — it silently evicts the MIDDLE of an over-long prompt and generates
    // from the mutilation. #139: 44k-token prompts observed riding ~13.4k slots with
    // no error anywhere. Overflow must 400, so the caller's fail-loud path can report
    // the real defect (a RAG budget that overshot the window).
    #[test]
    fn context_shift_stays_disabled_on_every_lane() {
        assert!(
            inv(1, 4096).has("--no-context-shift"),
            "overflow must FAIL loudly, never amputate the prompt middle"
        );
    }

    // what this catches: dropping --cache-reuse while "cleaning up flags", which
    // costs a full re-prefill per turn and shows up only as latency. It was measured
    // at ~306s per act on a 36k conversation before the #266 ordering fix.
    #[test]
    fn cache_reuse_is_configured_for_prefix_reuse() {
        assert_eq!(inv(1, 4096).value_of("--cache-reuse"), Some("256"));
    }

    // what this catches: flag/value pairs drifting out of adjacency. Upstream turned
    // `--flash-attn` from a bare switch into one taking [on|off|auto] on 2026-08-20,
    // and the bare form then ATE THE NEXT ARGUMENT ("unknown value for --flash-attn:
    // '--mmproj'") — the lane failed to spawn and serving sat at 0. Any invocation
    // this module builds must be an even, well-formed sequence.
    #[test]
    fn every_flag_that_takes_a_value_is_followed_by_one() {
        let i = inv(4, 65_536);
        for flag in ["-m", "--alias", "--host", "--port", "-c", "--parallel", "--cache-reuse", "--ubatch-size"] {
            let v = i.value_of(flag);
            assert!(
                v.is_some_and(|v| !v.starts_with("--")),
                "{flag} must be followed by its VALUE, not by another flag \
                 (upstream's --flash-attn change is how this class breaks)\n{:?}",
                i.args
            );
        }
    }

    // what this catches: the base invocation quietly acquiring an env dependency.
    // Envs are spawn-time state that a caller must remember to apply; the conditional
    // one we DO have (LLAMA_RESIDENT_OVERRIDE, #29) is why `envs` exists on the
    // struct at all. If a base env ever appears, it must be a deliberate edit here
    // with its own justification, not a drive-by.
    #[test]
    fn the_base_invocation_needs_no_environment() {
        assert!(inv(2, 16_384).envs.is_empty());
    }

    // what this catches: a conditional silently becoming unconditional (or vice versa).
    // Every flag below costs memory, changes numerics, or claims a capability, so
    // "present when it should be absent" is a real defect — the mmproj case literally
    // decides whether a citizen can SEE.
    #[test]
    fn no_options_means_no_conditional_flags() {
        let i = inv(2, 8192).with_options(&LaneOptions::default());
        for f in ["--cache-type-k", "--flash-attn", "--mmproj", "--spec-type", "--n-gpu-layers"] {
            assert!(!i.has(f), "{f} must be absent with default options\n{:?}", i.args);
        }
        assert!(i.envs.is_empty(), "no options must mean no environment");
    }

    // what this catches: THE 2026-08-20 SPAWN FAILURE, as a test instead of an outage.
    // Upstream turned --flash-attn from a switch into one taking [on|off|auto]; the bare
    // form ate the NEXT argument ("unknown value for --flash-attn: '--mmproj'") and the
    // lane refused to start, so serving sat at 0 lanes. The pairing below is the exact
    // adjacency that broke.
    #[test]
    fn flash_attn_carries_its_value_even_next_to_mmproj() {
        let mm = PathBuf::from("/models/mmproj.gguf");
        let i = inv(1, 4096).with_options(&LaneOptions {
            flash_attn: true,
            mmproj: Some(&mm),
            ..Default::default()
        });
        assert_eq!(i.value_of("--flash-attn"), Some("on"));
        assert_eq!(i.value_of("--mmproj"), Some("/models/mmproj.gguf"));
    }

    // what this catches: the resident override degrading into a command-line flag, or
    // being dropped entirely. It is an ENV var read by a loader hook (#29) — the reason
    // LaneInvocation models envs at all rather than returning a bare Vec<String>.
    #[test]
    fn resident_override_rides_the_environment_not_the_args() {
        let ro = PathBuf::from("/models/fit.gguf");
        let i = inv(1, 4096).with_options(&LaneOptions {
            resident_override: Some(&ro),
            ..Default::default()
        });
        assert_eq!(
            i.envs,
            vec![("LLAMA_RESIDENT_OVERRIDE".to_string(), "/models/fit.gguf".to_string())]
        );
        assert!(!i.args.iter().any(|a| a.contains("fit.gguf")), "must not leak into args");
    }

    // what this catches: KV quant emitting only one half of the pair. K and V are
    // separate flags; setting one and not the other is a silent asymmetry in the cache.
    #[test]
    fn kv_quant_sets_both_k_and_v() {
        let i = inv(1, 4096).with_options(&LaneOptions { kv_cache_type: Some("q8_0"), ..Default::default() });
        assert_eq!(i.value_of("--cache-type-k"), Some("q8_0"));
        assert_eq!(i.value_of("--cache-type-v"), Some("q8_0"));
    }
}
