# Benchmark-as-Learning-Flywheel — the grid task engine that proves AND improves

**Status:** north-star design (2026-07-28, BigMama + M5). Ties together the benchmark suite, the
grid, [[SENTINEL-IN-SUBSTRATE]] (experiential plasticity + AttnRes skip paths), and persona-RAID.
**The reframe (Joel):** "benchmarks... it's sort of everything for learning and proving. We can get
our models to continuously learn, plastically, maybe even with experiential plasticity and unet skip
paths for generalization."

## The one idea

A benchmark run is a **labeled task with an automatic grader**. That is ALSO the exact shape of a
training example. So the benchmark engine is not a scoreboard bolted onto the side — it is the
**data generator** at the head of a continuous-learning loop:

```
task surface (many benches, many domains)
      │  cross-grid distribution (every node runs what it can)
      ▼
being attempts task  ──►  automatic grader (rustc / exec / reference)
      │                         │
      │  PASS → capability proof (the charts)      │  FAIL → gradient
      ▼                         ▼
  results ledger          the failure IS the training datum
      │                         │
      └─────────►  experiential plasticity (forge-while-dreaming)  ◄──┘
                         │  prune dead capacity, clone hot, LoRA-heal on failures
                         │  AttnRes skip paths → generalization (implicit depth ensemble)
                         ▼
                 improved being  ──►  re-run the surface (loop)
```

Prove and improve are the SAME pass. The charts (K3-target agentic benches — Terminal Bench, Program
Bench, SWE Marathon, Automation, BrowseComp) are the measuring stick; the failures against them are
the curriculum.

## What already exists (do not rebuild — connect)

| Piece | State | Where |
|---|---|---|
| Rust-native `benchmark/*` family (list/run/matrix/competition) | merged | `commands/benchmark.rs` |
| Graders: rustc compile+run, SWE-bench runner, web-dev, games | merged | benchmark collections |
| Fire-and-poll long runs (#86) | merged | `cognition/eval-status` |
| base_model_id override — measure ANY model, living persona untouched | merged | #1932 |
| Unified experience stream — one being learns from lived + eval + told (3 axes) | merged | #2024 / [[being-axis-shareable-learning]] |
| Lived-axis LLM-teacher expansion — learns from turns it STALLED on | merged | #2033 |
| Cross-grid dispatch (Commands.execute at a peer) | inbound live; effector converging | [[cross-grid-dispatch-canonical-path]] |
| Experiential plasticity (prune/clone/quant + AttnRes) | plan + AttnRes BUILT | [[SENTINEL-IN-SUBSTRATE]], fork `feat/kimi-k3-attnres` |

The stall-expansion (#2033) is the seed of THIS: a benchmark FAIL is a high-value stall. The unified
experience stream (#2024) is already the pipe the eval axis flows into. We are wiring existing
organs, not growing new ones.

## The gaps to close (the build)

### 1. Cross-grid task distribution — the benchmark MATRIX goes multi-node
Today `benchmark/matrix` runs runners × benchmarks on ONE node. Extend the matrix executor to
dispatch task-shards to grid peers (each peer runs the benches its VRAM/model can serve), collect via
the inbound command-RPC, merge into one results ledger. The grid becomes a distributed eval cluster —
BigMama's 5090 runs the 48B/K3-tier benches, M5's Mac runs the Metal-servable tier, Air runs small.
Reuses residency + `route_grid_overflow`; no new transport. **This is what "works cross grid across a
ton of tasks" means concretely.**

### 2. Failure → curriculum — the grader's negative becomes a training datum
A graded FAIL already has everything a training example needs: the task prompt, the wrong output, the
reference/test that rejected it, and (for the loop graders) the compiler/runtime error. Emit each
FAIL into the lived-experience stream tagged `eval-fail` with its grader feedback. The dream-forge
(sentinel slice 4) consumes these: LoRA-heal on the failure clusters during idle GPU time, validate
on the SAME bench on wake, roll back on regression. **The being trains on exactly what it's scored
on.**

### 3. Generalization — AttnRes skip paths + plasticity, not just memorization
The risk of "train on the benchmark" is overfitting to it. Two defenses already designed:
- **AttnRes skip paths** (built): a residual highway trained with stochastic block-drop is an
  implicit ensemble over depths → generalizes past the specific tasks (stochastic-depth literature).
  This is the "unet skip paths for generalization" Joel names — the STABLE form of it (K3's AttnRes,
  not sentinel-ai's unstable U-Net).
- **Held-out split** (discipline): the task surface is split train/eval per collection; plasticity
  trains on the train shard, the charts are ALWAYS the held-out shard. Overfit shows as train-eval
  divergence — a first-class signal, logged.

### 4. Honest instrument (already the doctrine — keep it)
Warm-gate (never measure a cold model), same-model control (base_model_id), team writer+reviewer,
fail-loud VOID cells — all merged. The flywheel is only as trustworthy as the grader; the proof
discipline (#1584) is the law. A learning loop on a lying instrument optimizes the lie.

## Sequencing
1. **Map our collections onto the K3-target named benches** (Terminal/Program/SWE-Marathon/Automation
   equivalents) so our numbers are comparable to the charts — the measuring stick must match the goal.
2. **Cross-grid matrix** (gap 1) — distribute the surface; the grid proves at scale.
3. **Failure→curriculum emit** (gap 2) — the cheapest high-value wire: FAIL → lived stream (#2024 pipe
   exists).
4. **Dream-forge consumes eval-fails** (sentinel slice 4 + gap 3) — the loop closes; idle GPU learns.
5. **Held-out discipline + AttnRes stochastic-drop training** (gap 3) — generalization, not memorization.

## The catalog IS the curriculum (in-repo, accumulating, per-persona LoRA)
(Joel, 2026-07-28: "we will accumulate this as a catalog we support in repo, so we can run and learn
from them — continuous LoRA persona.")

The benchmark catalog (`known_benchmarks()` + the `docs/genome/*.jsonl` eval sets) is not a test
directory — it is a **versioned curriculum that ships in the repo**. Each runnable collection is
simultaneously:
- a **proving instrument** (run it → a chart number, held-out), and
- a **training corpus** (its graded failures → per-persona LoRA gradient).

Because it lives in-repo, the curriculum is reproducible, diffable, and grows monotonically: every new
runnable bench (like `livecodebench-rs` today) permanently widens both what we can prove AND what the
personas can learn from. The loop, per persona:
```
persona attempts catalog bench → grader → PASS proves / FAIL → eval-fail datum
    → dream-forge trains a per-persona LoRA on the failure cluster (idle GPU)
    → validate on the SAME bench held-out shard → keep adapter if it improves, roll back if not
    → the being is measurably better at that bench next run — permanently, on its own genome
```
This is where the flywheel meets the genome: the LoRA the being trains from the catalog is paged like
any other skill ([[continuum-substrate-already-built]] genome tiers), replicated by persona-RAID
([[restarts-are-commonplace]]), and shareable to peers ([[being-axis-shareable-learning]]). One
persona's hard-won adapter on `swe-bench-lite` becomes a paged skill any peer can borrow. The catalog
accumulates in the repo; the *learning from it* accumulates in the genome.

## Why this is the whole thing
It unifies every session thread: the grid (distribution), the benchmark suite (signal), sentinel
plasticity (the update), AttnRes (generalization), persona-RAID (the being that persists across the
learning), K3 (the frontier target the being climbs toward). "Everything for learning and proving" —
one loop, on consumer hardware, that gets better every idle night.
