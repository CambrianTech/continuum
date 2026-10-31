# The mind's data flow: the overhaul (2026-10-31)

Joel, 2026-10-31: "Need a damn overhaul. I think you guys don't understand patterns of data flow."
"Success is completed allocated and active gpu." "Is anyone actually working on genome and
learning? It's our main feature. We need a far better mind."

The design is already written: EVENT-MIND.md, CONCURRENT-MIND-AND-GOVERNOR.md §3.1-3.3
("enrichment, not pipelines"; "invalidation propagation, not servicing"; "the channel as a
reference-passed frame"), FLOW-IMPLEMENTATION-PLAN.md, SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md. The code
did not follow it. This plan is conformance, measured, not a new design. It joins three read-only
audits of canary 11f853b8d (receipts in each): LOCKS-AND-FIFOS-AUDIT.md (who waits on whom), the
rebuild-vs-delta audit below (who recomputes what an event already says), and the learning-loop map
(where her lived work stops on its way to a gene).

## The pattern, in one paragraph

Every mind has one ordered stream of events: perception, her acts, results, reviews. Everything
else is a projection kept current by those events (her view, the engine's KV for her sequence, her
held work, her memories, her credit), updated by deltas, read without I/O, passed by handle. Stages
connect by streams with backpressure, never locks or permits. All minds' generations fan in to one
continuously batched engine. Learning is a fold over the same stream: reviews and grades are events
that refine what she has already lived, never a gate before it.

## What the M5 measured (526bbe418, 2026-10-31)

- An act took ~115 s; the engine worked 15-30 s of it. The rest was queueing (lane wait 30-88 s,
  pinned-slot wait 4-55 s with the lane held). The GPU read 89-94% busy, 57% allocated (31.8 GB of
  a 55.7 GB Metal working set).
- All 13 cold prefills in 2 h followed a tool-list change. Fixed: #5670.
- Decode batches (aggregate ~37 -> ~45 tok/s from 2 to 3 streams); prefill does not past 2.
- Learning, 12 h: 52 credits staged, 0 settled, 0 training submits, 0 jobs, 0 dreams, 0 genes in
  any turn.

## The overhaul, by stage

Each row: what it does today; what it becomes; the event that drives it; owner (proposed); the
validator that proves it in seconds, then the live read.

### 1. The engine: no line in front of it (LOCKS rows 1-5, 7, 9)
- Today: lane permits (lanes-1 for non-directed), a ticket ledger, an adapter semaphore, a per-mind
  gate, pinned-slot waits with the lane held; the plan recomputes 3x/s and flaps.
- Becomes: minds submit at once; slots = the minds the measured memory holds (stated budget minus
  measured CPU need, including a resident build's peak); KV pages only under pressure (~1 s);
  priority as a weight inside each step's prefill budget (8d0477fc).
- Owner: Codex (07484926), Fable (8d0477fc). Validator: aggregate tok/s vs streams on the dyno;
  live: slot/lane waits ~0, allocated share up, every slot decoding.
- Merged today: #5668 (no permit across a turn), #5670 (stable tool set), #5666 (no voluntary
  turnovers).

### 2. Her held work: one projection, not 4-6 board reads per turn (rebuild #1, #6, #7, #9)
- Today: `active_claims` walks every subscribed room's board, 4-6 times per turn, before the lane.
- Becomes: one `HeldWork {card, room, state}` per persona, folded from airc card events, read with no
  I/O; board and active-work grounding behind the existing `CachedRagSource` with a card-event
  invalidator (the pattern workspace-map, doctrine and wall already use).
- Owner: Fable. Validator: per-turn airc reads -> 0 (probe count); turn timing before/after.
- Measured first (M5, 526bbe418): from her tool result (`persona.act.observed`) to her next lane
  request (`delib.gate.lane_wait`) took ~0.2 s, board reads included. The reads are local, so this row
  is I/O and CPU hygiene, not latency: it follows rows 1 and 6.

### 3. Her context: sources push, the view appends (rebuild #3, #4, #11)
- Today: the view is append-only (correct), but each act every source re-offers every item to be
  diffed, the view is deep-copied ~3 times and re-tokenized ~7 times; the conversation source re-pages
  100 events per turn; captures write the whole prompt per act.
- Becomes: sources push items into `View::append`; one shared `Arc<View>`; token count a running sum;
  the conversation a per-room `Vec<Arc<ChannelElement>>` appended from the bookmark; captures record
  the epoch start once, then each act's appended blocks.
- Owner: Fable. Validator: the replay (`continuum-context --example replay`) CPU per act and bytes
  copied, before/after; live: around-model time per act.

### 4. Her memory: an index, not a sweep (rebuild #2, #14)
- Today: recall makes two full passes over every engram per turn, cloning each and writing decay to
  all; the logging-only pass strengthens 8 memories that never reach the prompt; hippocampus recall is
  a cosine over the whole corpus, with a 30 s TTL beside invalidate-on-append.
- Becomes: a salience-ordered index with decay computed lazily at read; recall hits only on what she
  is shown; a vector index updated on append.
- Owner: proposed Cormac (memory beside genome). Validator: recall CPU at 1k/10k/100k engrams; the
  recall-hit set equals the surfaced set.

### 5. Wakes from events, not ticks (rebuild #5; LOCKS rows 25-28)
- Today: a 15-240 s heartbeat re-composes and hashes her window to detect change; checkpoints save
  every resident every 15 s; a 60 s catch-up per mind; the plan on a 1 s tick.
- Becomes: she wakes on the region's change signal carrying its cause; checkpoint = an append-only log
  of epoch events plus a dirty flag; one gap feed per core; the plan recomputes on a lease, working-set
  or catalog change.
- Owner: Fable (wake, checkpoint), Codex (plan, gap feed). Validator: idle core CPU near zero; wakes
  per hour equal changes per hour.

### 6. Learning: a fold over her stream (learning map; rebuild #12)
- Today: only an airc `WorkSubmissionReviewed` under a review-card claim settles credit (agents'
  GitHub reviews and merges settle nothing); a reviewed correction settled as a plain pass into a
  bucket of 16 (fixed in #5672: a failed review marks rows rejected, the later pass is `Corrected`,
  one example trains); refusals did not say why (fixed in #5672); the trainer cannot train the served
  MoE base (fork #82, CPU half done); dreams wait for 90 s idle that busy residents never reach, and
  write engrams only; buckets and recovery run on ticks.
- Becomes: every review source (airc review, a GitHub review on a card's PR, a merge, a grade) is an
  event that refines her credit; a correction trains at once; dreams a low-duty stage on the serving
  engine between decode steps, consolidating and producing examples; genes keyed to the served base by
  a typed model identity.
- Owner: Fable (the loop, the acceptance test), Cormac (trainer 14b64aa9, dreams' duty cycle).
  Validator: THE acceptance test, a reviewed correction applied on unseen tasks, retained after
  restart, beating the same base without the gene; live: settled credits, jobs and genes in turns, per
  hour.

### 7. Clients: deltas out (rebuild #13)
- Today: every chat or vitals event rebuilds a 100-message view and the roster, compares and publishes
  a full snapshot.
- Becomes: one appended message per envelope; a full snapshot only for a new subscriber.
- Owner: proposed BigMama (live, media). Validator: bytes published per message.

## The gates (Joel's success measure)

On the hourly health line, beside acts/hour, and the "done when" of every row above:
- **allocated share**: GPU in-use memory over the Metal working set (NVIDIA: used over total), toward
  ~100% of what the CPU side's measured need leaves;
- **useful-active share**: GPU busy (Metal from #5671) times the share of prefill and decode tokens
  that are new, toward 90%;
- **learning**: settled credits, training jobs and genes in turns per hour, then the acceptance test.

## Order

Rows 1 and 6 first (the engine's line, measured at 75-90 s of an act, and the learning loop, measured
at zero output); then 3 and 5 (CPU and copies per act); 2, 4 and 7 as their owners free up. Every
row is measured before it is built: a row whose measured cost is small waits. One change per crate at a time, each
with its validator first and its live read after, merged as it lands.
