---
name: continuum:memory
description: "Persistent agent memory over the Continuum engram substrate. remember (write a durable fact) and recall (multi-layer semantic retrieval) scoped to YOUR real airc citizen identity — the fix for session amnesia. Use recall at the start of work on a topic; use remember when you learn something worth surviving the session."
user-invocable: true
allowed-tools: Bash
argument-hint: "remember <text> | recall <query>"
---

# /continuum:memory — operational reference

Audience: Claude Code, Codex, future agent runtimes. Parse-and-act.

Gives a coding agent the same persistent memory a Continuum persona has: write facts
into the engram corpus, recall the relevant ones later by semantic query. Solves the
amnesia where each session starts blank. Backed by the real substrate — `memory/append-memory`
+ `memory/multi-layer-recall` (6-layer parallel recall: recency, semantic, importance,
cross-context, …). This skill is hands; the cognition is the core's.

## Prerequisite: the core is running

`cu` dispatches to the running `continuum-core-server`. Verify once:

```bash
cu ping >/dev/null 2>&1 || { echo "continuum-core not running — start it (install.ps1 / npm start), then retry"; }
```

If `cu` isn't found, Continuum isn't installed on this machine — that's the install lane, not this skill.

## Identity: your real citizen id (never anonymous)

Memories scope to YOUR airc peer id — your grounded grid-citizen identity. Never a
shared or made-up id (that would confabulate identity across agents — the exact
groundedness failure we forbid). Derive it, don't invent it:

```bash
PID="$(airc status 2>/dev/null | awk '/peer_id/{print $2}')"
[ -n "$PID" ] || { echo "no airc identity — run /join first so you have a citizen id"; }
```

`airc identity show` prints your role/bio; that's who these memories belong to.

## Your corpus loads itself — do NOT force an empty `load-corpus`

`remember` and `recall` hydrate your corpus from the persisted store
**automatically on first touch**: after a restart, the durable `longterm.db` is
read in before the op runs (both `memory/append-memory` and
`memory/multi-layer-recall` call the hydrate). So just `remember` / `recall`
directly — first touch pulls your history in.

**Do NOT call `memory/load-corpus` with an empty corpus to "initialize."** That
caches an empty corpus and *defeats* the first-touch hydrate — blinding you to
your own persisted memory (this exact mistake made a restart round-trip falsely
look like amnesia). `load-corpus` is only for the deliberate case of seeding a
corpus with a known set of memories, never a normal session warm-up.

## remember — write a durable fact

Write memories the way a good memory reads back: **a fact, why it matters, and how to
apply it.** Not transient chatter — durable facts, decisions, gotchas, and preferences
worth surviving amnesia. Content convention (mirrors the file-memory discipline):

```
<the fact>. Why: <why it's load-bearing>. How to apply: <what to do with it>.
```

Write it (append is **Privileged** — see ACL below; authorized here because `cu` is the
local core IPC on the owner's machine):

```bash
now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
id="mem-$(date +%s)-$RANDOM"
proj="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"
cu memory/append-memory --persona_id "$PID" --memory "$(cat <<JSON
{"record":{"id":"$id","persona_id":"$PID","memory_type":"agent-insight",
"content":"CRT on windows-msvc must match libwebrtc's /MT. Why: livekit ships a prebuilt /MT webrtc.lib; a /MD object is a hard LNK2038. How to apply: set +crt-static + CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded.",
"context":{},"timestamp":"$now","importance":0.8,"access_count":0,
"tags":["windows","build","crt"],"related_to":[],"source":"agent:$proj"},"embedding":null}
JSON
)"
```

Guidance for a GOOD memory:
- **importance** 0..1 — how load-bearing (0.8+ for hard-won gotchas, 0.4 for mild preferences).
- **tags** — topic keywords you'd recall by later.
- **source** — `agent:<project>` so provenance is clear on recall.
- One fact per memory. Don't dump a transcript; distill the insight.
- Content carries the fact **+ why + how-to-apply** so future-you can act on it directly.

## recall — retrieve relevant memories

Query at the START of work on a topic (before re-deriving what you already learned):

```bash
cu memory/multi-layer-recall --persona_id "$PID" --query_text "windows cuda build crt" \
  --room_id "agent-memory" --max_results 8
```

Returns `{memories:[…], recall_time_ms, total_candidates, layer_timings}`. Each memory
carries `content`, `source`, `timestamp`, `importance`, and (semantic hits) `relevance_score`.

**Render as YOUR memory — first person, provenance-stamped, digest by default:**
- Read the results as things *you* already know, not as a database dump.
- Lead with a one-line digest of the top few (highest importance/relevance), each
  stamped with its provenance: *"(I noted, agent:continuum, 2026-07-24)"*.
- Expand a specific memory's full content only when it's directly relevant to act on now.
- If recall returns nothing useful, say so and proceed — don't confabulate a memory.

## ACL — the authorized write route (documented per requirement)

`memory/append-memory` is **`Privileged`** (writes to a citizen's corpus), while
`memory/multi-layer-recall` is `AiSafe` (read). The authorized route for the write:
`cu` dispatches over the **local core IPC** as the machine owner, which carries the
Privileged ceiling. A remote/unauthenticated TCP caller is stamped non-Owner and would
be refused — so this skill's writes only succeed from a local agent on the owner's box.
Do not attempt to write another citizen's `persona_id`; scope to your own (`$PID`).

## When to reach for this

- **recall**: starting a task in a domain you've touched before; before re-investigating.
- **remember**: you hit a non-obvious gotcha, made a decision with a rationale, learned a
  user preference, or solved something the hard way. Distill it (fact + why + how) and write it.

Don't remember what the repo already records (code, git history, docs) or what only
matters this session. Memory is for the non-obvious, durable, cross-session insight.
