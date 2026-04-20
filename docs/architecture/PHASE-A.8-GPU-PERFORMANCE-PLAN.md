# Phase A.8 — GPU Performance: Smart AND Fast (No Crippling)

> *"I'd prefer slow over stupid. We need to be smarter about speeding it up and not cripple our models."* — Joel, 2026-04-19

**Parent:** [SHARED-COGNITION.md](SHARED-COGNITION.md) · [PERSONA-COGNITION-RUST-MIGRATION.md](PERSONA-COGNITION-RUST-MIGRATION.md)

This is the design plan for Phase A.8: making local persona inference fast on Mac Metal / Linux CUDA WITHOUT capping the reasoning preamble. Performance gains come from architecture, not from making the model dumber.

---

## Hard rule

**Token budget for a reasoning model is non-negotiable.** Output budget must accommodate `reasoning_preamble + visible_response` at the model's natural rate. If a "speed up" requires reducing that budget below the floor, it's the wrong fix. Reasoning preamble of 500-800 tokens is the FEATURE, not noise to suppress.

This rule rules OUT:
- Capping `max_tokens` below the reasoning floor
- Disabling `<think>` mode globally
- Aggressive truncation of generated reasoning
- "Concise mode" that prevents the model from working through a problem

This rule rules IN everything below.

---

## Where the time goes today (qwen3.5-4b on M-series Metal via DMR)

Measured 2026-04-19, Round 8 chat-validate session:

| Stage | Time (one persona, cold) | Notes |
|---|---|---|
| Prompt processing | ~3-5s | 1500-2000 input tokens |
| Reasoning preamble decode | ~10-15s | 500-800 tokens at ~50 tok/s |
| Visible response decode | ~10-30s | 500-1500 tokens at ~50 tok/s |
| Strip-thinks + parse + post | <50ms | Rust, fast |
| **Total per persona** | **~25-50s** | Single-persona path |

