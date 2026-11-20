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
  content-addressed, and referenced by every resident who wears it. It moves through `Candidate`,
  `Adopted`, `Refined`, `Archived` and `Retired` (GENOME-FOUNDRY-SENTINEL Part 12).
- **`GeneSet`** is the set she wears. Each turn it selects the genes the turn needs by importance
  (chapter 0.3) and binds them as her phenotype for that turn: the blend of small learned motions.
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

**The fastest start.** A resident joining a repo is useful at once:
- genes matching the repo's stack and history page in from the ether;
- a course is built from the repo's own history;
- the most recent PRs are held out as an honest exam, so whether she improved is measured from day
  one (REPO-GENOME-AND-COURSES §3, "clone to veteran", and §11, with and without arms).

## 6.5 Crates and what exists

| Object | Crate (chapter 8) | Today |
|---|---|---|
| Labelled rows, `Lesson` binding, shadow selection, `abandon` | `llama` | exists, #6203 in review |
| `Genome`, `GeneSet`, selection, residency | `continuum-genome` | exists in parts (`Genome`, `GeneTrials`, `GeneResidency`, `select`) |
| `Shadow` and its `LearningContext` paging | `continuum-genome` | proposed (IN-PROCESS-LEARNING slice 2) |
| `Lessons` as demands, the learning duty | `continuum-genome` with the governor | proposed |
| `Snapshot`, `Checkpoint` | `continuum-genome` on `Durable` | partly exists (learning checkpoints) |
| `Exam`, adoption | `continuum-genome`, `continuum-academy` | exists in the academy; to run on the live Core |
| Publication, discovery, inheritance, fitness | `continuum-genome` with `continuum-grid` | designed (GENOME-FOUNDRY-SENTINEL Parts 3 and 10); not built end to end |

## 6.6 What must be done

In order, each with its validator:
1. **#6203 lands:** the labelled batch and the bound lesson (validators exist and pass on Metal).
2. **Learning as a mode of her turn:**
   - `Step` carries labels;
   - the duty admits bounded pieces;
   - her learning context pages.

   Validators: IN-PROCESS-LEARNING slice 2, including worst speaker delay against no learning.
3. **Her dream faculty dispatches her lessons,** and the reserved-learning paths are deleted
   (IN-PROCESS-LEARNING §7).
4. **The live gate:** a reviewed correction learned in place by her own dream, applied on unseen
   related tasks, retained after a restart, with unrelated skill intact, against the same base
   without the gene (ONE-RESIDENT §9.6 P6).
5. **Self-assembly from the repo and the ether (6.4):**
   - lessons streamed from the repo's history, comments and distilled sessions;
   - candidate genes recalled from the ether by her gaps;
   - examined on held-out recent PRs and adopted, with no manual step.

   Validator: REPO-GENOME-AND-COURSES §11's with-and-without arms. On the held-out PRs, a resident
   who assembled her genome from the repo does measurably better than the same base without it.
6. **Spread:** publish, discover, adopt by exam, refine as a child, publish back. Validator: a gene
   grown on one node improves a resident on another node on her own tasks, and her refinement
   measurably improves it again.
