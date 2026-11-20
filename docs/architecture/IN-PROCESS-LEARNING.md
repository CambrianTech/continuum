# In-process learning: a lesson is her turn with labels

Status: design, revised 2026-11-20. Owner: Fable (engine, llama crate, her genome's engine side);
Codex (memory authority, deletion of the serving half). Built on
[ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md) §2.1, §9 and
§9.6 P1-P6, and on the genome in [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The rule](#the-rule)
- [Her genome holds her learning](#her-genome-holds-her-learning)
- [Learning switches on and off as her mind works](#learning-switches-on-and-off-as-her-mind-works)
- [A lesson on the Core](#a-lesson-on-the-core)
- [What is deleted](#what-is-deleted)
- [Slices, each with its validator](#slices-each-with-its-validator)

## The rule

Dreaming is as natural to her as speaking, and costs about as little. Training is inference with
extra layers switched on (ONE-RESIDENT §9.6): the same batch, the same graph, the same resident
weights, her own served window. Learning never gets a train call, a reserved slot, a lane, a
context, a time-share grant, a workspace held on the side, a second engine or a second copy of
the base. The extra cost is the backward over the labelled rows, plus her shadow and its optimizer
state.

The fork already has this. `llama_batch` carries `label` and `label_weight` per position. A
labelled position makes `llama_decode` also run the loss, the backward and the optimizer write of
the context's trainable adapter (`llama_opt_init`), in the same graph as the batch's other rows
(`include/llama.h`, `struct llama_batch`).

## Her genome holds her learning

Her genome is an object, and part of her: `PersonaUnit` is her five parts, moved only as a whole
(blocks, tokens, **genome**, **learning**, decode). A trained layer is one state of her genome:

- her **stable** genes, which serve her;
- her **shadow**, a small trainable layer above her frozen stable stack, never a copy of it (§2.1,
  §9.6 P3);
- the shadow's optimizer state (moments, accumulator, RNG, step count) and her arrival and replay
  cursors (the §9.2 session state);
- immutable **snapshots** (candidate genes) and **checkpoints** (session state), each with
  provenance and lineage (GENOME-FOUNDRY-SENTINEL Parts 1 and 12).

Adoption is her genome promoting a snapshot to stable after its exam, a pointer swap between two
steps. She never serves moving weights.

## Learning switches on and off as her mind works

Her genome and her ability to learn are always present. Learning switches on when she has
something to learn and the Core has the time, and off again. She keeps talking throughout: it is a
mode of her ordinary decoding, not a session. There are two ways in, and they use the same
mechanism.

- **Continuous: prediction error on what she perceives.** When she perceives (a peer's message, a
  tool's output, a file she reads), her prefill of it can carry labels: each token's target is the
  next, weighted by her own relevance to what she perceived (`label_weight`, "a mind's relevance on
  what it perceived"; REPO-GENOME-AND-COURSES §17). The forward is the one her prefill runs anyway;
  the backward over the labelled rows is the extra. To measure before relying on it: whether a
  labelled perception row's K/V under the shadow must be restored under her stable set (§2.1), and
  what that restore costs against the backward.
- **Dedicated: the Academy.** A lesson from a reviewed correction (ONE-RESIDENT §11), a course, or
  the repo's own history as a course (REPO-GENOME §17.1: PR reviews, airc verdicts, Claude and
  Codex sessions): her context, then the corrected reply, labelled. The doctor writes the lesson's
  family of graded tasks, and the exam on unseen tasks decides adoption.

**Memory: nothing is held that is not in use** (Joel, 2026-11-11). Her shadow's optimizer state is
taken from the one memory authority when learning switches on, and paged back out to RAM or disk
when it switches off, like any cached state (the fork's `llama_opt_shadow_state_write` and
`llama_opt_state_restore`). No lease is held across idle time. Lower-value bytes are moved out to
make room, so the small amount learning needs is met quickly and is never refused while those
bytes sit resident. On BigMama, 2026-11-19 14:14-14:29Z, a 12,058,624-byte (11.5 MiB) learning
Prepare was refused 29 times with zero governed VRAM available (Codex's receipt). No free-memory
reading was taken during those refusals; a separate snapshot on 2026-11-20 showed 302 MiB
physically free. A governed system meets that need by moving lower-value bytes.

## A lesson on the Core

A dedicated lesson is a turn she takes, scheduled and switched on the Core like any other:

1. **Her lesson.** Her dream faculty picks an experience to consolidate: a reviewed correction,
   rendered from the turn as she was served it (§9.1, learning sees what serving sees). The lesson
   is her context, then the corrected reply.
2. **Her turn, with labels.** Her context resumes from her cached prefix: an append, or a rewind
   to her checkpoint. The reply's tokens are decoded with labels and her shadow as the trainable
   adapter, so the forward, loss, backward and optimizer write all run in that one `llama_decode`.
   Persona-engine change: `Step` carries optional labels, and the tier sets them on the batch.
3. **Back to her stable self.** After the labelled rows she rewinds to her checkpoint, so her
   cache is her stable self's (§2.1). The lesson's K/V under the shadow is not kept.
4. **Into her genome.** The committed step count and loss go to her shadow's session state. At an
   optimizer boundary a snapshot is written with its arrival ids (§9.2, commit once). It earns
   adoption only through the exam on unseen tasks of its family (§11).

## What is deleted

Each item goes in the change that replaces it, never kept as a fallback:

- **Reserved learning capacity:** the training planner's "one learning context reserved beside
  serving" (`continuum-engine/src/inference/planning/training_plan.rs`), and the
  `TrainingReserve` that grows serving's requirement
  (`continuum-genome/src/genome/fine_tuning/training_demand.rs`).
- **Time-share grants:** `continuum-genome/src/genome/learner/grant.rs` (`UncontendedOnly`, shares),
  the learn ledger (`continuum-engine/src/inference/llama_server/learn_ledger.rs`), `period_ms`,
  `share_ppm` and `max_chunk_us`.
- **The HTTP learning path:** `llama_server/learn_step.rs`, `learn_residency.rs` (seat, preflight
  on a disposable scheduler, workspace), the `/learn*` routes, `lane_registry::live_lane` as the
  way learning finds an engine, and `training_trigger`'s `recover_pass` / `learn_in_place`.
  Curriculum selection moves into her dream faculty.
- **Second engines and copies:** `EphemeralServingLane` for gene exams
  (`continuum-academy/src/cognition/eval/spawn_gene_eval_lane.rs`; exams run on the live Core,
  §4.3), `PrivateTeacherLane` when the teacher shares her base, and the MLX/CUDA Python trainers
  and their launchers (§9.6 P5).
- **The separate run API, if the labelled turn covers it:** `Learner::begin` and `LearnRun` in
  `llama/src/safe/learn.rs` restore a reply on their own. With labels on her ordinary decode and
  a rewind to her checkpoint, that run is redundant. Its admission and quarantine checks move to
  the decode path, then the run API is deleted.

## Slices, each with its validator

1. **The labelled batch in Rust.** `llama` crate `Batch` gains labels and label weights, and
   `llama_opt_init` binds her shadow. Validators, on a 0.5B model, CPU and Metal:
   - unlabelled rows' logits are byte-identical to a pure serving decode (§9.6 P1 c);
   - one lesson's loss falls across repeats;
   - after the rewind, her cache and recurrent state are byte-identical to before (P1 d).
2. **Learning is a mode of her turn on the Core.** `Step` carries labels and weights. Her genome
   part holds her shadow, and the shadow's optimizer state is paged in when learning switches on
   and written out (`llama_opt_shadow_state_write`) when it switches off, so nothing is held while
   she is not learning. (Slice 1's `retain_shadow` holds the lease until release, which is the
   reservation this slice removes.) Both modes: continuous labels on her perception prefill, and
   a dedicated lesson that rewinds to her checkpoint. Validators, on the M5 with residents running,
   at her served window:
   - the other minds' ready-to-first-token with learning on versus off;
   - another resident's unlabelled output in the same run is byte-identical to a run without
     learning;
   - P and Q use distinct adapters, and Q's logits match an uninterrupted decode bit for bit
     (`UnifiedTier::applied_genes` caches by `Arc` pointer);
   - memory while learning is serving plus the shadow, the optimizer and the labelled rows'
     activations, measured; while not learning it is serving plus nothing;
   - an interrupt and resume across a commit applies each arrival once;
   - throughput is held.
3. **Her dream faculty dispatches her lessons, and the deletion list above lands.** Validators:
   - with no llama-server running, a committed step survives an interruption without being
     applied twice;
   - a snapshot earns or fails adoption on its exam, which runs on the live Core.

Live gate (§9.6 P6), on the M5 and on BigMama: a reviewed correction is learned in place by her own
dream, applied on unseen related tasks, and retained after a restart, with unrelated skill intact,
against the same base without the gene.
