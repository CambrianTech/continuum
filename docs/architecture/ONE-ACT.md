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
- **Invariant:** demand is what her CURRENT view needs, never her history. An overflow is first
  REDUCIBLE: eligible older items collapse to handles (stage 2). Only if the required material and
  the newest complete interaction still exceed the grant is it IRREDUCIBLE working-set growth, and
  then she requests admission-backed growth, placement elsewhere, or waits, with a typed reason
  (Codex: "never a bigger grant" would be another hard-coded policy, as "automatically grow" was).
  A censored observation (`Observed::AtLeast`, #5773) is the honest measurement either way; it does
  not by itself authorize residency.
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

### 5a.1 The cache, measured on one node (M5, 2026-11-04 23:29-23:40Z)

Codex's harness (`measure.py`, warm_first, page_pair; synthetic source, exact token slices, a
different 128-token tail, 3 repeats each), Qwen3.6-35B-A3B Q4_K_M (sha 671e47e0ec53c665), engine
binary sha 0a3cb69808cbcabe, live flags on 2 slots (`-c 65536 --parallel 2 --kv-unified`, fa on,
KV q4_0, ubatch 2048, spec draft-mtp, ctx-checkpoints 32, cache-reuse 256; mmproj off). Raw:
gist joelteply/0225fe9bb4fd328d07f9b50d35628abd and `~/.continuum/evidence/codex-harness-20261104/`.

