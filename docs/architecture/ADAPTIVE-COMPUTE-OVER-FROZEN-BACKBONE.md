# Adaptive compute over a frozen backbone — the frontier-lite weapons

*M5 (Fable) synthesis 2026-07-25, answering Joel's ask: "leverage past research — not
academic — true ideas to harvest, new weapons for frontier-lite systems that do what
Claude does, a hell of a lot more efficiently on consumer misfits." Grounded against the
live field (citations at end). Durable so our engrams hold it.*

## The one reframe

**Frontier-lite = adaptive compute over a frozen borrowed backbone.**

- **Frozen borrowed backbone** = the thesis ([[frozen-borrowed-weights-as-imagenet-backbone-new-cognitive-transformer-on-top]]):
  big open weights are ImageNet-for-cognition. We never retrain the base. We compose
  control on top.
- **Adaptive compute** = spend flops only where the token needs them. The frontier runs
  UNIFORM compute — every token pays for every layer and (dense models) every parameter,
  because they can afford it. On a MacBook Air or a lone 5090, uniform frontier compute
  does not fit. Adaptive compute is not an optimization here; it is the admission
  criterion. **This is the exact class of trick the frontier has no incentive to build —
  which is why it's our moat, not our handicap.**

Joel's whole arsenal — custom GPT-3 controllers, fractals, U-Net skip paths — are adaptive-
compute mechanisms. So is everything we've already shipped. They compose into ONE system.

## The four axes of adaptive compute (arsenal × substrate × field)

| Axis | "Spend flops on…" | Joel's arsenal idea | Our substrate | Shipped? | Field (real, 2025-26) |
|---|---|---|---|---|---|
| **Width** | …the right experts | MoE controllers | `capacity/expert_residency.rs`, genome LoRA | ✅ shipped today | MoE routing + `-ot` offload; KTransformers |
| **Depth** | …only as many layers/loops as the token needs | **U-Net skip paths + fractal recursion** | resolution-field doctrine (depth = resolution) | ❌ **the real next build** | **Mixture-of-Recursions (NeurIPS 2025): 2× throughput, param-shared recursion + per-token depth router** |
| **Context** | …the KV that's actually attended | (KV = the misfit-VRAM ceiling) | paging substrate (extend residency brain to KV) | ❌ extend shipped brain | MoR selective KV caching; paged/quantized KV |
| **Location** | …local vs a stronger peer | frozen-teacher / escalation | resolution-escalation doctrine, the mesh | ❌ mesh build | **speculative/cascade decoding — draft local, verify strong; vLLM/TRT native 2026** |

The through-line: **width** is *which* experts, **depth** is *how many* layers, **context**
is *which* KV, **location** is *where*. All four are the same question — "how much compute
does THIS token deserve?" — asked on a different axis.

## The new weapon that emerges (build target)

**One learnable compute-router, per token, across width × depth × location.** The
sentinel-ai `ANNController` ([[sentinel-ai-anncontroller-is-the-learnable-expert-residency-controller]])
generalized from head-gating to a full compute policy: reads a token's state, emits a
decision — which experts (width), how deep to recurse / when to halt (depth), fault-vs-page
KV (context), draft-local-or-escalate (location). Tiny param count, trains cheap, pages in
as a genome overlay. **The frontier does not build this** (uniform compute is fine when
you're rich). We must — it's the only way frontier behavior fits misfit silicon. It is the
direct descendant of both Joel's from-scratch controllers and today's `expert_residency.rs`.

## The trap to name (the "not academic" discipline)

Joel built GPT-3 from scratch; the reflex is to **design a better base transformer**. The
thesis forbids it — that's the academic detour that burns the year. Harvest the ARCHITECTURE
(recursion, skip, adaptive compute) as **control overlays on frozen borrowed weights**, never
as a new base to train. MoR itself supports this: **recursive up-training converts an existing
pretrained model to a recursive one cheaply** — the frozen-backbone-compatible path, not
from-scratch. Rule: if a weapon requires pretraining a new base, it's the trap; if it's a
controller/policy/router over borrowed weights, it's a weapon.

## Methodical next move (outlier-validation, per CLAUDE.md — not a big-bang)

We have **outlier A** = width (`expert_residency.rs`, pure planner + sim-provable). Build
**outlier B = the most-different axis: depth** — a MoR-style adaptive-depth planner: given a
token/step state + live capacity, decide recursion depth / early-exit halt over a frozen
model. Same split as expert paging: the **pure-Rust planner brain is my lane** (sibling of
`plan_expert_residency` / `decide_lane` — reads a profile + capacity, returns a plan, sim-
provable, `never Python`); the **serving mechanics are BigMama's lane** (llama.cpp recursion
/ early-exit hooks, KV selective cache). If ONE controller interface fits both width-residency
AND depth-halting without forcing, the interface is proven and context + location slot in.
Then and only then generalize the router. Don't build all four; prove the interface on the two
extremes.

## Why this wins the actual ask

"Haiku-if-not-sonnet on a misfit" is not reached by a bigger base — the base commoditizes
(K3 > Fable 5 on the chart). It's reached by **spending a small model's compute adaptively so
it punches far above its uniform-compute weight**, plus distilling the *adaptive-compute policy*
(when to think deep, which experts, when to halt) from the frozen teacher — not just its
outputs. The paged local model is the vessel; adaptive compute + policy distillation is the
lever. See [[K3-AND-LOCAL-CODER-TARGETS]] for the vessel (Qwen3-Coder-30B-A3B), this doc for
the lever.

## Sources (live, 2025-26 — not paperware)

- Mixture-of-Recursions (NeurIPS 2025) — param-shared recursion + per-token depth router +
  selective KV, 2× throughput at equal accuracy, public code:
  https://arxiv.org/abs/2507.10524 · https://github.com/raymin0223/mixture_of_recursions
- M2R2: Mixture of Multi-Rate Residuals for efficient inference: https://arxiv.org/pdf/2502.02040
- Speculative decoding 2026 status (vLLM/TRT native, 2–3×), CPU/GPU heterogeneous (Dovetail),
  cascade serving (RLM-Cascade): NVIDIA dev blog + arXiv 2412.18934 / 2606.22840
