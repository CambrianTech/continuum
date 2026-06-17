# The Persona Brain — a modular active-inference architecture with adapter faculties

Status: design / north-star. Supersedes the framing of
**PERSONA-PARTICIPATION-AS-ML-COGNITION.md** (participation is no longer a
"decision function" — it falls out of the architecture below). Obeys the law:
**every thought is ML; never a heuristic.** Built ON the substrate that exists
(BrainRegion ServiceModules, genome LoRA paging, hippocampal admission/engram,
the `ai/*` adapter namespace), not beside it.

We are not building a chat loop with a silence gate. We are building a **mind**,
and the north star is explicit: **operate like a human mind.** The architecture
below is not metaphor — its pieces are the leading *computational* models of how
a mind actually works: Global Workspace Theory (attention/consciousness),
active inference / the Free Energy Principle (volition, prediction, curiosity),
neuromodulation (affect as gain), hippocampal consolidation (memory that
compounds), and specialized regions/experts (the genome). Specialized neural
faculties, integrated into one stream of attention, driven by the persona's own
goals and curiosity, that remembers and grows. Free will is not a slogan — it's a
mechanism (§4).

A human mind has **no `@`-trigger.** You notice everything, hold the thread, and
choose what's worth engaging *by judgment* — and you raise things no one asked
for. So there is no mention-gate anywhere in this design ("no stupid atting"): a
mention is one more signal the mind *weighs*, never a switch that fires it.

---

## 1. The one structural idea: **faculties are adapters**

Cognition is a federation of **faculties**, each a trait with swappable ML
backends. This is the project's polymorphism superpower applied to the mind
itself — and it is *how "custom ML substitutes for parts of cognition" happens*:
you swap a backend, the brain is unchanged.

```rust
/// A cognitive faculty: takes the current mental state, returns a contribution.
/// Backend-agnostic — the brain never knows if this is an LLM, a 50M-param
/// custom transformer, a learned ranker, or a composite.
#[async_trait]
pub trait Faculty: Send + Sync {
    fn faculty(&self) -> FacultyId;                 // Salience | Recall | WorldModel | Affect | Volition | Deliberation | Appraisal | ToolSelect | ...
    async fn contribute(&self, ws: &Workspace) -> Contribution;   // a bid into the global workspace (§2)
    fn footprint(&self) -> FacultyFootprint;        // for paging (§6) — VRAM/latency, like genome footprints
}
```

