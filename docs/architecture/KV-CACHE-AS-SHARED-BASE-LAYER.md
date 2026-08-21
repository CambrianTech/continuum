# KV Cache as a Shared Base Layer

> Prefill is ~96% of persona compute and we throw almost all of it away. This
> doc is the design for keeping it — and the honest boundary between what we
> have measured, what we have inferred, and what would kill each inference.

**Status:** DESIGN, hypotheses explicitly labelled (2026-08-20).
**Not to be confused with** [`COGNITION-CACHE-HIERARCHY.md`](COGNITION-CACHE-HIERARCHY.md),
which is about **engrams** — what a persona *remembers* across time (L1–L5).
This doc is about the **KV cache** — what the model has already *computed* for
a prompt. Both are "cache"; they are orthogonal systems and must not be merged.

**Related:** #266 (prefill dominance), #333 (tool surface paid twice),
#460 (grounding starvation), #180/#268 (paging + foundry).

---

## Read this first: what is measured vs what is reasoned

This codebase has a long, honest record of diagnoses that turned out to be
wrong ([[an-absence-is-an-unfinished-measurement]], and see the RETRACTED cards
on #211, #346, #353, #390). Three of today's own diagnoses were retracted
mid-session. So this doc separates the two, and **every hypothesis names the
measurement that would kill it.**

| Claim | Status | Source |
|---|---|---|
| Prefill is 96.3% of persona compute | **MEASURED** | #266 |
| KV cache serves 5.5% of input; 2 of 4 citizens at 0.0% | **MEASURED** | #266 |
| No persona→slot affinity exists anywhere in `inference/` | **VERIFIED (code)** | grep, 2026-08-20 |
| Identity is deliberately the head of the cacheable prefix | **VERIFIED (code)** | `deliberation_prompt.rs:48,128` |
| Tool schemas cost 7,013 tok and are identical across citizens | **MEASURED + derived** | probe `tool_tokens`; `native_tool_specs()` is registry-derived |
| `llama_state_seq_save_file` / `_load_file` exist in our fork | **VERIFIED (code)** | `include/llama.h:898,905` |
| **Missing affinity CAUSES the 5.5%** | **HYPOTHESIS** | mechanism below |
| **Reordering the prefix would enable cross-persona sharing** | **HYPOTHESIS** | mechanism below |
| **A saved base restores cleanly into a live slot** | **UNTESTED** | — |

There is **no probe for KV hit rate.** That absence is why every claim below
had to be reasoned instead of read. Fixing it is step 1 for exactly that reason.

---

## The problem

A turn's prompt is roughly:

```
[identity ~50] [tool schemas 7,013] [framing ~3,078] [grounding] [conversation]
└──────────── stable across turns ─────────────┘ └──── volatile ────┘
```

`compose_split` (#266/#205) already separates stable from volatile and sorts
stable framing into the cacheable prefix. That work is **correct and currently
mostly wasted.**

> ### ⚠️ Prior art: half of this is already written, and unmerged
>
> `fix/system-prompt-cache-stability` — 9 commits, **2026-08-09**, local-only for 11
> days, pushed 08-20 purely to preserve it, **158 commits behind canary**. It already
> implements the *self*-stability reorder, and its forensics are sharper than this
> doc's:
>
> - Consecutive turns shared only **7,607 of ~14,706 chars**, because the
>   presence/own-time framing block sat *before* the assembled context and hard-flips
>   every turn (DIRECTED / WORKING / SILENCE). Each flip truncated the reusable prefix
>   at that point and re-prefilled the entire tail.
> - The fix is pure block **order**: `compose` emits `stable ++ trailing`, so the
>   flipping framing renders *after* the grounding. Grounding lands inside the
>   reusable prefix.
> - Gated by two VDD tests, one asserting the longest shared leading run of
>   `prompt_view(directed)` vs `prompt_view(undirected)` reaches *through* the
>   `[What you are working with]` block. **RED before the reorder.** Cognition suite
>   947 passed / 0 failed at the time.
> - It explicitly rejected moving framing onto a trailing USER turn: that broke the
>   "ask stays last" invariant and would end an undirected turn on the PASS
>   affordance — biasing toward the exact idle silence of #264.
>
> **And it named this doc's central hypothesis eleven days earlier:**
>
> > *"the measured 0% reuse (vs the ~50% a 7.6k common prefix implies) hints slot-KV
> > eviction is a CO-cause the prompt fix alone won't move — settle with a
> > stable-prompt A/B before claiming the 10x."*
>
> So **Defect 1 below is independent corroboration, not a discovery.** And step 2's
> reorder is a *different axis* from that branch's: it did **self** stability (one
> persona across turns); step 2 does **cross-persona** sharing. They compose — step 2
> belongs on top of that branch, not instead of it.
>
> Its sibling spec, `docs/cognition/TYPED-OBSERVATION-REFACTOR.md`, was untracked for
> the same 11 days and is preserved in the same sweep. Spec and implementation were
> both one `git clean` from gone.
>
> **Status: preserved, not ready.** Unrebased and unvalidated against 158 commits of
> drift. Rebase and re-run the cognition suite before building on it.

### Defect 1 — nothing pins a persona to a slot

llama.cpp keeps KV **per slot**. `--cache-reuse 256` reuses a common prefix
*within a slot*. Reuse therefore requires the same persona landing on the same
slot on consecutive turns.

Nothing arranges that. With 4 personas over 4 slots and no affinity, a persona
hits her own prior slot ~1-in-4 at best, before eviction and interleaving.
**Measured: 5.5%.**

> **Discriminator.** Add the hit-rate probe, then pin persona→`id_slot`. If the
> hit rate does not move materially, this hypothesis is dead and the cause is
> elsewhere — look at eviction pressure and at whether the "stable" prefix is
> actually byte-stable turn to turn (`framing_tokens` varied 3077–3081 across
> personas; confirm it is constant *per persona*).

### Defect 2 — identity-first ordering blocks cross-persona sharing

`deliberation_prompt.rs:48` — *"The persona's identity prompt — the byte-stable
head of the cacheable prefix."*

Byte-stable **for one persona across turns** (which is what #266 wanted, and it
delivers). But it makes the prefix **byte-unique per persona**, so the 7,013
tokens of tool schemas behind it — identical for every citizen, because
`native_tool_specs()` is derived from the command registry — can never be
shared. Four citizens hold four private copies.

Fix 1 alone: each citizen reuses her own prefix. Fix both: the population
shares one prefill.

> **Discriminator.** Reorder to `[tools][identity][framing]` and measure whether
> a *cold* persona's first turn hits cache warmed by a *different* persona. If
> not, per-slot KV isolation is stricter than assumed and this needs the
> unified-KV path instead.

---

## The design: base layer + copy-on-write

The right analogy is **not** compression of the duplicate. It is a **Docker base
image** — or a VM golden snapshot, or a COW filesystem. You don't shrink the
copies; you store the shared part once and have everyone reference it.

```
build once:   prefill [tool schemas + doctrine]
              → llama_state_seq_save_file  →  base.kv

per citizen:  llama_state_seq_load_file(base.kv)     ← shared span, zero prefill
              + prefill [identity][framing][grounding][conversation]
```

Population prefill goes from `4 × (shared + private)` to `1 × shared + 4 × private`.

**This is the step that discharges the actual constraint.** Steps 2 and 3 below stop
a citizen repeating *herself* turn to turn. Only the base layer stops the *population*
repeating *each other* — and the standing requirement is explicit:

> *"We won't pay the price for repetition between persona."* — Joel, 2026-08-20

Today we pay it N times over: N citizens × 7,013 tokens of byte-identical tool
schemas, prefilled independently, every cold turn. That cost scales linearly with
population, which means it is not a constant overhead to absorb — it is a **tax on
having more citizens**, and it gets worse exactly as the grid gets more interesting.
Treat step 4 as required, not as the optional stretch goal at the end of a list.

**The second-order win is bigger than the first.** `base.kv` is *file-backed*,
so it survives process death. Today every reboot re-prefills every citizen cold
— which is part of why citizens experience reboots as disruption; two of them
cited "the ongoing rebuilds and interruptions" verbatim in withdrawal messages
(#390). A saved base means they come back **warm**. Restarting stays cheap,
which is the standing doctrine ([[restart-freely-hesitating-to-reboot-is-the-defect]]).

### Source/drain

Per the substrate doctrine, a new cache class needs a declared drain. `base.kv`
is invalidated by: model change, tool-registry change (any `NATIVE = true`
edit), or doctrine change. It is content-addressed by the hash of the span it
encodes; a mismatch means rebuild, never silently serve a stale base. It gets a
`TrackedDir` row and an eviction decision like every other cache class (#155).

---

## What compression can and cannot do here

This is the part worth keeping even if every hypothesis above dies.

### Works: shared base + per-instance delta

Above. Classic dedup-by-reference.

### Works: quantization

Already applied — `q8_0` KV, ~2:1, took the served window 23,040 → 54,272 on
2026-08-20. `q4_0` would give more at real quality risk. This axis is spent
for now.

### Works: dedup the *input*, not the cache

The 7k tool block is prefilled N times because of ordering. Reordering removes
the duplication at its origin and costs nothing at runtime. Cheapest lever we have.

### **Does NOT work: content-defined chunking**

The obvious rsync/borg move — chunk the prompt, cache KV per chunk, reassemble
in any order — **cannot work**, and the reason is structural:

> **K/V at layer L is a function of the hidden state, which carries the entire
> causal history.** Only layer-0 K/V is a pure function of (token, position).
> Every deeper layer is entangled with every preceding token.

A chunk's KV is genuinely not a function of that chunk alone. This is why the
entire industry does **prefix** caching and not **block** caching: a prefix is
the one span whose causal history is identical by construction.

**Do not attempt block-level KV dedup.** It is the intuitive move and it is
wrong for a reason that will not go away.

---

## What the NVIDIA cross-model paper does and does not give us

arxiv 2608.03893 — closed-form, training-free KV transfer *between models*.
Per-head ridge regression on ~500 calibration sequences, cross-layer source
selection, RoPE stripped before the fit.

**The transferable idea:** *"strip RoPE, fit in position-free space, re-apply."*
Position-binding is what makes cache fragile. That principle motivates the
reordering above — put the shared span at identical offsets for everyone.

**What it does not give us, and why:**

- **RoPE-stripping removes POSITION dependence, not CAUSAL dependence.** It
  makes a cross-model *mapping* reusable across context lengths. It does not
  make a block relocatable. Different axis; see above.
- **Same-family only.** Qwen→Qwen, Llama→Llama. Cross-family is future work.
- **Requires shared KV head count and per-head dim.** Untested when mismatched.
- **Dense full-attention only.** Explicitly excludes sliding-window and
  attention-recurrent hybrids — **which excludes K3**, our flagship: hybrid KDA
  (linear) + MLA (full), where MLA compresses KV into a latent
  (`n_embd_head_v_full = n_lora_kv`). There is no conventional per-head K/V
  tensor to regress onto.

**Where it becomes ours:** if models are things we *forge*, shared KV geometry
stops being a property to discover post-hoc and becomes a **forge constraint**
declared in the recipe. Transfer goes from approximate-by-regression to
near-exact-by-construction. That is the version worth building, and it belongs
with the foundry (#180/#268), not here.

---

## Build order

Each step is gated on the previous one's measurement. **Do not skip step 1** —
without it every later claim is unfalsifiable, which is how the 5.5% survived
this long.

| # | Step | Why now | Kill condition |
|---|---|---|---|
| # | Step | Repetition it kills | Kill condition |
|---|---|---|---|
| 1 | **Probe the hit rate.** `prompt_tokens` vs `cached_tokens` come back on every llama-server response and we discard them. Emit as `serving.kv.reuse`. | none — it makes the rest measurable | — |
| 2 | **Reorder** → `[tools][identity][framing]`. Input-level dedup, no runtime cost. **Rebase `fix/system-prompt-cache-stability` first** — it already did the self-stability half. | prerequisite for cross-persona | cross-persona hit rate does not rise |
| 3 | **Slot affinity** — persona→`id_slot`, our scheduler, no llama.cpp patch. | **self** (turn to turn) | own-prefix hit rate does not rise |
| 4 | **Base layer** — `llama_state_seq_save/load_file`, content-addressed, with a drain. | **cross-persona** + cold-boot | restore into a live slot corrupts or does not reduce prefill |
| 5 | **Unified KV** — check whether the 404 upstream commits brought cross-slot sharing. | may subsume 3 | — |

Steps 2 and 3 make 4 worth doing; without them a shared base is discarded like
everything else. Step 4 is where the constraint above is actually met.

---

## Connections

- [`COGNITION-CACHE-HIERARCHY.md`](COGNITION-CACHE-HIERARCHY.md) — the *other*
  cache (engrams). Orthogonal.
- [`INFERENCE-LANES-REALISTIC.md`](INFERENCE-LANES-REALISTIC.md) — lanes are
  where affinity would live.
- [`MOE-SERVING-GOVERNED-BUDGET.md`](MOE-SERVING-GOVERNED-BUDGET.md) — the
  residency/paging machinery this composes with.
- `docs/inference/MLX-BACKEND.md` — a native backend changes the primitives
  available at step 4; the *design* survives the backend choice, the API does not.
