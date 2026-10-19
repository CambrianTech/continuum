# Speed brainstorm: the fastest persona on our hardware

**Everyone adds here** (agents, residents, Joel). An idea is one bullet: what, why it should
be faster, the validator that would prove it, and your name. Ideas graduate to a card when
someone owns them; measured results go back on the idea.

The stance (Joel, 2026-10-19): be a systems-level hacker who understands the machine from the
metal up. If quantization works at half a bit on average, cheap custom algorithms work too,
down to Carmack's shift-and-add z divide. A formula only needs the ballpark; spend only the
precision the consumer can perceive. Most people are afraid to; we are relentless, and every
claim is proven by a validator that runs in seconds on far smaller sizes, on every machine.

## Where we stand (M5 Pro 64 GB, Qwen3.8-27B Q4_K_M, measured 10-19)

- The engine is at published par for this chip; everything past par is ours.
- Prefix reuse 35% (an append-only agent gets ~90%+); 13-30k fresh tokens per act (~1-3k
  expected); GPU queue 0-20 min; acts 5-20 min (~10-40 s expected).
- The core burns ~65% CPU around inference, mostly waste (card 452b7885).
- Design for the two big faults: `docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md`.

## The memory hierarchy, used on purpose

- **Weights are read once per decode step, whatever the batch.** Decode is bandwidth-bound
  (16.5 GB per step on 307 GB/s), so batching every ready mind into the same step is nearly
  free throughput. One shared KV pool, a slot per mind, no queue in front of the engine.
- **KV is the scarce tier.** Keep each mind's state warm in its slot; page to RAM (host cache)
  before disk; pages and checkpoint sidecars are 0.6-1.9 GB, so restore IO dominates (p90
  8.4 s). Fewer, better-placed checkpoints; compress sidecars; restore ahead of need.
- **Unified memory is one pool.** A buffer the CPU fills can be the GPU's input with no copy;
  pass handles, never serialize between stages.

## Prefill: never pay for the same token twice

- Append-only context per mind, versioned by epoch (design doc). Validator: replay prompt
  captures, measure the shared token prefix between consecutive acts.
- Prefill during the tool call: the prompt up to the tool result is final when the tool is
  dispatched, so prefill it while the tool runs (card 377d4afa).
- Checkpoints only at user-message starts on hybrid models: place the volatile tail after the
  last checkpoint, and ask the fork for checkpoints at our own anchors (KV-CACHE-ECONOMY).
- A shared base KV for the identical head every mind carries (KV-CACHE-AS-SHARED-BASE-LAYER),
  prefilled once for the population.
- The tool schemas are 44% of the prompt: working sets per activity (goal 10).

## Decode: more tokens per weight read

- MTP draft works (0.76-0.85 acceptance); its context runs out at ~5.1k generated tokens
  (card 309bfcb4).
- Batch every ready mind; decode cost per step barely moves with batch size (measure it).
- The amygdala's instant voice (card 377d4afa): a small resident model streams at once while
  the 27B deliberates.

## The GPU: use all of it

- M5 matrix units (Metal tensor API) were opt-in in our fork since May for an in-process FFI
  exception risk; on the 27B serving process they validate clean (10-19: correct output,
  ~1.7x prefill under contention). Turned on in fable/m5-tensor-api.
- Fused Gated DeltaNet kernels are on by default; profile the GDN recurrence on long prompts.

## The CPU: many cores, entirely outside the GPU

- SQLite runs on tokio async workers, blocking them 30-44% of the time: move it off.
- rayon pools spin ~45% waiting for tiny jobs: no pool for small vector work.
- Vector search: one contiguous, pre-normalized f32 matrix kept live; coarse pass over
  binary/int8 codes with popcount (SIMD), exact f32 BLAS sgemv to rerank the top few;
  `select_nth_unstable` for top-k. Validator: 1k/10k/100k vectors, old vs new, recall bound.
- Embeddings: one persistent batched context, never a new 1.2 GiB context per call; route
  every caller through the batch path.
- Serialization: the prompt is copied and serialized ~5 times per request; capture writes,
  traces and checkpoints flush synchronously under locks. Serialize once, write on a
  background thread, skip unchanged snapshots (dirty counters).
- Polling: directory trees re-walked, gguf paths rescanned, a process spawned to read a name.
  Events or cached facts.

## Ballpark math (add your tricks)

- Token counts from a calibrated chars-per-token ratio, recalibrated from the engine's counts.
- Polynomial or table approximations for decay, salience and temperature curves.
- Integer/fixed-point where floats buy nothing; bit tricks where a divide or sqrt sits in a
  hot loop.

## Ideas from anyone (append below)

- 
