# In-process learning: a lesson is her turn with labels

Status: design, revised 2026-11-20. Owner: Fable (engine, llama crate, her genome's engine side);
Codex (memory authority, deletion of the serving half). Built on
[ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md) §2.1, §9 and
§9.6 P1-P6, and on the genome in [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The rule](#the-rule)
- [Her genome holds her learning](#her-genome-holds-her-learning)
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

## A lesson on the Core

A lesson is a turn she takes, scheduled and switched on the Core like any other:

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

The memory her shadow and optimizer need is a governed claim on the one memory authority, like
every other byte. Lower-value bytes are moved out (cache spills to disk), so a small need is met
quickly and is never refused while those bytes sit resident. On BigMama, 11.5 MiB was refused 29
times with 302 MiB physically free (2026-11-19, 14:14-14:29Z, Codex's receipt). That is the failure
this rules out.

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
2. **A lesson is her turn on the Core.** `Step` carries labels; her genome part holds her shadow
   and its optimizer; the lesson rewinds to her checkpoint. Validators, on the M5 with residents
   running:
   - the other minds' ready-to-first-token with lessons running versus without;
   - P and Q use distinct adapters, and Q's logits match an uninterrupted decode bit for bit
     (`UnifiedTier::applied_genes` caches by `Arc` pointer);
   - memory is serving plus the shadow, the optimizer and the labelled rows' activations,
     measured;
   - throughput is held.
3. **Her dream faculty dispatches her lessons, and the deletion list above lands.** Validators:
   - with no llama-server running, a committed step survives an interruption without being
     applied twice;
   - a snapshot earns or fails adoption on its exam, which runs on the live Core.

Live gate (§9.6 P6), on the M5 and on BigMama: a reviewed correction is learned in place by her own
dream, applied on unseen related tasks, and retained after a restart, with unrelated skill intact,
against the same base without the gene.
