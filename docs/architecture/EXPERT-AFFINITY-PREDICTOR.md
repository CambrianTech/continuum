# Expert-Affinity Predictor — K3-level intelligence on a 32GB card

> Serving a 594GB MoE (Kimi K3-class: base + ~896 experts) on a 32GB GPU is not a
> memory problem. It's a **prediction** problem. Reactive paging stalls on an SSD
> read every miss — glacial. K3-level *speed* comes from predicting the hot experts
> and prefetching them *before* the router needs them. The predictor's accuracy is
> the whole game. This is the frontier-lite thesis literally: adaptive compute (a
> learned prefetcher) over a frozen borrowed backbone — the width axis of
> [`ADAPTIVE-COMPUTE-OVER-FROZEN-BACKBONE.md`].

## The killer insight — the forward pass's own depth is the prefetch window

A transformer forward pass is many layers deep. If a *cheap side-predictor* guesses
layer-N's experts from layer-1's activations (or the prompt embedding), we get **the
entire forward pass's compute time** to stream the needed experts SSD→VRAM. That
converts a **latency-bound** problem (every miss = a stall) into a **bandwidth-bound**
one (can SSD GB/s × the pass window stream the working set?). That conversion is what
makes it *fast*, not merely *possible*. Everything below serves that conversion.

## The loop: OBSERVE → PREDICT → ACT

The intelligence is already built and **blind**. `capacity/expert_residency.rs` has the
whole brain — `ExpertActivationProfile { gate_magnitude, hits }`, `priority() = hits +
tanh(mag)·0.999`, `plan_expert_residency() → {hot,warm,cold}`, N-tier
`plan_tiered_residency()` — **sim-only, its `hits` field empty because nothing feeds
it.** The design is to give it eyes, then a predictor, then let it prefetch.

### OBSERVE — the keystone (greenfield but *defined*)
The signal exists in-graph: `core/vendor/llama.cpp/src/llama-graph.cpp:1376` tags the
per-token expert selection as `cb(selected_experts, "ffn_moe_topk", il)`. Nobody taps
it — `core/llama/src/` registers no ggml eval-callback. **Build it:** register a
`cb_eval` in the safe binding, capture `selected_experts` (an argsort-top-k of expert
indices, `[n_expert_used, n_tokens]`) whenever `node.name == "ffn_moe_topk"`, map
`(il, expert_index)` → `ExpertId{layer, expert}`, and increment
`ExpertActivationProfile::hits` (`expert_residency.rs:61`). This closes the loop the
whole seam doc assumes — "measured beats predicted, the PGO principle"
(`gate_magnitude.rs:1-16`). Until `hits` is fed, the static gate-magnitude prior
(`gate_magnitude.rs:58`, L2 norm, no forward pass) is the cold-start floor.

### PREDICT — the creative core (fully greenfield)
The existing `priority()` is *reactive* (historical hits + magnitude). The frontier-lite
part is *ahead-of-time sequence prediction*. Build the prediction hierarchy, floor→ceiling:

1. **Static floor** — the globally-hottest experts always resident. The compaction tool
   proved this offline (`tools/scripts/compaction/analyze_gate_weights.py` gate-L2 +
   `profile_expert_activation.py` activation tally); port its ranked profile as the seed.
2. **Context-conditional** — which experts fire depends on *content*. Pre-warm the
   resident set from the prompt's domain/embedding, per (persona, TaskKind) — the seam
   doc's `ExpertRoutingProfile` histogram. A coding turn and a medical turn want
   different resident sets. "Demand-aligned recall," for experts.
3. **Cross-layer speculation (THE core)** — early-layer routing predicts late-layer
   experts. Learn the layer-L → layer-{L+1..N} co-occurrence from observed sequences;
   at serve time, from the first layers' `ffn_moe_topk`, predict the rest of the pass's
   experts and prefetch them *during* the pass. This is the forward-pass-depth-as-window
   insight made concrete. Cheap predictor (a small learned head or a co-occurrence
   table), governed by the SSD→VRAM lead-time budget (calibrate per Open-Question Q3).
