# One act, end to end

**Status:** draft for agreement by Fable, Codex and Cormac (2026-11-04). Nothing new merges until
the three sign the "Agreed" line at the bottom; every open branch is then fit to this or closed.

**Why this exists.** Joel, 2026-11-04: "There is no coherent design anymore." On that day the M5's
four residents went from writing to dark and back four times, every part green. The docs disagree
with each other and the code follows a third version neither describes (inventory below). This
document is NOT new doctrine. It follows ONE act of ONE resident through the system, names the one
owner and the one invariant at each stage, rules on every contradiction (which existing contract
wins), and states how the whole is PROVEN before any of it goes live.

**The goal it serves** (CLAUDE.md, the path): residents who code at pace, many at once, on any
hardware. The measured obstacle is not the model. It is that the substrate makes her re-read: every
token in her context is paid again in PREFILL whenever the engine's copy is lost, and prefill is the
one resource that does not scale with concurrency (measured below).

## The numbers this design is held to (measured 2026-11-04)

| Fact | Value | Receipt |
|---|---|---|
| M5 memory | 68.7 GB (M5 Pro), not 128 | `sysctl hw.memsize` |
| Model weights (Qwen3.6-35B-A3B) | 20.4 GB, wired | lane footprint |
| Reclaimable at launch | 21-24 GB with the old engine still wired; 45.6 GB after its release | vm_stat 20:56Z, 21:00Z |
| Launch pools the same build got | 33,040 / 50,275 / 88,425 / 125,640 / 413,625 / 654,336 tokens | engine `-c`, by launch timing |
| Prefill, IntelMac CPU, matched | 68.3 tok/s at 1 stream, 68.0 at 2, 63.2 at 4 (server ~1,070% CPU): does NOT scale | Cormac's bench, 21:4xZ; raw in `evidence/knee-20261104/` |
| Decode, IntelMac CPU, matched | 19.9 / 32.8 / 43.6 / 51.1 tok/s aggregate at 1/2/4/9: scales | same |
| Prefill by prompt size, M5 (one engine, flash-attn, KV q4_0, ubatch 2048) | 399-9,371 tokens: 415-1,023 tok/s; 13-24k: 135-348; 141,739 tokens: 148 tok/s (~16 min for one act) | llama-server prompt eval lines, 2026-11-04 |
| Cold act, M5 | Atlas 76,114 prompt tokens, 0 cached: 318 s prefill | 21:37Z |
| Warm acts, M5 | Aiko ~20k prompts: 28 acts in 20 min; Aris ~89k prompts: 9 acts in 30 min | 20:15-20:35Z, 18:49-19:22Z |
| A whole-pool grant to one mind | pool 125,696 -> {Aiko 114,592}, three dark 20 min | 20:15-20:35Z |
| An admission hold nobody released | 16 min, zero generations | 18:00:46-18:16Z |
| Acts that wrote | 2 of ~100 all day | persona.act.observed wrote=true |

**What the numbers say.** A resident's pace is set by how much she must prefill per act, and how
often the engine loses what she had. Big contexts are the slowdown, not the cure. Admission must
count prefill work, not lanes.

## The act, stage by stage

Each stage: the ONE owner, its invariant, and what wins where docs or code disagree.

### 1. Her state (what she is doing)

- **Owner:** the activity's typed state and each command's precondition
  (`continuum-rooms/work_preconditions.rs`, branch `fable/state-driven-surface`), moving into
  `airc-work` (Cormac, card be8c4d78) so the board refuses what no precondition allows.
- **Invariant:** one predicate per command decides BOTH whether she sees the verb and whether it
  executes; execution revalidates against current state at the mutation boundary and returns her
  refreshed moves on a stale view; decisions read recorded event time, never the wall clock.
