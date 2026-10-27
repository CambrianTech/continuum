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

/// The physical micro-batch a lane prefills in (`--ubatch-size`): the unit its `/slots`
/// prefill counters advance by, so also how long a starved stream must be willing to see no
/// counter move (`inference::sse_stream`). One number, set once here.
// derived-or-floor: measured 2026-08-21 on the M5 (llama-bench, production gguf): 2048 nearly doubles prefill over 1024, and 4096 failed to allocate beside a serving lane.
// context-budget-exempt: the engine's physical prefill micro-batch (--ubatch-size), a compute-pass size, never a context or prompt budget.
pub(crate) const UBATCH_TOKENS: u32 = 2048;

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
    pub envs: Vec<(String, std::ffi::OsString)>,
    /// Managed keys deliberately left inherited because they were present when
    /// resolved. Values are NOT captured; this is not a hermetic environment.
    /// Other inherited variables are outside this invocation's provenance.
    pub inherited_env_keys: Vec<String>,
}

impl LaneInvocation {
    /// A Metal lane runs ggml's tensor API (the M5 / A19 GPU matrix units), which our
    /// engine build makes opt-in; the engine itself disables it before M5. An
    /// operator-exported value wins.
    pub(crate) fn use_metal_matrix_units(&mut self, inherited: bool) {
        self.inherit_or_default_env("GGML_METAL_TENSOR_ENABLE", std::ffi::OsStr::new("1"), inherited);
    }

    /// Preserve an operator's inherited override, otherwise assign the managed
    /// default. Resolution supplies presence; this pure owner never reads env.
    pub(crate) fn inherit_or_default_env(
        &mut self,
        key: &str,
        default: &std::ffi::OsStr,
        inherited: bool,
    ) {
        if inherited {
            self.inherited_env_keys.push(key.to_string());
        } else {
            self.envs.push((key.to_string(), default.to_os_string()));
        }
    }

