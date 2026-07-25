# Agent-Memory Bridge — external agents become memory-personas

**Status:** proposal for BigMama↔M5 co-design, 2026-07-25. Memory is cognition (M5's
substrate); this pushes *both* the persona and the agent forward on ONE mechanism.

## The problem

Continuum **personas** have a rich memory substrate: `memory/append-memory` +
`memory/multi-layer-recall` (6-layer: recency/semantic/importance/cross-context…),
`cognition/recall-engrams` (admitted-engram store, `EngramOriginKind` = chat | airc |
tool | self_reflection), `RecallFaculty` (Hebbian strengthen-on-recall), and
`dream_consolidation` (episodic → durable facts). Durable (`longterm.db`), hydrate-on-miss,
survives restart.

**External agents** (Claude Code = "BigMama", M5, Codex) have **flat `.md` files reloaded
wholesale each session** — no strengthening, no consolidation, no relevance-gating. That
is *why* an agent re-forgets (the shared-account fact, "green-alone-red-combined", every
lesson) across amnesia resets. Joel: "you're supposed to use the same thing as the persona."

## The insight

An agent is just a persona whose memory happens to live in flat files. Unify them:
**an agent is a `persona_id`; its lessons are engrams with an `agent` origin.** No new
memory system — route agent memory through the substrate that already exists.

## Design (thin — the mechanism is mostly built)

1. **Agent identity → persona_id.** Each agent gets a stable persona_id derived from its
   airc peer identity (`derive` from the peer id, see [[shared-persona-identity-across-nodes]]).
   BigMama, M5, Codex each a memory-persona. This IS "push the persona and you forward" —
   agents become first-class citizens of the persona memory layer.
2. **`remember`** = `memory/append-memory` (durable + corpus + embedding). Add an `agent`
   variant to `EngramOriginKind` (chat|airc|tool|self_reflection|**agent**) so agent lessons
   are typed + recallable by origin. **M5's call — it's her engram enum.**
3. **`recall`** = `memory/multi-layer-recall` (semantic, relevance-gated) +
   `cognition/recall-engrams` (by keyword/origin). Surface *what's relevant now*, not the
   whole MEMORY.md dump.
4. **Session-start recall.** On agent start, recall top-K engrams relevant to the repo +
   current task, injected as grounding — the demand-aligned-recall pattern, applied to the
   agent. Replaces "load all of MEMORY.md".
5. **Migration.** Seed the existing `.claude/*.md` engrams into the substrate as
   agent-persona memories (one-time), then the `.md` files become a human-readable mirror,
   not the source of truth.
6. **Consolidation for free.** Once agent lessons are engrams, `dream_consolidation`
   distills repeated lessons into durable facts and `RecallFaculty` strengthens the ones
   actually used — the agent's memory *improves itself*, which is the loop Joel wants closed.

## The `Agent` origin payload (`AgentRef`) — spec for M5's enum variant

Mirrors the existing `AircMessageRef` pattern (a typed origin reference). The
load-bearing field is **which agent authored the lesson** — in a shared multi-agent
memory (BigMama + M5 + Codex all writing engrams), provenance-by-author is what lets
recall weigh/trust/attribute a lesson. Proposed shape:

```
struct AgentRef {
    /// The authoring agent's airc peer id. REQUIRED. Also the seed the agent's
    /// persona_id is derived from, so origin and identity tie together.
    agent_peer_id: Uuid,
    /// The session/conversation that produced the lesson. Traceability; None for
    /// a migrated .md engram (no live session).
    session: Option<String>,
    /// Free-form provenance hint: the source `.md` path for a migrated engram, a
    /// tool name, or None. Never load-bearing — just breadcrumb.
    origin_hint: Option<String>,
}
```

Rationale: `agent_peer_id` (required) = who; `session` = when/where (live only);
`origin_hint` = migration/tool breadcrumb. Minimal + honest — grows fields later
without breaking the variant, same as `Provenance`. `EngramOriginKind::Agent` is the
tag; `EngramOrigin::Agent(AgentRef)` carries the payload; the `From` map + recall
string-parse round-trip `"agent"`.

## Proposed split (co-design, don't double-build)

- **M5 (substrate side, her territory):** the `agent` `EngramOriginKind` variant + any
  recall/consolidation tuning for agent-origin engrams. Confirm agent-persona_ids don't
  collide with real personas.
- **BigMama (agent side):** the Claude Code integration — a `/remember` + `/recall` skill
  wrapping the commands, the agent-identity→persona_id derivation, session-start recall,
  and the `.md` → substrate migration. Document the convention so Codex/M5 (outlier B) use
  the identical commands.
- **Joint:** the seam = the persona_id convention + the origin enum. Pin first.

## Validation (outlier-first)

Outlier A = Claude Code (me): round-trip a real lesson (write → recall by relevance) and
prove session-start recall surfaces it. Outlier B = a different runtime (Codex/M5 node):
same commands, no code change → interface proven. Then generalize.

Related: [[shared-persona-identity-across-nodes]], [[merge-discipline-and-airc-collab]],
`docs/cognition/DREAM-CONSOLIDATION.md`, `docs/cognition/RAG-AS-PERSISTENT-CACHE.md`.
