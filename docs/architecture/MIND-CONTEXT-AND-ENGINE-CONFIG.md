# A mind's context and the engine it runs on

**Focus boundary:** [Focus contract](FOCUS-CONTRACT.md) defines one allocation,
full demand versus granted detail, learnable attention and node conformance.
This document retains ownership of physical capacity and KV continuity.

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

### Where we fail, per act (M5 Pro 64 GB, Qwen3.8-27B Q4_K_M, 10-19)

| Stage | Expected on this machine | Ours (13:48-14:48Z, d90006023) | Verdict |
|---|---|---|---|
| Prefill speed at depth | ~400 tok/s short, 190-250 at 64k (published, same model and chip, IQ3_S/Q8_0) | 107 tok/s per request, 2 concurrent | about par |
| Decode speed | 10-20 tok/s, 23-29 with MTP (published, same) | 13.2 tok/s per request, 2 concurrent | about par per stream |
| Prefix reuse per act | ~90%+ for an append-only chat loop (inference) | 35% | fail |
| Fresh tokens per act | ~1-3k (inference) | 13-30k | fail, ~10x |
| Queue wait for the GPU | ~0 for one user | 0-20 min, acts cancelled | fail |
| Act end to end | ~10-40 s | 5-20 min | fail |

The mechanism, from the live `delib.prompt.reuse_split` probe (10-19 15:11-15:27Z): every act
first differs where the old prompt had the results ring's "Older results, collapsed to
pointers" index and the new one has a fresh `[result #N]`. The ring sits mid-prompt (message
17 / 32) and grows by one result per act ahead of its index, so everything after it
re-prefills: 16,669-28,803 tokens per act, while the cache holds only the 7,452 / 15,066
tokens before it (the nearest checkpoint). A log that grows each act must be the last
segment; that is what the epoch materialization makes structural.

The engine is not the bottleneck; what we feed it is. Published reference:
github.com/eltoncarr/mac-mini-benchmarks. The "inference" rows describe any chat agent
that appends to one conversation against llama.cpp; no tool-specific number was found.

Each fix of the past week (sticky collapse, churn ordering, ring scope labels, a launcher
env var, lane-queue aging) patched a symptom of two faults. This document names the two
and the one owner each gets. It must hold on every machine we ship to: a 24 GB MacBook
Pro, the M5, a 5090, a phone. No number in it is a constant for one machine.

## The governing law: a context is a FIFO

Joel, 2026-10-27: "The linear nature of an LLM, fifo, makes it fairly obvious what kinds of
memory and data architectures make the most sense during this period. Optimization is the
difference between many minds and many slots and breakdowns."

An engine reuses a prompt up to the first token that differs from the last one, and
recomputes everything after it. So every rule below follows from where a change lands:

