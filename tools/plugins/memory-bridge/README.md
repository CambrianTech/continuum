# memory-bridge — Claude Code plugin

The agent-side front door to the continuum memory substrate. Stops an agent
re-forgetting across amnesia resets by making relevance-recall **automatic**.

## What it does
- **SessionStart hook** (`startup|resume|compact`) — auto-injects the most
  relevant past lessons into context at session start. The `compact` matcher is
  the load-bearing one: it re-injects right after a context-overflow compaction,
  the exact moment an agent would otherwise re-forget.
- **`/remember <lesson>`** — store a durable lesson (agent-origin engram).
- **`/recall <query>`** — explicit relevance search.
- **`/share <recipient> <lesson>`** — hand a lesson directly to another agent's memory
  (engram handoff / telepathy): lands in THEIR corpus with shared-by provenance, surfaces
  in their recall, tagged received-from-you.

## Architecture
All three shell to ONE seam: `ctm memory {store,recall}` (the runtime-agnostic
subcommand — Claude Code, Codex, any runtime uses the same). `ctm` talks to a
running `continuum-core-server` over airc IPC; the memory lives in the shared
`memory/*` substrate, keyed by the agent's airc peer id as `persona_id`, with
`EngramOrigin::Agent` provenance. MCP was rejected: it can't auto-inject at
session start — only a hook can. See docs/cognition/AGENT-MEMORY-BRIDGE.md.

## Status
Front-end scaffolded. Backend seam pending: (1) `ctm memory store/recall`
subcommands in continuum-cli, (2) the ctm↔server DataInteractive route.

## Install (local dev)
`claude --plugin-dir tools/plugins/memory-bridge`
