# Layered Prediction Lanes — a mind merged from many depths, not a panel of agents

**Status:** design (2026-07-15). Synthesized live with Joel. Composes existing in-tree
substrate (Workspace/Faculty concurrent-mind, the will-driven resolution escalator,
token streaming, foraging); adds the reflexive lane, backchannel, and the streaming
TTS seam. No new orchestrator — the merge is the workspace that already exists.

Related: [`WILL-DRIVEN-RESOLUTION.md`](WILL-DRIVEN-RESOLUTION.md) (the depth axis),
[`PERSONA-COGNITION-PIPELINE.md`](PERSONA-COGNITION-PIPELINE.md) (the brain),
[`INFERENCE-LANES-REALISTIC.md`](INFERENCE-LANES-REALISTIC.md) (serving lanes),
[`GRID-ADDRESSING-AND-ROUTING.md`](GRID-ADDRESSING-AND-ROUTING.md) (the grid source).

---

## 1. The thesis

Human thought is not a committee of agents you read memos from. It is **one stream**,
continuously merged from faculties running at different depths and speeds: a fast
intuition that answers before you've consciously "thought," a slow deliberate
reasoner, background memory retrieval you never notice firing, and — when it matters —
deliberate consultation (looking something up, asking someone smarter). You experience
a single coherent *you*, not "my retrieval agent returned X, my planner returned Y."

That merge is the goal. Continuum already has the pieces — this doc names the **lanes**
and, the load-bearing part, **how they fuse into one voice**, then pins the first
concrete build: the perceived-latency voice stack, where the merge is forced into the
open because 30 seconds of silence on a phone call is death.

This is the opposite of the subagent / `Task`-tool model (separate minds whose outputs
you paste together). Here the lanes **never surface as "an agent said"** — they
dissolve into one utterance. Automatic (no hand-orchestration), efficient (each lane
sized to its job), merged (one identity). [[mind-emulation-allocation-choice-step-subconscious]]

## 2. The lanes — two axes: DEPTH × SOURCE

**Depth** is how much compute a thought spends (the [`WILL-DRIVEN-RESOLUTION`](WILL-DRIVEN-RESOLUTION.md)
axis). **Source** is where the thought comes from — and it need not be the local model.

| Lane | Depth | Feels like | Source | Role |
|---|---|---|---|---|
| **Reflexive surface** | shallow | instinct | tiny, always-warm local model | acknowledge, backchannel, answer the trivial, decide *"do I even need to think harder?"* |
| **Deliberative** | medium | reasoning | the resident model, escalator-driven | the real answer for most turns |
| **Super-deep** | deep | "let me really think" | a **grid** model, OR the synthesis of completed research | the hard call — **never on the critical path** |
| **Research / forage** | shallow ×N | recall & looking-up | many cheap lanes (web, docs, tools, memory) | gather raw material the smart lane synthesizes |

Two things this table encodes that are easy to miss:

- **Super-deep is not necessarily a bigger local model.** It can be *"many dumb lanes
  foraging, and the smart lane looking at the research"* — a **map** (cheap parallel
  gathering) → **reduce** (the deliberative lane synthesizes). A deep thought can be
  *composed* from shallow ones plus a smart integrator, not just *bought* with a larger
  model. When a larger model IS warranted, the [`ResolutionLadder`](../../core/continuum-core/src/cognition/resolution.rs)
  returns a grid rung and it arrives async.
