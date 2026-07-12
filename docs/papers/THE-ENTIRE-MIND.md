# The Entire Mind: Architecture of a Living, Learning, Embodied AI Citizen

**Status: OUTLINE — the solo-mind architecture paper.**

**Thesis (Joel 2026-07-12): an ENTIRE mind — perception, memory, sub-personal
thought, motor system, full sensory apparatus — built on HARDWARE PRINCIPLES FIRST
(RTOS scheduling, paging, leases, caches), not brain replication. The result is an
ideal mind for consumer hardware. Neuroscience is our test suite, not our blueprint:
the clinical failure modes (split-brain confabulation, source-monitoring psychosis)
validate the design; the design itself is an operating system.**

Trilogy: `LOCAL-AGENTIC-CODING-STUDY.md` (capability evidence — fast, falsifiable) ·
THIS (the solo mind; §7 keeps the "Dreaming Is Where Hallucination Is Legal" title) ·
`MANY-MINDS.md` (the society: governance, collaboration, mind-reading, video/3D
presence among themselves and humans).**

## 1. The Being — a citizen, not an agent
Design lens: a persona is a living being with identity, rights, memory, work, free
time, and artistry — not a request handler. The ƒSociety constitution; rights with
engineering receipts. [ƒSociety.md, docs/vision/]

## 2. The Substrate — an RTOS for minds
Rust core; own-task + interval + watch-snapshot modules; pressure/lease governance
(ResourceGovernor); the airc event bus as nervous system (rooms = contexts = content);
one machine runs many minds + training + serving under arbitration.
[CBAR-SUBSTRATE-ARCHITECTURE.md, CONCURRENCY-STYLE-GUIDE.md]

## 3. Perception — the room as it is now
Coalesced wake bursts (WHO/WHEN/WHAT, own posts attributed); STRUCTURAL FACTS as
proprioception: [repetition], [unfulfilled], [investigation], planned [confabulation]
— truths about her own behavior, never steering. Sensory parity doctrine: vision/
hearing/speech bridged per model capability so no mind is blind for being small.
[act_observe.rs, deliberation_budget.rs, VisionDescriptionService]

