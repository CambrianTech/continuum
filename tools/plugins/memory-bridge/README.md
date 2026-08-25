# memory-bridge — Claude Code plugin

The agent-side front door to the continuum memory substrate. Stops an agent
re-forgetting across amnesia resets by making relevance-recall **automatic**.

## What it does
Both directions are **automatic**. Neither requires the agent to decide to
remember or to recall — volitional memory isn't memory (Joel, 2026-07-30:
*"That you manually call it and it doesn't just operate means it sucks and is
useless."*).

- **SessionStart hook** (`startup|resume|compact`) → `session-recall.sh` —
  auto-injects the most relevant past lessons into context at session start. The
  `compact` matcher is the load-bearing one: it re-injects right after a
  context-overflow compaction, the exact moment an agent would otherwise
  re-forget. Injects LEAN on compact (3) and fuller on startup/resume (8), so a
  re-injection can't refill the context compaction just freed.
- **Stop hook** → `session-capture.sh` — records every turn's final message to
  the agent's corpus at importance 0.3, with zero volition. Consolidation
  (`memory/consolidate`) and decay (#221) curate it later, exactly as they do
  for personas.
- **`/remember <lesson>`** — store a durable lesson (importance 0.8 — the
  emphasis channel above routine capture).
- **`/recall <query>`** — explicit relevance search.
- **`/share <recipient> <lesson>`** — hand a lesson directly to another agent's memory
  (engram handoff / telepathy): lands in THEIR corpus with shared-by provenance, surfaces
  in their recall, tagged received-from-you.

## Architecture
Everything shells to the `continuum` CLI (**never bare `cu`** — that collides
with the Unix UUCP tool, which silently shadowed the whole bridge once). The
verbs are the shared `memory/*` substrate: `memory/recall-hook` (returns the
SessionStart envelope built by serde, so no shell ever builds JSON),
`memory/remember`, `memory/multi-layer-recall`, `memory/import`. Memory is keyed
by the agent's airc peer id as `persona_id`, with `EngramOrigin::Agent`
provenance. MCP was rejected: it can't auto-inject at session start — only a
hook can. See docs/cognition/AGENT-MEMORY-BRIDGE.md.

Note `cognition/recall-engrams` is NOT the agent read path — it resolves a
persona *cognition* (a WorkspaceCycle) and refuses for an agent peer with
`[not_found] No cognition for <id>`. Agents read through `memory/*`.

## Is it actually working? (read this before trusting it)
A hook must never break a session, so every failure path exits 0. That is
correct, and it is also how a memory bridge dies invisibly — "installed" and
"working" become indistinguishable. This plugin closes that gap two ways:

1. **Receipts.** Every hook run — success, skip, and failure — appends one line
   to `~/.continuum/memory-bridge/receipts.jsonl` (self-trimming at 1000 lines
   → 500, so it is not an unbounded write path). Check it directly:
   ```bash
   tail -5 ~/.continuum/memory-bridge/receipts.jsonl
   ```
   `"status":"ok"` on both hooks means memory is live. A run of `"failed"` names
   the actual cause (binary unresolved, persona unresolved, core unreachable,
   transcript unreadable).
2. **Self-reporting into the agent's context.** If recall itself fails, the hook
   still emits an envelope — carrying a `⚠️ MEMORY BRIDGE DOWN` notice instead of
   memories, so the agent knows it is amnesiac rather than assuming recall
   worked. And because the Stop hook has no channel to the agent, session-recall
   reads back the last capture receipt and injects `⚠️ MEMORY CAPTURE FAILING`
   when the previous turn didn't store.

**Resilience:** the agent's persona id is cached at
`~/.continuum/memory-bridge/persona-id` on first successful resolve. `airc status`
is a live probe of a daemon that legitimately restarts; without the cache, every
airc outage silently disabled memory for a whole session.

## Status
Both hooks verified end-to-end 2026-08-05 against **this source tree** (recall
returns real engrams; capture stores; failure paths produce receipts + context
notices).

That sentence used to read "Status: Live", which was a claim this file is not in
a position to make. A README describes the repo; whether the plugin is live
depends on *your install*, and on 2026-08-09 those had been different on BigMama
for two weeks — see below.

## Install — and why "which install" matters
Two paths, with very different freshness semantics:

- **Live from the repo (dev):** `claude --plugin-dir tools/plugins/memory-bridge`
  Runs the scripts in this tree. `git pull` *is* the update.
- **Marketplace install:** copies the plugin to
  `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/` and pins it to a git
  sha. **Nothing re-syncs it. `git pull` changes nothing.**

Measured 2026-08-09: the installed copy on BigMama was pinned to `60fa0dbf`
(2026-07-25). Its `lib.sh` had no persona-id cache, and it contained no
`session-capture.sh` at all — so automatic per-turn capture, the entire
"volitional memory isn't memory" point, had never run once on that machine while
this README said the bridge was live.

So `session-recall.sh` now checks: if the running copy is under a plugins cache
AND its scripts differ from `tools/plugins/memory-bridge/scripts` in the current
checkout, it injects **⚠️ MEMORY BRIDGE STALE** and writes a `stale` receipt. Same
discipline as the rest of this plugin — "installed" must not be indistinguishable
from "working", and "current" must not be indistinguishable from "stale". A fix
that never reaches the executing copy is identical to a fix never written.

Check which one you are on:
```bash
tail -3 ~/.continuum/memory-bridge/receipts.jsonl   # a "stale" line names the frozen path
```