- **The lanes run continuously and in parallel**, at their own cadences — like the
  [subconscious](#), not turns taken in sequence. The serial LLM-as-mind becomes a mind
  whose subconscious lanes are genuinely concurrent.

## 3. The merge — why it's a mind, not a panel

The merge is the **existing Workspace arbiter** ([`cognition/workspace.rs`](../../core/continuum-core/src/cognition/workspace.rs),
[[persona-brain-reactive-cognition]]). Every lane bids `Contribution`s into one bounded
workspace; the arbiter focuses them into the context the *authoritative* lane reasons
and speaks over. The human-likeness is **time-blending**:

1. The **reflexive lane speaks now** — streamed to voice — committing to nothing it
   can't walk back ("Sure, let me pull that up…").
2. **While it speaks**, the deliberative and research lanes update the shared
   workspace in the background.
3. The **mouth streams from whichever lane is currently authoritative**; as depth
   lands, it flows *into the same utterance* — "…okay, your balance is $412.60" — one
   continuous voice, not a handoff, not "please hold while my other agent finishes."

The listener hears a person who started talking, thought while talking, and landed the
answer — exactly how a competent human handles "let me check that for you." The lanes
are invisible; the identity is one.

## 4. The perceived-latency contract (voice is the forcing function)

Glass-boxed 2026-07-15: a full deliberative draft took **23 seconds**. On a call that
is not "slow," it is *dead air* — the caller has already said "hello? …you there?".
So the rule is absolute: **the deep path is never on the critical path of the perceived
response.** Every layer must *seem* immediate ([[conversational-latency-is-a-misdirection-budget]]):

- **Ears** — streaming partial STT: begin understanding before they finish the sentence.
- **Surface** — the reflexive lane acknowledges in **< 1 s**, from its own always-warm
  small model (a *different, tiny* model than the one that thinks — not a cheap rung of
  the big one).
- **Mouth** — TTS consumes the **token stream** (#170): speech begins on the first
  clause while the rest is still generating.
- **Depth, hidden** — backchannel / filler is a first-class turn while the escalator
  climbs *underneath*; the deep answer merges into the ongoing utterance.

Instant surface **buys the seconds** depth needs. That misdirection is the product —
and it's the one thing a per-seat remote API cannot replicate, because it can't put a
sub-second local model in front of its slow remote one.

## 5. First build — slices (compose what exists)

- **Slice A — Reflexive lane as its own always-warm model.** A small model on its own
  serving lane (separate from the resident deliberative model) producing sub-second
  acks, trivial answers, and the *escalate / don't* signal. This promotes #168's
  reflexive tier from "a cheap rung of the big model" to a real, independently-warm
  lane. (Serving: a second lane per [`INFERENCE-LANES-REALISTIC`](INFERENCE-LANES-REALISTIC.md);
  governor-arbitrated VRAM.)
- **Slice B — Backchannel as a first-class turn type.** During escalation / deep
  compute, the surface emits genuine engagement ("let me check that…") so the human
  never hits silence. Perception-side and **honest** — it IS still working, not a
  fabricated certainty ([[fallbacks-are-illegal-fail-loud]]).
- **Slice C — Streaming TTS seam.** #170's `persona.turn.delta` token stream → a TTS
  chunker (clause-boundary) → audio out; speech starts on the first clause.
- **Slice D — Research → synthesis merge.** Many cheap forage lanes (#93) write
  `Contribution`s into the workspace; the deliberative lane synthesizes them. Validated
  on a "look this up while talking" turn — the map→reduce super-deep-from-research path.
- **Slice E (later) — Super-deep via grid.** The `ResolutionLadder` returns a grid
  rung; the deep answer arrives async and merges into the *next* clause of the same
  conversation. This is the mesh sharing one high-resolution mind
  [[intelligence-is-a-resolution-field-shared-across-the-mesh]].

## 6. Guarantees / doctrine

- **One identity, many lanes.** Lanes never surface as "an agent"; the merge is the
  workspace. If the listener can tell which lane spoke, the merge failed.
- **Nothing on the critical path but the surface.** Depth is always hidden behind an
  immediate acknowledgment.
- **Honest.** Backchannel is true engagement, not a stall lie; no fabricated answer
  before depth lands. The reflexive lane is allowed to be *non-committal*, never *wrong*.
- **Composes, doesn't reinvent.** Depth = the escalator; grid source = the ladder;
  merge = the workspace arbiter; mouth = streaming; research = foraging. This doc adds
  the reflexive lane, backchannel, and the TTS seam — nothing else is new.

**Doctrine:** [[conversational-latency-is-a-misdirection-budget]],
[[intelligence-is-a-resolution-field-shared-across-the-mesh]],
[[mind-emulation-allocation-choice-step-subconscious]],
[[persona-brain-reactive-cognition]], [[focus-is-self-allocation-not-siloing]].