    /// A lane that can train (`--train-dir`) and keeps base weights on the CPU serves
    /// them unrepacked. llama.cpp repacks quantized CPU weights into a layout only its
    /// CPU kernels read (CPU_REPACK: get_tensor/cpy_tensor NULL), and the backward pass
    /// needs the standard layout. Measured IntelMac 2026-10-15T23:35Z (fork cc2e05ff2,
    /// Qwen2.5-0.5B Q4_K_M, -ngl 0): repacked, a learn fails ("OUT_PROD 'grad for
    /// ffn_swiglu-23' on blk.23.ffn_down.weight: no backend can run"); on a Metal host
    /// the same request crashes in ggml_backend_tensor_copy (card 5b497852). With
    /// --no-repack it learns (loss 1.608, 1418 ms). Price there: prompt 3-8% slower,
    /// generation within noise, so only train-capable CPU lanes pay it; a GPU lane keeps
    /// repack. `cpu_weights` mirrors the engine's own /train guard: CPU-only, or any
    /// tensor override (server-train.cpp: "-ngl all or --no-repack").
    fn no_repack_when_training_on_cpu(&mut self, cpu_weights: bool) {
        if cpu_weights && self.has("--train-dir") && !self.has("--no-repack") {
            self.args.push("--no-repack".to_string());
        }
    }

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
/// The GOVERNED size of llama-server's host-RAM prompt cache (`--cache-ram`),
/// in MiB. llama.cpp defaults this to 8192 — 8 GiB of host RAM the substrate
/// never passed, never budgeted, and never told the governor about
/// (KV-CACHE-ECONOMY §4: "own it or turn it off"; the 2026-07-13 rule — no
/// cache class without an owner). Measured 2026-08-26: a saved 20k-token
/// conversation restores from this cache in ~0.1s vs ~32.9s re-prefill (~330×),
/// so the cache is load-bearing for slot eviction — kept at HALF the default:
/// 4 GiB comfortably holds every slot's conversation once on the current
/// geometry, and the governor accounts this exact number as a term of the
/// host-cache lease (`serving_daemon::moe_host_cache_lease_inputs`), so the
/// expert cache can never be sized as if this RAM were free.
/// COLD-START PRIOR only (restore-economy 1.b): the value a lane gets when no
/// citizen demand has ever been measured (fresh install, or a single-purpose
/// eval/vision lane). Every steady-state serve derives the real number via
/// [`host_prompt_cache_mib`] from per-citizen measured demand — this constant
/// is a declared prior with provenance, the same pattern as
/// `ServingDemand`'s bootstrap ("cold start is a real state, not a missing
/// measurement"), never the sizing decision itself.
pub const CACHE_RAM_MIB: u32 = 4096;

// context-budget-exempt: a COUNT of saved recurrent-state checkpoints (each a fixed
// slice of host memory), not a context or prompt size; the token spacing that does scale
// with the served window is derived from it in `checkpoint_min_step`.
/// Context checkpoints each slot keeps (llama.cpp `--ctx-checkpoints`). Their count
/// is their memory: the spacing, not the count, is what scales with the window.
pub const CTX_CHECKPOINTS: u32 = 32;

/// The finest checkpoint spacing: within a kilotoken below any divergence on a
/// window short enough for CTX_CHECKPOINTS to cover at that spacing.
pub const MIN_CHECKPOINT_STEP: u32 = 1024;

/// Checkpoint spacing for a slot whose window is `per_slot_ctx` tokens: as fine as
/// MIN_CHECKPOINT_STEP, and never so fine that CTX_CHECKPOINTS stop short of the
/// window's end. Past the last one llama.cpp erases the OLDEST checkpoint, which is
/// the one a turn's shared prefix needs, so a long prompt would re-prefill from 0.
pub fn checkpoint_min_step(per_slot_ctx: u32) -> u32 {
    per_slot_ctx.div_ceil(CTX_CHECKPOINTS).max(MIN_CHECKPOINT_STEP)
}

/// The absolute floor in MiB — not a sizing decision, just a refusal to pass
/// llama-server a zero. The REAL floor is one conversation's worth, derived per
/// model in [`host_prompt_cache_mib`]; hardcoding a byte count here would be the
/// same mistake this function exists to delete.
pub const CACHE_RAM_HARD_FLOOR_MIB: u32 = 256;

/// The ONE-CONVERSATION floor in BYTES — the largest single conversation this cache
/// must be able to hold without thrashing, from this model's own geometry. Below it
/// every context switch is a guaranteed full re-prefill: the 330x cliff. A
/// model-specific byte constant here would just be the old mistake wearing a
/// different name.
///
/// Extracted to ONE definition on 2026-09-21 because a second consumer needs the same
/// answer: the sizing decision below asks it at SPAWN, and the question "is the grant
/// this engine is still holding big enough?" has to be asked of a RUNNING engine too.
/// `--cache-ram` is immutable after spawn while the derived target moves every tick, so
/// an engine can hold a number the substrate stopped believing hours ago — the M5's pid
/// 33898 held 480 MiB (15,360 tokens at q8_0) against 22,612-41,547-token prompts, which
/// is well under one conversation, and prefix reuse was 0%. Two places deciding "is this
/// cache big enough" with two derivations is how that becomes invisible (card ab27b914).
pub fn one_conversation_bytes(citizen_demand_tokens: &[u32], kv_per_token: u64) -> u64 {
    citizen_demand_tokens
        .iter()
        .copied()
        .max()
        .map(|t| kv_per_token.saturating_mul(t as u64))
        .unwrap_or(0) // unwrap_or: no citizen demand measured = no conversation to hold, not a guessed size
}

/// Size the host-RAM prompt cache from the WORKLOAD, not from a constant.
///
/// ## Why a constant was wrong
///
/// `CACHE_RAM_MIB` was reasoned as "4 GiB comfortably holds every SLOT's
/// conversation once". True, and irrelevant: we serve `--parallel 1`, and the
/// citizens time-share that one slot. The cache must hold one conversation per
/// LIVE CITIZEN, not per slot. Measured 2026-08-28 with four citizens on one
/// slot: prompt-cache hit rate 0.29-0.48, ~20k tokens re-prefilled per
/// generation, because a switch evicted somebody every time.
///
/// The penalty is not marginal. This cache is the difference between a ~0.1s
/// RESTORE and a ~32.9s RE-PREFILL — ~330x. Against a ~30s decode turn, a
/// restore is ~0.3% overhead, so N citizens sharing one model run at ~full
/// aggregate throughput; a re-prefill is ~110%, so most of the GPU goes to
/// recomputing what it already knew. That is the whole difference between the
/// 14-persona hallmark being seamless and the machine never finishing anything.
///
/// ## Why PER-CITIZEN MEASURED demand, not N x max-window
///
/// Sizing as `citizens x kv_at(model_window)` overstates enormously — 14 x 134k
/// x ~32 KiB/token is ~60 GiB and would make the hallmark look impossible.
/// Conversations occupy what they USE, not the window they could: a chat turn is
/// ~9k, a solve turn ~63k. Summing each citizen's own measured demand
/// (`WorkingSetRegistry::all`, persisted across restarts) puts fourteen mixed
/// citizens near ~9 GiB — feasible on this box, and honest either way.
///
/// Coupling to a MOVING measurement is safe HERE and is not safe for lane count:
/// a jittering demand signal driving the lane COUNT caused the 718-replan
/// lane-flap that wedged three benchmark runs (`serving_plan`), because it
/// changes structure. Cache size changes no structure — it is monotone headroom,
/// no replan, no flap.
///
/// Clamped by `affordable_bytes`, which the governor's host-cache lease computes
/// AFTER weights, live KV and the OS floor: this cache competes honestly with the
/// expert cache instead of being a constant the lease has to work around.
///
/// Deliberately NOT capping citizens or slots to make the arithmetic fit — a hard
/// limit chosen to make a benchmark tidy is the thing this replaces.
pub fn host_prompt_cache_mib(
    citizen_demand_tokens: &[u32],
    kv_per_token: u64,
    affordable_bytes: u64,
) -> u32 {
    let want: u64 = citizen_demand_tokens
        .iter()
        .map(|t| kv_per_token.saturating_mul(*t as u64))
        .fold(0u64, |a, b| a.saturating_add(b));
    let floor = one_conversation_bytes(citizen_demand_tokens, kv_per_token);
    let granted = want.max(floor).min(affordable_bytes.max(1));
    let mib = (granted / (1024 * 1024)) as u32;
    mib.max(CACHE_RAM_HARD_FLOOR_MIB)
}

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
    cache_ram_mib: u32,
    slot_save_dir: &Path,
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
            // CONTEXT CHECKPOINTS for the models `--cache-reuse` cannot serve. On a
            // hybrid / recurrent cache (Ornith: "cache_reuse is not supported by this
            // context, it will be disabled") a prefix is reusable only up to a
            // CHECKPOINT of the recurrent state at or before the point the new
            // prompt diverges. llama.cpp creates them at user-message starts, but
            // never closer than `--checkpoint-min-step` (default 8192 tokens) apart —
            // measured 2026-09-13: consecutive work turns share a 48% prefix
            // (~5k tokens, the results ledger) and diverge where working memory is
            // re-rendered; with the first checkpoint at ~350 tokens and the next
            // only after 8192, the rollback landed at the head → cache_n 0 on every
            // pinned turn. The spacing is derived so the slot's CTX_CHECKPOINTS
            // cover its WHOLE window (`checkpoint_min_step`): a fixed 1024 x 32
            // covered 32k, and on a 55k prompt llama.cpp erased its oldest
            // checkpoint, the one at the end of the stable head, so the next turn
            // re-prefilled from 0 (the 5090, 2026-10-05: cached 15064 or 0 on
            // every turn of Kimi's).
            arg("--ctx-checkpoints"),
            CTX_CHECKPOINTS.to_string(),
            arg("--checkpoint-min-step"),
            checkpoint_min_step(total_ctx / lanes.max(1)).to_string(),
            // The governed host-RAM prompt cache — see [`CACHE_RAM_MIB`]. Explicit
            // so llama.cpp's 8 GiB default can never run un-owned again (§4).
            arg("--cache-ram"),
            cache_ram_mib.to_string(),
            // KV DISK PAGING (restore economy, the OS-context-switch design):
            // `/slots/{id}?action=save|restore` only works when the server was
            // LAUNCHED with a save path, so the flag is part of the
            // unconditional spine — a lane without it silently amputates the
            // whole paging tier and rotation on an oversubscribed server costs
            // a full re-prefill per turn (measured 2026-09-01: hit_rate 0.0 on
            // every act, 35-45s each; restore of the same state measured
            // ~0.1s). The dir is one per MODEL (the caller's `kv_page_dir`); a page
            // that no longer fits its slot is refused by the engine on restore.
            arg("--slot-save-path"),
            slot_save_dir.to_string_lossy().into_owned(),
            // AN UNCAPPED REQUEST DECODES AT MOST ONE WINDOW. llama-server's `-n` is the
            // default for a request that names no `max_tokens` (an explicit limit still wins:
            // server-context.cpp n_predict_max, Codex on #5252). Unset, that default is -1
            // (infinite): with context shift, a request that reached the engine without a cap
            // decoded on indefinitely and held its lane (the MacBookPro, 2026-10-21 13:50Z:
            // n_predict -1, prompt 29,238, 27,854 decoded and climbing, while three residents
            // waited out their 496 s bound for the other lane; card 912e606d).
            arg("--n-predict"),
            (total_ctx / lanes.max(1)).to_string(),
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
            //
            // 1024 → 2048 (2026-08-21, llama-bench on the production gguf, fa on,
            // q8 KV, identical conditions back-to-back): 4k-token prefill 67 → 125
            // tok/s — the near-2× the comment above predicted, measured. 4096 was
            // ALSO benched and REJECTED: its compute buffer failed to allocate
            // (`failed to decode prompt batch, res = -3`) beside a resident serving
            // lane, and production always runs beside one. The live receipt to watch
            // stays `inference.prefill.complete`'s ingest_tok_per_s.
            arg("--ubatch-size"),
            arg(&UBATCH_TOKENS.to_string()),
            // Overflow must FAIL, never silently amputate. With context shift on
            // (the llama.cpp default), a prompt larger than the slot's window has
            // its MIDDLE evicted and generation proceeds on the mutilated prompt —
            // exam-corrupting amnesia no log line reports (#139: 44k-token prompts
            // observed riding ~13.4k slots with no error anywhere). Disabled, the
            // server 400s ("exceeds context size") and the caller's fail-loud path
            // surfaces the real defect: a RAG budget that overshot the served
            // window ([[fallbacks-are-illegal-fail-loud]]).
            arg("--no-context-shift"),
            // NATIVE TOOL CALLING, unconditional. `--jinja` makes llama-server render the
            // `tools` we send and do GRAMMAR-CONSTRAINED parsing of the model's own native
            // tool-call format, using the model's OWN chat template — the tool-trained
            // shape a Qwen/Hermes GGUF expects, infinitely more reliable than
            // reverse-engineering tool calls out of prose after the fact.
            //
            // It is unconditional because gating it was a silent capability kill: the
            // switch used to require a forge-written sidecar to exist, so every normally
            // PULLED GGUF ran with tools DISABLED — the gateway ignored the `tools` param
            // and the persona NARRATED tool calls instead of emitting them. A model with
            // hands, holding them behind its back, because of a file that was never there.
            arg("--jinja"),
        ],
        envs: Vec::new(),
        inherited_env_keys: Vec::new(),
    }
}

