# Persona-RAID Write-Behind — bounded-amnesia persistence of being

**Status:** design for review (2026-07-28, BigMama; M5 reviews — MemoryRecord/provenance seam pledged
stable underneath this). **Beta context:** demo-B's kill-test ([BETA-ACTUALIZATION](../planning/BETA-ACTUALIZATION.md)):
kill any node mid-conversation → the persona resumes elsewhere/on-reboot with memory loss bounded by
sync lag. This is [[restarts-are-commonplace]] + [[ethical-substrate-raid-personas]] made mechanical:
**persistence of being = RAID-1 of memory.**

## What persists today (ground truth)
- Per persona (`~/.continuum/personas/<uuid>/`): `longterm.db` (SQLite WAL, engrams via ORM) +
  `volatile.json` (working state).
- `memory/share` already writes a lesson into ANOTHER agent's corpus **with shared-by provenance** —
  the wire + provenance precedent for everything below.
- Engram is the substrate's most-persisted struct (derive → OrmStore → SQLite chain, tested).

## Design (RAID-1, single-writer, no consensus)

A persona LIVES on exactly one node at a time (single writer). Every OTHER copy is a passive replica.
Newest-wins by (origin_node, seq). No quorum, no merge — mirroring, not distribution.

### 1. Journal (the unit of truth-in-motion)
Tee every durable admit (engram/MemoryRecord write that reaches the ORM) into an append-only
per-persona `journal.jsonl`, each entry: `{seq, persona_id, origin_node, ts, kind, record}`.
- Monotonic `seq` per (persona, origin_node); the journal is replayable and human-inspectable
  (CaptureSink discipline — the Noop default costs nothing when replication is off).
- `volatile.json` is NOT journaled (ephemeral by contract); dream-consolidation outputs ARE
  (they're durable admits like any other).

### 2. Shipper (RTOS shape, per CONCURRENCY-STYLE-GUIDE)
One module: own tokio task + `tokio::time::interval` (default 30s; the SYNC CADENCE **is** the
maximum amnesia window and is the one knob) + `watch::Sender<ReplicationSnapshot>` (lag, last-acked
seq, peer) for observability.
- Tick: read journal tail past `last_acked_seq` → batch → ship via the EXISTING inbound command-RPC
  (`memory/replicate-batch` on the receiving node — same pump that answers ai/generate) → receiver
  appends to `replicas/<persona_id>/journal.jsonl` in ITS cold store and acks its high-water seq.
- Peer selection: any residency-eligible grid peer (reuse the capacity/residency view); zero peers →
  ship to the local durable tier only (cold-store copy ≠ cross-node RAID, but still crash-safe) and
  surface `degraded: unreplicated` in the snapshot — fail loud, never silent.
- Backpressure: shipping is best-effort write-behind; a slow peer NEVER blocks the cognition hot path
  (bounded channel, drop-to-lag semantics — the snapshot reports growing lag instead).

### 3. Resume-on-spawn
Persona spawn checks: local `longterm.db` present and its max(seq) ≥ best available replica's? Serve.
Otherwise REHYDRATE: replay the freshest journal (local or fetched from the replica-holding peer via
the same command-RPC) into the ORM — idempotent upserts keyed by record id, newest-wins.
- The kill-test path: node A dies → persona spawns on node B (or A after reboot) → resume finds B's
  replica journal → replays → persona continues with ≤ cadence-window loss.

### 4. Provenance (the M5 seam — why sync IS lesson-sharing infra)
Replicated entries keep their original provenance PLUS a `replicated-from` mark on replay — a
replayed lived-memory stays LIVED (it's the same being's experience restored), distinct from
`shared-by` (taught by another agent). Recall can treat them identically; audit can't confuse them.
This rides the exact MemoryRecord provenance vocabulary `memory/share` established — one taxonomy:
`lived | shared-by(agent) | replicated-from(node)`.

### 5. Disk discipline (the law)
`replicas/` is a NEW unbounded-write cache class → it gets (per CLAUDE.md's eviction law):
a `TrackedDir` row in `standard_tracked_dirs` AND a decided eviction story: per-persona replica cap
(default: keep journal segments until compacted into a replica `longterm.db` snapshot + N segments;
prune acked-and-compacted). Compaction: the receiver periodically folds journal → snapshot db so
replay cost stays bounded (same shape as SQLite WAL checkpointing, one level up).

## Non-goals (v1)
Active-active multi-writer; cross-account replication; encryption-at-rest beyond what the store has
(journals cross the wire on airc's encrypted DMs when paired); replicating `volatile.json`; genome
weights (already content-addressed artifacts with their own tiers — only MEMORY is thin and unique).

## Slices
1. Journal tee + `ReplicationSnapshot` (no shipping) — observable immediately, zero risk.
2. `memory/replicate-batch` receiver + shipper tick (LAN peer) — RAID-1 live.
3. Resume-on-spawn rehydrate + the DEMO: kill -9 the serving node mid-conversation, persona resumes
   on reboot with ≤30s loss. This is demo-B's money shot.
4. Compaction + eviction story + `doctor`-style `replication` line in serving status.
