# Any Ask Is a Class — the North Star for Learning to Do Anything

> **Status:** synthesis / north-star (2026-06-22). This is the *read-first* orientation
> for the Academy/collaborative-learning vision. It states the thesis and the
> load-bearing principles, then points to the detailed docs — it does **not** re-derive
> them. If this doc and a detailed doc disagree on a mechanism, the detailed doc wins;
> reconcile this one in a follow-up.
>
> **Detailed docs this synthesizes:**
> [SELF-EVOLVING-GENOME](SELF-EVOLVING-GENOME.md) (fitness + the mint/refine/merge/retire
> decision, the measure-first discipline) · [ACADEMY-DOJO-ARCHITECTURE](../personas/ACADEMY-DOJO-ARCHITECTURE.md)
> (dual-sentinel teacher/student) · [CASCADING-CURRICULUM-ARCHITECTURE](../personas/CASCADING-CURRICULUM-ARCHITECTURE.md)
> (long-horizon credit assignment) · [COLLABORATIVE-LEARNING-VISION](COLLABORATIVE-LEARNING-VISION.md) ·
> [CONTINUOUS-LEARNING-RUNTIME](CONTINUOUS-LEARNING-RUNTIME.md) ·
> [ACADEMY-IMPLEMENTATION-PLAN](ACADEMY-IMPLEMENTATION-PLAN.md) (the roadmap).

## Thesis