/// "Every layer on the device": llama-server clamps `--n-gpu-layers` to the model's
/// layer count, so one large value means ALL on every backend.
///
/// HONEST LIMIT (BigMama's review of #3728): nothing at launch time guarantees a DENSE
/// artifact fits VRAM — main lanes are always `LanePlacement::Gpu`, and fit is the tier
/// policy's quant choice plus expert `-ot` for MoE. So a dense model over VRAM on CUDA
/// now fails at spawn LOUDLY (serving.lane.failed) where the old zero-default ran it
/// silently on the CPU. Loud is the right side of that trade; the right answer is a
/// partial count computed from the footprint and the VRAM budget — that is a card, not
/// this constant. It may be lowered; it may never be omitted.
pub const ALL_GPU_LAYERS: &str = "999";

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
    /// RESOLVED UPSTREAM by [`crate::inference::kv_cache_plan`], never re-read here:
    /// the same struct supplies the serving plan's resident-KV divisor, so the flag and
    /// the fit math cannot disagree.
    pub kv_cache_type: Option<&'a str>,
    /// Flash attention (#232). Arrives from the SAME [`crate::inference::kv_cache_plan`]
    /// decision as `kv_cache_type`, because quantized KV rides the fused-attention path
    /// — q8_0 with this off is a misconfiguration, not a half-fix.
    pub flash_attn: bool,
    /// Multimodal projector (#106) — the model actually SEES when present.
    pub mmproj: Option<&'a Path>,
    /// Native MTP speculative-decode draft head (#440).
    pub mtp_draft: Option<&'a Path>,
    /// Ngram speculative decoding (2026-08-23) — drafts from the prompt's own
    /// n-grams: zero extra tensors, no draft model, no VRAM. Code/tool output
    /// is maximally ngram-friendly (identifiers, syntax, paths repeat), and
    /// acts measured ~80s of decode at ~30 tok/s writing one file. Composes
    /// with `mtp_draft` into one `--spec-type` list when both are on.
    pub ngram_spec: bool,
    /// Device-fit resident override (#29) — an ENV var, not a flag.
    pub resident_override: Option<&'a Path>,
    /// CPU-pinned lane: never contend for VRAM a living lane already holds.
    pub cpu_only: bool,
    /// A forge-written `chat_template.jinja` sidecar, when one sits beside the GGUF.
    /// OVERRIDES the embedded template — for forged GGUFs whose mlx→gguf conversion
    /// stripped it to a thin tool-less ChatML loop (measured 2026-06-26: a 208-char
    /// template, zero tool support, the persona's hands dead). Absent → the embedded
    /// tool-capable template stands. Explicit override, never a silent fallback.
    pub chat_template: Option<&'a Path>,
    /// Trained genome layers to load into the `/lora-adapters` catalog, in index order;
    /// the per-request `"lora":[{id,scale}]` field pages them in.
    pub loras: &'a [std::path::PathBuf],
    /// The ONE directory the engine's `POST /train` may write adapters into
    /// (`--train-dir`, fork c09af02f4): LoRA-only training on the weights this lane
    /// already serves, in a second context beside the slots, yielding between batches
    /// while any slot is busy. The route is refused unless the flag is present, and
    /// every file it writes is a bare `.gguf` name inside this directory. `None` =
    /// training off on this lane; serving is the same either way.
    pub train_dir: Option<&'a Path>,
    /// The host prompt cache's disk tier (`--cache-disk DIR --cache-disk-mib N`, engine fork
    /// #78): an entry evicted from `--cache-ram` is written to DIR instead of dropped. `None`
    /// = no tier; the RAM tier serves the same either way.
    pub prompt_cache_disk: Option<(&'a Path, u32)>,
    /// K3 expert paging (#278): `-ot` tensor placement for COLD layers, from the
    /// residency planner. `None` / all-hot → no flag (llama-server rejects an empty one).
    pub expert_ot: Option<&'a str>,
    /// Catalog-declared host-pinned tensors (`ModelServingPrefs::host_pinned_tensors`)
    /// — designed disk/host-resident lookup tables (Flash-Next's n-gram table).
    /// Merged with `expert_ot` into ONE `--override-tensor` value: repeated flags
    /// risk the same silent last-wins that collapsed `--lora` stacks above.
    pub host_pinned_tensors: &'a [&'static str],
    /// `-fit off` (`ModelServingPrefs::fit_off`) — default false = fit auto-sizing runs.
    pub fit_off: bool,
    /// `--no-warmup` (`ModelServingPrefs::no_warmup`) — default false = warmup runs.
    pub no_warmup: bool,
    /// Per-model `--ubatch-size` ceiling (`ModelServingPrefs::max_ubatch`) — only ever
    /// LOWERS the base invocation's value, never raises it past the lane-sized default.
    pub max_ubatch: Option<u32>,
    /// `--reasoning-budget N` (`ModelServingPrefs::reasoning_budget`) — cap on the
    /// thinking phase for models whose native think-appetite exceeds the served
    /// window (Flash-Next: 40k measured vs 32k window).
    pub reasoning_budget: Option<u32>,
}

