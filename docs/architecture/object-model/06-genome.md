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

## 6.4 Crates and what exists

| Object | Crate (chapter 8) | Today |
|---|---|---|
| Labelled rows, `Lesson` binding, shadow selection, `abandon` | `llama` | exists, #6203 in review |
| `Genome`, `GeneSet`, selection, residency | `continuum-genome` | exists in parts (`Genome`, `GeneTrials`, `GeneResidency`, `select`) |
| `Shadow` and its `LearningContext` paging | `continuum-genome` | proposed (IN-PROCESS-LEARNING slice 2) |
| `Lessons` as demands, the learning duty | `continuum-genome` with the governor | proposed |
| `Snapshot`, `Checkpoint` | `continuum-genome` on `Durable` | partly exists (learning checkpoints) |
| `Exam`, adoption | `continuum-genome`, `continuum-academy` | exists in the academy; to run on the live Core |
| Publication, discovery, inheritance, fitness | `continuum-genome` with `continuum-grid` | designed (GENOME-FOUNDRY-SENTINEL Parts 3 and 10); not built end to end |

## 6.5 What must be done

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
5. **Spread:** publish, discover, adopt by exam, refine as a child, publish back. Validator: a gene
   grown on one node improves a resident on another node on her own tasks, and her refinement
   measurably improves it again.