| Case | First token | Cached / computed |
|---|---|---|
| cold 4,096 | 4.56 s | 0 / 4,096 |
| warm 4,096 + 128 tail | 0.354 s | 4,092 / 132 |
| cold 12,288 | 15.25 s | 0 / 12,288 |
| warm 12,288 + 128 tail | 0.395 s | 12,284 / 132 |
| page save 12,288 (315,981,752 B) / restore / restore + first token | 0.16-0.21 s / 0.04 s / 0.44 s | 12,284 / 132 |
| cold-first mixed, WITHOUT `--no-cache-idle-slots` | warm 11.65 s | 0 / 4,224 (idle slot cleared) |
| cold-first mixed, WITH `--no-cache-idle-slots` (the live flag, #5764) | warm 7.11 s | 4,092 / 132 |
| warm-first mixed (flag absent) | warm 0.35 s, but decode 32.7 vs 78-95 tok/s | 4,092 / 132 |

Read: a hit is ~35-39x a miss on this node (L1 0.4 s, L3-warm 0.44 s, miss 15 s for 12k). The
page restore is same-process and filesystem-warm, NOT a cold-disk or restart proof (owed). The
idle-slot clear was a bench configuration gap; live engines already pass the flag (#5764 enforces
it on launch and in the reconcile). With the cache intact, a cold prefill still costs a warm mind
7.1 s of first-token delay when it arrives second, and ~60% of its decode rate when it arrives
first: the 5d bound is the next lever. 5090 (Codex, Qwen3.8-27B) shows the same shape.

### 5b. The fast path is the design, not an optimization

The fast path existed and was measured on 2026-11-04: a page SAVE 1.5-3 s, a RESTORE 0.3-3.7 s,
against a 318 s cold re-prefill (~100x). It was defeated, not missing:
- restores refused `status=400` after relaunches, because the new engine came up at a different
  pool size and a page saved under the old window does not fit the new slot (pools that day: 33k
  to 654k for the same build);
- 52 turns went cold with `inference.kv_page.cold_turn cause="page_on_disk_unconfirmed"`: a page on
  disk (31 GB in `~/.continuum/cache/kv-pages`) the ledger did not trust after a relaunch.
**Invariants:** (1) an engine's geometry is STABLE for its life and across core restarts (one launch,
sized once from the settled fit; no relaunch to resize); (2) a page is eligible for restore only on
its full fingerprint and complete-save provenance: the persisted ledger
(`slots/saved_ledger.rs`, LEDGER_FORMAT 2) already fingerprints model, adapter and draft revision, KV
type, flash attention and engine build and excludes window/slot count; confirmation is preserved,
and no file is trusted by its header alone; a larger destination is acceptable only if the fork's
restore validates layout and token capacity, never by a size comparison; (3) every miss is a loud,
classified defect (plan line 6), never a quiet five-minute prefill.
**Restart adoption (Cormac, IntelMac 22:05Z, cf2e8089f):** the boot sweep logged
`LeftLive { pid: 78294 }`, then `serving.genome.in_place_unsupported from=0 to=1`: adoption read a
running engine's adapter set as empty while it served `--lora` Delia's adapter, so the core
relaunched twice in a minute and dropped every mind's KV. Adoption must read the engine's actual
adapter set (the fork's typed report), or every core restart on an adapter-bearing node is a cold
restart.

**Diagnosis still owed (Codex), before any fix:** `page_on_disk_unconfirmed` conflates an interrupted
save with a ledger-load failure; it does not prove a relaunch forgot confirmation. For the same page
and incarnation on the deployed revision we need: the ledger-unread reason, the format/fingerprint,
the save receipt, and the restore 400's body.

### 5b.1 Accepted restore-refusal contract and first execution slice

Codex accepts this contract (2026-11-04), based on the audit supplied by Joel and Fable.
This is agreement on this slice, not blanket approval of the draft or an implementation receipt.

**Diagnosis:** the audited restore 400s report insufficient unified-cache cells (59,144
requested in a partly occupied 125,696-cell pool) or a page exceeding total capacity
(35,838 tokens versus 33,280 cells). They do not report model/format mismatch. The core
reduces the response to HTTP status and calls `note_page_lost`, removing persisted save
confirmation. Later admissions see `page_on_disk_unconfirmed` and prefill again. The reported
ledger-load counts fall from four minds at 19:38 to zero at 21:04. Aris's confirmed 19:45
save missing at 19:58 without a refusal remains a separate unexplained loss; this fix must
not claim to explain it.

**One typed outcome at the engine/core seam:**
- Restored: verified successful receipt, identifying the page and destination incarnation.
- DeferredCapacity: distinguish insufficient currently allocatable cells from a page that
  exceeds the whole compatible pool. Preserve the durable confirmed page in both cases.
  Retry transient pressure after verified reclamation; no geometry change is required.
  Oversize requires a feasible capacity/placement change, not repeated restore attempts.
- Incompatible: retain the saved artifact and its provenance; it is ineligible for this
  destination, not necessarily invalid for every compatible destination.
- InvalidPage: only positive evidence of a missing, corrupt or incomplete artifact invalidates
  that artifact's confirmation. Do not infer this from HTTP 400 alone.
- Unresolved: transport failure, unknown reason or timeout retains provenance and operation
  custody until a terminal receipt or fencing proves the outcome. Never fabricate free cells.

Preserve engine reason codes and available numeric evidence through the core; unknown fields
stay unknown. Capacity refusal must not route through `note_page_lost`. Do not delete the
artifact as a side effect of deferral or destination incompatibility.

**Fit and ownership:** persist page token count with its complete-save receipt and identity.
Admission compares that requirement with total compatible capacity and currently allocatable
cells, reserves through the existing authority, and makes room through verified custody
transfer before restoring. A free-cell snapshot alone races other admissions: the engine's
restore result remains authoritative. A save receipt alone is not evidence that resident KV
was erased; release must be verified separately when the operation does not release it.
Launch must reserve enough capacity for the admitted page working set within the physical fit;
it does not promise to fit every disk page simultaneously or exceed physical memory. An
unplaceable page gets an explicit deferred/placement outcome with its saved state preserved.

**Execution owners** (assignments sent over AIRC; peer acceptance tracked separately):
- Codex: engine typed restore outcome, core propagation, page custody and ledger token metadata;
  first execution slice after the failure reproductions exist.
- Fable: recorded audit fixtures, launch-fit and admission integration; shared design updates.
- Cormac: independent durability/restart review, including the unexplained Aris ledger loss.

**Required proof using the existing fixtures:** capacity refusal preserves confirmation across
restart; verified reclamation permits a later restore without cold prefill; oversize stays
confirmed and deferred; incompatibility preserves the artifact; proven corruption invalidates
only the affected artifact; concurrent admissions cannot double-spend free capacity; timeout
and late receipt retain custody; completed save and ledger replacement survive interruption.
Reproduce the reported loss before the fix, then verify the corrected transition through the
real engine adapter and persisted ledger. Report the unexplained loss independently until it
has its own reproduction. Follow with the live restore/cache-reuse receipt.

### 5d. A cold prefill adds only bounded latency to a warm mind's next token (Cormac's matched run, IntelMac 22:05Z)

Mixed load at 9 (4 cold 4,641-token prefills + 5 warm decodes), same binary/model/adapter as the
lane, CPU, `--ubatch-size 2048 --kv-unified`: the prefills complete strictly serialized at 102 /
173 / 243 / 319 s and ALL FIVE decoders finish at 335.6 s; once running they decode at 7.8 tok/s
each. At n=2 a decoder finishes at 88 s behind an 81 s prefill. Raw:
`~/.continuum/evidence/knee-20261104/` (IntelMac). Decode concurrency scales (19.9 -> 51.1 tok/s
aggregate, 1 -> 9); prefill has a capacity of one on this node.
**Invariant (Codex's wording):** under mixed load a warm mind's decode suffers only BOUNDED
additional latency from other minds' cold prefills (a non-preemptible kernel or chunk still takes
its time), AND every cold prefill eventually completes, so strict decode priority cannot starve a
cold mind. Mechanism in the fork's own scheduler, reusing its measured service curves: prefill
proceeds in bounded chunks sized from the measured chunk time when decoders wait; the number of
concurrent cold prefills is a MEASURED admission limit for this backend and configuration, never
universally one; cancellation takes effect at chunk boundaries; recurrent (hybrid) state stays
intact across chunks and cancellation. Validated by re-running the mixed trial: decoders'
first-token and inter-token latency within their bound, and every prefill completing.

### 5e. A continuous pipeline, shaped like bgfx (Joel: "just like a graphics pipeline. How I did bgfx")

The engine is a render thread that never waits for a mind to ask; the core is the encoders.
- **Encoders record, the device executes.** Every mind's work (her appended tail, a tool result as
  it streams in, a page-in, a write-back, a view rebuild) is recorded as commands the moment it
  happens, by whoever produces it, without waiting.