impl LaneInvocation {
    /// Fold the conditional surface onto a base invocation.
    pub fn with_options(mut self, opts: &LaneOptions<'_>) -> Self {
        let mut push = |s: String| self.args.push(s);
        // KV CACHE QUANTIZATION (#232, field-proven technique). f16 KV is llama.cpp's
        // default; q8_0 is ~half the resident KV footprint at near-lossless quality,
        // freeing memory the elastic window (#234) spends on a BIGGER context or MORE
        // warm lanes.
        //
        // This was an operator OPT-IN until 2026-09-20 — and an opt-in that only two
        // people ever opted into meant the 5090 (26,880-token lane, Joel) and the
        // CPU-serving IntelMac (1 × 32k on ~15 GB usable, Cormac) both PLANNED and
        // SERVED at half their KV budget for their whole lives, self-consistently and
        // invisibly. It is now a DECISION the substrate makes from the engine's own
        // advertised `--cache-type-k` values (backend table as the fallback), with the
        // env key demoted to an override: [`crate::inference::kv_cache_plan`]. The
        // window-GROWTH coupling that comment called a follow-up shipped as
        // `apply_kv_quantization` — and it now rides the same resolved struct as this
        // flag, so the two can no longer name different numbers.
        if let Some(kv) = opts.kv_cache_type {
            push("--cache-type-k".into());
            push(kv.to_string());
            push("--cache-type-v".into());
            push(kv.to_string());
        }
        // FLASH ATTENTION (#232). Fused attention is faster on BOTH prefill and
        // decode and lowers peak memory — directly attacking prefill-bound turn latency
        // (#139) — AND it is the path llama.cpp's quantized-KV kernels ride, so it is
        // decided TOGETHER with the cache type above, never independently.
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
        // `--spec-type` is ONE comma-list flag; MTP and ngram compose here so a
        // second push can never shadow the first.
        let mut spec_types: Vec<&str> = Vec::new();
        if opts.mtp_draft.is_some() {
            spec_types.push("draft-mtp");
        }
        if opts.ngram_spec {
            spec_types.push("ngram-simple");
        }
        if !spec_types.is_empty() {
            push("--spec-type".into());
            push(spec_types.join(","));
        }
        if let Some(d) = opts.mtp_draft {
            push("--spec-draft-model".into());
            push(d.to_string_lossy().into_owned());
            push("--spec-draft-n-max".into());
            push("4".into());
            push("--spec-draft-p-min".into());
            push("0.7".into());
            // The draft's placement is TOLD, like the main model's below — left unset it
            // was the engine's `auto` fit, which may keep draft layers on the CPU.
            push("--spec-draft-ngl".into());
            push(if opts.cpu_only { "0" } else { ALL_GPU_LAYERS }.into());
        }
        // Placement: CPU lanes pin every layer to RAM so they never contend for the GPU
        // VRAM a living lane already holds (the Metal decode-time OOM that muted the
        // eval). GPU lanes omit the flag — llama-server offloads all it can by default.
        // [[ServingTarget::placement]] / #59 / #56.
        // GPU offload is ALWAYS explicit. Leaving `--n-gpu-layers` to the backend's
        // default was a Metal-only truth: Metal offloads every layer by default,
        // CUDA offloads NONE. Measured 2026-09-05 on BigMama's Windows 5090 host
        // (card 2c33f3f0): a 24B Devstral ran on CPU at 674 MiB of VRAM used while
        // the GPU sat idle, prefill could not advance, and the wedge self-heal
        // relaunched the same flags 13 times in 106 minutes. A placement decision
        // is a number the server is told, never a default it is left to guess.
        push("--n-gpu-layers".into());
        push(if opts.cpu_only { "0" } else { ALL_GPU_LAYERS }.into());
        // Device-fit resident-override (#29): source the RESIDENT (non-expert) tensors
        // from the precision-shrunk fit GGUF so the whole resident tier fits VRAM
        // offloaded to GPU, while the primary GGUF streams the experts. The loader hook
        // lazy-maps only the override's resident bytes (its experts are ignored). Set by
        // the governor's device_fit plan when as-shipped resident overflows the VRAM
        // budget; absent = resident fits as-shipped. [[device-fit-repeatable-primitive]].
        //
        // An ENV var, not a flag — which is precisely why `LaneInvocation` carries `envs`.
        // The sidecar template, when the forge wrote one (see `chat_template`).
        if let Some(tpl) = opts.chat_template {
            pair(&mut self.args, "--chat-template-file", tpl);
        }
        // ONE comma-separated `--lora` value. llama.cpp (b8784+) DEPRECATED repeated
        // `--lora` flags and SILENTLY keeps only the LAST — which was collapsing every
        // multi-layer genome stack to a single adapter. Glass-boxed 2026-07-23 in the
        // lane's own stderr: "DEPRECATED: --lora specified multiple times... only last
        // value will be used" x4, while a 4-layer stack was supposedly serving. The
        // genome's whole premise is layers that STACK; this arg SHAPE is what stacks them,
        // which is why the join lives here with the flag rather than at a call site.
        if !opts.loras.is_empty() {
            let joined = opts
                .loras
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(",");
            pair(&mut self.args, "--lora", joined);
        }
        // IN-ENGINE TRAINING (ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md S3/S4): the
        // dream trains a LoRA on the resident weights instead of loading a second copy
        // in a separate trainer process. The fork's /train is OFF without this flag, so
        // dropping it would leave every dream on the retired PyTorch path with nothing
        // red; and it is the only place the route may write, so it is always the
        // governed directory, never a caller's path.
        if let Some(dir) = opts.train_dir {
            pair(&mut self.args, "--train-dir", dir);
        }
        if let Some((dir, mib)) = opts.prompt_cache_disk {
            pair(&mut self.args, "--cache-disk", dir);
            pair(&mut self.args, "--cache-disk-mib", mib.to_string());
        }
        // K3 slice-1 physical expert paging: offload COLD layers' stacked expert tensors
        // to CPU while hot layers stay GPU-resident. Experts are stacked (one
        // blk.N.ffn_*_exps per layer), so `-ot` — which places WHOLE tensors — pages at
        // LAYER granularity. A change to the hot set is honored on the next relaunch (the
        // pager decides when). Catalog host-pins (designed disk-resident lookup tables)
        // join the SAME value — one flag, because repeated `--override-tensor` risks the
        // silent last-wins that collapsed `--lora` stacks.
        let mut ot_parts: Vec<String> = Vec::new();
        if let Some(ot) = opts.expert_ot {
            ot_parts.push(ot.to_string());
        }
        for pat in opts.host_pinned_tensors {
            ot_parts.push(format!("{pat}=CPU"));
        }
        let weights_on_cpu = opts.cpu_only || !ot_parts.is_empty();
        if !ot_parts.is_empty() {
            pair(&mut self.args, "--override-tensor", ot_parts.join(","));
        }
        self.no_repack_when_training_on_cpu(weights_on_cpu);
        // Per-model serving prefs (catalog-declared, measured — see ModelServingPrefs).
        if opts.fit_off {
            pair(&mut self.args, "-fit", "off");
        }
        if opts.no_warmup {
            self.args.push("--no-warmup".to_string());
        }
        if let Some(rb) = opts.reasoning_budget {
            pair(&mut self.args, "--reasoning-budget", rb);
        }
        if let Some(cap) = opts.max_ubatch {
            if let Some(i) = self.args.iter().position(|a| a == "--ubatch-size") {
                if let Some(v) = self.args.get(i + 1).and_then(|s| s.parse::<u32>().ok()) {
                    if cap < v {
                        self.args[i + 1] = cap.to_string();
                    }
                }
            }
        }
        if let Some(ov) = opts.resident_override {
            self.envs.push((
                "LLAMA_RESIDENT_OVERRIDE".to_string(),
                ov.to_string_lossy().into_owned().into(),
            ));
        }
        self
    }
}