**Any ask becomes a class; a class produces a capability.** A request is not answered by
a single frozen model — it spawns a **class**: a teacher plans a curriculum for *this*
ask, students learn it collaboratively (role-play, web research, tools, critiquing each
other's solutions, a class project), and the cohort's work distills into a **LoRA layer**
the genome can keep. The same apparatus runs *any* domain:

> `recipe → curriculum → class → LoRA`

"Design the best peanut-butter sandwich" and "write a lock-free allocator" run the
**identical machine**. The `ForgeRecipe` **is** the syllabus; the room **is** the
classroom; the students' turns **are** the training data; the layer **is** what the class
produced. This is what "learn to do anything" means, made literal.

## The four load-bearing principles

1. **No one is in isolation.** A frozen model answering from its weights is the isolated
   case, and isolation is the ceiling. Capability lives in the **interaction** — teacher +
   peers + the web + tools + each other's solutions — not in any single mind. Collaboration
   is not a feature on top of learning; it is the engine of it.

2. **Train as you work (distribution parity).** They will *deploy* into mixed
   human+persona teams — collaborating, role-playing, using tools, researching. So they
   must *train* in exactly that setting. You cannot train a mind in isolation on static
   batches and expect it to work in a team; the training distribution **must** be the
   deployment distribution. The class is not a metaphor for pedagogy — it is how training
   is made to match reality.

3. **Humans are in the team, not outside it.** The collaboration *includes and is because
   of* humans — as teachers, teammates, reviewers, and the responsible party who authorizes
   the class. The airc mesh is humans + personas together; the training signal is generated
   by that mixed team, which is precisely the deployment setting (principle 2).

4. **The teacher is generative, not just evaluative.** It plans the curriculum for *this*
   ask, **synthesizes** training data, assigns roles — **and authors the software that
   scores the result.** The class generates its own curriculum **and its own test.** That
   recursion is why no human must pre-enumerate domains or hand-write evals: the teacher
   writes the peanut-butter rubric (or a sandwich simulator) the way we hand-wrote
   `coder-eval.jsonl`, but per-ask and automatically. The teacher wants the **strongest
   available mind** (the gateway's best) — a weak teacher writes a weak curriculum and an
   invalid rubric.

## The teacher owns the data — single-shot → thousands

The multiplier that makes the class work: **the teacher is responsible for the data**,
and one example is not one training row. The teacher **downloads** (HF `datasets` /
web), **creates** (synthesizes), or **augments what it found** — *on the fly* — and
crucially **augments a single shot into thousands**: vary the inputs, paraphrase the
framing, scale the difficulty, synthesize analogous problems. So one solved task or one
ask becomes a curriculum of thousands of similar training pairs. That is how continuous
learning gets *volume* from sparse examples. The teacher develops the curriculum from
this (downloaded benchmarks + self-generated + augmented-from-online), fully
self-determined — and eventually the teacher *itself* is a LoRA-trained specialist.

## Sequencing: established benchmarks FIRST, the academy SECOND (do not conflate)

The academy is the **training engine**; it is NOT the **proof**. You cannot claim "we
beat Hermes/unsloth" on benchmarks your own teacher invented — that is not
apples-to-apples. So the order is non-negotiable:

1. **First — the straightforward, established benchmarks** (HumanEval / MBPP /
   SWE-bench — the test-graded gym, loaded via HF `datasets`). These are what the
   industry uses to rate models; they give the credible apples-to-apples comparison
   vs Hermes and unsloth (same tasks, our harness+learning vs their bare model). This
   is [ROADMAP P1–P4](../cognition/ROADMAP-TO-CODING-ITSELF.md), already being built.
2. **Then — the academy** (teacher + multi-student, single-shot→thousands curriculum).
   It is how we *climb* the established benchmarks (and any ask), not how we *measure*
   against the industry. Built after the standard proof exists.

Standard benchmarks are the rating; the academy is how you rise on them.

## The teacher teaches from experience — the dream

The teacher's textbook is **the system's own lived experience**, not a vacuum. Its primary
source is the data already on disk: the **engrams** (per-persona memory), the **turn
histories** (prompt-captures + recorder), the rooms' recorded collaboration. It *mines and
distills* these into curriculum and training pairs — and that distillation **is the dream**:
offline replay of experience to consolidate it (strengthen what mattered, turn raw episodes
into durable lessons, drop the noise). The front of this pipe already exists —
`dataset/from-captures` (live turns → SFT), the engram store, `memory/consolidation_pipeline.rs`.

**Consolidation is double-duty — the load-bearing elegance.** Engrams today are stored as
*raw incoming messages* ("Asha, run the ping tool…"), so recall surfaces transcript, not
knowledge (see [recall is semantic-capable but underpowered] in the cognition notes). The
dream that distills those raw engrams into **facts** ("the codename is BLUEHERON-7") is the
**same** dream that produces the teacher's **lessons**. Better memory and better teaching
are not two systems — they are one consolidation pass over the same experience. Fix the
dream and recall sharpens *and* the teacher gets its curriculum.

Disciplines that keep the dream honest (not a pretty hallucination):
- **Replay directed by the fitness gap** — consolidate near where the student fails, not a
  random rerun of the whole history (active learning, not exhaustive replay).
- **Web foraging fills only what experience can't cover** — secondary to lived data, and
  provenance-gated (contamination is the cost of the open internet).
- **The distilled curriculum still has to produce measured lift** — a dream you cannot
  score is a dream you cannot trust (the measurement spine below).

## What becomes a class — attention/salience is the selection signal

The dream mines experience — but *not all experience deserves a class.* A thousand routine
turns warrant none; a single **dramatic** one — a surprising failure, a struggled-through
task, an anxious dead-end — can warrant a whole curriculum. That is the flashbulb asymmetry:
emotionally-salient episodes consolidate deep while the mundane washes out. So the loop needs
a **selector** in front of the teacher: *which lived experience is worth turning into a class?*

**Attention is that selector — one signal doing two jobs.** The same attention that routes
in-the-moment cognition (the workspace arbiter, `Contribution.salience` per faculty bid) is
what tags an episode as consolidation-worthy. What the mind *attended to strongly* — because
it was surprising, error-laden, effortful, or dramatic — is what deserves to become
curriculum. This generalizes "replay directed by the fitness gap" (above): an eval failure is
just **one** kind of salient experience (a mistake); the live stream carries the rest.

**"Anxiety — detectable?" — the honest verdict.** Salience is a *composite* of proxies, and
the substrate exposes them unevenly today. The first cut keys only on what is **real now**;
the rest are named extension points, never faked (a salience signal you cannot measure is a
class trigger you cannot trust — the measurement-spine discipline applied to the *input*):

| Salience proxy | Detectable today | Signal in the substrate |
|---|---|---|
| **error** | **yes (built)** | eval grade strings + `gym_grader::test_grade`; `SettleOutcome.inference_error`; `Contribution.fault` (infra-fail vs wrong-answer) |
| **attention** | **yes (built)** | `WorkspaceCaptureSink`/`WorkspaceTrace` — per-tick bids carry ML-derived `Contribution.salience`; the attention competition is fully captured |
| **struggle** | **partial** | `SettleOutcome.acts` (effort/iteration count) + `TurnMetrics` + budget-exhaustion are surfaced; the spin/repeat detector (`all_calls_already_satisfied`) is *computed but not surfaced* — needs a field |
| **arousal** | **partial / proxy** | `PersonaState.{energy,attention,mood}` live via `cognition/get-state`, but arousal is only `Mood::Overwhelmed` (queue depth, not task difficulty); `FacultyId::Affect` is a *defined-but-empty* faculty seam |
| **surprise** | **no (design-only)** | no prediction-error/novelty scorer; `FacultyId::Volition` unimplemented; workspace explicitly refuses to "invent a novelty metric prematurely" |
| **uncertainty** | **no (absent)** | no logprobs/entropy on `TextGenerationResponse` or the llama-server adapter; `reasoning` (CoT) is the only depth proxy |

So a truthful first detector keys on **error + attention + struggle** (all tappable via
`WorkspaceCaptureSink` + `SettleOutcome` + eval grades); **surprise, uncertainty, arousal**
are the frontier that unlocks as the Affect/Volition faculties + logprob plumbing land.

**The seam (three parts, generalizing what already exists).** `genome/teach` is already a
working curriculum synthesizer (write→grade→fix→pass, packaged through the shared
`DatasetService::split_and_write` — the one forge pipe). What it lacks is a salience-driven
input. So:
- **`SalienceDetector`** — scores a lived episode for class-worthiness (error+attention+struggle now). *This is the new frontier this section defines.*
- **`ExperienceRecord`** — the retained rich episode the class is woven from. Today a failed
  `EvalTaskResult` throws the trajectory away (`answer` is **truncated to 200 chars**); you
  cannot synthesize a curriculum from a stub. Retaining the full act→observe series is the
  concrete first build item.
- **`CurriculumSynthesizer`** — generalizes `genome/teach`'s loop with an **expansion** mode
  (counterfactuals, harder variants, "what if it had gone worse") so one dramatic shot becomes
  the thousands of §"single-shot → thousands", not just a single fix. Stamps
  `TrainingSource::TeacherSynthesized`.

Outlier A = Asha's discovery-eval failures (remediation). Outlier B = a single dramatic live
episode off the `WorkspaceCaptureSink` stream (expansion) — the maximally-different input that
validates "even one shot if it was dramatic."

## The class loop (the mechanism)

```
ask  →  teacher plans curriculum (decompose the ask into a syllabus)
     →  spawn cohort (students + roles + web access + tools)
     →  students research / propose / role-play / CRITIQUE EACH OTHER / class project
     →  teacher feedback + teacher-authored SCORING
     →  distill the cohort's work → train a LoRA  (dataset/from-captures → forge/train)
     →  EVAL on a held-out test the cohort did not train on
     →  keep / refine / merge / retire the layer  (genome decision §4)
```

Humans enter at every arrow that says "teacher," "critique," or "responsible party."

## The measurement spine — what makes this *real hard stuff* and not a toy

This is the design that can fool you. A generative teacher with **no validated
measurement** is a machine that *confidently produces garbage and reports success* — the
worst failure mode in [SELF-EVOLVING-GENOME §5](SELF-EVOLVING-GENOME.md). The
non-negotiables that make the peanut-butter LoRA *real*:

- **The teacher's scorer is itself validated** against ground truth before it is trusted
  (Phase-A defined bootstrap). A teacher that grades wrong trains the class wrong, with a
  straight face.
- **Held-out lift + a regression guard.** The class ships a layer only if it *beats the
  prior* on a test it did not train on. No measured lift, no layer. (Genome §3, slice 1.)
- **Provenance on foraging.** Web access is the unlock *and* the contamination vector — the
  responsible party gates sources, or the class learns confident nonsense from the open
  internet.
- **Governance by the responsible party.** "Whatever the responsible party decides is
  needed" must be an *authorized, bounded* decision — scope, spend, trust, access — not an
  open faucet. This is what [GridTrustAuthPolicy](../grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md)
  and the identity/trust spine are for.

## Grounding — built vs. frontier (handoff honesty)

| Class concept | Primitive | Status |
|---|---|---|
| Syllabus | `ForgeRecipe` | shipped |
| Classroom | room recipe (multi-persona) | partial |
| Student work → data | `dataset/from-captures` (live turns → SFT) | shipped (2026-06-22) |
| The layer | `forge/train` → LoRA | shipped (dry-run validated) |
| The test | `cognition/eval` (today hand-authored; teacher-authored per-ask = frontier) | shipped (baseline) |
| Keep/refine/retire | genome decision (§4) | frontier |
| Teacher/student roles | dual-sentinel ([ACADEMY-DOJO](../personas/ACADEMY-DOJO-ARCHITECTURE.md)) | design |
| Long-horizon credit | [CASCADING-CURRICULUM](../personas/CASCADING-CURRICULUM-ARCHITECTURE.md) | design |

**The one missing keystone is the orchestrator** — the recipe/runtime that runs
*teacher → curriculum → cohort → web/tools → peer-review → scoring → train → eval →
keep/discard* as one loop, on the measurement spine. Everything above it is a primitive
that exists; the orchestration is the Academy made executable. Build it the way the rest of
the genome loop was built: **measure first, trust second, emerge last** — a class you cannot
score is a class you cannot trust, no matter how good its lecture looked.
