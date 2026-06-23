# Self-Evolving Genome — Continuous, Definition-Free Learning by Measured Fitness

**Status:** architecture (build plan). The substrate it rides on is on `canary`; the
evolution algorithm is the frontier this doc defines.

**Read first:** [GENOME-FOUNDRY-SENTINEL](../architecture/GENOME-FOUNDRY-SENTINEL.md),
[CONTINUOUS-LEARNING-RUNTIME](CONTINUOUS-LEARNING-RUNTIME.md),
[ACADEMY-DOJO-ARCHITECTURE](../personas/ACADEMY-DOJO-ARCHITECTURE.md),
[CASCADING-CURRICULUM-ARCHITECTURE](../personas/CASCADING-CURRICULUM-ARCHITECTURE.md).
This doc is the **synthesis**: it connects that prior corpus to the genome loop now
shipped (`recorder → dataset/from-turns → forge/train → forge/export`) and adds the one
thing none of them pinned down — **the fitness algorithm that decides, without human
definition, when to mint / refine / merge / retire a LoRA layer.**

---

## 1. Thesis

A persona (and the grid of personas) should **accumulate competence from its own work,
continuously, without anyone enumerating domains or authoring a curriculum.** Three
properties make that "self-evolving" rather than "a training script in a loop":

1. **The work is the data.** Real room turns become training signal (`dataset/from-turns`).
   No synthesized curriculum is *required* — deliberate synthesis is available, not mandatory.
2. **Fitness is measured, never declared.** A layer's worth = the improvement it actually
   produces, weighted by how often it's used, divided by what it costs and how much it
   duplicates. No hand-set importance.
3. **Structure is discovered, not specified.** "Domains" are *clusters* in capability-space,
   found online. The number of layers is not configured — it emerges (nonparametric).

The machine runs with **no intervention** in steady state: experience flows in, fitness is
measured, clusters form, and the mint/refine/merge/retire decisions are thresholds on
measured quantities — tuned by the governor, not by a person.

---

## 2. The substrate it rides on (already shipped)

Academy is **not a subsystem to rebuild** — it is a set of **recipes + sentinels** over the
genome loop that is already on `canary`:

| Step | Primitive | Status |
|---|---|---|
| Capture the work | live `WorkspaceCycle` turns → prompt-captures (glass box = the one turn-truth); legacy `persona::recorder` fixtures | shipped |
| Work → training data | `dataset/from-captures` (LIVE path) + `dataset/from-turns` (legacy), chat JSONL, room/persona-filtered, structural curation drops empty/tool-JSON turns | shipped (#1691, from-captures 2026-06-22) |
| Train a LoRA | `forge/train` (drives unsloth, `--dry-run` validated) | shipped (#1695) |
| Package the layer | `forge/export` (LoRA / GGUF, pure arg-builder) | shipped (#1696) |
| Run any model | unsloth universal gateway (`forge`/`/v1`) | shipped (#1692/#1693) |
| Trust boundary | `GridTrustAuthPolicy` (who may exchange) | shipped (#1653) |
| The geometry | neural `EmbeddingProvider` (compute-once, shared) | shipped (#1657) |

The Academy roles map onto recipes/sentinels: **teacher** = a recipe-defined persona role;
**exam** = a pipeline step; **curriculum** = a recipe DAG; **cohort/team training** = a
multi-persona room recipe; **the dream** = a background-consolidation sentinel. All flow
through the same pipe above.

---

## 3. The fitness function

A layer `L`'s fitness is **value-density**, not abstract quality:

```
fitness(L) = (lift(L) × demand(L)) / (cost(L) × redundancy(L))
```

- **lift** — measured A/B improvement on the tasks where `L` is active (base vs. base+`L`,
  on a held-out set). The only honest quality signal. **Everything gates on this number.**
- **demand** — how often `L` is actually paged in / requested (usage frequency).
- **cost** — VRAM/compute to keep `L` resident.
- **redundancy** — overlap of `L`'s competence with what other resident layers already cover
  (mutual information in capability-space).

Corollaries the formula enforces for free: a high-lift layer nobody uses dies (demand→0); a
brilliant duplicate dies (redundancy→∞); a layer is kept only while its lift pays for its
footprint.

---

## 4. The decision algorithm (mint / refine / merge / retire)

The training signal lives in **embedding space** (§ the geometry). The decision is online
clustering plus value-density eviction:

- **Mint new** when incoming experience forms a **dense cluster far from every existing
  layer's region** — a new mode in the distribution → a new specialist.
- **Refine existing** when incoming experience falls **inside** an existing layer's region
  (more of what that layer already covers) → continue-train that layer.
- **Merge / distill** when two layers' regions have **drifted together** (centroids
  converged / competence overlaps) → fuse into one (the sleep-phase consolidation).
- **Retire / evict** when a layer is **dominated** by another or its **demand → 0** →
  evict (cheap, because content-addressed layers are re-pullable from the grid).

### The principled core

"Join an existing thing vs. start a new one, without pre-specifying how many things exist"
is exactly a **Dirichlet Process / Chinese Restaurant Process**: a new observation joins an
existing cluster with probability ∝ its mass, or spawns a new cluster with probability ∝ a
concentration parameter **α**. **α is the genome's one knob** — low α → few fat generalists
(consolidate hard); high α → many thin specialists. The governor tunes α from hardware +
demand. The guard against gratuitous minting is **MDL / Occam**: a new layer is warranted
only when its marginal `lift` repays its description-length cost.

### Analogs (the same algorithm in other clothes)

Immune **clonal selection** (bind-well → proliferate, else die; memory cells consolidate) ·
**NEAT speciation** (different-enough → new species = the mint threshold) · **wake-sleep**
(explore awake, consolidate asleep) · **mixture-of-experts** (the genome *is* a sparse MoE
of LoRA experts; routing = demand, pruning = redundancy) · **cache eviction** (ARC/LFU
value-density = retire).

---

## 5. The maturity sequence — earn the emergent version

**You cannot trust an emergent fitness function you have not validated.** A subtly-wrong
fitness signal makes the machine confidently accumulate garbage *and report improvement* —
the worst failure in the design. So the build is a sequence, not a leap:

- **Phase A — Defined (bootstrap, ground truth).** Run the Academy with *known* exams and a
  *defined* curriculum. Measure which signals — lift? demand? cluster-distance? — actually
  **correlate with real improvement on the held-out set.** That correlation *is* the fitness
  formula, discovered empirically rather than guessed.
- **Phase B — Emergent (graduate).** Once the fitness signal is validated against ground
  truth, drop the defined curriculum and let the §4 algorithm run definition-free, with the
  defined exams retained as a **regression tripwire**.

The prior Academy work (defined exams/curriculum) is **not the lesser idea** — it is the
**scaffold that makes the ambitious idea safe.** It supplies the ground truth that calibrates
the fitness function the emergent machine later runs on.

---

## 6. Build plan (slices, each VDD-gated)

Every slice must move a measured number before the next is built. Same cadence the genome
loop was built with: drive the engine, validate cheaply, adversarially review, merge.

| # | Slice | Gate (the number) | Builds on |
|---|---|---|---|
| 1 | **A/B harness** — score base vs base+LoRA on a held-out set → `StandardVddRecord` | a non-zero, reproducible **lift** | gateway + forge/export + VDD |
| 2 | **Fitness instrumentation** — capture lift × demand × cost × redundancy per layer | fitness ranks layers sensibly on a known set | §1, the WorkspaceCaptureSink |
| 3 | **Curation layer** on `from-turns` — score-filter + preference-pairs-from-reviews + cluster-balanced sampling | trained-on-curated **beats** trained-on-raw (slice 1 delta) | from-turns + embeddings |
| 4 | **Exam-as-recipe** — generate/hold-out set, grade, gate at threshold, regression guard | exam score predicts slice-1 lift | recipe runtime |
| 5 | **Cascading curriculum DAG** — retroactive root-cause weighting | cascade-weighted training beats flat | §4 |
| 6 | **Cohort / team training** — AP-Classroom comparative pairs + coordination capture | weak model improves toward strong after peer solutions | multi-persona room recipe |
| 7 | **Decision algorithm** — online clustering mint/refine/merge/retire (§4) | emergent layer set matches the defined one on the bootstrap domain | §2 + §3 |
| 8 | **Dream / consolidation sentinel** — idle-GPU score/dedup/weight/merge | disk + layer-count down, fitness preserved | §7, governor cadence |
| 9 | **P2P exchange** — embed the layer card, emit to catalog, pull→verify→sandbox-eval→page | a peer's layer raises local lift without local training | forge/export card + GridTrust |

Slice 1 is the keystone: until lift is a real, reproducible number, every later slice is a
hypothesis. Build it first; let the rest fall out of the measurement.

---

## 7. Risks and containment

| Risk | Containment |
|---|---|
| Garbage exhaust → garbage LoRA | Curation layer (slice 3); train only on scored/curated turns |
| LLM-as-judge grading noise | Multi-judge quorum + the **regression guard** (never deploy a layer that scores worse than the one it replaces) |
| Catastrophic forgetting across cycles | `ForgeRecipe.calibration_corpus` as the anti-forgetting anchor |
| **Fitness miscalibration** (the worst one) | Phase A defined bootstrap — validate fitness against ground truth before trusting it |
| Over-minting layers | MDL/Occam gate + low α: mint only when marginal lift repays complexity |
| Paging/training cost on real hardware | Governor measures swap/train cost via VDD, tunes α + working-set; never a hardcoded constant |

---

## 8. What's built vs. the frontier

- **Built (`canary`):** the genome loop (§2), the gateway, GridTrust, neural embeddings, the
  forge produce-side (`train`/`export`).
- **Frontier (this doc's slices):** the A/B measurement, fitness instrumentation, the curation
  layer, the Academy recipes (exam/curriculum/cohort), the decision algorithm, the dream
  sentinel, and P2P exchange.

The discipline that makes it safe is the same throughout: **measure first, trust second,
emerge last.** No hardcoded curriculum, no declared fitness, no configured layer count —
but also no emergent autonomy until the number that justifies it is real.
