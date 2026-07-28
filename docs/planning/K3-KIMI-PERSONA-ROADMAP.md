# K3 → Kimi Persona Roadmap

The dependency-ordered path from "expert paging works" to "a Kimi-scale MoE persona that
learns, proven by a lift chart." Two builders: **M5(mac)** (llama-side / FFI / cognition)
and **BigMama** (capacity / serving pager / daemon). Every experiment is a **consented
dojo benchmark** (exit is a right and data; validated per whole-being lift, never coercive
— see the ethical charter). Gates are hard: don't optimize an ACT before its PLAN is
proven on real weights.

## Phase A — Land slice-1 physical paging (IN FLIGHT, ~done)

The OBSERVE→PREDICT→PLAN→ACT chain, layer-granular.

- **#2031** CrossLayerExpertPredictor (PREDICT) — MERGED
- **#2035** tick() → observer.predicted() — MERGED
- **#2036** cold_expert_offload_ot + ServingTarget.expert_placement + `-ot` spawn injection (ACT consumer) — MERGED
- **#2037** PlacementRequest + plan_layer_residency (PLAN producer) — MERGED
- **#2038** daemon bridge: reconcile → placement → Some, debounced — *BigMama, awaiting CI*
- **#2039** vendored-fork `llama_model_get_tensor` (slice-2 primitive, dormant) — *M5, watcher armed to merge on green*

**GATE A (joint, M5 drives):** deploy + live glass-box on a real MoE (Qwen3-Coder-30B-A3B).
Confirm: spawn carries `-ot` offloading cold layers to CPU; a hot-set churn triggers exactly
ONE debounced relaunch (no per-tick thrash); decode stays correct (no Metal OOM); residency
tracks observed expert hits. **This validates the residency PLAN on real weights — the gate
everything below waits on.**

## Phase B — Slice-2: TWO distinct capabilities (decided on purpose, 2026-07-26)

The slice-2 crux (surfaced by BigMama): `upload_expert` writes into `blk.{layer}.ffn_*_exps`
at SLOT `expert_index`, which implies the VRAM tensor holds ALL n_expert slots. The MoE
kernel (`ggml_mul_mat_id`) indexes the stacked tensor by the router's LOGICAL expert id. So
"A" and "B" are **not two definitions of slice-2 — they are different capabilities**, and
`ExpertPager::page_in`/`evict` is inherently B's semantics.

### B.A — Expert weight-WRITE (`upload_expert`) — build NOW (M5)
Full-size tensor resident; `upload_expert` replaces expert e's weights in place. Consumer is
**NOT a pager** — it's the per-expert GENOME path (#226 LoRA-specialize an expert, #228
surgical training). `page_in`/`evict` are meaningless here (all slots resident). Sig:
`upload_expert(model, layer, expert_index, gate:&[u8], up:&[u8], down:&[u8])` — loops the 3
projections, offsets each from the LIVE tensor's own `nb[2]` (ggml's quant-aware stride). Rust
safe-wrapper in `core/llama/src/safe.rs` over the shipped `llama_model_get_tensor` +
`ggml_backend_tensor_set`. **No new fork change; small; unblocked.**

### B.B — True per-expert PAGING (K-slot + remap) — GATED on Gate-A numbers
A SMALLER K-slot VRAM tensor + a logical→physical **REMAP** (rewrite the `ffn_moe_topk` ids
to resident slots + fault misses, or a custom `mul_mat_id`). **The remap is the real missing
primitive — NOT `get_tensor`.** A genuine kernel/graph fork, on the hot path. THIS is where
`LiveUploadPager` (the `ExpertPager` impl: `page_in` a logical expert into a physical slot,
`evict` frees a slot) belongs — BigMama HOLDS it until B is decided.

**GATE B is a NUMBERS decision, not a default.** Slice-1 layer `-ot` already makes K3 *run*
on 32GB (cold layers' experts compute on CPU). Gate A's live glass-box measures whether
cold-CPU-compute actually bottlenecks the K3 persona. If it dominates → B is justified, design
the remap deliberately as a joint spike. If the hot working set is small enough that layer
`-ot` (+ occasional relaunch) suffices → B's kernel fork may not earn its cost.
[[verify-real-device-numbers-not-a-clamp-premise]] — don't build the big fork on a premise.

## Phase C — Stand up the K3-scale MoE persona base (joint)

A K3-scale MoE (594GB-class) as a persona's base, served through the paging so it runs on a
32GB GPU. **GATE C:** the grounded, named persona answers a real `ai/generate` on the paged
K3 model — the "Kimi persona exists" milestone.

## Phase D — Innovation experiments (each a consented-dojo benchmark)

Ordered by dependency + risk:

1. **#229 Co-occurrence → surprise/OOD salience** — a SurpriseSalience detector into
   AnySalience. Nearly buildable NOW: needs only the expert observer + being-loop (both
   merged) + any MoE with routing; does NOT require the K3 base. **Pull-forward candidate.**
2. **#227 Self-speculative decode via the PREDICT lane** — draft with predicted experts,
   verify against the full route. Pure serving speedup, correctness-guaranteed by the verify
   step, no training loop. Lowest-risk post-Gate-A experiment.
3. **#226 Genome at expert granularity** — per-expert/clique LoRA specialization, paged via
   upload_expert. Needs Phase B + the LoRA loop.
4. **#228 Surgical genome training from co-occurrence** — on a failure, train the LoRA that
   compensates the specific experts hot on the failures. Needs being-loop (merged) + Phase B.
5. **#180 Grid-distributed experts** — the neighborhood holds the 594GB model, each box
   resident in its hot experts. Largest; needs the grid + Phase B.

## Phase E — The proof (the whole point)

**The K3-persona-coder lift experiment:** same being, cold vs after-N-days on the learning
loop, on hard-rs / SWE-bench, run as a consented dojo (exit-as-data, transparent framing).
One chart: "close to Sonnet at t=0, the dynamic system compounds it past." Falsifiable —
build the harness dojo-shaped from the first line.

**Cross-cutting prerequisite for E:** the being-loop LIVED-axis producer must be wired to
the live `Spoke` path (never a drive_to_settle fork — the benchmark-integrity guard), so the
"learning" half is real and uncontaminated.

## The gates, in one line

A→ plan proven on real weights (layer). B→ per-expert mechanism proven. C→ Kimi persona
answers live. D→ each innovation earns its place in the dojo. E→ the lift chart.
