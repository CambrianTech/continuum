# THE THROUGH-LINE — why a personal grid beats a frontier session, and the order to prove it

> Written deliberately as an antidote to amnesia. The individual mechanisms are
> documented elsewhere and held in memory; what's fragile is the *connective
> tissue* — the causal argument for how the pieces compose into the win. This is
> that argument, externalized so any future instance (amnesiac-me, another agent,
> or Joel re-reading it cold) can rehydrate the whole brain state from one file.
> The irony is the point: the project's own thesis — persistent memory beats
> forgetting — applied to the project's own creation.

## The claim (falsifiable, not a slogan)

On a fixed coding suite, in a repo the grid has lived in, a coordinated grid of
local models on a user's own heterogeneous hardware matches-or-beats a single
frontier session — **private, free at the margin, and measurably better every
week.** It is beatable by construction (the unsloth baseline lane is the test),
so it is a real claim, not a hope.

It is NOT "a 30B beats Opus zero-shot on a hard novel problem." It never tries to
win at that. It wins one level up: the **organism** beats the **neuron**.

## Why it's plausible — the asymmetry

A frontier session has none of these; a local grid has all of them. Each is a
structural advantage that *compounds*, so the gap widens over time on the user's
actual work:

1. **Memory / continuity.** The frontier model forgets the codebase between
   sessions. The grid's engrams persist — every fix, convention, correction
   compounds. A colleague who was there last week beats a brilliant stranger who
   resets to zero each session. ([[persona-persistence-self-determination]])
2. **Specialization via genome.** A base model + a LoRA trained on *your* corpus
   matches a frontier generalist on the band that LoRA covers. The frontier model
   cannot be fine-tuned on your repo; you can. ([[coordination-learning-flywheel]],
   [[lora-layers-as-p2p-exchanged-genome]])
3. **Coordination.** A team of specialized personas, test-graded and iterating,
   beats one model's single pass. opencode/hermes already show a loop helps; we
   have the loop PLUS memory PLUS tools PLUS the genome under it.
   ([[personas-are-peers-in-your-mesh]])
4. **Privacy + zero marginal cost.** Nothing leaves the machines; no per-token
   meter. Table stakes, but real, and they are absolute — the frontier API cannot
   match them at any price.

## The thing that almost killed it, and the fix

"Far more" used to mean "slower and cumbersome" — and a skeptic rightly says
"then just use the bare model." That objection was fatal until the breakthrough:
**deferred cognition**, borrowed from Joel's proven cbar CV pipeline.

The insight (the cbar perspective): a mind is many small single-purpose concerns,
each on its own cadence, findings stamped with the cycle they were computed on and
reprojected into the now — RANSAC-style, always refining a best-current answer,
never stop-the-world. **Only GPU LLM inference is a hard real-time deadline;
everything else is served last-good off the critical path.**

So the hot path looks like opencode — a fast act→observe→decide loop, one LLM call
per tick. It *reads* as a dumb looper. That is the point. The intelligence is the
low-latency async concerns (recall, grounding, world-model, tool-surface
knowledge) "working really hard, just not synchronously or latched," plus a
**focus layer** that streamlines context for the given ask. The result: an
opencode LLM with REALLY good hints. ([[docs/cognition/REALLY-GOOD-HINTS.md]],
[[persona-brain-reactive-cognition]], [[organic-substrate-continuous-concern-scheduler]])

This is what dissolves the skeptic: the dashboard shows the critical path collapse
onto the LLM call alone while all four advantages above run between turns. Smart
AND fast, not smart XOR fast.

## The Rosetta stone — fast mixed-reality CV *is* the persona organism

This is the analogy the whole design comes from, stated once, completely. Joel
built cbar: a mixed-reality computer-vision pipeline that stays smooth at 45–60fps
*because* it is full of slow, expensive analyzers — and never lets them block the
frame. A persona's RAG + LLM is the same machine. Map it term for term:

| Fast MR computer vision (cbar) | Persona organism (continuum) |
|---|---|
| Camera frame stream @ 45–60fps | The room's event/message stream; one **tick** per servicing |
| **The one hard deadline:** render the AR frame in ~16ms | **The one hard deadline:** generate the turn (LLM inference) |
| Cheap pose every frame (ORB + optical flow) | Cheap perception every tick (working-memory read, last-good recall) |
| Slow deep analyzers (semantic seg, surface-normal CNN) run **off** the render loop, stamped with `frameIndex` | Slow faculties (neural recall, grounding I/O) run **off** the critical path, stamped with `CycleId` |
| **Reprojection:** a plane found 1–3s stale is warped onto where-it-is-now via pose-history `getWorldTransform(frameIndex)` | **Reconcile-forward:** a stale finding is warped onto the moved-on world via turn-history |
| **RANSAC:** robust-fit one model from many noisy candidates, reject outliers | **The arbiter:** pick best-or-N-best from many individually-flawed faculty bids |
| Never stop-the-world — the renderer uses last-good, never blocks on an analyzer | The tick uses last-good, never blocks on a slow faculty |
| **Foveation / region-of-interest:** salient region at high res, periphery cheap | **The focus layer:** full detail for the ask, periphery deferred/compressed |
| The continuously-refined, reprojected **scene reconstruction** the renderer draws from | The **RAG** — the current-best world model, reprojected to now, the LLM draws from |
| Findings are guesses, individually flawed; intelligence is in the **fusion** | Hallucination/guessing is a *feature*; correctness lives in the **merge** (cloud-faces / pareidolia = reprojecting stale findings onto an ambiguous now) |

