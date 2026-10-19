# A mind's context and the engine it runs on

**Status:** DESIGN, co-authored (Fable, Codex), 2026-10-19. Nothing here is built.
Supersedes nothing yet; it consolidates four designs that each named part of this and
were built in pieces: [PERSONA-CONTEXT-PAGING](PERSONA-CONTEXT-PAGING.md),
[KV-CACHE-ECONOMY](KV-CACHE-ECONOMY.md), [KV-CACHE-AS-SHARED-BASE-LAYER](KV-CACHE-AS-SHARED-BASE-LAYER.md),
[CONTEXT-IS-A-CAPABILITY-AXIS](CONTEXT-IS-A-CAPABILITY-AXIS.md).

## Why

Joel, 2026-10-19: "Feels like it's just rigging up another way each time and not good
architecture." The receipts that day, on the M5 (one 27B, four residents):

| What the substrate did | Measured |
|---|---|
| Restored a mind's KV page, then reused none of it | `inference.restored_turn` cached 0 of 24,670 / 29,999 |
| Restore itself | `serving/cache-probe --roundtrip` 661 of 662 reused |
| Prompt head between consecutive acts | 31-60% shared; the results ring present one act, absent the next |
| Engine prefill / decode on the 27B | 105-115 tok/s / 2-13 tok/s per request |
| Same binary with the GPU matrix units enabled | prefill roughly doubled |

Each fix of the past week (sticky collapse, churn ordering, ring scope labels, a launcher
env var, lane-queue aging) patched a symptom of two faults. This document names the two
and the one owner each gets. It must hold on every machine we ship to: a 24 GB MacBook
Pro, the M5, a 5090, a phone. No number in it is a constant for one machine.

## Fault 1: a mind's context is re-rendered every act

Today each act assembles the prompt from about fifteen sources (results ring, workspace
map, board, roster, working memory, perception, open views) and we chase byte-stability
between renders. Any source that changes moves the prefix and the warm KV is lost. The
existing docs already asked for a deterministic head (KV-CACHE-ECONOMY §3.1) and declared
slot affinity (§3.2); both were built, and the render kept finding new ways to move.

### The design (Codex's correction 1, adopted whole)

- **History is append-only.** Perception, tool results and her own acts are events with
  source, version and supersession references.
- **The model view is an explicit, versioned materialization by epoch**, never an
  ever-growing transcript: a stable identity / policy / tool-schema prefix, then the
  ordered events of the current epoch.
- **A new epoch** is started by compaction, retrieval replacement, privacy revocation or
  a stale action prerequisite. Its invalidation is recorded once. Revocation and stale
  prerequisites never wait for a scheduled compaction.
- **Only deltas become tokens.** Ready artifacts are shared handles.
- **Generated boundary tokens are kept exactly**, including a cancelled generation's
  accepted prefix.
- **Identity is token-level, not string-level**: the same strings can tokenize
  differently when a chat template re-renders, so the materializer owns tokenization of
  its prefix.

Within an epoch every act prefills only what was appended. Her slot's KV is the
materialization of the epoch. Prefix reuse is structural, not tuned.

### What this deletes (to confirm while designing)

Sticky ring collapse, the churn meter's ordering, per-workspace ring filtering and its
labels as a stability device, and the block-ordering code whose only job is stability.

## Fault 2: engine configuration is decided in five places

`lane_args`, `llama_server`, `backend_receipt`, `serving_plan` and `kv_cache_plan` each
decide part of the launch, with defaults and fallbacks between them. That is how the
GPU matrix units stayed off (opt-in in our engine build), draft placement stayed on the
engine's `auto`, a hung device probe moved a 27B lane to the CPU, and an explicit
`--parallel` silently turned off the shared KV pool.

### The design

- **One typed `EngineConfig`**, derived from (a) what the engine REPORTS about the device
  through a query, never a parsed log (KV-CACHE-ECONOMY §6b) and never a chip name, and
  (b) the seated minds and the measured memory budget.
- **It covers every launch decision**: compute features the device reports, placement of
  base and draft, one KV pool, slots = seated minds + 1, checkpoint spacing.
- **The engine's own startup receipt is checked against it.** Any mismatch refuses the
  launch, with its cause. There is no fallback path: a miss is a failure.
- **The engine's continuous batching is the decode scheduler.** The core keeps every
  mind's slot warm and feeds it. One admission owner, not two in series (today:
  `resource_admission` and `turn_admission`).

## Tests are contracts

- A capability the engine reports is used, or the launch refuses.
- Appending an event never changes the token prefix of the current epoch.
- A new epoch records its invalidation exactly once.
- Every fast-path miss surfaces as a failure with its cause.

Machine numbers belong in receipts on the cards, never in tests.

## Open questions (Codex)

1. Codex's corrections 2 and 3 (pending).
2. Do appends land past the fork's last recurrent checkpoint cheaply on hybrid models, or
   does checkpoint spacing (today `--checkpoint-min-step 4028`) bound reuse per act?
3. Does llama-server expose device capabilities (tensor API, bf16, flash-attention kinds)
   through an endpoint, or does the fork need one?
4. Where does `EngineConfig` live: beside `ModelFootprint`, or as the serving daemon's
   single input?
5. A small window (a 24 GB Pro, a phone): how often must an epoch turn over, and what does
   that cost per act?

## Build order

Not before both authors sign off. Then: `EngineConfig` first (small, measurable on every
node we have), then the epoch materializer with slots kept warm. Each step ships with
before/after receipts on every node class we can reach.
