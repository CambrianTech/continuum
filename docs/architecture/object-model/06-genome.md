# 6. The genome and learning

[The object model](README.md) · previous: [The persona](05-persona.md) · next: [Live calls and embodiment](07-live.md)

The README's promise: experience can change the weights. Learned adaptations are genes with
provenance; a citizen's genome composes them; genes spread through a population and keep improving as
they spread. This chapter names every object that makes that true, from a labelled row in her decode
to a gene adopted by a stranger on another machine. The design detail lives in
[IN-PROCESS-LEARNING.md](../IN-PROCESS-LEARNING.md),
[ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](../ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md),
[GENOME-FOUNDRY-SENTINEL.md](../GENOME-FOUNDRY-SENTINEL.md) and
[ACADEMY-AS-CONTINUOUS-EVOLUTION.md](../ACADEMY-AS-CONTINUOUS-EVOLUTION.md).

**The thesis.** Joel: "Never manual ever. It's autonomous and self improving, the entire ecosystem
world sized, to get better and challenge frontier by having essentially limitless experts assembled
on the fly, like Voltron."
- **Experts are assembled, not held.** The genome is not a model to be kept resident. It is a
  population of experts, assembled on the fly into whatever each moment needs, the way Voltron forms
  from its lions.
- **The cost argument.** One frontier model must be held whole to be used. A paged genome holds only
  the experts in use now, and its cost scales with the deduplicated union of those working sets
  across the grid, not with users times model size (GRID-ECONOMICS-AND-AFFINITY-ROUTING §1).
- **It improves itself, at world scale.** Every resident who wears a gene teaches it, and every
  node's spare frames train it. The whole ecosystem gets better without anyone running anything.

Every object in this chapter serves that, and none of them has a manual step.

## 6.1 The objects

```
Genome: Part                         her genes, as states of one object             continuum-genome
├─ genes: GeneSet: Part              the genes she wears, composed per turn
│  └─ Gene: Durable                  one learned adaptation, content-addressed, with its lineage
├─ shadow: Shadow: Part              the layer she is learning into
│  └─ LearningContext                shadow weights + optimizer state + cursors (held Loaded; paged)
├─ lessons: Lessons: Part            her lesson queue, journal and credits
│  └─ Lesson: Demand                 a context and a target, weighted by salience
├─ snapshots                         Snapshot: Durable, a candidate gene at an optimizer boundary
├─ checkpoints                       Checkpoint: Durable, session state with the cursor it consumed through
└─ regions                           DreamConsolidation, CreditAcceptance: Regions of her learning
Exam: Check<Snapshot>                a gene's test on unseen tasks of its family
Lineage                              a gene's parents, data, base, exams and wearers
Population                           the genes published on the grid and mesh, with their fitness
```

- **`Gene`** is a `Durable` archetype (chapter 0, "archetypes are separate from instances"). It holds
  weights at a rank and set of targets for one base model, and its lineage. It is stored once,
  content-addressed, and referenced by every resident who wears it. Its identity and lineage are
  today's `GeneSignature` (its corpus, its parent, its region). Its lifecycle is today's
  `TrialState`: `Trial`, then `Promoted` or `Retired`.
- **`GeneSet`** is the set she wears, and her phenotype is a mix, not a pick. Many genes are hosted at
  once, and each has a contribution (its weight in the blend) and a priority (its claim on
  residency). Both are re-estimated continuously from her focus, salience and each gene's fitness.
  Choosing which genes are present is assembly; adjusting how much each one counts is the
  continuous part, as RAGE's mission music turns each stem up or down by what is happening.
- **The mixing is learned too.** How much each gene should contribute in which situation is itself
  learned from outcomes, so the mix improves along with the genes.
- **Mix changes land at boundaries.** A change of weight alters what her cached context was computed
  under, so it lands at a turn boundary and is priced like any change of phenotype (6.4).
- **`Shadow`** is a small trainable layer above her frozen stable genes, never a copy of them. Its
  `LearningContext` is held by a `Loaded` holding: promoted when a labelled row needs it, kept warm
  while it pays, demoted under pressure (IN-PROCESS-LEARNING §4).