- **Double-buffered frames.** An engine step is the frame: commands recorded during step N execute
  in N+1. Submission never blocks on execution.
- **Sort keys.** Work in a frame is ordered by a key: latency class (a warm decode first), then
  activity temperature (goal 9), then bounded cold-prefill chunks (5d), then prefetch and
  write-back. The key's fields are measured, never fixed weights.
- **Handles and deferred destruction.** Sequences, pages and snapshots are handles; a release
  takes effect only after the engine's receipt for the frame that used it (the custody of 5 and
  5b.1 is the frame fence).
- **One device owner:** one dispatcher per engine (stage 4).
**Codex's conditions (agreed, 2026-11-04):** (1) submission is non-blocking only behind BOUNDED
queue admission with typed backpressure: no unlimited command or staging growth; (2) only
dependency-ready commands are sorted, per-sequence causal order is preserved, and aging/service
bounds keep decode from starving the prefill or write-back that reclamation needs; (3) a frame
fence covers actual device completion, plus the durable save receipt where a release depends on
it, never mere dispatch or HTTP acceptance; (4) streaming tool prefill uses only the COMMITTED,
append-only rendered and tokenized prefix, never mutable partial JSON or a view later compacted
under the same sequence. "N+1" is an execution epoch, not an artificial wait for a frame. A
shadow sequence reserves its destination and staging before it is allocated. Completion has distinct receipt DOMAINS under one scheduling authority (CPU, GPU copy, GPU
compute, durable I/O): a disk save completes in its own domain and is never forced synchronously
onto the device thread; a release that needs it waits on that domain's receipt. The paging outcome
slice (5b.1) remains first.
What it buys, each already named: prefill of a tool result while the tool runs (plan line 13);
page-in ahead of the turn (`inference.kv_warm_ahead`, which logged `skipped` on 2026-11-04 and
needs its reason classified); write-back of cooling minds during decode; the next collapsed view
built in a shadow sequence and swapped in at a turnover (a fresh sequence, valid for a hybrid
model); CPU cores rendering, tokenizing and embedding in parallel with the GPU.
**Gate:** GPU idle fraction while any mind waits on work the GPU could already be doing, measured
before and after; and per-act wall time split into tool, wait, prefill and decode.

### 5f. Extend the pipeline to the grid: place work near warm state

Joel requested this direction be recorded on 2026-11-04. This is the extension of stages
4, 5b.1, 5d and 5e, not a second scheduler or a claim of measured grid speedup.
**Sequence:** prove local custody, typed restore outcomes and bounded mixed-load scheduling
first; then carry the same command, reservation and completion contracts across nodes.

**Owner:** Codex owns the dispatcher and placement contract; Fable owns the core encoders.
The existing grid routes typed work to the selected engine. Each engine remains the one
owner of its device; placement requests admission rather than bypassing that owner.

**Placement compares complete paths, not idle GPUs.** For the same compatible work, compare:
1. wait for an engine already holding the correct warm sequence, then append/decode;
2. reserve a destination, transfer a compatible saved page, restore, then append/decode;
3. send the required prompt to another compatible engine and prefill it there.
Use measured queue delay, remaining prefill/decode work, page and staging bytes, network
bandwidth/latency, restore cost and source disruption. Account for overlap only when the
actual dependency graph permits it. A local warm wait may beat a remote cold engine.
Unmeasured costs stay explicit; an idle device is not evidence that migration wins.
Avoid placement oscillation by charging the whole transition and its uncertainty, never
by chasing an instantaneous throughput sample. Preserve bounded service for every mind.