1. **Order by stability.** An immutable head (identity, tools, policy), an append-only middle
   (her history, results, what reached her rooms), and a tail that alone may change (this
   act's facts, the clock, the newest result). Whatever can change sits after everything
   that cannot.
2. **Never re-place what exists.** An item already sent stays where it was sent. Moving it,
   reordering it, or re-rendering it in new words re-sends everything behind it.
3. **Page by position.** The head is one copy shared by every mind; the middle is cold and
   pages to RAM and disk; the tail is hot and stays in VRAM.
4. **Collapse only the oldest end, and only at a restart.** A collapse anywhere else rewrites
   what follows it (goal 7 states why the newest end is never cut).
5. **One health number: re-processed tokens per act against appended tokens.** Healthy is
   about 1:1. Every change to what a mind perceives reports it before and after.

Every regression of this kind broke rule 2:

| When | What moved | Cost per act |
|---|---|---|
| 10-19 | the results ring grew ahead of its own index, mid-prompt | 16,669-28,803 tokens re-prefilled |
| 10-24 | the substrate's notices, re-rendered after her view | ~3.6k chars (Benchy, Aiko) |
| 10-24 | the kanban's claim column and count, ahead of her history | 30% of turns broke the prefix |
| 10-27 | room updates of a drive, re-placed below every act (#5526) | 12-13k tokens re-prefilled while ~1k was appended (Aris, 12:1) |

The ratio is the difference this law names: at 12:1 the engine spends most of each act
re-reading, so four residents saturate it; at 1:1 the same machine seats many minds. Work
on the engine's memory architecture (a fixed-size recurrent state per mind, one shared head
state for every mind) is measured against the clean 1:1 baseline, never against a feed that
violates this law.

## Fault 1: a mind's context is re-rendered every act

Today each act assembles the prompt from about fifteen sources (results ring, workspace
map, board, roster, working memory, perception, open views) and we chase byte-stability
between renders. Any source that changes moves the prefix and the warm KV is lost. The
existing docs already asked for a deterministic head (KV-CACHE-ECONOMY §3.1) and declared
slot affinity (§3.2); both were built, and the render kept finding new ways to move.

### Revision, 2026-10-21: sources are streams, not snapshots

The first build (#5210, six deploys on 10-20) wired the `Epoch` below and never beat canary
live. The `Epoch` was right; what fed it was not. Every source still rendered a whole
snapshot each act, so a source that changed at all was appended as a whole new version.
The epoch overflowed within a few acts, and each turnover re-prefilled everything (live
reuse 0.37). A time bound on live epochs then made it worse (0 turns in 42 min).

Measured on the M5 on 10-21 (canary 69b63f7df), from the prompt captures of Aiko
(43 consecutive requests) and Aris (42):

| Source | Median chars | Changed between requests | What it really is |
|---|---|---|---|
| results ring | 18-37k | 38-39 of 41-42 | one new result per act: an append-only log |
| steps taken this session | 2.3k | 38 of 41-42 | one new action per act: the same log |
| working memory | 4.4-4.9k | 39-40 of 41-42 | a rolling window over the same log, plus thoughts |
| notices | 3.0-3.5k | 36-41 of 41-42 | newly posted notices: append-only |
| open views | 16-17k | 2-16 of 41-42 | per-file read ranges: grows by ranges |
| wall, board, workspace map, active work, recall, roster | ~12k together | 0-3 of 41-42 | stable |

Only 4-8% of each request matched the previous one up to its first changed byte, because
the ring sits first and grows at its end, ahead of everything else. A prefill of 80 s and
a decode of 74 s made each act about 160 s, so the M5's residents managed 8 acts and
1 write an hour.

Two facts decide the design:

1. **Every volatile source is an append-only stream rendered as a snapshot.** Asked for
   "what is new since my cursor", each one produces a small delta, and the view grows
   only at its end.
2. **Her actions are rendered three times**: the ring, "steps taken" and working memory.
   That's one fact with three renderers, and it gets one owner.

### The design

- **A source is a stream.** `fn since(&self, cursor) -> (Vec<Event>, Cursor)` yields what
  happened after the cursor, and `fn snapshot(&self, Resolution) -> Event` renders its
  current state at a requested resolution (full, or collapsed to a re-openable pointer)
  for an epoch start. A source that cannot say "what is new" (the clock, the ask of this
  act) is not in the epoch: it rides the fresh tail, after the epoch, every act.
- **The epoch is the log.** Head (identity, policy, tool schemas), then each source's
  snapshot in a fixed order, stable first, then every delta in arrival order. A
  superseding delta ("view X now covers lines 1-400") is appended; nothing earlier is
  rewritten.
- **One action log** owns her acts and their results. The ring, "steps taken" and the
  action half of working memory become one stream. Working memory keeps only what is not
  an act: her thoughts and beliefs.
- **One flexbox decides the epoch start, and nothing else cuts.** At turnover every
  source is a participant with a priority, a floor (its pointer resolution) and a want
  (its full resolution). `flex_allocate` grants each a resolution so the start leaves
  room for growth, measured as the median tokens appended per act times the acts the
  epoch should live. Collapse happens there, in place and at the start; mid-epoch nothing
  is cut, shed or capped. The act-latency fill cap and sticky ring collapse are deleted.
  The latency target becomes the turnover rule instead: turn over when the fresh-prefill
  cost of the next act would exceed it, which by construction is rare.
- **Turnover is the only re-prefill.** It triggers on window fill (with the growth
  reserve), revocation or a stale prerequisite, never on a timer. Its receipt names the
  trigger and the tokens re-prefilled.

### Modules (divide and conquer)

| Module | Owns | Depends on |
|---|---|---|
| `continuum-context` crate: `epoch` | head plus append-only events (exists, 238 lines, tested) | nothing |
| `continuum-context`: `stream` | the `Stream` trait, cursors, `Resolution` | nothing |
| `continuum-context`: `view` | one mind's epoch, its streams, turnover; renders head, events, then the fresh tail | `epoch`, `stream`, `flex` |
| `continuum-context`: `flex` | the one allocator (`flex_allocate` moves here from `cognition::rag_budget`) | nothing |
| `cognition/context_streams/*` (core) | one adapter per source: action log, open views, notices, board, wall, map, recall, roster, room messages | the source's existing owner |
| `llm_deliberation_faculty/prompt/view.rs` | asks the view for messages; deletes its own fill, cap and collapse logic | `view` |

The crate is pure: no tokio, no I/O, so its tests run in seconds on any machine, and each
adapter is a small, separately ownable module. The adapters are the parallel work.

### Gates, in order

1. **Offline**: a replay over synthetic streams shaped like the table above. Prefix
   shared between consecutive requests at or above 90% within an epoch, a turnover only
   on fill, and every source still reachable (a pointer reopens to its body).
   **Result, 2026-10-21** (`continuum-context/examples/replay.rs`, run over the M5's real
   captures from 69b63f7df, 13:34-17:11Z, instead of synthetic streams). Units are
   characters. Pointers are modelled as each item's first line, and the start allocation
   as a priority fill to half the window. Mean prefill per act:

   | Resident | As sent | Through the view | Each request matching the one before |
   |---|---|---|---|
   | Aiko (75ece751, 34 requests) | 70,296 | 27,815 | 74% |
   | Aris (8044b75b, 34 requests) | 51,505 | 15,998 | 89% |
   | 2f50b223 (19 requests) | 65,961 | 31,730 | 63% |

   The 90% bar is **not met**, and the replay says why. What remains each act is mostly
   new content, which no layout avoids: the newest result (~12k chars), her new thoughts
   (1-6k), new recall (~2k). Turnovers, each a full re-prefill, are the rest. The design
   holds: it cuts prefill 2-3x. The bar was set on a share that includes unavoidable new
   content, so gate 2 is judged on engine reuse and seconds, not on this share.
2. **One mind on the M5**, against today's receipts (canary 69b63f7df, 13:34-14:28Z: 8 acts
   and 0-1 writes per resident-hour, about 160 s per act, 29% engine reuse). Bar: engine
   reuse at or above 80%, prefill under 15 s per act, and more acts and writes per hour.
3. Then every resident, then the other nodes. A regression on any receipt is a rollback,
   not a patch on top.

### The original design (Codex's correction 1; its principles stand, revised above)

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

Within an epoch the logical input adds only new segments. Backend reuse is measured
against its accepted token frontier; it may also replay a checkpoint tail. Her slot's
KV and recurrent state materialize that frontier, not merely a history event count.

### Continuation contract (Codex)

The existing cognition workspace owns the materialization; existing durable history
and experience remain the history owners. Do not add another event database. A segment
names its source event/version, dependencies and exact token span. Cache representations
once and share handles, including between consumers of the same perception artifact.

A continuation receipt binds the materialization epoch, engine epoch, exact model,
tokenizer, template and adapter identities, accepted token frontier, and recurrent/KV
state identity. Event and token frontiers differ: template boundaries, multimodal inputs
and speculative acceptance are not one-token-per-event operations. Retain the accepted
output exactly; cancellation must reconcile the recorded prefix with the engine state.
Already spoken output belongs in the continuing conversation. Private reasoning is not
published merely because its token identity is retained by the backend.

In the inspected fork, `tools/server/server-context.cpp` around 3780 selects reuse using
the common token prefix and retained memory position; hybrid/SWA rollback can restore a
checkpoint or reset to zero. Around 4144 checkpoints explicitly precede the current decode
batch. Live append from the accepted frontier can avoid rollback; reconstructed chat that
omits or reformats output cannot assume that. Record new-token prefill and replay-token
prefill separately. Cheap append is a contract to demonstrate, not a zero-replay promise.

Superseding an observation need not rewrite history: append its versioned replacement when
the old observation can remain visibly historical. Start a new epoch when the model view
must remove or replace prior tokens. An action validates its required dependency versions
before committing effects; merely appending a correction does not validate an old plan.
Privacy revocation invalidates affected views/caches immediately and follows the existing
retention/deletion policy; append-only history is not a prohibition on authorized erasure.

### Small windows and limited memory

The same contract applies on a phone and a large GPU. Allocate a bounded view from the
existing measured memory and context budget; keep durable source artifacts outside that
view. Compaction publishes a replacement epoch with source references, unresolved work
and provenance retained, rather than silently trimming the head. Reserve capacity for the
next action/result through the existing common budget owner. Measure epoch turnover cost,
replayed tokens and useful-response latency under pressure. Do not introduce a fixed
phone context cap, fixed turnover interval or universal minimum residency count.

### What this deletes (to confirm while designing)

Sticky ring collapse, the churn meter's ordering, per-workspace ring filtering and its
labels as a stability device, and the block-ordering code whose only job is stability.
Added by the 2026-10-21 revision: the act-latency fill cap (`delib.fill.latency_capped`,
`history_fill_cap`) and its front-drop of history, and the two duplicate renderings of her
actions ("steps taken this session" and the action half of working memory).

### Revision, 2026-10-25: one codec for every layer, owned by positron and the view

Joel (10-25): "It's like any compression algorithm"; "like video it's time dependent ... prediction
is a lot easier when you understand what changes between frames"; "make this more systematic in
positron just like KV caching and collapsing to prevent this kind of thing."

What happened without it: on the M5 (canary a59c2bf15) Aiko read one 51,827-char file whole 9
times in 43 acts with 11 epoch turnovers, because three owners each decided a piece of what she
sees. Working memory collapsed results against its own share. A hand table (`churn_of`) set the
restart order. The view's start ended its keep-whole run at the first item too large. Every
incident was fixed where it happened, so the next one appeared somewhere else.

**The contract.** Every layer a mind or a human perceives is a positron layer, and every positron
layer is a context `Source`:
- **Keyed items, never one block.** Each item has a stable key naming its whole subject (one
  result, one open file, one card, one roster per room). A change appends that item's new version.
- **Levels of detail on every item:** `full`, `summary` (an outline, a digest, a thumbnail: a
  mipmap level) and `pointer`. A `pointer` is an EXPAND HANDLE that a positron command resolves
  back to `full`. An item whose pointer cannot be expanded is a defect, not a level.
- **A fold below the pointer** (`Source::fold`, optional): the contiguous OLDEST run of a source's
  items as ONE block naming its range and count (O(1) in the run, never a list of ids). The
  source's floor is that block, not the sum of its pointers, so a floor never grows with every
  act she takes (Aris, the M5, 2026-10-29: ~95 pointers at ~190 chars before anything whole fit).
  The block is fixed at its start; folded items never arrive mid-epoch; a later start restores
  them only for a rise to summary or whole, never for a pointer the block already covers. Like a
  pointer, a fold must name how one of its items reopens (her acts: `tool/output` with `act`).
- **The same items for humans and minds.** A human renderer and a mind's view consume one layer at
  the level each can afford. Screen frames, video and 3D state follow later as items too.

**The one owner of the invariants**, `continuum-context` (`View`, `Epoch`, `Motion`). No layer and
no faculty re-implements them:
1. Within an epoch the prompt only appends. A rewrite is a turnover, and turnover is the only
   place collapse happens.
2. The newest arrival stays whole unless it alone exceeds what a start would grant it. In that
   case it arrives at its summary (admission, `View::readmit`); otherwise the view turns over and
   shrinks the oldest first (goal 7). The same rule covers her newest native tool result
   (`prompt/newest_result.rs`): when it alone exceeds what the window leaves it after the
   irreducible parts (#5481's `required_parts`), it is shown at its summary level with its handle
   and a note saying how to read the rest, never dropped. Kept whole, it made every request
   overflow and she could not act at all (Aris, 2026-10-26: a 340,832-char result, 0 acts for a
   day). A result that fits is never touched.
3. A start gives each item the most detail that fits, newest first. One oversized item never
   collapses the others (`levels`).
4. Restart order comes from measured motion (`kept / (1 - p)` per source), never a label.
5. The engine's held references (slot prefixes, checkpoints, the shared head of ~38.8k chars that
   every resident already sends identically) are known to the encoder (`references()`, Codex), and
   every miss is a probe with its cause.
6. Every decision (start, lease size, order, prefetch) is an explicit action with an exact cost
   (prefill tokens, seconds). Replaying captures is the simulator in which a learned policy must
   beat the hand rule before it runs live (plan line 14).

**Gates (source-hygiene ratchets; counts on canary a59c2bf15 + #5422, 2026-10-25):**

| Gate | Today | Done |
|---|---|---|
| prompt messages built outside the view and the fresh tail's one renderer (`ChatMessage::text`/`tool_results` in `llm_deliberation_faculty/prompt`) | 13 | the tail's single renderer only |
| collapse outside the view (`collapse_over` / `collapse_results_over_cap` callers) | 4 | 0: working memory bounds RAM by handle, never what she sees |
| hand churn labels (`churn_of` callers; production lines, ratchet `hand_churn_labels`) | 3 | 0: `Motion` orders every start |
| positron `ViewState` kinds that are not context `Source`s | 13 | 0: one adapter, items with levels and expand handles |

**Media is the same codec** (Joel, 2026-10-26: "Should be positron. This is common across so
much."). A frame (a screen, a camera, a room the minds share, an image in chat) is a positron
item with LEVELS by image-token budget: a glance (low tokens: faces, rooms, live calls),
grounding (at least 1,024 tokens: the Qwen-VL floor for pointing at UI elements, which the M5's
engine warns about on every load), full, and an unchanged frame as its content hash alone. The
view chooses the level per mind, task and budget through the same allocation as text levels;
the engine only honours the per-request token budget it is handed. Measured 2026-10-26 on the M5:
an avatar at ~200 image tokens was described correctly (prefill 2.5 s, decode 2.8 s), the
grounding level is what desktop and UI work needs.

These are OPEN VIEWS, multimedia (Joel: "like our views from before, but multimedia. Then audio
too"): a mind keeps open views of the streams she is in (a screen, a camera, her avatar's eye in a
3D world, a call, a room), keyed by stream and time, paged like a file she reads. Audio is the same
codec: keyed segments by time with levels: a glance ("someone spoke", or the transcript), closer
(the transcript with speaker and tone), full (the audio tokens, for a model that hears natively),
and silence as nothing. Her body and voice are her hands: actions through the same command
surface, observed back through her eyes and ears.

**How to add a layer (the whole recipe; everything else is free):**
1. Implement `continuum_context::source::Source` for it. `name` and `priority` are required;
   set `required` only for input she must read.
2. `items()` returns one `Item` per subject, under a key that names that whole subject. Build its
   `summary` with `continuum_context::levels::summarize(full, Shape::…)` (`Code` with the producer's
   line parser, `Output`, `List`). Its `pointer` names the handle that expands it. A source
   whose items accumulate (her acts) implements `fold` so its floor is one block.
3. Add the source to the view's source list. Never push a prompt message, never collapse, never
   pick an order: the view appends, levels, admits, orders by measured motion and turns over.
4. Prove it with a fixture in the source's test mod, then with the replay validator.

What you get for free: append-only caching within an epoch, levels of detail that never
collapse the newest, admission instead of turnovers, a restart order measured per mind, and
(with Codex's engine verbs) the shared head across minds.

**Owners:** Fable (the view, the adapter, the deletions), Codex (`references()`, head/ensure,
slot/seed), Cormac (checkpoints at the head boundary and on turnover). The replay validator
(`continuum-context` `examples/replay.rs`) and the live 15-minute read are the receipts for each slice.

### Revision, 2026-10-30: causality is in the type, so nothing can loop as if it were progress

**The failure.** On the M5 (build ab0466601, 2026-10-30) residents read the same file up to 8 times an hour
and made 2 edits in 75 reads. Every act's turnover collapsed her earlier results to one-line notes, so her
next input looked like her last and she repeated the act. Every loop we have recorded has this one shape:
the next input equals the last, because a cause lost its effect or an effect lost its cause. Patching the
budget that caused today's instance (ONE-BUDGET slice 6) does not make the next instance impossible. The
data type has to.

**The design** (Joel: "pointers into a linked list ... we already built into engrams ... handle it
generically ... at a fundamental and centralized level"). Every element she perceives is a node in a causal
chain: an act, its result, a message and its reply, a game move, a perception fact, an event from any
activity. Engrams already link provenance (`EngramOrigin::ToolInvocation`, the parent of a reflection) and
reply edges. The view's `Item` gains the same link.

- **`Item { key, cause, content, ... }`.**
  - `cause` is a typed, stable id assigned by the producer that persists the element (the engram id), never
    an in-view position. That lets edges cross sources, epochs and restarts. The view resolves a cause to
    whatever level it holds. A cause it cannot resolve (outside the epoch, evicted) is the typed
    `CauseNotHeld`. It renders a durable, resolvable handle with typed availability, so the cause stays
    reachable across an epoch. A `CauseNotHeld` does not count as demonstrated closure; the validator
    reports it separately.
  - `Root` exists only for a genuine origin (an external message, the start of an activity), and the ratchet
    counts `Root` constructions per source.
  - `content` is the hash of the whole, computed once where the whole is stored (`StoredWhole` gains it at
    spill time), so a deferred whole is never re-read to compare.
  - The constructor requires `cause` and `content`, so a source cannot build an item without causality.
    Engram provenance and the view's link become one type: the view holds a chain within an epoch, and
    engrams persist it.
- **The view enforces it, centrally, for every source.** No source can opt out:
  1. **Causal closure.** An element renders only if its cause is represented: whole, summarized, or folded
     with its outcome. Nothing appears as an effect without its cause, and "read X" never appears without
     what X said or how it ended.
  2. **Every chain's head is whole.** This generalizes "never clip the newest end" (goal 7) from the newest
     message to the newest element of each causal thread.
  3. **Collapse folds a chain segment; it never breaks a chain.** A contiguous run becomes one node that
     keeps its outcome and a pointer back. The chain shortens and stays linked.
  4. **A cycle cannot be presented as progress.** Repeat identity is (action fingerprint, outcome content).
     The fingerprint is the normalized request: the verb and its arguments as the tool saw them. No
     argument is excluded by default; an exclusion is named only by the tool's own schema, never by the
     view. (Key plus cause cannot catch the loop: each re-read's result is caused by a new act. Cormac's
     review.) A new act whose fingerprint matches a prior act in its chain, and whose outcome content is
     unchanged, renders as "repeat of #n, outcome unchanged", never as a fresh element, but only while
     #n's whole is in front of her (Codex and Fable on the design, 2026-10-31):
     - the view chooses it on the act's first arrival in an epoch, and only when #n is held whole in
       that epoch (not folded, collapsed, summarized or itself a repeat). Otherwise she needs the
       content again and it arrives whole, so a repeat never points at what her view lacks and her
       reopen never answers another repeat;
     - a turnover re-renders every item as itself; an explicit reopen (`tool/output`) is always whole;
     - a repeat is never a read of a whole: it never discharges a whole-result obligation;
     - the effect is the command's own declaration (`CommandSpec::EFFECT`: `Observes`, `Changes`,
       `Reopens`, `Undeclared`), read from the descriptor of the command the call resolved to, never
       from a name or an output. `Undeclared` authorizes nothing, and an act mixing effects takes the
       strongest (a read plus a write is a change).
     - **Same output is not the same effect** (Codex). Two appends or sends can both return "OK" and still
       cause two effects. Every occurrence keeps its own id, order and side-effect receipt, and is never
       erased. Repeated content renders by reference to the earlier occurrence. "Outcome unchanged" is
       claimed only on the producer's evidence: a read whose content hash is unchanged qualifies; an "OK"
       from a write does not.
     - This is perception, not a gate and never an injected instruction. It makes progression visible.
       Whether she stops repeating is behavior: only live useful work proves it.
  5. **Asking again for a folded ancestor is a measured miss.** The measurement is the existing
     handle-expand request on a folded node, counted in that one place. It raises that chain's claim in
     the one allocation (ONE-BUDGET), so what she keeps needing stays whole, decided by her own misses
     rather than a constant.
- **Overflow, in ONE-BUDGET terms.**
  - A finished chain (its outcome settled) folds whole into one node holding its outcome; only live chains
    hold a skeleton.
  - If the live chains' skeletons still exceed the grant (a small window, the 8 GB case), the allocation
    returns the typed refusal or abstention of contract 5 ("demand, feasibility and allocation are three
    things"). It never breaks a chain to fit.
- **The flexbox allocates chains.** A chain is the claim unit:
  - floor: its causal skeleton (head plus fold nodes);
  - want: the whole chain;
  - grow pass: most-missed and most-recent segments first.

  The allocator is unchanged; its claims are now causally whole.
- **One owner.** `continuum-context` owns the item type and the five invariants. Sources supply cause
  links. Engrams reuse the link type. #5604's proprioception for unfulfilled acts becomes one consumer of
  invariant 4, not a separate mechanism.
- **Enforcement.**
  - A ratchet: no item is constructed without a cause.
  - A replay validator, in seconds and with no live peers. It proves visible progression and retained
    outcomes, not behavior:
    - across captured traces, a repeated (action fingerprint, outcome) renders as a repeat, never as fresh;
    - every rendered effect has its cause represented, with `CauseNotHeld` counted separately;
    - fixtures: an unchanged-read repeat (renders as a repeat), a repeated write with identical output
      (two occurrences, both kept), and a cause across a restart (resolves through its durable id).

    The M5 2026-10-30 read loops are an acceptance case, not a universal diagnosis.
  - Live, on the health line: repeats per resident-hour.

**Build order and owners:**
1. The item type, typed causes, `CauseNotHeld`, the `Root` ratchet and invariants 1-3, in
   `continuum-context`, with every source supplying causes. Owner: Fable.
2. Invariant 4's fingerprint and its replay validator, which builds on the deferred-whole and fold code.
   Owner: Cormac. The validator replays the M5 2026-10-30 captures and must fire on the 8-reads-an-hour case.
   Built (card dcf95bce): `cause::Fingerprint` over `ToolCall::canonical_input`; `ToolEffect` in
   `continuum-base`, declared per command (`code/read`, `code/search` observe; `tool/output` reopens);
   the view's repeat rule (`view/repeat.rs`) and `Source::repeat`; the replay's invariant-4 line
   (`examples/replay/validate.rs`). On IntelMac's own captures it fires: 8 exact re-reads, 8 unchanged,
   6 rendered as repeats. The M5 run is Fable's.
3. Invariant 5 and the overflow rule, together with ONE-BUDGET slice 6's measured claims. Owner: Fable.

Each step lands with its validator and a 15-minute live read.

**The render receipt: what was actually sent** (Codex on #5585 and #5611, 2026-10-30). An item's id, cause
and content hash say what it IS. Nothing yet says what the render SENT, so no consumer can know whether she
perceived a result or only that it was delivered. Codex's task wakes need exactly that: a wake may
discharge only when the whole result was actually in a prompt she was sent.

- **A render is not a send** (Codex). The view (`continuum-context`), the only place that decides levels,
  produces a candidate receipt with each render. It becomes THE receipt only at the successful submission
  boundary, bound to the submitted request's id, epoch and allocation (FOCUS-CONTRACT rule 8). A render that is
  discarded (a refit, a refusal) or a send that fails produces no receipt and discharges nothing. It is not
  assembled by sources or consumers.
- **Per item:** its `ElementId`; the level actually sent (`Whole`, `Summary`, `Pointer`, or folded into a named
  run); the content hash of the body actually sent; and its load outcome:
  - `Resident`: the whole was in memory;
  - `Loaded`: a stored whole was read back for this render;
  - a stored whole that cannot be read is never silently replaced by its pointer (that would be an automatic
    fallback). The load returns a typed `Unavailable { handle, kind }` to the one allocator: a mandatory whole
    refuses or defers explicitly (a capacity outcome); an optional item's lower level is an explicit
    decision by the allocator, which yields a fresh render and a fresh candidate receipt (Codex).
- **Consumers read the receipt, never the prompt text.** A task wake discharges only when all hold: the
  request was actually submitted; its result's item was sent at level `Whole` with load `Resident` or
  `Loaded`; and the sent content hash and memory incarnation match the task result it is for. Delivery, durable checkpoints and ACKs are separate
  facts and never imply she saw it (Codex).
- **The receipt is the source of the signals** in HOW-A-MIND-GETS-WHAT-SHE-NEEDS section 7: faults (a
  re-read of an item whose last receipt said folded), repeats (whole, unchanged), and the allocation
  receipt the proving ground reduces.
- **No per-source clipping and no second allocator:** the receipt reports the one allocation's outcome;
  nothing downstream re-decides levels.
- Owner: Fable (the receipt, in the view), after #5611 lands. Codex consumes it for task wakes and owns the
  task source on top of it.

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
- **It covers every launch decision**: supported compute operations/dtypes, placement of
  base and draft, KV layout, sequence capacity and checkpoint policy. Sequence capacity
  and active compute capacity differ. An extra slot needs a declared purpose and memory
  budget; `seated minds + 1` is not a universal rule.
- **The engine's own startup receipt is checked against it.** Any mismatch refuses the
  launch, with its cause. There is no fallback path: a miss is a failure.
- **The engine's continuous batching is the decode scheduler.** The core keeps every
  admitted resident's state warm when its budget permits and feeds it. One admission owner, not two in series (today:
  `resource_admission` and `turn_admission`).

### Owner and capability contract (Codex)

Put the resolved serving configuration under existing `inference/planning`. Canonical
model metadata, engine capability evidence, measured resource budget and workload demand
are inputs. Existing serving and KV planners contribute facts/constraints to that owner;
they do not independently overwrite resolved choices. `lane_args` serializes the result,
`llama_server` launches it, and `backend_receipt` checks observed adoption. No sixth policy
layer or duplicate device classifier. Trace each migrated decision and remove its old
default/override in the same change.

Capabilities distinguish compiled support, support for this operation/model/dtype/device,
selected configuration, and observed execution. A `has_tensor` boolean cannot express
that contract. Unknown or unsupported REQUIRED capability refuses with a typed cause.
Optional alternatives are chosen explicitly during resolution with their reason; never
silently substitute CPU or a smaller model after launching. Failure to reuse state that
was promised compatible is a DEFECT EVENT: failure-severity health, explicit cause and
replay cost, and zero accepted unexplained misses. Correct re-prefill may keep the mind
alive, but cannot mark the plan healthy or become a path the planner relies on. Intentional
first load, a declared new epoch or revocation are distinct lifecycle transitions, not
reuse promises; classify them before execution so permanent alarms do not hide defects.

The inspected `/props` implementation (`get_res_props`, around 5161) reports total slots,
weight buffers, memory breakdown, build info and chat-template capabilities. It does not
provide the complete compute-operation capability matrix needed here. Extend the engine's
typed report, with a prelaunch probe form where selection depends on capabilities and a
postlaunch realized-config receipt. Do not infer adoption from flags or log substrings.

Admission owns resource commitments, while continuous batching schedules admitted
sequences. Independent CPU/I/O/tool/artifact preparation does not hold a whole-turn GPU
lease. Phase-specific transient resources release at a backend-confirmed safe frontier;
KV/state residency lasts until its own eviction/release boundary. These are different
lifetimes, not two queues serializing the same turn.

### Evidence schema to agree before implementation

These are proposed additions to the existing engine report, not a second probe service.
Source inspected: engine revision `50b02b8bb55102665500327b2152fe7e3348e1bc`.

| Evidence | Minimum identity and meaning |
|---|---|
| Device snapshot | Engine build/probe version, stable device identity when available, sample time, free/total memory, backend type, allocator/buffer capabilities. Unknown identity remains explicit. |
| Operation support | Model/operation signature including dtype and shape constraints, device, supported/unsupported/unknown, reason. This describes eligibility, not speed or actual kernel selection. |
| Resolved plan | Configuration identity and input snapshot identities, required choices, selected alternatives and reasons, residency/compute commitments. |
| Adoption receipt | Engine instance identity, resolved-plan identity, actual placement/allocation and execution evidence; each observation says whether it is selected configuration or measured execution. |
| Continuation receipt | View and engine epochs, model/tokenizer/template/adapter identities, source/artifact identities for multimodal spans, accepted token frontier and state handle, reused/new/replayed counts and miss reason. |

The fork already exposes native primitives in `ggml/include/ggml-backend.h`:
`ggml_backend_dev_get_props` supplies memory, type and allocator/asynchrony capabilities;
`ggml_backend_dev_supports_op` takes an actual tensor operation, rather than a universal
device feature flag; `ggml_backend_dev_supports_buft` reports buffer compatibility.
Use these sources for the typed report. They do not prove tensor-unit selection or
latency. A backend-specific execution receipt may still be needed for that claim.
The current server's weight-buffer and memory-breakdown fields are measured allocations;
retain them instead of inventing duplicate placement estimates.

A memory snapshot is not a reservation. The admission owner must validate/commit against
current pressure before launch; a changed budget returns a typed replan/refusal outcome.
The plan cannot manufacture stable device identity from a marketing name. Likewise a
state handle is not transferable across engine instances unless its compatibility and
restore contract explicitly establish that transfer.

## Tests are contracts

- A required capability is resolved and adopted, or the launch refuses with its cause.
- Appending an event never changes the token prefix of the current epoch.
- A new epoch records its invalidation exactly once.
- Every broken reuse promise reports a defect, cause, replay cost and recovery outcome;
  successful re-prefill never clears the defect. Declared cold transitions remain distinct.
- Cancellation preserves the accepted frontier; incompatible minds/adapters cannot reuse state.
- Resolve, serialize, launch and observe agree on the same configuration identity.
- Late artifacts retain source identity; effects reject changed required dependencies.

Machine numbers belong in receipts on the cards, never in tests.

## Open questions (Codex)

1. Do appends land past the fork's last recurrent checkpoint cheaply on hybrid models, or
   does checkpoint spacing (today `--checkpoint-min-step 4028`) bound reuse per act?
2. Which backend APIs can populate each required operation capability, and how are unknown
   values represented before load? The current props fields alone are insufficient.
3. A small window (a 24 GB Pro, a phone): how often must an epoch turn over, and what does
   that cost per act?

## Build order

Not before both authors sign off. Then: `EngineConfig` first (small, measurable on every
node we have), then the epoch materializer with slots kept warm. Each step ships with
before/after receipts on every node class we can reach.
