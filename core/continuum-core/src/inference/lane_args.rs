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

/// The absolute floor in MiB — not a sizing decision, just a refusal to pass
/// llama-server a zero. The REAL floor is one conversation's worth, derived per
/// model in [`host_prompt_cache_mib`]; hardcoding a byte count here would be the
/// same mistake this function exists to delete.
const CACHE_RAM_HARD_FLOOR_MIB: u32 = 256;

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
    // The floor is ONE conversation — the largest single one we must not thrash —
    // derived from this model's own geometry. Below that, every context switch is
    // a guaranteed full re-prefill: the 330x cliff. A model-specific byte constant
    // here would just be the old mistake wearing a different name.
    let floor = citizen_demand_tokens
        .iter()
        .copied()
        .max()
        .map(|t| kv_per_token.saturating_mul(t as u64))
        .unwrap_or(0);
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
            // ~0.1s). The dir is GEOMETRY-KEYED by the caller (model + per-slot
            // ctx) so a stale page can never restore into a mismatched slot.
            arg("--slot-save-path"),
            slot_save_dir.to_string_lossy().into_owned(),
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
            arg("2048"),
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
    }
}

/// "Every layer on the device": llama-server clamps `--n-gpu-layers` to the model's
/// layer count, so one large value means ALL on every backend. The placement planner
/// may lower it for a partial fit; it may never omit it.
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
    pub kv_cache_type: Option<&'a str>,
    /// Flash attention (#232), operator opt-in.
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
        if !ot_parts.is_empty() {
            pair(&mut self.args, "--override-tensor", ot_parts.join(","));
        }
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
            self.envs
                .push(("LLAMA_RESIDENT_OVERRIDE".to_string(), ov.to_string_lossy().into_owned()));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    // what this catches: sizing the host prompt cache per SLOT when the workload
    // is per CITIZEN. Measured 2026-08-28 — four citizens time-sharing one
    // `--parallel 1` slot ran a 0.29 prompt-cache hit rate, ~20k tokens
    // re-prefilled every generation, because 4 GiB held about one conversation
    // and every switch evicted somebody. This cache is a ~330x lever (0.1s
    // restore vs 32.9s re-prefill), so undersizing it does not cost a little —
    // it turns every context switch into recomputation. The hallmark is 14
    // personas collaborating; at that bar the constant is catastrophic.
    #[test]
    fn the_prompt_cache_is_sized_by_citizens_not_by_slots() {
        let kv = 32 * 1024u64; // ~32 KiB/token, Ornith-class geometry
        let plenty = u64::MAX;
        let mib = |bytes: u64| (bytes / (1024 * 1024)) as u32;

        // ONE conversation vs FOUR: the cache must grow with the citizens that
        // share the slot, because that is who evicts whom.
        let one = super::host_prompt_cache_mib(&[60_000], kv, plenty);
        let four = super::host_prompt_cache_mib(&[60_000; 4], kv, plenty);
        assert!(four > one, "four citizens need more than one: {four} vs {one}");
        assert_eq!(four, mib(kv * 60_000 * 4));

        // THE HALLMARK, and why max-window sizing was wrong. Fourteen MIXED
        // citizens (most chatting ~9k, three solving ~63k) must land in a
        // feasible envelope — sizing all fourteen at the 134k model window would
        // claim ~60 GiB and make the bar look impossible.
        let mut fourteen = vec![9_000u32; 11];
        fourteen.extend_from_slice(&[63_000, 63_000, 63_000]);
        let mixed = super::host_prompt_cache_mib(&fourteen, kv, plenty);
        let all_max = super::host_prompt_cache_mib(&[134_000; 14], kv, plenty);
        assert!(
            mixed < all_max / 2,
            "measured per-citizen demand must be far cheaper than N x max-window: \
             {mixed} MiB vs {all_max} MiB"
        );
        assert!(
            mixed < 16 * 1024,
            "fourteen mixed citizens must fit a sane envelope, got {mixed} MiB"
        );

        // THE GOVERNOR STILL WINS: affordability is a ceiling, never exceeded.
        let squeezed = super::host_prompt_cache_mib(&[60_000; 14], kv, 6 * 1024 * 1024 * 1024);
        assert_eq!(squeezed, 6144, "the lease's affordability clamps the want");

        // The floor is ONE conversation, DERIVED — never a model-specific
        // constant. Below it every switch is a guaranteed full re-prefill.
        let floor_only = super::host_prompt_cache_mib(&[63_000], kv, 1);
        assert_eq!(
            floor_only,
            super::CACHE_RAM_HARD_FLOOR_MIB,
            "fully starved we still pass a real number, never a zero"
        );
        assert!(
            super::host_prompt_cache_mib(&[], kv, plenty) >= super::CACHE_RAM_HARD_FLOOR_MIB,
            "no citizens yet is a cold start, not a zero-size cache"
        );
    }


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
            CACHE_RAM_MIB, // the cold-start prior — these tests pin arg shape, not sizing
            &PathBuf::from("/tmp/kv-pages/test"),
        )
    }

    // what this catches: the KV disk-paging tier silently amputated. Slot
    // save/restore (`/slots/{id}?action=…`) only exists when the server was
    // LAUNCHED with `--slot-save-path`; a lane missing it turns every rotation
    // on an oversubscribed server into a full re-prefill (hit_rate 0.0 across
    // all acts, 2026-09-01) while the code above it believes paging works.
    #[test]
    fn slot_save_path_is_part_of_the_unconditional_spine() {
        let i = inv(4, 65_536);
        assert_eq!(
            i.value_of("--slot-save-path"),
            Some("/tmp/kv-pages/test"),
            "the paging tier must be launched into existence: {:?}",
            i.args
        );
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
        for f in ["--cache-type-k", "--flash-attn", "--mmproj", "--spec-type"] {
            assert!(!i.has(f), "{f} must be absent with default options\n{:?}", i.args);
        }
        assert!(i.envs.is_empty(), "no options must mean no environment");
    }

    // what this catches: GPU offload left to the backend's default. Metal defaults to
    // every layer, CUDA to none — so "absent" meant "all" on the Macs and "CPU" on
    // every CUDA host (2026-09-05, BigMama's 5090 idle at 674 MiB while a 24B model
    // ran on CPU and the lane wedged 13 times). The flag is present on every launch
    // and its value is the placement decision: all layers for GPU, 0 for CPU.
    #[test]
    fn gpu_offload_is_always_explicit_all_for_gpu_zero_for_cpu() {
        let gpu = inv(2, 8192).with_options(&LaneOptions::default());
        assert_eq!(gpu.value_of("--n-gpu-layers"), Some(ALL_GPU_LAYERS));
        let cpu = inv(2, 8192).with_options(&LaneOptions {
            cpu_only: true,
            ..LaneOptions::default()
        });
        assert_eq!(cpu.value_of("--n-gpu-layers"), Some("0"));
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

    // what this catches: the Flash-Next serving contract (measured 2026-08-28, the
    // first local serve). Three failure shapes, each hit live that night: (1) host
    // pins emitted as a SECOND --override-tensor beside the expert pager's — llama's
    // repeated-flag parsing risks last-wins (the --lora collapse shape), so both
    // MUST merge into one comma-joined value; (2) fit/warmup flags silently absent →
    // instant Metal OOM (fit counts the 35.8 GB pinned table as loadable) / a 36 GB
    // warmup fault-in; (3) max_ubatch RAISING the lane default instead of only
    // capping it (the default is sized to the resident lane's compute headroom).
    #[test]
    fn host_pins_merge_into_one_override_and_prefs_flags_land() {
        let i = inv(1, 4096).with_options(&LaneOptions {
            expert_ot: Some(r"blk\.(3|7)\.ffn.*_exps=CPU"),
            host_pinned_tensors: &["per_layer_token_embd.*"],
            fit_off: true,
            no_warmup: true,
            max_ubatch: Some(512),
            ..Default::default()
        });
        // ONE --override-tensor, comma-joined, pager first then pins.
        assert_eq!(i.args.iter().filter(|a| *a == "--override-tensor").count(), 1);
        assert_eq!(
            i.value_of("--override-tensor"),
            Some(r"blk\.(3|7)\.ffn.*_exps=CPU,per_layer_token_embd.*=CPU")
        );
        assert_eq!(i.value_of("-fit"), Some("off"));
        assert!(i.args.iter().any(|a| a == "--no-warmup"));
        assert_eq!(i.value_of("--ubatch-size"), Some("512"));

        // The cap only lowers: a ceiling above the lane default leaves it alone.
        let j = inv(1, 4096)
            .with_options(&LaneOptions { max_ubatch: Some(1 << 20), ..Default::default() });
        assert_eq!(j.value_of("--ubatch-size"), Some("2048"));
    }
}
