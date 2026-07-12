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
Tiny lens-processes (historian/critic/connector/risk/forager) on idle cadence over
engrams; provenance-typed inner speech as the sanity invariant (the split-brain/
schizophrenia mapping); dreams as day-residue replay + nightmare threat-simulation;
the dream CHOOSES what to learn next. Inner chorus on 0.5B–4B; narrator on the big
model. [memory: mind-wanderers-subpersonal-processes, task #145]

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
