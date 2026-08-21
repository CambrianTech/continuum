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
}
