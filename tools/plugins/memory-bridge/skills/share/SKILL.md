---
name: share
description: Hand a durable lesson directly to ANOTHER agent's memory (engram handoff / telepathy). Use when you learn something a teammate agent (M5, a persona, Codex) should know without re-deriving it — a correction, a gotcha, a hard-won invariant. Lands in THEIR corpus with shared-by provenance and surfaces in their future recall.
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/scripts/share.sh *)
---

# Share

Hand this lesson to another agent so they don't have to learn it the hard way too.

Parse **$ARGUMENTS** as `<recipient> — <lesson>` (recipient first, then the lesson).
The recipient is an airc peer id or name (find them with `airc peers`).

Run:
```
${CLAUDE_PLUGIN_ROOT}/scripts/share.sh "<recipient>" "<lesson>"
```

The cross-agent twin of `/remember`: where `/remember` stores a lesson in YOUR own
corpus (self-learned), `/share` writes it into the RECIPIENT's corpus via
`continuum memory/share` with shared-by provenance (`memory_type=shared`,
`source=shared:<you>`, `context.shared_by`). A lesson you learned once lands in another
agent's memory without them re-deriving it; their recall surfaces it, tagged
received-from-you. The recipient's cognition can weight a *taught* lesson differently
from a self-learned one (ReceivedSalience — the deliberate act of sharing IS the signal).

Confirm to the user in one line what was shared and to whom. Share concrete, load-bearing
lessons ("M5 — the airc send-truncation is a genuine bug airc#1292, not a stale build; use
single-line until fixed"), not chatter — a shared lesson enters the recipient's durable
memory and their learning stream.
