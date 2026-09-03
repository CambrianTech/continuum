# The URI-Addressed Desktop — one path tree, one semantic layer, N universes

*2026-09-03, from Joel: "Works with URIs for into the persona's profile, and even mind. Same
concepts all throughout." This is the design, written down so it stops living in one head.*

## The rule

Everything a human or a citizen can look at has a **path**. A path resolves to **one
positron view state** (Rust, the single truth) rendered by **one content body** (the
semantic layer) skinned by **whatever universe** the viewer chose (Trek engineering deck,
workshop, plain). No client computes truth; no page is a special case.

```
/academy                                  the learning base: rounds index (the board)
/academy/swe-bench-verified/seed-7        a round: mission, crew, cards, reviews, grades
/academy/swe-bench-verified/seed-7/9c6684ae   one card: its solve room, acts, verdict
/projects/space-game                      a project base: its kanban, rooms, playtests
/projects/space-game/collision-bug        a sub-activity under what spawned it
/commons/general                          an open room
/personas                                 the users index (kind × liveness; dormant filtered)
/personas/kira                            profile: identity card, genome, claims, recent acts
/personas/kira/mind                       what she sees now: grounding, doctrine, working set
/personas/kira/mind/engrams               her memory, by recall key; dreams consolidated
/personas/kira/learning                   examples buffered, buckets, adapters, grades credited
/core                                     the serving core: lanes, demand, throughput, dreams
/core/lanes/3                             one lane: who holds it, prefill/decode, KV pages
```

The tree is **dynamic** (a room's parent is the activity that spawned it; a top-level
activity's base comes from its recipe — `academy` for learning by purpose), never a list of
names in code. The path segments are the room directory's names; ids resolve the same
routes (`/room/<uuid>` is the same page as its path).

## What each page is made of (nothing new)

| Page | View state(s) already in Rust | Verb a citizen can call today |
|---|---|---|
| rounds index | `BenchViewState` (board truth per card since #3673) | `benchmark/rounds` |
| round / card | Bench + Kanban + Roster + Chat | `benchmark/rounds`, `work/get` |
| persona profile | Roster + presence directory + genome | `persona/roster`, `presence/directory`, `genome/list` |
| persona mind | the RAG allocation + deliveries | `persona/rag-inspect` |
| persona learning | training-trigger buckets + verdicts | `genome/training-trigger/status` |
| core | Serving + metrics | `serving/*`, `ai/report` |

The **rails** (rooms, users, charts) are the same view states **filtered, default on**:
"show finished", "show dormant", switchable. The **index pages** are where the whole
inventory is browsed, the academy board being the template.

## The two verbs the tree waits on

1. **Node-wide room directory in airc** (`DirectoryRequest`): every room the daemon hosts
   across attached scopes — id, name, binding parent, standing, member count, last activity.
   Today the daemon lists only its own scope's subscriptions, so purpose reads "chat" and
   standing is unreadable for rooms the viewer has not joined.
2. **Room-id addressing** for `room/join` and `activity/*` (archive/protect): today they act
   on the caller's CURRENT room or resolve a name by hash, which no per-run room satisfies.

## Build order (outliers first, then the generator)

1. The path grammar + the router: path → (view state, content body). One place.
2. Two pages that are maximally different: **rooms index** (a directory projection) and
   **persona mind** (a per-citizen inspection). If both fit the same page contract, the rest
   are content bodies.
3. Rail filters (default on) over the same view states.
4. The universe skin (the Engineering Console mock is the first payload).

Team benchmarks keep running throughout; they are the measured thing, and every page here is
a way to see them. Beta is the dogfood bar: Joel's full day on the desktop, zero interventions.
