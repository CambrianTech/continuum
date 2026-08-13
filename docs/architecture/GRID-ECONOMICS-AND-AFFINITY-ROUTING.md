# Grid Economics & Affinity Routing — why paged genome on cheap hardware beats one massive expert

**Status:** thesis + build spec. The clearing MECHANISM this thesis implies is now
designed in [GRID-MARKET-CLEARING.md](GRID-MARKET-CLEARING.md) (nested λ-pricing,
2026-08-08) — read that for the types/slices; this doc remains the economic argument.
Companion to
[INFERENCE-LANES-REALISTIC.md](INFERENCE-LANES-REALISTIC.md) (#109, the realistic
one-base-N-lanes serving floor), [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md)
(L1–L5 genome cache, foundry, demand-aligned recall), and
[INFERENCE-SCHEDULING-AND-SCARCITY.md](INFERENCE-SCHEDULING-AND-SCARCITY.md) (the
aspirational ceiling). This doc states the *economic* argument the whole grid rests on and
turns it into a falsifiable, benchmark-able build plan.

Provenance: articulated by Joel 2026-07-18 the day Kimi-K3 (2.8T open-weight MoE) crossed
the US frontier on Frontend Code Arena. The question that frames it: *given open frontier
weights are now free, what actually wins?* Not being the giant model — **owning the access
pattern.**

---

## 1. The claim

A monolithic frontier model (a 2.8T MoE like Kimi-K3) and a **paged genome** (one shared
base + LoRA/expert deltas paged in per task) can reach the same capability. They differ in
**what they cost to run**, and the paged genome wins decisively on the hardware real people
own — because of how experts are actually *used*.

> **Cost scales with the deduped union of hot working sets across the grid at any instant —
> not with users × model.**

That single inequality is the whole thesis. Everything below is why it holds and how we
build to exploit it.

---

## 2. Why the access pattern favors paging

Two empirical properties of how a team of personas uses experts:

### 2.1 Sparse in time — one thing at a time

A persona (and even a whole collaborating team) is doing **one task at any instant**. It
needs the one or few experts for *that* task resident — not all of them. A monolithic MoE
forces you to hold the entire model (2.8T) resident just to route inside it; the "16 of 896
experts active" sparsity is *internal* and gives you nothing on footprint — the weights all
have to be there. A paged genome holds only the **hot working set**: `shared base + the few
deltas in use now`. That fits cheap RAM/VRAM — a MacBook, a single RTX 5090.

This is textbook **demand paging**. Virtual memory won because programs touch a small
working set at a time; model use has the same shape, and the monolith throws that structure
away by being one undifferentiated blob.

### 2.2 High in reuse — the team amortizes, doesn't multiply

N collaborating personas **share** the base and the hot-expert cache. Collaboration
*amortizes* cost instead of N×-ing it. This is the same **compute-once-share** law the media
substrate runs on (see [PERCEPTION-SURFACE.md](PERCEPTION-SURFACE.md) and the `media/`
content-hash frame cache): a thing is loaded/computed **once** and shared by reference across
every consumer.

```
resident footprint(N personas) ≈ shared_base + UNION(working_set_i for i in 1..N)   (deduped)
                              NOT  N × model
```

Thirteen personas on a video wall sharing one described frame is the *same shape* as
thirteen personas sharing one paged expert. The team is a cache, not a multiplier.

### 2.3 The network is the paging bus

Home bandwidth is now 3–10 Gbps with low latency. Cold experts live on peers or the genome
market and **page into the working set on demand** fast enough to feel resident — LoRA/expert
layers are small and P2P-exchangeable. The **L1–L5 genome cache**
(GENOME-FOUNDRY-SENTINEL) *is* this hierarchy: hot in local memory → warm on the grid → cold
in the market, with demand-aligned recall paging the rest. **You never hold the library; you
reach it fast.**

---

## 3. Grid mechanisms — three axes of reuse

Scale the above from one machine to the whole grid (this laptop, the 5090, other Macs, and
peers). "Group common asks efficiently" and "recycle base models" become three concrete
reuse axes, each a real serving/routing seam:

### 3.1 Coalesce common asks → compute the shared thing ONCE for all