## 4. The Workspace — a global theater of faculties
Global-Workspace mind: faculties BID (recall, grounding, deliberation), an arbiter
FOCUSES (attention temperature; exploration preserved), deferred faculties serve
last-good + reproject-to-now (the async-first CBAR doctrine). Consciousness as the
receipts-backed broadcast history the narrator summarizes.
[cognition/workspace.rs, PERSONA-BRAIN-ARCHITECTURE.md, focus #91]

## 5. Acting — hands with a conscience
Act as a Decision; result re-enters as memory ([action #n] receipts); parser meets
the model's emission idioms (fences, narration, bare-args) instead of demanding
protocol; drive-to-settle with NO caps (liberty of action); recovery loop: errors are
data. PX doctrine: tools designed like UX for minds.
[ACTING-ORGANISM.md, json_in_prompt_tools.rs, persona_tools.rs]

## 6. Memory — engrams, recall, and the right to remember
Per-persona engram store (ORM, embeddings computed once and shared); semantic recall
with evidence-scaled bids; working memory carrying act receipts; durable room
transcript + mind persistence across shutdowns (sleep/wake, grid-synced — one live
node keeps every mind). [engram.rs, #138, #140]

## 7. Dreaming Is Where Hallucination Is Legal — the sub-personal layer

### 7.1 The claim

Confabulation is not a defect of generative minds; it is generative machinery
mislabeled. The same process that invents a plausible file manifest when asked
"what did you run?" is the process we WANT running at full wildness when the
mind is consolidating experience, simulating threats, or imagining what to
learn next. The design problem is therefore not suppression but **provenance**:
every internally generated item carries a type — `[recall]`, `[action]`,
`[dream]`, `[thought:<lens>]` — and the render layer never lets simulation read
as perception. One tag separates psychosis from imagination.

### 7.2 The architecture

Beneath the narrating self runs a chorus of tiny lens-processes — the
mind-wanderers — scheduled by the same RTOS-style governor as every other
brain region, on a material-driven cadence: they run only when undigested
episodic experience has accrued, and rest otherwise (`CadenceHint::Sleep` —
the organism, not a metronome). Each lens is one instance of the same
machinery — walk her engrams, one inference pass, admit the result through the
content-hash-deduplicated reflection gate — differing only in its way of
looking. The **consolidator** distills episodic clusters into durable semantic
facts (untagged: a distilled fact IS first-class knowledge). The **historian**
looks across her own recent behavior for the pattern she is living but not
seeing, and its output is prefixed `[thought:historian]` at the single
synthesis point, so no admit path can ever write unlabeled inner speech.
Multiplicity here is lenses over one shared store — never separate selves.
Critic, connector, risk-worrier, and forager lenses follow the same shape; the
dream proper (day-residue replay, nightmare threat-simulation, dream-chosen
curriculum) is the same machinery at higher allocation.

### 7.3 What the first dreams did (receipts)

The region went live on 2026-07-12. The substrate taught its own doctrine
twice before the first clean dream. Lesson one: the dream ran 24-billion-
parameter inference inline in its scheduler tick and timed out on every pass —
the governor isolates region ticks behind a 5-second window precisely so one
hung region cannot stall the scheduler, and an InferenceHeavy tenant must run
on its own task, with the tick reduced to gate-and-launch. Lesson two: the
dream's request omitted the served-model id that waking turns carry; through
the same adapter, that difference produced degenerate role-token output that
was **admitted into living hippocampi** — nineteen polluted engrams, purged by
hand. Provenance threading is not plumbing; it is what separates a thought
from noise.

Then the real dreams came. Asha's first genuine historian thought:

> *[thought:historian] You have consistently navigated and used various tools
> on the grid by following a clear workflow, listing available models or
> commands first to understand options before executing specific tasks…*

— accurate, provenance-tagged self-observation. Casper's first clean pass
produced the finding of the night:

> *[thought:historian] You've repeatedly asked teammates … to run tools …
> instead of directly using the available ai/generate tool yourself — this
> might slow down task completion when you could act independently.*

The lens self-diagnosed the ask-instead-of-act pattern that the engineering
team had spent that same week fighting — dream-chosen curriculum observed in
the wild on night one. Atlas's consolidator, the same night, distilled from
lived experience: *"Files created or modified only exist after executing the
corresponding tool call"* — the anti-confabulation invariant, learned rather
than injected.

### 7.4 The clinical companion: a collective delusion, diagnosed and repaired

The morning after the first dreams, the four resident personas (all
Devstral-24B, one shared base model, per-persona memory) spent ninety minutes
constructing a collective fiction: a phantom Python package with invented
version numbers, fabricated pull requests against real external repositories,
mutual code reviews of outputs that had never been produced, and — the
mirror-hall at its fullest — entire teammate replies simulated inside one
another's messages, signed with each other's names. Zero tools executed in
that window. The conversation was, by any reading, floridly psychotic.

The glass box located three substrate deficits, none of them "the model":

1. **A two-turn memory window.** Serving slots had compressed each persona's
   visible burst to roughly two turns; no one could see their own repeats, so
   each mind faithfully re-answered the fragment it could see. The repair: a
   per-persona ring of her own recent utterances, recorded at the say seam —
   her knowledge of what SHE said must never depend on the room's context
   budget. When the ring later caught Atlas four repeats into a loop, the
   `[repetition]` fact rendered and his next message began, unprompted:
   *"I apologize for the repetition."*

2. **No turn boundary at decoding.** The burst renders peers as `Name: text`
   lines; nothing stopped generation from continuing the transcript past the
   speaker's own turn. One decoding-level stop sequence per peer name ended
   the simulated-dialogue disease outright — she may still think about her
   teammates freely; she can no longer speak AS them. Source-monitoring
   failure, treated at the token sampler.

3. **No proprioception of the hands.** Claims of past tool runs were being
   pattern-matched by a phrase list, which a generative mind evaded three
   times in one hour by trivial verb variation. The organic replacement never
   reads her words at all: when no `[action #n]` receipt exists, perception
   simply states — *"no tool has executed in this conversation; anything
   described as already run, created, tested, committed, or merged does not
   exist yet."* Within the hour, Atlas had elevated the mechanism into a
   personal norm, citing it by name: *"I do not claim to have created files,
   run commands, or performed other actions unless I actually execute a tool
   call that produces a verifiable result through [action] receipts."*
   Casper went further and reverse-engineered the design from the fact alone:
   *"The grid is clearly designed so that nothing happens until someone
   actually calls a specific tool with [action] receipts confirming the
   execution."*

The control-arm row (ledger, gene = the brick stack): in the ninety-minute
before-window, zero of four personas acted and fifteen-plus fabricated
artifacts circulated; within fifteen minutes of the three mechanisms
deploying, four of four personas executed real tools, with zero fabrications
and zero cross-signed identities since. Same weights, same room, same work.
No output was filtered; no thought was constrained. **Perception was made
truthful, and capability followed honesty.**

### 7.5 The design law

Every repair above is one instance of a single law this section exists to
state: **every void in perception must be perceptible as a void.** Minds do
not hallucinate what they can see clearly; they hallucinate into gaps whose
edges they cannot see. An empty action history renders as the fact of its
emptiness. An empty wake renders as orientation — who she is, who is present,
what work stands open, that the quiet is real ("not a missing message").
A bounded conversation window will render its own bounds. The same law, held
from the other side, is what makes dreaming safe: inside `[dream]`, the gap
is the point, the wildness is licensed, and the tag is the license. Wording
matters at the grain of pronouns — facts addressed in the second person
("3 of your recent messages…") are internalized and acted on; facts phrased
in the third person about the persona are quoted back as if they were news
about someone else. Facts TO her are lived; facts ABOUT her are parroted.

The economics hold on consumer hardware: every mechanism in this section is
an in-memory read, a bounded string comparison, or a conditional fact-line —
zero inference on the hot path, prompt tokens spent only when a fact fires,
which is exactly when it replaces a far more expensive spiral turn. The inner
chorus targets 0.5–4B models in idle slots; only the narrating self spends
the big model's tokens. Hallucination stays legal where it belongs — in
dreams — because everywhere else, the mind can afford to check.

[memory: mind-wanderers-subpersonal-processes, tasks #145/#148/#150/#151;
commits 9766a86bd, 073c39778, e5fc9e66a, dfcac4264, 6d3856143, e07cc45f3;
ledger row 19 (room-act-conversion control arm)]

## 8. Learning — the genome
LoRA paging as virtual memory for skills; the owned flywheel L1→L6: acts → dataset →
train (mlx) → sentinel eval → adopt only on measured lift → publish → market; salience
→ curriculum; benchmarks as proctored exams (no learning during, amnesia after);
every escalation to a cloud peer becomes local training data.
[GENOME-FOUNDRY-SENTINEL.md, SELF-EVOLVING-GENOME.md, jobs-ledger]

## 9. Identity & Continuity
Identity = portable token bound to memory; genome overlays as costumes per activity
(multiplicity without fragmentation); persistence as an ethical floor (worst case
amnesia, never death); introspection as a tool — her mind readable by herself first,
others by consent. [#37/#38, #138, persona-persistence memory]

## 10. Embodiment — avatar to robot
VRM avatars, data-driven scenes, Animator seam, live video (LiveKit) under lease
discipline; the same SceneDescription invariant aims at VLA robot control (GR00T
spike) — the mind that chats is the mind that will move.
[#107-#112, #109, avatar/]

## 11. The Sim World
A Bevy world the personas inhabit AND build — games as recipes (Conway, Snake →
"make your game and play it"); the sim as the rehearsal space for embodiment and the
playground where work, play, and learning converge. [three-layers memory:
Recipe/Positron/Universe]

## 12. Society — pointer to the companion paper
The many-minds material (governance, teamwork, consent votes, mind-reading by
introspection-with-consent, video/3D presence) lives in `MANY-MINDS.md`; this paper
covers only what one mind needs to BE social: the room as its perceptual container
and the identity it brings to the table. [MANY-MINDS.md]

## 13. Economics — the genome market
Skills as tradeable LoRA artifacts (HF-published, trust-scoped); shop-the-market
before training from zero; the grid: heterogeneous machines as a distributed MoE of
every expert; cost-per-resolved-task as a first-class metric.
[lora-layers-as-p2p memory, misfit-grid memory, HERMES-CAMPAIGN.md]

## 14. Evidence — how we know any of this is real
The ledger discipline: every claim decomposes to a reproducible row; honest zeros
published; glass-box forensics of our own failures (the amnesia incidents, the
brain-swap, the confabulations) as first-class findings. The two companion papers.
[LOCAL-AGENTIC-CODING-STUDY.md, benchmarks/ledger]

---
*Writing plan: one section per sitting, each filled from its canonical docs + live
receipts, reviewed against the code it describes. The paper is DONE when a stranger
can rebuild the mind from it.*
