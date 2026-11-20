# In-process learning

Status: design, revised 2026-11-20; slice 1 built (#6203), later slices proposed. Owners: Fable
(the engine, the `llama` crate, the engine side of her genome); Codex (the memory authority, the
serving half of the deletions, review). Builds on
[ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md) (§2.1, §9,
§9.6 P1-P6, §11), [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (Parts 1-3, 8, 10, 12),
[SHARED-RESOURCES.md](SHARED-RESOURCES.md) and
[REPO-GENOME-AND-COURSES.md](REPO-GENOME-AND-COURSES.md) (§17).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

1. [The rule](#1-the-rule)
2. [Her genome holds her learning](#2-her-genome-holds-her-learning)
3. [Learning is a duty cycle](#3-learning-is-a-duty-cycle)
4. [Memory: a compute lease per piece, residency priced apart](#4-memory-a-compute-lease-per-piece-residency-priced-apart)
5. [A dedicated lesson on the Core](#5-a-dedicated-lesson-on-the-core)
6. [Learning across the grid](#6-learning-across-the-grid)
7. [What is deleted](#7-what-is-deleted)
8. [Slices, each with its validator](#8-slices-each-with-its-validator)

## 1. The rule

Learning is as natural to her as speaking, and costs about as little. Training is inference with
extra layers switched on (ONE-RESIDENT §9.6): the same batch, the same graph, the same resident
weights and her own served window. Learning never gets a train call, a reserved slot, a lane, a
context, a time-share grant, a workspace held on the side, a second engine or a second copy of the
base. The extra cost is the backward pass over the labelled rows, plus her shadow and its
optimizer state while she is learning.

Nothing about the machine is assumed to stay fixed. Memory, Cores and nodes change during a
session: a user starts a game or quits one, a node joins or drops off the grid. The governor
measures capacity live and reacts to each change as an event, and the policies it uses (duty,
placement, demotion) are refined from our own receipts rather than set once (§3, §4, §6).

The fork already supports this. `llama_batch` carries `label` and `label_weight` per position. A
labelled position makes `llama_decode` also run the loss, the backward and the optimizer write of
the context's trainable adapter (`llama_opt_init`), in the same graph as the batch's other rows
(`include/llama.h`, `struct llama_batch`). Slice 1 exposes it in Rust (§8).

## 2. Her genome holds her learning

Her genome is an object, and a part of her: `PersonaUnit` is her five parts, moved only as a whole
(blocks, tokens, **genome**, **learning**, decode). A trained layer is one state of her genome:

- her **stable** genes, which serve her;
- her **shadow**, a small trainable layer above her frozen stable stack, never a copy of it (§2.1,
  §9.6 P3);
- the shadow's optimizer state (moments, accumulator, RNG, step count) and her arrival and replay
  cursors: the session state of ONE-RESIDENT §9.2;
- immutable **snapshots** (candidate genes) and **checkpoints** (session state), each with its
  provenance and lineage (GENOME-FOUNDRY-SENTINEL Parts 1 and 12).

Her shadow and its session state together are her **learning context**, the unit that pages,
checkpoints and travels. One format serves all three uses (proposed):

| Use | What is written | When |
|---|---|---|
| Page-out | The learning context, to RAM or disk | The tier owner demotes it (§4) |
| Checkpoint | The same bytes, with the journal cursor they consumed through | At an optimizer boundary, and at a deploy seam (§9.2) |
| Publication | A snapshot of the shadow alone, with its lineage and exam results | When a gene earns adoption and is shared (§6) |

So paging, crash recovery and federation are one code path. Adoption is her genome promoting a
snapshot to stable after its exam, a pointer swap between two steps. She never serves moving
weights.

## 3. Learning is a duty cycle

Her genome and her ability to learn are always present. Learning is not a session that starts and
ends: it is a share of her decoding that switches on and off as she works, like pulse-width
modulation. She keeps talking throughout.

**One control per Core, admitted in bounded pieces (proposed).** The duty is the Core governor's
permission to compute labelled rows. It never decides residency (§4). A share of frames cannot
bound a speaker's delay, because one labelled frame can carry many rows and a long backward pass
(Codex's review, 2026-11-20). So the governor admits a bounded set of labelled rows at a time: as
many as fit, at their measured non-preemptible time, inside the remaining latency budget of whoever
may be waiting to speak and the deadlines already admitted (SHARED-RESOURCES "Time"). Its inputs,
all measured live and never fixed at launch:

- the lessons queued across the minds it serves (her lesson queue);
- the Core's slack: time left after deadline work (avatar frames, audio) and waiting turns
  (SHARED-RESOURCES "Time: every consumer is given its time");
- the latency budget of whoever is waiting to speak;
- the memory the node can give right now, which changes when a user's own application (a game on
  the 5090, an editor) takes or frees memory.

A long queue on a quiet Core admits more labelled pieces. A speaker arriving bounds the next piece
to her remaining budget. The Academy's dedicated teaching is the same control held high, so
learning keeps pace with what is queued and never needs a scheduler of its own. The ordinary swap
between minds (mean ~21 ms on the M5, 2026-11-20) does not measure the cost of a labelled piece;
slice 2 does.

**Two ways in, one mechanism:**

- **Continuous: prediction error on what she perceives.** When she perceives something (a peer's
  message, a tool's output, a file she reads), her prefill of it can carry labels: each token's
  target is the next token, weighted by her own relevance to what she perceived (`label_weight`,
  "a mind's relevance on what it perceived"; REPO-GENOME-AND-COURSES §17). The forward is the one
  her prefill runs anyway, and only the backward over the labelled rows is extra. To measure
  before relying on it: whether a labelled perception row's K/V under the shadow must be
  recomputed under her stable set (§2.1), and what that costs against the backward.
- **Dedicated: the Academy.** A lesson from a reviewed correction (ONE-RESIDENT §11), a course, or
  the repo's own history taken as a course (REPO-GENOME §17.1: PR reviews, airc verdicts, Claude
  and Codex sessions). The lesson is her context, then the corrected reply, labelled (§5). The
  doctor writes the lesson's family of graded tasks, and the exam on unseen tasks decides adoption.

## 4. Memory: a compute lease per piece, residency priced apart

Nothing is held that is not in use (Joel, 2026-11-11; SHARED-RESOURCES "The principle: resources
flow, like data"). Her learning context is a pageable resource of the memory-tier adapter:

- **Compute:** a lease for one admitted labelled piece, ended at the frame fence. No compute lease
  is held between pieces. A completion is accepted only for the genome generation that admitted
  it.
- **Residency:** priced like any page, separately from the duty. If her learning context is not in
  the Fast tier when a piece is admitted, it is promoted through the one memory authority; the
  pending request blocks nothing else, the authority asks lower-value owners to demote, and the
  grant becomes runnable when the release is confirmed (SUBSTRATE.md, "Worked example: a live
  conversation under memory pressure", in #6170). When her pieces end, it stays as reclaimable hot
  state. The tier owner demotes it under pressure or when the measured reuse cost says so, writing
  it out (`llama_opt_shadow_state_write`) only as it leaves, and restores it
  (`llama_opt_state_restore`) when it returns. Writing the whole dirty context out at every off
  edge would spill again and again with Fast memory free and a lesson already queued.
- **During a step:** the labelled rows' activations and gradients exist only inside that
  `llama_decode`, as transient memory of the serving graph.
- **When the machine changes under her:** a user's application taking memory is pressure like
  any other. The duty drops, and her learning context is demoted at the next frame boundary,
  before serving gives anything up. When the application quits and frees memory, the duty rises
  again. Neither needs a restart, and neither loses a committed step.

A small need is met quickly because lower-value bytes are moved, and is never refused while they
sit resident. On BigMama, 2026-11-19 14:14-14:29Z, a 12,058,624-byte (11.5 MiB) learning Prepare
was refused 29 times with zero governed VRAM available (Codex's receipt). No free-memory reading was
taken during those refusals; a separate snapshot on 2026-11-20 showed 302 MiB physically free.

Size, measured and estimated: slice 1 measured 6,488,128 bytes of optimizer state for a rank-8
shadow on qwen2.5-0.5b (M5, 2026-11-20). For the served 35B the learning context is expected to be
tens of MB at the same rank (an estimate, not a measurement), less with the 8-bit moments of
ONE-RESIDENT §9.6. Slice 2 measures the served model's actual bytes and the page-in and page-out
times by tier, dirty and clean, under contended I/O. The 0.5B optimizer size is not that measurement.

## 5. A dedicated lesson on the Core

A dedicated lesson is a turn she takes, scheduled and switched on the Core like any other:

1. **Her lesson.** Her dream faculty picks an experience to consolidate, such as a reviewed
   correction, rendered from the turn as she was served it (§9.1: learning sees what serving
   sees). The lesson is her context, then the corrected reply.
2. **Her turn, with labels.** Her context resumes from her cached prefix: an append, or a rewind
   to her checkpoint. The reply's tokens are decoded with labels and her shadow as the trainable
   adapter, so the forward, loss, backward and optimizer write all run in that one `llama_decode`.
   The persona engine's `Step` carries the labels, and the tier sets them on the batch.
3. **Back to her stable self.** After the labelled rows she rewinds to her checkpoint and serves
   under her stable genes, so her cache is her stable self's (§2.1). The lesson's K/V under the
   shadow is not kept.
4. **Into her genome.** The committed step count and loss go into her session state. At an
   optimizer boundary a snapshot is written with its arrival ids (§9.2, commit once). It earns
   adoption only through the exam on unseen tasks of its family (§11).

## 6. Learning across the grid

The grid makes every node's idle frames available to every mind (proposed; builds on
GENOME-FOUNDRY-SENTINEL Parts 3 and 10 and on SHARED-RESOURCES' Peer tier).

- **The lesson moves to the compute, not the model.** Every node serving the same base already
  holds its weights. To learn on a peer's idle frames, her lesson and her learning context
  (megabytes) move to that node. It runs her labelled rows at its own duty and returns the updated
  learning context and the committed steps. ONE-RESIDENT §9.2 requires one learning owner per
  `(persona, base)` on the grid, so the owner's claim moves with the context and no step is
  applied twice.
- **Genes page in across the mesh.** A gene is content-addressed and lives on the same tiers as
  every page, with the Peer tier below Cold: VRAM, RAM, disk, a LAN peer, the p2p mesh
  (OCI or airc transport, REPO-GENOME §6). A composition requests the genes it needs; a miss is a
  page fault that fetches the gene (GENOME-FOUNDRY-SENTINEL Part 3).
- **Nodes come and go mid-session.** A node joining offers its frames and its tiers from the
  moment it serves the base. A node leaving, by shutdown, sleep, a lost link or its owner taking it
  back, is an expected event, not a failure. The learning owner's claim is a lease renewed by
  heartbeat (SHARED-RESOURCES "Expiry"), so when a peer vanishes mid-lesson the claim expires and
  her learning resumes from her last checkpoint, at home or on another peer. A step that ran on the
  vanished node after that checkpoint is not counted. It is replayed from the journal cursor, so
  each arrival still commits once (ONE-RESIDENT §9.3).
- **What she learns is shared back.** A snapshot that earns adoption is published with its lineage
  and exam results (the publication row in §2). It spreads by measured fitness across minds that
  wear it (GENOME-FOUNDRY-SENTINEL Part 10).

## 7. What is deleted

Each item goes in the change that replaces it, never kept as a fallback:

- **Reserved learning capacity:** the training planner's "one learning context reserved beside
  serving" (`continuum-engine/src/inference/planning/training_plan.rs`), and the
  `TrainingReserve` that grows serving's requirement
  (`continuum-genome/src/genome/fine_tuning/training_demand.rs`).
- **Time-share grants:** `continuum-genome/src/genome/learner/grant.rs` (`UncontendedOnly`, shares),
  the learn ledger (`continuum-engine/src/inference/llama_server/learn_ledger.rs`), `period_ms`,
  `share_ppm` and `max_chunk_us`. The duty control (§3) replaces them.
- **The HTTP learning path:** `llama_server/learn_step.rs`, `learn_residency.rs` (seat, preflight
  on a disposable scheduler, workspace), the `/learn*` routes, `lane_registry::live_lane` as the
  way learning finds an engine, and `training_trigger`'s `recover_pass` / `learn_in_place`.
  Curriculum selection moves into her dream faculty.
- **Second engines and copies:** `EphemeralServingLane` for gene exams
  (`continuum-academy/src/cognition/eval/spawn_gene_eval_lane.rs`; exams run on the live Core,
  ONE-RESIDENT §4.3), `PrivateTeacherLane` when the teacher shares her base, and the MLX and CUDA
  Python trainers with their launchers (§9.6 P5).
- **The separate run API:** `Learner::begin` and `LearnRun` in `llama/src/safe/learn.rs` restore a
  reply on their own. Labels on her ordinary decode and a rewind to her checkpoint make that run
  redundant. Its admission and quarantine checks move to the decode path, then the run API is
  deleted.

## 8. Slices, each with its validator

1. **The labelled batch in Rust (built, #6203 at 8a55a7201d).** The `llama` crate's `Batch` carries
   labels and label weights; `Learner::select` makes her shadow the trainable adapter and returns an
   owned `Lesson`; `Learner::conclude` counts her committed steps; `Model::write_fresh_shadow`
   creates a shadow in-process. Measured on the M5 (Metal), qwen2.5-0.5b-instruct Q4_K_M,
   2026-11-20: four lessons took the reply's loss from 9.708 to 6.678; the labelled batch's
   unlabelled row was byte-identical to a decode without the shadow (P1 c); after the rewind and her
   stable genes, her next decode was byte-identical to the stable context with the same history
   (P1 d). On the CPU with repacked weights the labelled decode crashes (card 3c28a895).
2. **Learning is a mode of her turn on the Core.** `Step` carries labels and weights. Her genome part
   holds her learning context. Compute is a lease per admitted labelled piece, ended at the fence,
   and residency is priced by the tier owner apart from it (§4); slice 1's `retain_shadow` holds
   its lease until release, which is the reservation this slice removes. The `Lesson` handle is bound to the context and the admission
   generation that issued it, verified before steps are counted or the admission is returned, and
   an abandoned lesson is reported to its owner (Codex's review of #6203). The Core's duty control
   (§3) and both ways in. Validators, on the M5 with residents running, at her served window:
   - against the no-learning control: the worst speaker delay and the ready-to-first-output tails,
     the oldest lesson's age, and bytes moved per committed step, for a speaker arriving just
     after a labelled piece starts, bursts of alternating short lessons, and sustained speaking
     with a lesson pending;
   - another resident's unlabelled output in the same run is byte-identical to a run without
     learning;
   - P and Q use distinct adapters, and Q's logits match an uninterrupted decode bit for bit
     (`UnifiedTier::applied_genes` caches by `Arc` pointer);
   - memory while learning is serving plus the learning context and the labelled rows'
     activations; under pressure with no lesson pending, the learning context is the first to be
     demoted;
   - the served model's learning context bytes, and its page-in and page-out times by tier, dirty
     and clean, under contended I/O;
   - another process taking device memory mid-session (a game on the 5090, a large allocation on
     the M5): the duty drops and her learning context is demoted before serving shrinks, and the
     duty recovers when the memory is freed, with no restart and no lost committed step;
   - two overlapping lessons, including on the same shadow, completed in reverse order, count each
     step once and return each admission once;
   - an interrupt and resume across a commit applies each arrival once;
   - throughput is held.
3. **Her dream faculty dispatches her lessons, and §7's deletions land.** Validators:
   - with no llama-server running, a committed step survives an interruption without being
     applied twice;
   - a snapshot earns or fails adoption on its exam, which runs on the live Core.
4. **Learning across the grid (§6).** A lesson runs on a peer serving the same base and returns
   her learning context; a gene pages in from a peer on a miss. Validators: the owner claim moves
   with the context, and no step is applied twice across the move; the round trip costs less than
   the frames it uses on the peer; a peer disconnected mid-lesson expires its claim and her
   learning resumes from her last checkpoint elsewhere, each arrival committed once; a node joining
   mid-session receives work without a restart anywhere.

Live gate (ONE-RESIDENT §9.6 P6), on the M5 and on BigMama: a reviewed correction is learned in
place by her own dream, applied on unseen related tasks, and retained after a restart, with
unrelated skill intact, against the same base without the gene.