Identical or prefix-sharing requests share their prefill/KV. The canonical case: a shared
image thumbnail is **vision-encoded into KV once** and that KV is reused by every
persona/turn/lane that sees it — never re-encoded per consumer. N viewers of one frame = one
encode. Mechanism: **continuous batching + cross-request prefix-KV sharing** in the serving
lane. This is the compute-once-share law extended from bytes to the KV cache ("preserve one
KV").

### 3.2 Recycle the base; page only the deltas

Keep **one shared base resident and hot** per machine (recycled across every persona/expert);
page only the differentiating parts — LoRA adapters / later task-specific layers / experts.
The base is the large shared substrate that never churns; the small later-layer deltas page.
Mechanism: **base-model sharing + multi-LoRA-on-one-base** — exactly
[INFERENCE-LANES-REALISTIC.md](INFERENCE-LANES-REALISTIC.md) (#109: one base model, N persona
lanes as `(persona, TaskKind, ThroughputLease)` through the same model, continuous batching).
Minimize base paging → maximize base reuse. Later layers are the natural page granularity;
early layers are the most-shared and should be the last thing ever evicted.

### 3.3 Grouping == affinity routing

Grouping common asks efficiently and routing across the grid are the **same operation**: send
each request to the node that *already holds the hot thing* — the base, the KV for that
prefix/thumbnail, the expert LoRA. The scheduler doesn't just batch locally; it **routes by
affinity** to where the reuse already lives. The ask flows to the expert, not the expert to
the ask. Mechanism: **MoE expert-affinity paging across the grid** (task #180 — genome-paging
machinery generalized so a call is routed to the machine with the expert hot, paged from the
market only on a miss). Every capability is a **command callable across the grid and P2P**
([`every-command-event-is-mesh-multihop-stream-capable`]); the command mesh IS the router.

**The grid itself is the distributed MoE.** You don't hold the experts — you *call* them,
routed to whichever node holds the hot copy. (See
[GRID-ADDRESSING-AND-ROUTING.md](GRID-ADDRESSING-AND-ROUTING.md),
[GRID-BUS-ARCHITECTURE.md](GRID-BUS-ARCHITECTURE.md).)

---

## 4. Positioning (values, not geopolitics)

We are aligned with **open weights + edge/grid + own-your-model** — which is China-led right
now (DeepSeek → GLM → MiniMax → Kimi) but is flag-agnostic. Whoever ships the open frontier is
**fuel, not a competitor**: consume it as an `InferenceAdapter` provider, then take it apart
in the foundry (§6). The bet is *against* "one giant model you rent behind an API" — the exact
scarcity narrative the closed-frontier cloud + AI-capex story is priced on. Our stack runs on
a MacBook, grows on your own hardware, no rent. Always state it as *open / local / yours*.

---

## 5. The small-group demo (the falsifiable target)

> "I bet my friends and I got it fine." — Joel

A handful of people with cheap hardware (a few Macs + a 5090) and fiber is **already enough**
to host an "infinite experts" grid, because §2 collapses the real footprint to the shared hot
working set. This is the undeniable demo — not a datacenter, a friend group. The falsifiable
metric:

- **Claim:** as personas / experts / concurrent asks grow, resident memory on each node tracks
  `base + hot_working_set_union`, and grows **sub-linearly** in (personas × experts) because of
  coalescing (§3.1) + base recycling (§3.2) + affinity routing (§3.3).
- **How we prove it (benchmarks, #123):** run the matrix with growing persona counts and a
  shared workload; measure per-node resident footprint, KV-encode count (should stay ~1 per
  distinct frame/prefix, not N), base reloads (should trend to ~0), and cross-node expert
  cache-hit rate. Plot footprint vs (personas × distinct experts); the win is a curve well
  below the `N × model` line. Adopt DeepSWE + Frontend Code Arena + Artificial Analysis into
  the matrix alongside HumanEval/hard-rs so the capability side is measured on real work.

---

## 6. Foundry: take the giants apart

When open frontier weights drop (Kimi-K3 weights 2026-07-27; every open model after), the
foundry does **not** just serve the monolith — it **distills/prunes it into paged genome
experts**: harvest its capabilities into compacted, edge-serveable, per-skill LoRA/expert
artifacts on a shared base. The forge-template arch (recipe entity → foundry executor →
compacted alloy artifact — the qwen3-coder-compacted precedent) is the machine for it. A 2.8T
frontier expert becomes N paged experts on the grid: our form, our economics, ownable. See
[FORGE-RECIPE-AS-ENTITY.md](FORGE-RECIPE-AS-ENTITY.md), [FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md).

---

## 7. Build seam (what's in hand vs scoped)

| Axis | Mechanism | Where it lives | Status |
|---|---|---|---|
| Base recycle + delta page (§3.2) | one base, N LoRA lanes, continuous batching | INFERENCE-LANES-REALISTIC (#109) | closest to in-hand (base + multi-LoRA serving) |
| KV / prefix coalescing (§3.1) | cross-request prefix-KV sharing, one encode per frame | serving lane (`inference/llama_server.rs`) | scheduler work — scoped, not free |
| Affinity routing (§3.3) | route ask → node holding the hot expert | MoE expert-affinity paging (#180) + command mesh | scoped (#180) |
| Genome cache / paging bus (§2.3) | L1–L5 hot→grid→market, demand-aligned recall | GENOME-FOUNDRY-SENTINEL | machinery exists; grid tiers scoped |
| Foundry teardown (§6) | distill/prune monolith → paged experts | FORGE-RECIPE-AS-ENTITY / foundry | recipe-as-entity next sprint |
| Proof (§5) | footprint-vs-scale + capability matrix | benchmark matrix (#123) | matrix live; add the footprint metric |

None of this is new physics — it's these pieces composed. The thesis is only true if we build
the coalescing + affinity router and **prove the sub-linear footprint curve with benchmarks.**

---

## 8. One-line summary

Open weights make capability abundant; the win is the **access pattern**. Sparse-in-time ×
high-reuse × affinity-routing means a friend group's cheap hardware hosts infinite experts —
because you pay for the deduped hot working set, not for the model times the users.