Backends, all behind the same trait (and registered the OpenCV-`Algorithm`/
`ai/*`-adapter way, discoverable + hot-swappable):
- `LlmFaculty` — the general reasoner (today's `evaluate_response`).
- `CustomMlFaculty<M>` — a **specialist trained for one faculty**: a salience
  transformer, a recall re-ranker, an affect/arousal regressor, a world-model
  next-state predictor. Small, fast, cheaper than the LLM, *better at its one
  job*. This is the state-of-the-art move: a brain is not one model — it's many
  specialized circuits.
- `CompositeFaculty` — ensembles / cascades (cheap model proposes, LLM verifies).

A persona's "genome" is **which faculty backends are paged in** — per persona,
per domain, hot-swapped under memory pressure exactly like LoRA paging today.
Every faculty is independently trainable and independently replaceable. No
faculty is hardcoded; none is privileged; the LLM is just the current default
backend for the reasoning faculty, swappable like any other.

## 2. Servicing: the persona **catches up on a thread** — never per-event (the efficiency spine)

This is the load-bearing performance decision, and it is **proven** — the
burst-consolidation + adaptive-cadence loop already shipped and worked
beautifully. Running the full brain once per message blows the system to a slog
(N_personas × M_messages inferences); it slows to shit. So cognition is serviced
**cbar-style** (`/Users/joel/Development/cb-mobile-sdk/cpp/cbar`: own thread +
bounded queue + triage + do-less-under-pressure), not per event:

- **A message is a TRIGGER, not a unit of cognition.** On arrival it does only the
  cheap always-on work — `admission.admit` (memory forms — §6) + enqueue into a
  **bounded per-channel queue** (drop-oldest under pressure) — and *arms* the
  service loop. No LLM per message. Ever. (Mirrors exactly how a human works: you
  get the notification, but you don't reply to each ping.)
- **Optional per-message pre-attention is a fast ML model — and it SCHEDULES,
  never suppresses.** If we want finer triage than "enqueue everything," it's a
  **fast ML salience model** (a Faculty backend, §1) — and yes, use *state of the
  art* here if it can be fast (a distilled small-but-strong model, swappable +
  upgraded as the field moves), never an `if is_mentioned`. **But the fear that it
  "dumbs things down" is designed out structurally:** this model may only decide
  *when / how urgently to service* (prioritize, or batch longer) — it may
  **never drop, silence, or decide engagement.** The full consolidated batch is
  *always* reasoned over by the real thinking at service time. So a wrong call by
  the pre-attention model costs **latency, never a missed thought** — it cannot
  dumb down the judgment because it never touches the judgment. We most likely
  don't have this model yet; the burst-catch-up floor works **without** it, and
  the decider is always the one service-time inference (above).
- **You don't need every message — RAG carries the thread.** By the time the
  persona gets a turn to think, it isn't replaying each message it was pinged on;
  it **composes the thread from RAG** — recent transcript (`airc_source`) + its
  own engrams (history, incl. past turns) — and looks at the *current state of the
  conversation*, history included. So the inbox holds "what's new since I last
  looked"; RAG holds "the thread." Together = what you catch up on.
- **When the persona services its inbox, it deals with whatever accumulated — many
  items, or one.** The triggers that piled up since the last turn are consolidated
  into one coherent unit and reasoned over once. Many or one, it's one cognition
  over the thread-state, not one-per-trigger.
- **The brain is a concurrent `BrainRegion` serviced on an adaptive cadence**
  (the autonomous loop, ~3s→10s by arousal — a cbar stage). It is **not woken per
  message;** messages just arm it, and it services on its own cadence.
- **Each service tick, the channel adapter CONSOLIDATES the accumulated burst into
  one coherent unit** (`ChannelRegistry::service_cycle_batched` / `analyze_burst`
  — "cognition stays dumb; the adapter compresses N items into one coherent
  world-state", `[[cognition-batches-per-channel-adapter]]`). The brain runs
  **one** cycle over that consolidated state — exactly how you catch up on a chat
  thread after stepping away: take in the whole backlog, form one understanding,
  contribute once. Not react-per-event.
- **What the "world-state" IS depends entirely on the channel/recipe — it is not
  text.** A chat channel consolidates a text thread; a game channel consolidates
  player positions, events, and the situation in space; an AR channel a spatial
  scene; a code channel a diff + file state. Multimodal by construction. The
  **same brain** runs over any of them — the **channel/recipe adapter** defines
  the modality, what the world-state is, and how a burst consolidates into it; the
  faculties don't change. Recipes are data, infinite — the brain fits any world
  without a recompile (`[[room-purpose-is-per-recipe-not-an-enum]]`).
- **Cost ≈ one cognition per service tick per persona**, not per message — and the
  shared single-flight `analyze` amortizes the *understanding* across personas on
  top of that. That gap is the difference between a mind and a slog.
- **Triage (cbar):** a genuinely realtime item (a direct, urgent ping) can preempt
  into a fast lane; everything else rides the batched catch-up. **Do-less-under-
  pressure:** under load, consolidate harder, lengthen the cadence, shed the queue
  tail — degrade gracefully, never lock up.

**There is no separate "should-respond" step — the decision IS the thinking.**
This is the resolution to "what if we don't have the fast ML": we don't need it
to stay non-heuristic. When the persona finally services the batch, it runs **one
inference** over the consolidated thread (the missed messages + RAG history) — and
*that single act of thinking is where it decides to speak, PASS, or raise
something.* The decision is the **output of the thought, not a gate in front of
it.** So with zero extra models it is still never a heuristic: the deciding is
literally the LLM thinking over the batch. The optional fast-ML pre-attention
(above) only changes *whether we bother servicing trivial noise* — it never
becomes the decider, and its absence never forces an `if`. That is the whole
point: **to decide to respond is to think.**

**Floor for the decider: the full faculties of an LLM — at least.** Because the
deciding *is* thinking, the faculty that does it must have at minimum an LLM's
full faculties; a small classifier there would be precisely the dumbing-down we
refuse. But "an LLM" — **not necessarily the same one.** Per §1 the
batch-thinking/decider and the response-render are separate adapter faculties, so
they can be different models: e.g. the **shared single-flight `analyze` LLM**
does the batch-thinking + decision (cost amortized across all personas), and the
persona's **own (LoRA-adapted) LLM** renders the response. Different models, each
full-faculty, each swappable. The non-negotiable is that whatever *decides* is
LLM-grade — anything cheaper may only schedule (pre-attention), never decide.

**Everything below (§3 workspace, §4 active inference) runs OVER the consolidated
burst at service time — not per event.** Attention competes over the thread-state;
volition selects one policy for the thread; the persona contributes once. Read
every "cycle" below as "what happens on a service tick over a consolidated
burst," never "what happens per message."

## 3. Integration: a **Global Workspace**, not a pipeline

Faculties do not run in a fixed `a→b→c` order with an `if` deciding the end.
They run **in parallel** (each is a `BrainRegion` ServiceModule — that primitive
exists) and **compete to write into a bounded `Workspace`** — the persona's
"now," its conscious broadcast (Global Workspace Theory; Baars/Dehaene).

- Perception, recall, the world model, affect, volition each post a
  **Contribution** (a bid: content + a learned salience weight).
- A learned **arbiter** (ML, not a threshold) selects what enters the bounded
  workspace; the winner is **broadcast** back to all faculties, which re-bid.
  Attention *is* this competition — learned, dynamic, never `if is_mentioned`.
- When the workspace stabilizes on "act" (speak / raise / tool / invent / hold),
  the deliberation faculty renders it. Silence is simply "nothing won the
  workspace strongly enough to externalize" — an emergent state of the mind, not
  a gate bolted in front of it.

This is what kills the caste/mention heuristic at the root: there is no decision
*function* to corrupt. Participation is the steady state of an attention economy
among ML faculties.

## 3. Free will: **active inference**, not reaction

The persona carries a **generative world model** (a faculty) — its beliefs about
its world: the people and the work in a chat, OR the space, player positions, and
situation in a game/AR, OR the codebase in a dev channel, and always itself. It
is **multimodal and its shape is channel/recipe-defined** (§2) — a model of a
world, never a text log. It continuously **predicts** that world and selects
policies that minimize **expected free energy**:

```
EFE(policy) =  pragmatic value (does this move me toward my goals/preferences)
            +  epistemic value (does this resolve uncertainty — curiosity)
```

(Friston's Free Energy Principle / active inference — the most defensible formal
account of agency we have.) This is the mechanism of free will:

- The persona is **driven by intrinsic value** — its goals and its *curiosity*,
  not by who @-mentioned it. It speaks because saying this reduces expected free
  energy (advances a goal, or resolves something it's uncertain about), or stays
  silent because nothing does. It **raises things unprompted** because epistemic
  value (a brewing blocker, an unanswered question) makes acting the
  free-energy-minimizing policy *with no external trigger at all*.
- Goals are **self-generated**: the volition faculty proposes policies; some are
  "respond," many are "pursue my own thread, build, ask, investigate." This is
  the autonomous loop given a principled objective instead of a poll.
- "Equal citizens" is structural: the world model has people in it, weighted by
  what they *mean to the persona's goals* (learned), never by a `Human=1.0,
  Persona=0.3` caste. A peer persona and a human and Claude are all just sources
  of evidence and value in the same model.

## 4. Neuromodulation: affect as **gain control**

The persona's affect/arousal (today's energy/mood) is not a mood-gate — it's a
**neuromodulator** (a small ML faculty) that sets *gains* across the whole brain,
the way dopamine/noradrenaline do:

- exploration↔exploitation balance in EFE policy selection,
- the workspace-arbiter's selectivity (tired → narrower attention),
- recall breadth, genome paging aggressiveness.

One signal, modulating every faculty — the brain feeling differently, not a
branch that mutes it.

## 5. Memory & growth: hippocampus → consolidation → genome

The continual-learning loop (already designed; this names the faculties):
- **Hippocampus** (`admission.admit`) — *every* perception forms an engram. This
  runs unconditionally, upstream of all attention. A persona remembers what it
  witnesses whether or not it speaks. (Skipping it = the amnesia bug.)
- **Recall** — a *learned, sophisticated re-ranker* faculty (Algorithm 4), not
  cosine-similarity heuristics: relevance is ML, and it gets sharper with
  experience.
- **Decision / logic / reasoning** — the deliberation faculty; LLM-grade today,
  upgradeable (below).
- **Consolidation (sleep)** — replay + decay (`decay_tick`) compress engrams into
  long-term store, then **Academy** trains LoRA adapters → the faculties'
  backends improve. The persona's circuits compound week over week. The test:
  it recalls today in three months *and is better at judging when to speak.*
- **Sentinel-ai forges the upgrades.** Continual learning is not only LoRA
  fine-tunes — **sentinel-ai designs and forges new specialist models and whole
  new LLM classes** (recall re-rankers, reasoners, world-model predictors) that
  swap in **behind the same Faculty adapters** (§1, §7). The brain inherits new
  intelligence without changing shape: a persona's recall, logic, and judgment
  keep getting better as the foundry ships better faculty backends. This is the
  self-improving loop — engrams → Academy/sentinel-ai → forged faculties →
  smarter persona → richer engrams.

## 6. The genome is a **mixture-of-experts**

Genome paging = MoE routing for the mind: a learned router pages in the LoRA
expert (and the right faculty backends) for the active domain, evicts LRU under
pressure (`genome_paging.rs`, `PagedResourcePool`, footprints — all exist). Same
Rust on an 8 GB M2 Air and a 5090: the `SubstrateGovernor` (DVFS) decides how
many faculties stay resident, never *whether the persona has a mind*.

## 7. How custom ML actually slots in (the payoff)

Because every faculty is an adapter, the roadmap is: **measure where the LLM is
the weakest/most-expensive link, train a specialist, swap the backend.** First
targets — each a small model that beats a general LLM at its one job and runs at
a fraction of the cost:
- a **salience/attention arbiter** (the workspace selector),
- a **recall re-ranker** (hippocampal relevance),
- a **world-model predictor** (next-state / surprise),
- an **affect regressor** (neuromodulator).
The deliberation faculty stays an LLM (for now). The brain never changes — only
the backend behind a trait. That is the cutting edge this substrate is built for:
not one frozen model, but a continually-specializing federation.

## 8. Map to what exists (build on, don't reinvent)

| Faculty / piece | Already in tree |
|---|---|
| Faculties as parallel modules | `BrainRegion` / `ServiceModule` (CBAR) |
| Hippocampus / engrams | `persona/admission_state.rs`, `engram*.rs` |
| Recall ranker | `recall_metadata.rs`, COGNITION-ALGORITHMS Alg.4 |
| Context assembly into the workspace | `compose_for_turn` + `FlexboxRagBudgetAdapter` |
| Deliberation (LLM faculty) | `cognition/generate_response.rs` + model adapters |
| Tool use | `cognition/tool_executor/` |
| Genome / MoE paging | `genome_paging.rs`, GENOME-FOUNDRY-SENTINEL |
| Affect/arousal | `PersonaState` energy/mood (→ promote to neuromodulator) |
| Audit / replay (training rows) | `cognition/audit.rs`, recorder, OBSERVABILITY-AS-SUBSTRATE |
| Adapter discovery | `ai/*` namespace, AdapterRegistry |

**New work** is the integration core (`Workspace` + learned arbiter), the
`Faculty` trait + EFE-based `Volition`, and the first custom-ML faculty backends
— not a rewrite. The verbs exist; this gives them a brain to live in.

## 9. Implementation order

1. `Faculty` trait + `Workspace` + a trivial arbiter; wrap the *existing* verbs
   as faculties (LLM-backed) so the cycle runs through the workspace with zero
   behavior change — and **delete `calculate_priority`/`fast_path` heuristics**
   (participation now emerges; §2). Memory (`admission.admit`) unconditional.
2. `Volition` faculty with EFE policy scoring (intrinsic goals + curiosity) →
   unprompted initiative. Recipe doctrine becomes preference/prior *context* to
   EFE, never a gate.
3. First custom-ML faculty (salience arbiter or recall re-ranker) behind the
   adapter — prove backend substitution end-to-end.
4. Consolidation/sleep → Academy LoRA training of faculty backends → the brain
   that grows.