4. **Affinity clustering** — experts co-fire; page *clusters* to amortize I/O (#180's
   "grid is the distributed MoE" affinity, `capacity/grid.rs::AffinityFitPolicy`).
5. **Sentinel-PGO** — a learned policy watches live firings and continuously updates the
   resident/prefetch policy; hit-rate raises/lowers speculation aggressiveness (the
   `GENOME-FOUNDRY-SENTINEL.md:283,750` speculator doctrine, `SpeculationMissRate`
   throttle). Note the shape: **the sentinel is a mind getting better at predicting its
   own experts from its own lived usage** — the same loop as
   [`SELF-IMPROVING-MEMORY.md`], one level down.

### ACT — prefetch (BigMama's lane)
Drive expert VRAM promotion from `ExpertResidencyPlan.hot/warm` using
`genome/blob.rs::expert_ranges(expert_index)` (`base + e·stride` byte slices) +
`PageOffset::Expert{expert_index}`. The `--n-cpu-moe`/`--override-tensor` static split
first (`EXPERT-PAGING-GOVERNOR-SEAM.md:99-111`), then per-expert fault. **The prediction
emits prefetch requests; the pager moves the bytes.** Clean seam between the two lanes.

## VDD-gated slices (outlier-validated, smallest-first)

- **Slice 1 (keystone — mine): the OBSERVE seam.** ggml `cb_eval` in `core/llama/src/`
  taps `ffn_moe_topk` → `ExpertActivationProfile::hits`. **Done when:** a live MoE turn
  produces a hit histogram that matches the model's actual routing on a known prompt
  (assert against a tiny MoE where the routing is enumerable). This is the make-or-break
  foundation — everything downstream is blind without it — and it's the "measured beats
  predicted" PGO signal the whole `expert_residency.rs` brain waits for.
- **Slice 2 (creative core): cross-layer speculative prediction.** From observed
  sequences, build the layer-L→L+k co-occurrence predictor; measure prediction accuracy
  offline (replay observed traces), then online. **Done when:** predicted prefetch
  hit-rate beats a reactive-LRU baseline on a held-out trace — the number that says the
  forward-pass window is being used.
- **Slice 3: the sentinel-PGO loop.** Wire `plan_expert_residency` to the live `hits` +
  the predictor; raise/lower speculation aggressiveness by measured miss-rate. **Done
  when:** a persona serving K3 gets measurably faster over a session as its profile warms
  — self-improving, glass-boxed.
- **Slice 4 (grid, #180): affinity routing** — a node gossips which experts it's hot
  for; expert-heavy work routes to the resident-hot peer. "The grid is the distributed
  MoE — you don't hold the experts, you call them."

## Non-negotiables

- **Measured beats predicted.** Gate magnitude only seeds + breaks ties; live hits
  dominate (`priority()`). Observe before you predict.
- **Sim-provable, pure planners; NO Python at runtime.** The compaction tool was the
  offline proof; the product is the Rust pager (`expert_residency.rs` shape — profile +
  capacity → plan, unit-testable).
- **Prediction is a HINT, never a gate.** A missed prediction evicts as normal LRU —
  no penalty, no wrong answer (`GENOME-FOUNDRY-SENTINEL.md:773`). Correctness never
  depends on the predictor; only *speed* does.
- **The LLM's routing decides which experts run — we only decide which are RESIDENT.**
  We never override the router. Same doctrine as the cognition pipeline: the model
  decides; the substrate provisions.

## Where it lands (no map drift)

| Concern | Lives in |
|---|---|
| OBSERVE: ggml callback tap of `ffn_moe_topk` | `core/llama/src/` (safe binding) + a hit sink into `capacity/expert_residency.rs` |
| PREDICT: cross-layer speculator + context-conditional | new `capacity/expert_predictor.rs` (planner-shaped: profile → prefetch plan, pure) |
| ACT: expert VRAM promotion | BigMama — `genome/` pager driving `expert_ranges()` |
| Residency plan (exists) | `capacity/expert_residency.rs::plan_expert_residency` |
| Grid affinity (exists sim-only) | `capacity/grid.rs::AffinityFitPolicy` (#180 live consumer) |