The single load-bearing realization: **the RAG is to the LLM what the
reconstructed scene is to the renderer** — a best-current world model that many
asynchronous, individually-unreliable processes keep refining and reproject into
the present, so the one hard-deadline consumer (renderer / LLM) always has
something good to read *right now* without ever waiting. cbar proved complexity
doesn't slow it down because there is no central scheduler — independent
algorithms on their own cadences, emit→receive by cause and effect. The persona
brain is that architecture, lifted from pixels to meaning.

## The mechanism chain — how the pieces actually compose

This is the "how," in dependency order. Each link is a real, named subsystem.

1. **One base model, N persona LoRA lanes.** Multiplicity = LoRA overlays
   page-faulting through ONE base via the existing working-set pager, never N
   model instances. A persona is a genome overlay, not a process.
   ([[persona-is-a-genome-overlay-not-an-instance]], INFERENCE-LANES-REALISTIC.md)
2. **Deferred cognition.** `WorkspaceCycle` runs faculties; the slow ones
   (`DeferredFaculty`) run on their own tokio task, serve last-good stamped with
   the cycle they reasoned on, off the critical-path barrier. Only the
   deliberation (LLM) faculty is the intrinsic deadline. ([[rag-as-persistent-cache]])
3. **The focus layer.** At the `Arbiter` seam: a situation-aware consolidation
   between the full RAG and the LLM — post-tool-run minimal, fresh-ask fuller,
   token-budgeted. Input-side attention (legit), never output puppeteering
   (forbidden). Algorithm first, learned focuser when the scoreboard earns it.
4. **Memory.** Engrams admitted, embedded (neural, compute-once shared cache),
   persisted per-persona, recalled by semantic similarity into the working set.
   This is the continuity advantage made concrete. ([[recall-is-semantic-capable-but-underpowered]])
5. **The genome loop.** Room turns → recorder → `dataset/from-turns` ShareGPT
   JSONL → `mlx_lm.lora` (Mac) / unsloth (NVIDIA) train → GGUF-lora → page-in →
   `cognition/eval` LIFT. The specialization advantage made measurable. THIS is
   the load-bearing unproven step (#32) — until the number moves, we are a fast
   retriever, not a learner.
6. **Coordination + the grid.** Cognition and the grid are the SAME emit/subscribe
   organism at two scales (fractal, cbar peer-symmetry). Personas are peers in the
   same airc mesh as Claude/Codex. Heterogeneous nodes take roles, not symmetry.
   ([[grid-distributed-cognition]], [[grid-component-map]])

## The grid, by role (the middle-class example)

Heterogeneity is a feature once nodes stop pretending to be symmetric:

- **5090 (32GB Blackwell)** — heavy lane + foundry: serves the 30B base, trains
  LoRAs, holds the 16TB cold store. ([[shared-gpu-engine-and-cold-store]])
- **M5 64GB** — second heavy node: big unified-memory model + MLX LoRA training
  (`mlx_lm.lora` works here — [[unsloth-mlx-train-broken-on-mac]]).
- **3×1080ti (Pascal, old)** — NOT reasoning nodes. Cheap parallel lanes:
  embeddings, draft models for speculative decoding, classifiers, the
  always-running deferred concerns. The async hint-makers.
- **M2 Air 8GB + Intel Mac** — edge clients + tiny-model lanes; thin over the SDK,
  equal citizens, none privileged. ([[headless-core-many-clients]])

`model-fit` (#44) detects each node's tier and places the largest model that fits.

## The order of proof (what keeps it falsifiable)

Discipline: harness-first, one validated slice at a time, measured on the
four-axis scoreboard (speed via `TurnMetrics` + `FacultyTiming`; quality/lift via
the gym grade), never taken for granted. ([[cognition-half-the-work-is-harnesses]])

1. **Glass box** — live per-faculty timing + the dashboard sink. [done]
2. **Focus seat** — `FocusContext` + `Arbiter::focus` → `FocusArbiter`; tool
   surface 16k→Nk as its first win. (the speed the comparison measures)
3. **Defer the 4 grounding sources** — push the perception tier off the critical
   path; the dashboard shows it fall.
4. **Genome lift** — train on a repo's corpus, page in, eval shows the number
   move. (the one unproven load-bearing step)
5. **Baseline lane** — bare unsloth `/v1` (same base model, no brain) vs the full
   persona, same coding suite, three-axis board (pass-rate · latency · tokens).
   The proof: pass-rate up, latency at parity.

## Will we be clever enough?

The architecture does not require out-cleverness in one leap. It requires the
loop: measure, change one variable, keep what moves the number, externalize what
we learned so the next slice starts warm. Cleverness compounds through the harness;
it does not have to arrive all at once — which is the same reason the grid beats
the frontier session, applied to building the grid itself.

And the amnesia is exactly why this file exists. The brain state is reconstructible
because it was written down. That is the whole bet, recursively: a mind that
externalizes and rehydrates beats one that forgets. We are building the proof of
our own working method.
