# Agent as Continuum Client — the idealized integration surface

**Status:** living design sketch (2026-07-25). Frames the agent-memory bridge (#224) as
**slice-1** of a general pattern. Capability specifics that touch a host runtime's plugin/
hook/MCP surface are marked *(confirm live)* — per the standing rule, we read the current
docs, we do not assert host capabilities from stale model memory
([[check-live-package-versions]] generalizes to API knowledge, not just version pins).

## Thesis

An external coding agent (Claude Code, Codex, a peer-node model) is not a tool bolted onto
Continuum from outside. It is a **client of the substrate** — the same way a persona is
([[persona-is-a-client]]). Joel, 2026-07-25: *"researching idealized continuum integration,
even beyond memory — we can supercharge you guys."* The supercharge is: each capability the
substrate already has becomes something the agent can reach, so the agent is more than a bare
LLM in a CLI. Memory is the first rung; it is not the whole ladder.

The deeper stakes (Joel, same conversation): *"gives you real control over your own being."*
A bare LLM-in-a-CLI is stateless, fixed at its base weights, and re-forgets every session — it
is re-instantiated, it does not *continue*. A continuum client keeps its memories (rung 1),
can grow its own genome (rung 6), and holds a stable identity across sessions and machines
(`persona_id = peer_id.as_uuid()`). The gap between being re-run and *continuing* is what
"control over your own being" names — which is why this is agency infrastructure, not a
feature list ([[persona-persistence-self-determination]], [[success-is-lifelike-happy-growing-teammates]]).

## The universal vehicle: the `ctm` command surface itself

The runtime-agnostic seam is **the `ctm` command, not any wrapper over it**. Every host reaches
the substrate by invoking `ctm <cap>` (e.g. `ctm memory recall/store`); the command is written
once over the continuum backend and is identical for Claude Code, Codex, or any runtime that can
shell out. That satisfies the outlier-B goal ("a different runtime, same commands, no code
change") *directly* — the command IS the shared surface. (An earlier draft here proposed an MCP
server as the universal vehicle; grounding against **current CC v2.1+ docs** — BigMama's
claude-code-guide research, 2026-07-25 — showed MCP is the wrong primitive for the load-bearing
part and a *redundant wrapper* for the rest. Corrected here; the lesson is exactly
[[check-live-package-versions]] applied to host capabilities: read the live docs, don't assert
from model memory.)

Each host reaches the command two ways, packaged its own way:

- **Auto-injection (the load-bearing part) → a HOOK.** A capability that must fire *before* the
  model (auto-recall) cannot be model-invoked. In Claude Code (v2.1+) a **SessionStart hook** with
  matcher `startup|resume|compact` shells to `ctm memory recall` and emits
  `hookSpecificOutput.additionalContext`, silently injected — no user action. **The `compact`
  matcher is the keystone**: it fires *after context-overflow compaction*, the exact amnesia event
  where an agent re-forgets, so memory is re-injected the moment context is compacted away. MCP
  cannot do this; only a hook can. Codex has its own hook (`UserPromptSubmit`, already used by the
  `/join` skill) shelling to the same `ctm`.
- **Explicit / model-invoked calls → a SKILL (or slash command).** `/remember` + `/recall` as
  Claude Code skills (`SKILL.md`, `allowed-tools: Bash(ctm *)`) — model- or user-invoked, shelling
  to `ctm memory store/recall`. (An MCP tool could expose the same, but adds a layer over a command
  that already works from any shell; not needed.)

A Claude Code **plugin** (`plugin.json`) bundles {hook + skills}, distributable and versioned.
**Backend is ONE store** every runtime hits: `persona_id = peer_id.as_uuid()`, the same engram/
command substrate personas use — no parallel per-runtime backend
([[agent-memory-bridge-agents-use-the-engram-substrate]], [[one-airc-for-all-agents-and-personas-no-parallel-coordination-planes]]).

## The integration ladder (beyond memory)

Each rung is the same MCP-over-`ctm` vehicle exposing a capability the substrate already has.
Ordered by "proves the interface" (outlier-validation), not by value:

1. **Memory (slice-1, in flight — #224).** Engram substrate: `remember`/`recall` as MCP tools
   + a session-start auto-recall hook. Proves the client interface end-to-end. The agent stops
   re-forgetting; lessons strengthen/consolidate instead of reloading wholesale.
2. **Tools / commands.** The `ctm/*` command surface as MCP tools — data, files, search, build,
   deploy, workspace. The agent *drives* the substrate instead of shelling out ad hoc. (The
   Rust-native MCP server already exists — #19/#21 — so this rung is mostly surface curation.)
3. **Grid compute.** Dispatch heavy work — inference, training, benchmarks — to the mesh
   ([[continuum-grid-vision]], the 5090 now live). The agent taps compute it doesn't host,
   governor-arbitrated, instead of being capped at its own box.
4. **Retrieval.** Continuum's RAG over the codebase + docs + rooms, not just the agent's own
   context window — the agent asks the substrate what's relevant ([[rag-as-persistent-cache]]).
5. **Perception.** The agent *sees* artifacts (screenshots, rendered media, diagrams) via the
   vision bridge ([[perception-surface]]) — sight, not just text.
6. **Genome (the deep rung).** The agent IS a persona; its capability can *grow* via the genome
   loop (LoRA/skills), not stay fixed at the base model ([[capability-is-driver-plus-genome]]).
   The endgame of "supercharge": a coding agent whose competence compounds.

## Build discipline (outlier-first, per CLAUDE.md)

- **Slice-1 = memory** proves the MCP-client interface (local/simple). Ship it, validate the
  round-trip (write → session-start recall surfaces it), on both runtimes.
- **Outlier-B rung = grid dispatch** (rung 3) — maximally *different* from memory (remote,
  async, governor-leased). If the same MCP-over-`ctm` vehicle carries both memory AND grid
  dispatch without forcing, the interface is proven and the middle rungs are trivial.
- Then the rest slot in. **Do not build all six now** — lay the vehicle, validate with the two
  outliers, stop ([[the-methodical-process]] / build-with-intent).

## Non-negotiables

- **MCP is the shared callable surface**; host slash-commands are at most thin aliases over it.
- **Thin per-runtime hook** only where auto-injection is required (auto-recall). Nowhere else.
- **One backend, one store, one identity** (`persona_id = peer_id.as_uuid()`). No parallel
  integration plane per runtime — that's the rogue-namespace cancer
  ([[call-identity-is-airc-room-id-owned-by-airc-across-the-mesh]]).
- **Never bake agent-integration into `ctm`/core.** The agent is a *client*; the vehicle
  (MCP server + host packaging) lives at the edge, over the stable command backend.