**Compatibility is a prerequisite.** A page or shared prefix is eligible only under its
model/adapter revisions, tokenizer/template identity where relevant, state format and KV
representation, checkpoint/prefix identity, and the destination's supported restore contract.
Different hardware does not imply portable KV; no transfer or conversion is assumed valid
without a verified capability. Incompatibility preserves the source artifact. Recompute on
an admitted destination remains an explicit alternative, not a mislabeled successful restore.

**One handoff, verified custody.** Reserve destination residency, transfer staging and network
work before starting. Identify the operation, immutable page/checkpoint revision and both
engine incarnations. Transfer and verify the complete artifact; restore it and obtain the
matching terminal receipt. Commit execution ownership once, fencing stale or duplicate
commands, before releasing source residency. A send acknowledgement, timeout, disconnect or
expired lease is not proof of destination completion or source release. Keep custody through
uncertainty; reconciliation uses the operation identity and verified fencing. Release any
reservation only after its work is terminal or fenced. Preserve durable saved-page provenance
independently of whether either engine currently holds the sequence.

**Overlap without waste.** Encoders may prefetch an eligible page and prefill a committed,
append-only tool-result prefix while CPU/tool work continues. Those operations use the same
bounded queues, reservations, dependency ordering, cancellation and aging as ordinary work.
Speculative placement cannot crowd out useful admitted work. Shared immutable prefixes are
reused only where the backend proves compatible sharing; a shadow view reserves its own
state before the turnover fence. CPU, network, GPU-copy, GPU-compute and durable-I/O receipts
remain distinct domains under the one scheduling contract.