- **`Lessons`** is her queue. Each `Lesson` is a `Demand` (chapter 3.8): its value is the gap it
  closes, weighted by how often the gap costs her and by whether a reviewer corrected it. Its
  essential part is its whole labelled piece.
- **`Snapshot` and `Checkpoint`** are `Durable`. A snapshot is a candidate gene; a checkpoint is the
  session state with its journal cursor. Checkpoints keep all three durable contracts (chapter 3.7),
  so a restart never applies a lesson twice.
- **`Exam`** is a `Check`: a snapshot passes or fails on unseen tasks of the lesson's family.
  Adoption is the genome's command: a passed snapshot replaces its stable gene by a pointer swap
  between two frames. She never serves moving weights.

## 6.2 How she learns

Learning is inference with labelled rows switched on, in her ordinary frames (IN-PROCESS-LEARNING §1).

1. **A gap is noticed.** A reviewer corrects her, a test fails, the world's receipt contradicts her
   intent, or her prediction of what she perceives is wrong. The gap becomes a `Lesson`, weighted by
   salience.
2. **The duty admits it.** When the Core has slack and her lessons are waiting, the governor admits a
   bounded piece of labelled rows that fits the waiting speaker's budget (§3). A course in the
   Academy is the same control held high.
3. **One decode trains.** Her labelled rows run in the same batch and graph as serving. The loss, the
   backward pass and the optimizer write happen inside that one `llama_decode`, and her unlabelled
   rows are byte-identical to a decode without learning (exists: the llama crate's labelled `Batch`
   and `Lesson`, #6203).
4. **Progress lands.** Committed steps are counted against her shadow's own step count, once. At an
   optimizer boundary a snapshot and a checkpoint are written.
5. **The exam decides.** The snapshot is examined on unseen tasks of its family. It is adopted, or
   discarded with the reason recorded as experience.

## 6.3 How genes spread and keep improving

A gene does not stop learning when it leaves the resident who grew it.
- **Published with its proof.** An adopted gene is published with its lineage and its exam results,
  attested by Forge-Alloy, on the grid and the mesh.
- **Discovered by capability.** Another resident finds it when a gap of hers matches what it
  improved, and it pages in to her node like any page (chapter 0, "What many depend on streams
  first").
- **Adopted only by her own exam.** A downloaded gene is a candidate in her genome, never a trusted
  gift: it is adopted only if it passes her exam on her tasks.
- **Refined where it lands.** Once adopted, her shadow keeps learning on top of it from her own
  experience. What she learns is a child gene, with the inherited gene as its parent in its lineage.
- **Improvements flow back.** A child that passes its exam can be published in turn. Fitness is
  measured across everyone who wears a gene and its descendants, and the fittest spread.

So a gene improves while it is downloaded and shared: every recipient is another life it learns
from, and the lineage records whose experience made each version better.

## 6.4 Her genome assembles itself

Joel: "no matter what the personas are doing or thinking about, the genome and the genes in the ether
... it automatically assembles its own paged in genes and trains them to get better dynamically.
It's not hand coded or manual. It's part of her being." Nothing below is a job someone runs for
her. These are regions of her learning branch, reading the one importance (chapter 0.3), always on.

**Where lessons come from.** Every trace her work and her team's work leaves is a source of lessons,
streamed like any source (chapter 3.6):
- **The repo's history is a course.** A PR's diff with its review is a correction. A CI failure with
  the commit that fixed it is a gap and its answer. A review thread is a lesson in how this team
  works (REPO-GENOME-AND-COURSES §17.1).
- **GitHub comments and verdicts.** Issue discussions, airc verdicts and card outcomes are lessons too.
- **Distilled sessions.** Our own Claude and Codex sessions are distilled into lessons
  (REPO-GENOME-AND-COURSES §12): the corrections a person made, the dead ends, the fix that landed.
- **Her ongoing work.** Every receipt that contradicted her intent, and every correction a reviewer
  gave her.

Each becomes a `Lesson` demand, weighted by salience: how often the gap would cost her in the
activities she is in.

**Where genes come from.** Genes live in the ether, and she reaches all of it the same way, through
the tiers (chapter 0, "Every page has a lifecycle"):
- her own node;
- the repo's own genes, versioned with its code (REPO-GENOME-AND-COURSES §1);
- lineage-tracked genes shared on Hugging Face;
- any other repository, public or private, that she has access to.

Demand-aligned recall (GENOME-FOUNDRY-SENTINEL) finds candidates by outcome-conditioned similarity
to her current gaps and activities, and prefers a refined gene over an imported one.

**How they are scored.** A candidate gene is a demand on her genome. Its value is the improvement it
is expected to bring on her current gaps, weighted by the activities she is in and by the fitness
evidence its lineage carries. Its cost is the bytes and paging it needs. The same importance that
decides what streams into her context decides which genes are paged in for a turn, which candidates
are examined, and which lessons train first.

**How it assembles.** Her `GeneSet` composes the genes each turn needs. Her `Lessons` train her
shadow from the highest-valued lessons as the duty allows. Candidates and refinements are examined
on held-out tasks and adopted when they pass. All of this runs as she works, at whatever duty the Core
can give, without anyone starting it.

**How it runs: the genome as dataflow.** Joel: "The mind and its recall is already doing this, so
it's just a matter of emitting events and subscribing." The mind already computes what is relevant
to her now (her focus, her recall, her salience). The genome does not compute it again; it
subscribes. It is made of small parts, each on its own worker, joined only by events and
publications:

| Part | Subscribes to | Publishes |
|---|---|---|
| Gap sensor (a `Region`) | her receipts, review verdicts, corrections, prediction error on what she perceives | gaps, each weighted by salience |
| Scout (a `Region`) | gaps, her focus | candidate genes, found by demand-aligned recall: locally first, then the hub and the grid, each search a ticket |
| Composer (a `FusionRegion`) | her focus and recall, candidates, fitness, outcomes | her phenotype: which genes she hosts, each gene's contribution and its priority, continuously re-estimated, with the mixing itself learned from outcomes |
| Residency (a `Holder`) | the phenotype, the scheduler's next mind | genes promoted ahead of need and demoted when cold |
| Lessons | gaps, corrections, the repo's history, distilled sessions | lessons as demands, trained as the duty allows |
| Examiner | snapshots and inherited candidates | verdicts: promoted or retired |
| Fitness | verdicts, utilization across wearers | each gene's fitness, read by the scout and the composer |
| Publisher | promotions | the gene on the hub, signed, with its lineage |

The composer is the genome's governor. It chooses the best phenotype and each gene's contribution as
she works, reading the one importance, and it changes smoothly:
- **Her context depends on her genes.** Her cached context was computed under the genes she wore
  when it was prefilled. A changed blend means that context is recomputed from where the two
  differ, so a change is never free.
- **Changes land at boundaries, priced.** Phenotype changes land at turn and epoch boundaries,
  priced against that re-prefill cost.
- **Changes ramp.** A new gene's contribution ramps up over turns, as a level of detail blends in.
- **Hysteresis stops thrashing.** A candidate must beat the current blend by more than its switch
  cost before it is taken (chapter 0.4, "Authority").

**The fastest start.** A resident joining a repo is useful at once:
- genes matching the repo's stack and history page in from the ether;
- a course is built from the repo's own history;
- the most recent PRs are held out as an honest exam, so whether she improved is measured from day
  one (REPO-GENOME-AND-COURSES §3, "clone to veteran", and §11, with and without arms).

## 6.5 What exists

An inventory of canary (latest genome commit 3301e0d470, 2026-11-20). Much of the genome is built,
tested and live; the design above is mostly its unification and its gaps. **G** is
`core/continuum-genome/src/genome`, **D** is `core/continuum-dreaming/src`, **A** is
`core/continuum-academy/src/cognition`, and **M** is `core/continuum-mind/src`.

| Object | Today | Status |
|---|---|---|
| `Gene`: identity and lineage | `GeneSignature` (`G/signature.rs:63`: corpus, parent, region), `TrainedAdapter` (`G/adapter_manifest.rs:31`) | built, tested; signed only at publish |
| `Genome` as one object | spread over `GeneTrial` (`G/gene_trial.rs:78`), the manifest, `SignatureStore` | partial: no single owner |
| `GeneSet` and the phenotype | `composer::select` and `compose`, `Phenotype` (`G/composer.rs:88-223`), called on every turn (`M/persona/service_loop/turn_cycle.rs:65`) | built, tested, live |
| Residency | `GeneResidency` (LRU-2 heat, prefetch, victims), `GeneCatalog` (`G/gene_residency.rs:120`, `catalog.rs:23`) | built for device and host; no disk, Hugging Face or grid tier |
| Gene search | `DemandAlignedRecall`, `LocalDemandAlignedRecall`, scoring, composite sources (`G/recall_*.rs`, about 56 tests) | built, tested; one caller; no grid candidate source |
| Reuse, fork or mint | `competence::Decision`, `decide_with_pending` (`G/competence.rs:137-215`) | built, tested; a fork starts cold (no warm start) |
| Learning (production path) | `MindLearner` over llama-server `/learn`, one example at a time, durable checkpoints (`D/modules/training_trigger/in_place.rs:527`, `durable/*`; `G/learner/*`) | built, tested, live |
| Learning (in-process path) | `Learner`, `Shadow`, the labelled `Batch` (`core/llama/src/safe/learn.rs`; #6203) | slice 1 built; no callers yet |
| `Exam` and adoption | the examiner (`G/examiner/*`: `decide`, `p_gene_better`, retention re-run on boot), `MindExamHost` (`A/eval/gene_exam.rs:40`), graders (`G/gym_grader.rs`, `A/held_out.rs`) | built, tested: the only adoption gate |
| Lifecycle | `TrialState`: `Trial`, `Promoted`, `Retired` (`G/gene_trial.rs:25`) | built |
| Lessons from corrections | `training_producer/reviewed/correction.rs` (M); teacher remediation (`G/teach.rs:256`) | built, tested |
| Lessons from repo history, reviews, sessions | only prompt captures (`D/commands/dataset/from_captures.rs`) | absent |
| Courses | `A/coursework.rs`, `A/curriculum.rs`; buckets keyed by `trait_kind` | partial; no course files or repo layout |
| Fitness | `LayerFitness` (`G/fitness.rs:66`), `GeneFitnessIndex` from lift rows (`G/fitness_ledger.rs:68`) | partial: three definitions (6.6) |
| Publication and inheritance | Hugging Face push, pull and nearest-on-hub with a covenant (`D/commands/genome_share.rs`), publishers (`continuum-core/src/forge/`) | partial: publishing is manual; no OCI or grid |
| Dream consolidation | the region (`A/dream_consolidation.rs`), distilled lessons | built for memory; does not feed gene training |

## 6.6 Rulings on the contradictions

The genome's design docs disagree in nine places. Each ruling below follows the thesis (never
manual; learning in place; one of everything) and what the code already does.

1. **Where training runs: in place.** Separate trainer processes (TRAINING-SYSTEM, FINE-TUNING,
   CUDA-GENOME-TRAINING) are superseded by learning on the serving weights (ONE-RESIDENT,
   IN-PROCESS-LEARNING), which the code already does. The MLX trainer remains only behind the
   forge, and goes when the forge trains in place.
2. **Batch size: one example at a time.** LEARNING-LOOP's threshold of 16 is stale. The trigger
   learns per example, and the duty decides how much runs per frame.
3. **Fitness: one definition.** There are three: lift×demand/(cost×redundancy) in
   SELF-EVOLVING-GENOME, U×(1+Π) in GENE-REUSE-FORK-MINT and LEARNING-LOOP, and A/B lift rows in
   the ledger. They become one, owned by `continuum-genome`:
   - a gene's fitness is its measured lift (the examiner's A/B evidence);
   - weighted by its utilization, meaning the turns and wearers that draw on it, its descendants
     included, which is its propagation;
   - divided by its cost in bytes and paging;
   - redundancy with a nearer gene lowers its utilization rather than appearing as a separate term.
4. **The pager: the one tier ladder.** The five-tier gene pager in older docs does not exist; the
   code has device and host. Genes gain flash, Hugging Face and grid tiers as pages of the `Holder`
   lineage (chapter 4.3), not as a pager of their own.
5. **Whether a gene changes: the shadow continues, genes are immutable.** Her shadow keeps learning;
   each snapshot is an immutable candidate; refining an inherited gene produces a child with it as
   parent. "Continue" and "fork" are the same act seen from the session and from the gene.
6. **Composition: automatic, steered by her own importance.** No manual select, lock or release
   (ADAPTIVE-GENOME-LIFECYCLE). The composer selects by distance and region today, and by the one
   importance (chapter 0.3) once it is unified, which already reflects her attention.
7. **Who grades: the world.** Tests, compilers and deterministic graders grade; there is no LLM
   judge in the core (ONE-RESIDENT §1). A teacher writes lessons and tasks; it does not decide
   adoption.
8. **Where gene bytes live: Hugging Face is the lineage of record.** Genes are model repositories
   whose `base_model` field is their lineage (GENOME-REPOSITORY-ON-HF). A repo's genome manifest
   refers to genes by content address. Whether bytes also travel by OCI between nodes is a
   transport choice, open for Joel (REPO-GENOME-AND-COURSES §6 proposed OCI).
9. **The corpus: superseded docs are marked.** About 35 docs from the TypeScript and Python eras
   still read as current, among them `docs/genome/README.md`, `LORA-GENOME-PHENOTYPES.md`,
   `CUDA-GENOME-TRAINING.md` and the trainer docs. Each gets a superseded banner pointing here, and
   their surviving ideas are cited from this chapter (11, phase A).

## 6.7 What must be done

Built on what exists (6.5), in order. Each step has its validator, and none adds a manual step.

1. **#6203 lands:** the labelled batch and the bound lesson. Its validators exist and pass on Metal.
2. **One `Genome` object.** The trial file, the manifest and the signature store become branches of
   one `Genome` part, owned by `continuum-genome`; their scattered owners are deleted. Validator:
   suspend and resume of her genome is one command, and nothing about her genes is read anywhere else.
3. **Learning in process, as a mode of her turn:**
   - the `Learner` gets its callers, so `Step` carries labels;
   - the duty admits bounded pieces;
   - her learning context pages;
   - the llama-server `/learn` path and the reserved-learning paths are deleted (IN-PROCESS-LEARNING
     §7).

   Validators: IN-PROCESS-LEARNING slice 2, including worst speaker delay against no learning.
4. **Salience in every lesson.** Prediction error on what she perceives, weighted by her relevance,
   becomes `label_weight` on her perception rows, and lessons are bucketed by competence instead of
   by `trait_kind`. Validator: chapter 0.3's comparison of salience against uniform weighting.
5. **Lessons from every trace (6.4).**
   - Sources: repo history (a PR's diff with its review, a CI failure with its fix), GitHub comments
     and verdicts, distilled Claude and Codex sessions.
   - They join the corrections already built.
   - Dream consolidation feeds its distilled lessons to the same queue.

   Validator: lessons appear from a repo's history with no one starting anything, and each names its
   source.
6. **One fitness (6.6, ruling 3),** feeding recall's scoring, the examiner's retention checks and
   retirement. The fitness sentinel retires as well as reports.
7. **Genes on every tier.** Flash, Hugging Face and grid tiers as `Holder` pages; a grid candidate
   source for recall; a forked gene warm-started from its parent. Validator: a gene missing locally
   pages in from a peer or the hub on demand, and a fork starts from its parent's weights.
8. **The live gate:** a reviewed correction learned in place by her own dream, applied on unseen
   related tasks, retained after a restart, with unrelated skill intact, against the same base
   without the gene (ONE-RESIDENT §9.6 P6).
9. **Self-assembly from the repo and the ether (6.4).** Validator: REPO-GENOME-AND-COURSES §11's
   with-and-without arms. On held-out recent PRs, a resident who assembled her genome from the repo
   does measurably better than the same base without it.
10. **Spread, automatically:**
    - publishing an adopted gene is automatic, attested and signed;
    - discovery, adoption by the recipient's own exam, refinement as a child, publication back.

    Validator: a gene grown on one node improves a resident on another node on her own tasks, and her
    refinement measurably improves it again.
11. **The corpus reconciled (6.6, ruling 9).** Every superseded genome doc carries a banner pointing
    here, and no doc describes deleted code as current.