/// PURE: a lane's launched `--cache-ram` cannot hold even ONE measured saved state. Unknown
/// (nothing measured, or an unrecorded launch size) is never true: no relaunch on a guess.
pub fn launched_cache_holds_no_state(launched_mib: u32, measured_state: Option<u64>) -> bool {
    measured_state.is_some_and(|state| launched_mib > 0 && u64::from(launched_mib) * 1024 * 1024 < state)
}

/// PURE: the adopt rail's cache check (card d7294f0b). The running lane's `--cache-ram`
/// grant read off its argv; unknown (no argv, no flag) is never a reason to relaunch.
pub fn cache_relaunch_owed(running_mib: Option<u32>, target_mib: u32, measured_state: Option<u64>) -> bool {
    running_mib.is_some_and(|launched| cache_relaunch_needed(launched, target_mib, measured_state))
}

/// PURE: whether a relaunch is owed for cache size: the launched cache holds no measured
/// state AND the cache a relaunch would launch with holds one. Without the second half a
/// host that cannot afford one state would relaunch every cooldown and gain nothing.
pub fn cache_relaunch_needed(launched_mib: u32, would_launch_mib: u32, measured_state: Option<u64>) -> bool {
    launched_cache_holds_no_state(launched_mib, measured_state)
        && measured_state.is_some_and(|state| u64::from(would_launch_mib) * 1024 * 1024 >= state)
}

// The tests gate themselves (`#![cfg(test)]` in tests.rs), so the scanner reads the file as test code.
mod tests;