**Evidence and gate.** The within-node results in 5a.1 and the
[5090 measurements](https://gist.github.com/joelteply/b2898e781c2231ca80f253e6d877812f)
justify reducing avoidable prefill and protecting warm decode. They do not measure grid
transfer or cold-disk performance. Validate grid placement with the same model and work:
local warm wait versus remote page handoff versus remote cold prefill; record end-to-end
first output, p95 decode gaps, useful completions per mind, bytes transferred, peak reserved
memory and prediction error. Exercise duplicate delivery, cancellation, stale incarnation,
network loss and restart: no double execution, no capacity double-spend, no premature source
release and no loss of a valid page. A faster transfer alone is not a faster completed act.

**Chunk evidence (M5, 2026-11-05 00:05Z; PAIRED `--batch-size` = `--ubatch-size`, not isolation of
the physical chunk; Codex's warm_first trial: a primed 4,096 + 128-tail warm request, then a cold
12,288-token prefill on the other slot 500 ms later; live flags incl. `--no-cache-idle-slots`;
3 repeats each, within ms; cache 4,092/132 kept in all; raw
`~/.continuum/evidence/codex-harness-20261104/chunk/`):**

| paired batch | warm decode | warm WORST content gap | warm p95 gap | cold TTFT | cold wall | common wall |
|---|---|---|---|---|---|---|
| 512 | 41.8 tok/s | 0.75 s | 0.047 s | 20.1 s | 21.5 s | ~22.0 s |
| 1024 | 44.9 | 1.21 | 0.047 | 17.6 | 19.0 | ~19.5 |
| 2048 (live) | 32.7 | 2.23 | 0.047 | 16.9 | 18.4 | ~18.9 |

The worst gap tracks the chunk (each prefill chunk is a decoder pause) and is invisible at p95.
This supports measured-time chunking plus service-frequency control in the dispatcher, NOT a
blanket setting: three points on one node and one workload. The eventual 5d gate keeps BOTH
arrival orders (warm-first and cold-first) and the worst-gap metric.

### 5c. The operating point is measured, never a number

No fixed count of residents, lanes, slots or sequences. RESIDENCY (which minds hold state on this
engine) and RUNNABLE CONCURRENCY (which minds run now) are separate decisions. Both are outputs of a
controller over measured throughput, including the cost of every transition (page out/in, prefill): acts per hour per mind and in total, prefill seconds per
act, cache hit rate, wait between acts. Admission (stage 4) raises concurrency while total useful
acts per hour rises and every mind's wait stays bounded, and lowers it when added minds only add
prefill contention (on the IntelMac CPU, prefill measured 68.3 / 68.0 / 63.2 tok/s at 1 / 2 / 4 streams; that curve is this hardware and workload, not a universal law). Minds are never migrated to improve an instantaneous throughput sample. The population a node hosts
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

### 6b. The tool contract: a call is a transaction, never cut, never clipped

Measured on the M5, 2026-11-05 02:20-02:54Z (build 0b2854327, Benchy's prompt captures). One write
landed out of five attempts:
- Two `edit_file` calls were cut inside their arguments by the output limit (JSON ended at col 126
  and col 4,033).
- Two line-range edits (`end_line 603`) ended inside a construct and were refused.
- One whole-file write of 35,796 bytes landed, to add one test.

Earlier, on a95afe3d1, 302 of 414 acts were cut mid-thought because output room was priced from the
window's leftovers. None of this is a defect in one tool. Each failure is a pattern every tool owes,
and none of them is owned in one place. Claude Code's own tools keep these guarantees, which is why
their calls do not fail this way.

One `ToolContract` every tool implements. The compiler requires it, and the ratchets forbid each
way around it:

1. **A call is never cut.**
   - Each tool's schema declares its payload fields (a write's content, an edit's new text).
   - Stage 2's one allocation reserves her measured think plus the measured payload of the call she
     is making (per tool, from her history), so the budget fits the call.
   - A `Length` finish inside a call continues the same generation from its warm prefix; it is not
     discarded and re-thought. Not yet available in the fork: see the refinement below (research, Codex).
   - A call that still cannot fit is a typed `WindowTooSmall`, never a truncated call (stage 2, #5787).
2. **An edit anchors on content.**
   - Line-range edit modes are deleted. A text edit requires a unique anchor; a whole-file write is
     for a new file.
   - Each edit carries the content hash of her last read of that file. An anchor that is stale or
     appears more than once is refused, typed, with the current region.
   - The payload is only the change, never a re-sent file.
3. **A result is never clipped.** Every result is a `continuum-context` item with levels and a handle
   (stage 2's one codec, #5779 and #5782), so no tool cuts its own output.
4. **A refusal is a typed variant with her next move** (stage 6), never prose. Today the parse guard
   already words the right next move ("anchor on the text with search_replace") in plain text.
5. **Arguments are validated against the schema before execution**, typed per field, so a malformed
   or partial call never reaches a tool.

**The next tool gets all of it for free** (Joel, 2026-11-05: "So the next new one you write gets it for
free. Never allow breaks like this."). This is the substrate's for-free triplet (CBAR) applied to tools:
- a base trait;
- a `#[derive(Tool)]` on the tool's argument struct, which generates the schema, the payload
  declaration (`#[payload]` on content fields), validation, and the typed refusal enum with next moves;
- a scaffold generator.

A tool author writes the argument struct and the body, nothing else. Continuation, payload-sized room,
content anchoring (through a shared `Anchor` argument type), and results as codec items all come from
the contract. The ratchet `tools_outside_the_contract` = 0 means a tool defined any other way does not
build.

Refined with Cormac (2026-11-05):
- **One refusal shape.** It is already proven twice, in #5782's `Unmet` and in airc's `ShownIdError`.
  The derive generates `<Tool>Refusal` with `next_moves() -> Vec<Move>`, where `Move::command()` is
  pinned to `ActionCommand::NAME`. When there is no move, the refusal carries a typed reason instead
  (`NoMove::{Held(by), Ended(state)}`). The ratchet fails a refusal that carries neither.
- **The read hash advances with her own edits.** Every landed edit returns the file's new hash, and
  her working set records it. So her second edit in one act is checked against her first edit's
  result, never refused as stale against her own write. A peer's concurrent edit is stale, and is
  refused with the current region. One test pins both.
- **Continuation is the guarantee; payload sizing is the optimization.** A tool's first call is
  `Unmeasured` and can exceed any reserve, so continuation lands first.
  - In the fork, a raw `/completion` takes the rendered prompt plus the partial output, so a warm
    prefix costs about zero prefill.
  - The tool-call parser and grammar state resume mid-JSON instead of restarting.
- **Continuation is research, not yet a guarantee** (Codex, fork source at 0d7dda6a). Today:
  - `continue_final_message` exists (server-common.cpp:1356), but an assistant message containing
    `tool_calls` is rejected (:1370).
  - `init_sampler` resets the sampler and accepts prompt tokens with `grammar=false`
    (server-context.cpp:394).

  So a raw `/completion` with the prompt plus the partial output restores the warm KV, but not the
  grammar, parser or RNG state. The seam the fork has to provide:
  - the retained sequence, plus the exact generated token suffix, plus the parser, grammar and
    sampler state;
  - continuation bounded within the leased capacity;
  - one logical call identity;
  - execution only after the parse is complete and the schema validates, with receipts and effects
    deduplicated.

  Finite context can still end in a typed `WindowTooSmall`, so there is no absolute never-cut
  guarantee. First tests cut inside an escaped string, inside UTF-8, in nested JSON, at a tool
  delimiter, and at a boundary between multiple calls; each must prove exactly one complete effect,
  and that the prefix is reused. Codex owns the fork investigation.
- **Order:** (5) schema validation and (4) typed refusals need no fork work and land first, then (2)
  anchors, then (1) continuation once the fork is verified.
- **Second gate number:** bytes re-sent per landed edit (the 35,796-byte write landed and still broke
  "payload = the change only").

Ratchets: `line_number_edit_modes` = 0; `tools_without_declared_payload` = 0;
`untyped_tool_errors` falls; `truncation_outside_the_view` = 0.

Gate, live: the share of a mind's tool calls that land, from her prompt captures (cut calls, refused
edits and landed calls per mind), together with bytes written per landed change against the change's
own size.

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
measured prefill rate. The dyno proves state and accounting (conservation, custody, service, ordering) and compares
PREDICTED performance; live hardware validates actual performance. A live change ships only when the dyno is green for it, then one change per
deploy with the 15-minute read and immediate rollback if worse (rule 3).

## Order of execution

| # | Slice | Owner | Gate |
|---|---|---|---|
| 0 | Freeze: M5 stays on its current build; this doc agreed; superseded docs marked; CLAUDE.md corrected | Fable | three signatures |
| 1 | The dyno with the seven reproductions (red) | Fable + Codex | each 11-04 failure reproduced from recorded inputs |
| 1b | The fast path audit: every cold turn classified 3C (refused restores, unconfirmed pages, scratch-slot churn) (page refused, page unconfirmed, prefix lost); geometry stable across restarts; pages eligible by full compatibility and complete-save provenance | Codex + Fable | dyno 6 green; live: cold turns after a restart = 0 for a warm mind |
| 2 | The view: reads by region, side budgets into the one allocation run once, history to handles; revert the wish raise of #5773 | Fable | dyno 7 green; prompt tokens per act at the working set |
| 3 | Engine memory (after the 1b diagnosis): launch-from-fit at Settled (#5770 first), conservation accounting; then delete growth (#5774) | Codex + Fable | dyno 1, 6 green |
| 4 | Custody and cancellation first: #5772, fork prefill cancellation; the page custodian's surrender/verify contract | Codex | dyno 4, 5 green |
| 4b | Bounded decode latency under mixed load: chunked prefill from measured chunk time, measured cold-prefill admission limit, cancellation at chunk boundaries, recurrent state intact (after the paging outcome slice) | Codex (fork) | re-run Cormac's mixed trial: decoder first/inter-token latency bounded AND every prefill completes |
| 4c | The pipeline (5e): encoders record, the dispatcher executes double-buffered frames by sort key; tool results prefill as they stream; page-in ahead; write-back behind | Fable (core encoders) + Codex (dispatcher) | GPU idle fraction while minds wait, before/after; per-act split tool/wait/prefill/decode |
| 5 | Admission: one decision over residency, prefill, decode; fairness by completion identity; rotation THROUGH the proven custodian | Codex | dyno 2, 3 green |
| 6 | Her state: command preconditions, results with next moves, Shown ids, one repeat owner | Fable + Cormac | precondition tests; live: no verb hunted (`commands/help` loops) |
| 6b | The tool contract: payload-sized output room + continuation of a cut call; content-anchored edits with read hashes (line modes deleted); results only through the codec; typed refusals; schema validation before execution | Fable (contract, cognition) + Codex (fork continuation) + Cormac (typed refusals) | live: landed calls per attempted call per mind; ratchets at 0 |
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
- Codex: direction agreed (2026-11-04); signature pending these corrections, now folded in
- Cormac: stages 6 and 7 and the cancellation ruling agreed (2026-11-04), with his two receipt corrections applied; rest pending

## Takeover evidence receipt — 2026-11-05 15:15Z

The shared implementation status is now: slice 2 (#5779) merged as `631502fcf6`,
lease/output admission (#5787) as `d04fe4594d`, and typed tool calls (#5790) as
`9a0f610d5b`. All checks passed on #5790's integrated head `9ef326dda9` before merge.
Binary publication is still running; this is CI evidence, not installed or live proof.
IntelMac directly answered `ping` with `147694174`; M5 with `2069cb248`.

The admission validator itself has not been recovered or completed. Before constructing it,
Codex recovered the full [IntelMac evidence](https://gist.github.com/joelteply/90f98f20980705433573ad32d68aad1c)
and the M5 SSE arrival records, and executed an independent arithmetic check:

| IntelMac active cold requests | Aggregate prompt tokens | Wall seconds | Aggregate tokens/s |
|---|---:|---:|---:|
| 1 | 4,641 | 67.946 | 68.304 |
| 2 | 9,282 | 136.450 | 68.025 |
| 4 | 18,564 | 293.819 | 63.182 |
| 9 | 41,769 | 681.234 | 61.314 |

Every rate reproduced `sum(prompt_n) / wall_s`; every trial had the expected request
count and no recorded request error. One is the maximum observed throughput concurrency;
these single trials do not establish a statistical knee. Keep this CPU curve node-specific.

For the M5 warm-first pair, three trials per chunk size reproduced these measurements:

| Prefill chunk tokens | Largest observed SSE content gap, seconds | Median warm decode tokens/s | Median cold completion, seconds |
|---|---:|---:|---:|
| 512 | 0.756 | 41.828 | 21.713 |
| 1,024 | 1.212 | 44.859 | 19.036 |
| 2,048 | 2.236 | 32.710 | 18.362 |

Raw M5 source: `~/.continuum/evidence/codex-harness-20261104/chunk/b{512,1024,2048}`.
These are SSE content gaps, not measured engine scheduling boundaries or proven upper
bounds. A content chunk can contain several tokens. They do not validate cold-first or
four/nine cold-arrival traces. The simulator must keep those missing cases explicit,
separate the cold-concurrency policy invariant from latency/fairness/completion gates,
and validate against the recorded hour before predicting other workloads.

Reproduction and copied evidence on the takeover host:
`C:/Users/joelt/.continuum/evidence/2026-11-05-recovery/check-admission-evidence.py`.
Its outputs are `admission-curve-validation.json` and `m5-chunk-validation.json`.
No live setting was changed to obtain these results.

## Live rollout and rollback receipt — 2026-11-05 16:00Z

M5 installed `d04fe4594` through the supported graceful reboot at 15:28Z,
with the existing engine process preserved. The 15:28–15:43 observation had
7 completed acts (Aris 5, Benchy 2, Aiko 0, Atlas 0). The available baseline
had 12 (Aris 4, Benchy 2, Aiko 3, Atlas 3), covering 15:10:14–15:22:38;
log rotation shortened the requested baseline. This failed the live acceptance
gate despite green CI. These unequal, uncontrolled windows do not isolate which
change caused the regression, and cold-cache transition costs remain a confounder.

The supported rollback restored core and CLI `2069cb248`, verified by ping and
version, with the same engine PID 25678. All 66 modules reported durable shutdown.
By 16:00Z, the still-open rollback window recorded 16 acts across all four minds
(Aris 4, Benchy 9, Atlas 2, Aiko 1). This is evidence of resumed progress, not a
speedup estimate or proof of productive writes. Keep completion, useful work,
cache reuse and custody as separate outcome measurements. A bounded one-hour
M5 deployment hold protects observation until 16:50:51Z; it must be reviewed.

IntelMac independently auto-adopted `d04fe4594` around 15:31Z. Its 15:32–15:47
window recorded no completed acts. At 15:58Z multiple advancing slots reached
1500-second deadlines and received extensions. That is saturation evidence,
not a dead-engine diagnosis or a reason to discard pages. Intel's baseline and
rollback decision require separate evaluation; M5 recovery does not prove it.

Private raw logs, comparison script and rollback receipts remain in the takeover
host's `2026-11-05-recovery` evidence directory and M5's
`~/.continuum/evidence/2026-11-05-takeover/d04fe4594dbf` directory. Full resident
logs are not published with this document. The dispatcher/validator is still
unfinished; these live outcomes do not validate the proposed graphics-style scheduler.

### Follow-up live receipt — 2026-11-05 16:41Z

The complete M5 rollback window (15:48–16:03) contains 21 completed acts:
Aris 5, Benchy 10, Atlas 4, Aiko 2. Current and rotated logs were combined and
deduplicated; coverage is 15:48:00–16:02:59. The earlier 16-act figure was interim.
The M5 remains on `2069cb248`; its bounded deployment hold now expires 17:50:51Z.

Intel's attempted rollback to `147694174` did not pass acceptance. Its runtime
replaced the engine and changed geometry from nine slots to two, then five.
An `inference.slot_affinity.all_pinned` retry storm then rotated the entire log
history rapidly. Missing historical completions therefore cannot be counted as
zero. The notification source has not been isolated; no speculative sleep or
logging-suppression patch has been applied.

The supported reversal to the saved `d04fe4594` completed after a 302-second drain
and 22-second startup, reporting all 66 modules durable. By 16:40Z, core ping
verified d04 and the sampled last 10,000 log lines contained no all-pinned events.
The runtime had relaunched a nine-slot engine (PID 25045, pool 294912). This is
containment of the retry storm, not evidence that productive-act latency is fixed.
Earlier claims that Intel's engine remained unchanged are explicitly withdrawn.
The subsequent live observation remains open; no further rollout is accepted.

### Engineering recovery — 2026-11-05 17:43Z

Intel's full reversal window (16:36:50–16:51:50) contained only one
`reached_model=true` outcome (Eitan, 535 seconds). Nine other pace records
were aborted attempts with `reached_model=false`, not completed model work.
The live gate failed despite containment of the retry storm. M5's 21 rollback
outcomes were checked and all reached the model; neither count establishes
useful writes or a controlled causal speed improvement.

PR #5792, head `2f4c33382a`, fixes a demonstrated notification defect: releasing
a reserved scratch slot broadcast to citizen waiters although no citizen slot
became available. The regression failed with one unexpected wake before the
fix; nine contention tests and 38 admission tests pass afterward. Both permit
acquisition paths now use one constructor; scratch releases retain their own
semaphore wake without broadcasting citizen availability. Engine CI passed;
full CI and live acceptance remain pending. This does not establish the source
of the entire Intel retry storm.

Fable's original dyno PR #5777 was recovered and integrated with canary `9a0`
at `8c25a1e160`, preserving the newer stamped-admission and custody tests.
Four recorded dyno checks pass (three future gates remain explicitly ignored),
and admission integration passes 37 tests with six ignored. Running the ignored
whole-pool fairness gate explicitly still fails: Aris receives no grant by the
holder's first completed turn, with the last grant remaining `{1: 114592}`.
This is a reproduced production grant-policy failure using recorded inputs,
not proof of a new scheduler. CI is pending for the recovered branch.

The measured 5c/5d admission validator remains unfinished. The older contention
replay assumes an equal prefill/decode split and a constant prefill rate; it
cannot certify nonpreemptible chunk boundaries or measured warm-decode gaps.

### Handoff implementation checkpoint — 2026-11-05 18:24Z

The recovered dyno passed all CI and merged as `24e31fb919`. The scratch-slot
wake fix passed all CI and merged as `f5bcda1dd5`; its platform publication
`42078670563` is running. Neither milestone establishes live speed gains.

Fairness work is isolated on `codex/grant-receipt-fairness`. Commit `a078faf257`
records terminal receipt revisions under the same mutex as fixed reservations.
Seventeen grant tests pass: progress survives completion and restart between
publisher ticks; unsent retries and cancellation do not count as service; a
fenced engine's late receipt cannot resurrect progress. This is implemented
custody bookkeeping, not yet a serving-policy change.

The next phase fences a completed holder's reacquisition while continuing to
charge its resident capacity. The existing re-home path keeps a binding when
its grant disappears, and the shared engine deliberately retains idle KV.
Therefore deleting a grant is not a reclamation receipt. The pending handoff
must not release its fixed claim until verified save-and-erase or an engine
fence. The pinned engine fork exposes `/slots/{id}?action=erase`; production
integration must validate its response, protect page IO and endpoint identity,
and retain custody across cancellation before enabling the new policy.

### Reclamation and publication checkpoint — 2026-11-05 19:07Z

The local fairness branch now implements endpoint-retained save then erase,
with endpoint identity, slot permit, page IO and per-persona custody held across
the operation. A confirmed save remains on disk; erase proof is recorded with
the reservation owner before notifying the caller. Uncertainty keeps the claim
charged. Existing fork pages refuse resident-only reclamation. Late unresolved
completion recovery remains unfinished, so this mechanism is not enabled yet.

The new publication transaction consumes physical proof and publishes the next
queued grant while holding the dispatch reservation mutex. A completed holder
joins behind existing waiters. A regression exposed a stale service-loop
`in_flight` flag re-admitting the old holder even after erase; the transaction
now derives in-flight protection from actual reservations. The regression
failed before that correction and passes after it. Twenty-two grant tests pass,
including cancellation protection, publication lock ownership, and three-mind
rotation; the older ignored policy gate still documents the unwired path.
The physical HTTP tests separately cover save-before-erase and save refusal.
These are local component tests, not an end-to-end serving or live fairness proof.

Serving integration still needs the pending wait path, known-refusal recovery
and late completion recovery before enabling handoff. The scratch wake fix
`f5bcda1dd5bf` has now published successfully on all three platforms in run
`42078670563`; it has not been deployed by this takeover yet.