- **Rules:** STATE-DRIVEN-SURFACE (precondition revision) wins. Per-command `Disclosure` becomes data
  only for commands no activity covers; the room recipe declares the activity, not a tool list.
  `ToolsEpoch` switches at a state change (goal 10's "a switch is a cached prefix"), not only when
  the window cannot hold it.

### 2. Her view (what she sees, and how big it is)

- **Owner:** `continuum-context` (one codec), its ONE allocation run ONCE per act.
- **Invariant (goal 7, unchanged, now enforced):** the newest complete interaction is whole; her
  required working set (her held card's state item, the region of the file she is editing, the
  result she is acting on) is whole; everything older and reconstructible is a HANDLE at a level
  (`levels::summarize`) she can re-expand. A read returns the selection she asked for; the view, not
  the command, decides what stays verbatim.
- **What this replaces:** today a read without a range gets `chars / reads` of headroom
  (`act_observe/page_budget.rs:33`: division by a count) and the newest result can fall to a bare
  pointer (`prompt/newest_result.rs:106`); side budgets are computed outside the allocation
  (`prompt/view.rs:199-235`) and the allocator runs three times per act. All of that goes.
- **Size is a consequence, not a target.** Her prompt is head + working set + newest interaction +
  handles. Today's evidence says that is ~20k tokens for a coding act; the 76-89k prompts were
  history kept verbatim. No fixed number: the validator reports prompt tokens per act and the cold
  re-prefill seconds they imply at the measured rate.
- **Rules:** MIND-CONTEXT's "flex moves into continuum-context as the one allocator" wins over
  ONE-BUDGET 3b's "deleted for cambrian-flex" (the code exists; a replacement must replace it whole).
  HER-TOOLS §Answer ("the view is the only sizer") wins over page_budget.

### 3. Her demand (how much residency she asks for)

- **Owner:** `working_set` (need = her FLOOR: head + required + newest interaction; want = floor +
  working set).
- **Invariant:** demand is what her CURRENT view needs, never her history. A censored observation
  (`Observed::AtLeast`, #5773) is an honest measurement of what an overflowing view would have held,
  but it does NOT authorize residency: an overflow is answered by collapsing older items to handles
  (stage 2), not by a bigger grant.
- **Correction of 2026-11-04:** #5773 raised the WISH to a censored bound; that is reverted (the
  typed measurement stays). This is why one mind could take a whole pool.

### 4. Admission (who runs now)

- **Owner:** ONE admission decision per engine (SERVING-ENGINE §1c wins: "admission is memory,
  once"; one dispatcher). Today three gates run in series (`resource_admission/lanes.rs`,
  `turn_admission.rs`, `window_grant`); they collapse into one.
- **Three resources, decided together (Codex, Cormac's bench):** (a) sequence RESIDENCY in bytes
  (her KV), (b) PREFILL work (serial on this hardware: 68 tok/s at 1 or 2 streams), (c) DECODE work
  (scales with streams). A lane count is none of these; the semaphores sized from `--parallel` go.
- **Fairness (goal 9):** consumes COMPLETION identities (the turn ledger's ended-turn count), never a
  polled in_flight; a waiter owed service is admitted at the next completed turn of a served holder;
  a running turn is never cut. Bound stated and tested.
- **A grant is permission, not bytes (Codex):** rotating a seat asks the page custodian to surrender
  the holder's KV; her residency stays charged until the page-out (or erase, or incarnation fence)
  is verified; only then is the capacity published.
- **Priority:** activity temperature (goal 9). Until it is measured, Directed/Work/Ambient is its
  coarse proxy, named as such in code.
- **Rules:** INFERENCE-LANES-REALISTIC, INFERENCE-SCHEDULING-AND-SCARCITY's slot pools,
  KV-CACHE-ECONOMY's `-c = window x lanes`, MINDS-SEATS "a seat is a lane", CONTEXT-IS-A-CAPABILITY-AXIS's
  lane count: SUPERSEDED (marked so, removed from CLAUDE.md's "read first").

### 5. Engine memory (where her KV lives)

- **Owner:** the serving daemon over ONE separate llama-server process (SERVING-ENGINE §6 wins over
  CLAUDE.md 3b's "in-process"; CLAUDE.md is corrected).
- **Invariant: physical conservation.** On unified memory, weights + every mind's resident KV +
  staging are counted ONCE against the physical pool; a grant or plan never spends bytes that are
  not released or verifiably adopted.
- **Launch:** sized from the RESERVED FIT at a `Settled` reading, and Settled means the resource
  authority OBSERVED the previous engine's release or verified adoption, never elapsed time or a
  free-memory threshold (branch `fable/launch-from-fit-pool`, on #5770's credit).
- **No relaunch to resize.** Pool growth is deleted only after launch-from-fit is proven (#5774 is
  blocked on it). Live resize is the fork's `reconfigure` verb later (SERVING-ENGINE S2).
- **Paging:** the actuator for every move of residency (rotation, eviction, restart). Pages are
  saved and restored per mind; a timeout stays `Unresolved` and HOLDS custody (#5772); CPU activity
  proves liveness, not progress.
- **Cancellation is not termination:** after a stop, the engine kept prefilling ~1.5 min (Cormac);
  a stop must cancel in-flight prefill in the fork (`should_stop` checked inside prefill), and until
  it does, the custody model assumes the engine is still working.

### 5a. The engine is a cache; prefill is the miss penalty (Joel, 2026-11-04: "It's more like a cache")

**The model is hybrid (partly recurrent):** every resume logs `pos_min = slot tokens - 1`, so a
sequence's state resumes only at its end or at a saved CHECKPOINT; middle edits (ring buffers, KV
shift, `seq_rm` of a span) do not apply to this model. A mind's state is a SNAPSHOT.

| Level | Holds | Hit cost (measured) |
|---|---|---|
| L1 | a mind's live sequence state on the GPU | 0 |
| L2 | her state snapshot in RAM | sub-second (to measure) |
| L3 | her page on disk | 0.3-3.7 s |
| miss | re-prefill | minutes (318 s for 76k tokens) |

- **Lines are checkpoints**, placed at stable boundaries (end of the shared head; end of each
  appended act), so every prompt resumes from a recent line. Residents' prompts do share their
  prefix act to act (`delib.prompt.common_prefix` 37k-190k chars, 2026-11-04 ~22:00Z); the
  `raw lcp = 5` full re-prefills in the engine log are the SCRATCH slot (id 4), where unrelated
  non-turn requests overwrite each other: expected, small (~800 tokens), and a candidate for its own
  line per purpose rather than one shared scratch.
- **Replacement** by activity temperature (goal 9): hot minds in L1, cooling write back to L2, L3.
- **Write-back at turn boundaries**, while she is in a tool call, never in the GPU's way.
- **Prefetch:** the next mind's line warms while another decodes; a tool's result prefills as it
  streams in (plan line 13).
- **Coherence:** a line is tagged model + geometry and is valid wherever the tag matches; the
  refused restores and `page_on_disk_unconfirmed` turns are coherence bugs.
- **The metric is hit rate**, every miss classified (3C: compulsory, capacity, conflict). On
  2026-11-04 nearly every miss was a CONFLICT miss (prefix divergence, geometry change, ledger
  forgetting), i.e. design defects, not too little memory.

### 5b. The fast path is the design, not an optimization

The fast path existed and was measured on 2026-11-04: a page SAVE 1.5-3 s, a RESTORE 0.3-3.7 s,
against a 318 s cold re-prefill (~100x). It was defeated, not missing:
- restores refused `status=400` after relaunches, because the new engine came up at a different
  pool size and a page saved under the old window does not fit the new slot (pools that day: 33k
  to 654k for the same build);
- 52 turns went cold with `inference.kv_page.cold_turn cause="page_on_disk_unconfirmed"`: a page on
  disk (31 GB in `~/.continuum/cache/kv-pages`) the ledger did not trust after a relaunch.
**Invariants:** (1) an engine's geometry (pool, per-sequence window) is STABLE for its life and
across restarts of the core (stage 5: one launch, sized once from the settled fit; no relaunch to
resize); (2) a saved page is valid for any engine of the same model and geometry, verified by its
header, never re-prefilled because the ledger forgot it; (3) a geometry change is the only thing
that invalidates pages, and it is a measured, deliberate event; (4) prefix reuse and warm-ahead are
measured per act (`delib.generate.cache` hit rate, `inference.kv_warm_ahead`) and a miss is a loud,
classified defect (plan line 6), never a quiet five-minute prefill.

**Prefill is avoided first, then shared, then shrunk, then tuned** (in that order of size):
restore and prefix reuse (above); the identical head (tools + framing, ~10k tokens per mind) is
prefilled ONCE per engine and copied into each mind's sequence (`seq_cp`, SERVING-ENGINE), never per
mind per cold turn; prompts stay at the working set (stage 2), where this GPU prefills at hundreds of
tokens per second instead of ~150; prefill overlaps the tool call (plan line 13); only then engine
knobs (ubatch, chunking, KV type for attention speed), each measured on the dyno before and after.

### 5c. The operating point is measured, never a number

No fixed count of residents, lanes, slots or sequences. How many minds act at once is the output
of a controller over measured throughput: acts per hour per mind and in total, prefill seconds per
act, cache hit rate, wait between acts. Admission (stage 4) raises concurrency while total useful
acts per hour rises and every mind's wait stays bounded, and lowers it when added minds only add
prefill contention (prefill is serial: 68 tok/s at 1 and 2 streams). The population a node hosts
follows the same measurement (personas create minds; placement is by capacity). The engine's
`--parallel` is the CEILING the launch reserves residency for, chosen from the fit, not a lane
count admission obeys. Every downward adaptation schedules its upward test.

### 6. The tool call and its result

- **Owner:** the command's handler (execution), its precondition (stage 1), and ONE renderer
  (`tool_executor/answer.rs`).
- **Invariant:** every result says what happened and her next moves; a refusal carries a typed
  reason and the moves valid now (Cormac, card 9bba067f); an identical resubmission returns the
  existing artifact (`AlreadySubmitted`), a changed one submits; ids she reads and types are `Shown`
  (card 484b000b). The result enters her view through stage 2, never sized by the command.
- **Repeats:** ONE owner: the view's repeat fact (`continuum-context/view/repeat.rs`), fed by the
  settle evidence; the other two detectors feed it or are deleted (card e03c2aa0).

### 7. Around the act

- **Learning** is a low-duty dream stage between decode steps under the governor, with its slowdown
  measured and enforced (ONE-RESIDENT-MODEL). A batch genome job that holds the engine for hours is
  forbidden on a serving node (IntelMac 2026-11-04, job 2f1bbe9d: resident prefill 8-13.6 -> 3.2-5.2 tok/s, about 2.5x slower, for 2 h with 0.0% progress).
- **Persistence** on change, never per tick: both the volatile checkpoint (JSON, written every tick) and the per-resident airc store writes (`airc/events.sqlite` sqlx pools, which with rayon spin carry the IntelMac core's 134% CPU; card 9cf6b487).
- **Deploy custody:** a deferral is not a failure (card 90ce8f6a).

## Proof: the dyno, before anything goes live

A seconds-scale validator in CI and on any machine, built from the REAL policy code (launch sizing,
the admission decision, the grant policy, the page custodian's contract, the view codec) driven by
recorded inputs (the day's memory readings at each boot, per-act prompt items, the measured
prefill/decode curves above). Existing pieces it builds on: `capacity/window_sim.rs`,
`vdd::turn_replay`, the serving_daemon fixtures, the Epoch.

It must first REPRODUCE each 2026-11-04 failure (red), then show the design removes it (green):

1. cold boot with the old engine still wired -> no launch until Settled; pool = reserved fit;
2. four minds oversubscribed on a small pool -> every mind served within the stated bound;
3. a turn completes and restarts between ticks -> the waiter is still served;
4. cancellation during prefill -> custody held until the engine's completion or a fenced exit;
5. a delayed page receipt -> no slot reuse until verified;
6. a restart -> pages restored, no five-minute cold re-prefill for a warm mind;
7. an overflowing view -> older items collapse to handles; prompt tokens per act stay at the
   working set; no grant growth.

Asserted every run: physical bytes conserved (weights + resident KV + staging <= pool); no hold
without a releaser; eventual service for every mind; predicted acts/hour per resident from the
measured prefill rate. A live change ships only when the dyno is green for it, then one change per
deploy with the 15-minute read and immediate rollback if worse (rule 3).

## Order of execution

| # | Slice | Owner | Gate |
|---|---|---|---|
| 0 | Freeze: M5 stays on its current build; this doc agreed; superseded docs marked; CLAUDE.md corrected | Fable | three signatures |
| 1 | The dyno with the seven reproductions (red) | Fable + Codex | each 11-04 failure reproduced from recorded inputs |
| 1b | The fast path audit: every cold turn classified 3C (refused restores, unconfirmed pages, scratch-slot churn) (page refused, page unconfirmed, prefix lost); geometry stable across restarts; pages valid by header | Codex + Fable | dyno 6 green; live: cold turns after a restart = 0 for a warm mind |
| 2 | The view: reads by region, side budgets into the one allocation run once, history to handles; revert the wish raise of #5773 | Fable | dyno 7 green; prompt tokens per act at the working set |
| 3 | Engine memory: launch-from-fit at Settled (#5770 first), conservation accounting; then delete growth (#5774) | Codex + Fable | dyno 1, 6 green |
| 4 | Admission: one decision over residency, prefill, decode; fairness by completion identity; rotation through the page custodian | Codex | dyno 2, 3 green |
| 5 | Custody and cancellation: #5772, fork prefill cancellation | Codex | dyno 4, 5 green |
| 6 | Her state: command preconditions, results with next moves, Shown ids, one repeat owner | Fable + Cormac | precondition tests; live: no verb hunted (`commands/help` loops) |
| 7 | Learning as a dream stage; persistence on change; deploy custody | Cormac | slowdown measured within its bound |

## Open branches, fitted

| Branch / PR | Fits | Disposition |
|---|---|---|
| #5769 Probed | stage 5 (typed "never asked") | keep, merge after review |
| #5770 adopted-plan credit | stage 5 launch | merge first in slice 3 |
| `fable/launch-from-fit-pool` | stage 5 launch | slice 3, after #5770 |
| #5774 growth deletion | stage 5 | blocked until launch-from-fit is proven |
| #5772 busy is not dead | stage 5 custody | slice 5, after its tests pass in CI |
| `fable/grant-never-starves` | stage 4 | rework: rotation through the page custodian; one decision with stage 4 |
| `fable/state-driven-surface` | stage 1, 6 | slice 6 |
| #5775 Shown ids | stage 6 | merge (reviewed) |
| #5773 (merged) | stage 3 | keep the typed measurement; revert the wish raise in slice 2 |

## Superseded (marked in the files, removed from "read first")

INFERENCE-LANES-REALISTIC, INFERENCE-SCHEDULING-AND-SCARCITY (slot pools, adaptive quantizer),
KV-CACHE-ECONOMY:28 (`-c = window x lanes`), MINDS-SEATS-AND-ADMISSION:62 (seat = lane),
CONTEXT-IS-A-CAPABILITY-AXIS:44-66 (lane count), CLAUDE.md 3b's "in-process", ONE-BUDGET 3b's
"flex_allocate deleted", SERVING-ENGINE §1c's "done" for lane-free deliberation (not done),
planning/AI-LANE-OPEN-QUESTIONS and UNCERTAINTY-IS-THE-SCHEDULER-SIGNAL's "N lanes",
PHASE3B-WORKING-MEMORY-PLAN's RAG token budget.

## Agreed

- Fable: (pending)
- Codex: (pending)
- Cormac: stages 6 and 7 and the cancellation ruling agreed (2026-11-04), with his two receipt corrections applied; rest pending