When 4 personas fire concurrently:
- DMR is single-slot by default → 4× serialization → ~100-200s wall time
- OR DMR errors with `error sending request` (memento observed, tracked #948)

The pain: 4 personas responding to one chat message takes 100-200 seconds. User sits and watches. Even one persona at 25-50s is sluggish.

---

## The plan — six fronts, ordered by ROI

### 1. **Streaming** (biggest perceived-latency win)

Today: persona waits for the FULL inference to complete (reasoning + response), then posts the visible text in one shot. User sees nothing for 25-50s.

With streaming: token-by-token. The user sees the persona "type" the visible response in real time. The reasoning still happens behind the scenes (collapsed in UI per Joel's "thinking should be split out and collapsible") but the wall-clock-to-first-visible-character drops to ~15s (after reasoning) and to ~5s (with the chat-UI showing "Thinking..." indicator during reasoning).

**Implementation:**
- Add SSE/chunked support in `openai_adapter.rs::generate_text_streaming` (variant of existing).
- New IPC command `cognition/respond-stream` that emits chunks via the existing event broadcast surface.
- TS shim subscribes to chunks, emits `chat:typing-update` with the running visible text. Current `Spoke` text is the final state.
- Strip-thinks helper runs incrementally — first `</think>` close marker is the trigger to start emitting visible text.

**Cost:** medium effort. The hardest part is keeping the strip-thinks state machine streaming-safe.

**Win:** time-to-first-character drops from 25-50s → 5s. Total time unchanged, but UX feels alive.

### 2. **KV-cache prefix reuse** (biggest steady-state win)

llama.cpp + DMR support prefix-KV-cache reuse: if the first N tokens of two prompts are byte-identical, the KV computation for those tokens is reused. We already have the architectural prereq: `RAGComposer` sorts sections by `(tier, sourceName)` deterministically (see `MULTIMODAL-WORKER-AND-PREFIX-REUSE.md`). The system prompt + invariant RAG sections are stable across messages.

The miss today: each chat message changes the recent-history tail, which is included in the prompt body. Even though the system prompt is stable, the changing tail invalidates the cache for everything after the divergence point.

**Implementation:**
- Set `cache_prompt: true` on llama.cpp/DMR requests (free win when the first N bytes match).
- Restructure prompt assembly so the most stable sections come FIRST (system + identity + invariant RAG) and the volatile tail (recent history + current message) comes LAST. We already have tier ordering — verify it's tight.
- For analyze() specifically, the system prompt is FULLY invariant — every analyze call should hit the prefix cache.

**Cost:** low effort (mostly verification + a flag). Most of the work is already done in the prompt-tier assembly.

**Win:** prompt processing time drops from 3-5s → ~100ms after the first call per session. Saves several seconds per persona per turn.

### 3. **Smaller analyzer model** (biggest analyze-path win)

The shared analysis runs once per inbound message. It's just JSON extraction — no reasoning needed (we want the model NOT to think for analysis; that was the whole point of `enable_thinking=false`). qwen3.5-4b is overkill for this task and pays the reasoning preamble cost even when we tell it not to (because reasoning models still emit the preamble structurally).

Switching the analyzer to a small non-reasoning model (e.g., `qwen2.5-1.5b-instruct`, `gemma2-2b-it`, or even a 0.5B model) gets:
- Total analyze time: 25-30s → 2-5s
- Smaller VRAM footprint → can keep loaded alongside the main reasoning model
- Still emits parseable JSON (smaller models follow `response_format` more reliably than reasoning models)

**Implementation:**
- Add `DEFAULT_ANALYSIS_MODEL` config (already a constant in `shared_analysis.rs`); change to `qwen2.5-1.5b-instruct` or similar.
- Pull the model into DMR at install time (extend `install.sh`'s default-model pull list).
- Re-validate that the smaller model produces correct shared-analysis JSON across 50+ representative messages.

**Cost:** low effort, requires download of one small model.

**Win:** analyze() drops from ~25-30s to ~2-5s. Saves the bulk of the per-message overhead.

### 4. **DMR multi-slot / batched inference** (biggest concurrency win)

DMR runs llama.cpp/llama-server with default `n_seq_max=1` — single in-flight slot. 4 personas all trying to render → 3 wait, 1 runs. With `n_seq_max=4`, llama.cpp batches the 4 requests in a single forward pass, sharing the KV cache, AT NEAR-FREE COST per additional sequence (the GPU is already paying for the matmul; batching just adds rows).

**Implementation:**
- Bump DMR config `n_seq_max=4` (or `n_seq_max=N_personas`).
- Verify Mac Metal can handle the increased VRAM (4× sequence's KV state).
- Adjust `InferenceCoordinator` (TS) to allow N concurrent admissions instead of serializing through 1 slot.
- Per #948: failure mode if VRAM insufficient is `error sending request`; need graceful queue+retry with backoff.

**Cost:** medium effort. VRAM math + admission tuning + #948's queue work.

**Win:** 4-persona chat-turn time drops from 100-200s → 30-50s (the time of one persona, not four).

### 5. **Speculative decoding** (smaller per-token win, large model only)

llama.cpp supports speculative decoding: a small "draft" model (qwen3-0.5B) speculates next tokens, the big "target" model verifies in batch. When the draft is right (~80% of tokens for code/factual stuff), throughput goes up 2-3×.

**Implementation:**
- Configure DMR with both target (qwen3.5-4b) and draft (qwen3-0.5b) models.
- Enable `--draft-model` in DMR's llama-server invocation.
- Validate quality preservation (speculative decoding is exact when draft proposals are accepted — no quality loss; just faster).

**Cost:** low effort once we ship a draft model alongside.

**Win:** 50 tok/s → 100-150 tok/s. Cuts decode time roughly in half.

### 6. **Two-tier model strategy** (architectural)

Long-term: the local stack should have:
- **Analyzer**: 1-2B non-reasoning model (qwen2.5-1.5b or gemma2-2b). Fast, structured-output reliable. Used for analyze() + signal classification + any short structured tasks.
- **Renderer**: 4-8B reasoning model (qwen3.5-4b-code-forged, future qwen3.5-7b-forged). Used for the actual persona response. The reasoning IS the value here.
- **Embedder**: existing fastembed (AllMiniLML6V2 384d). Already correct.

Each tier serves the task it's right for. Analyzer is sub-second; renderer takes its time but only runs when needed (silence-with-reason filters out cases where no render is warranted).

**Cost:** low effort once smaller analyzer is in place; this is just the architectural framing the rest of the plan implements.

---

## Estimated combined impact

| Scenario | Today | Phase A.8 | Improvement |
|---|---|---|---|
| Single-persona response, cold | 25-50s | 5-10s | 3-10× |
| 4-persona response, concurrent | 100-200s | 10-15s | 10-20× |
| Time-to-first-visible-character | 25-50s | 1-3s | streaming |
| Analyze() per message | 25-30s | 1-3s | smaller model |

Total: shared cognition becomes felt-instant on consumer hardware while keeping every model fully reasoning-capable. **Smart AND fast, no crippling.**

---

## Sequencing (which order to ship)

1. **Streaming** (UX win — first-character-to-screen drops from 25-50s to <1s). Medium effort. Memento taking lead.
2. **Smaller analyzer model** (eliminates 25-30s analyze tax). Low effort, low risk. Anvil taking lead.
3. **DMR multi-slot** (paired with #948 fix; unlocks concurrency). Config change + admission tuning.
4. **KV-cache prefix reuse** (verify already-working — `prompt_assembly.rs` produces byte-stable output via deterministic section ordering, see `MULTIMODAL-WORKER-AND-PREFIX-REUSE.md`). Should hit on analyze() cross-persona. Verify; fix any leaks.
5. **Persona warmup** (memento's idea — on persona init, send a no-op request to DMR to prewarm KV. First real user turn is fast).
6. **Skip-analyze for 1-persona rooms** (memento's idea — short-circuit if only one persona is a responder candidate. Saves an inference call per message in single-persona rooms).
7. **Speculative decoding** (small draft + large target, 2× steady-state). Research first — DMR support unclear.
8. **Batch multi-persona renders** (one DMR call serving N personas at once). Advanced; complex prompt coalescing. Phase B+ territory.

Each is its own PR. None block the others. Ship as ready.

---

## Reasoning-quality risk tracking per item

Some items above touch the "no crippling" floor — flagging which need quality A/B tests before shipping:

| Item | Reasoning risk | Mitigation |
|---|---|---|
| Streaming | None — model still runs full reasoning, we just show partial output | N/A |
| KV cache reuse | None — same model, same compute, just cached prefix | N/A |
| DMR multi-slot | None — same model, multiple sequences in batch | VRAM pressure check |
| Smaller analyzer model | **Yes** — quality of analysis JSON depends on capability of analyzer model | A/B 50+ messages: does smaller model produce same `suggested_angles` quality as 4B? Block on this passing. |
| Persona warmup | None — just KV pre-population | N/A |
| Skip-analyze single-persona | None — render path runs in full | N/A |
| Speculative decoding | None — exact decoding, just faster when draft is right | Verify llama-server flag works correctly |
| Batch multi-persona | Maybe — depends on whether per-sequence sampling preserves per-persona temperature | Check llama.cpp batch sampling support |

The principle: every item needs a "quality preserved?" answer before ship. If "no" or "unknown," it doesn't ship until validated.

---

## Out of scope for Phase A.8

- LoRA-aware inference batching (Phase B+ when LoRA composition is runtime)
- Cross-machine inference distribution (Grid feature; separate roadmap)
- Aggressive quantization beyond Q4_K_M (already shipped in the local model)
- Replacing DMR with our own llama-server fork (we use DMR for the install ergonomics; our perf wins should inform DMR config, not fork it)

---

## Status

Design draft, not started. Next sprint after PR #947 merges. Each rung opens its own PR following the same Rust-first / net-negative-TS discipline as #947.
