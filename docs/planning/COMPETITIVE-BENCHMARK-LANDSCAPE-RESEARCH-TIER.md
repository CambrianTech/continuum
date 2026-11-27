# Competitive Benchmark Landscape — ML-Research / Kernel / Data-Science Tier
*(Agent-researched 2026-08-22, Opus-level sweep. Companion to the coding-agent tier doc.
Consumer profile: ~27-35B local model on Apple Silicon, no CUDA, own agent loop.
Full uncertainty register preserved at bottom-of-report in the session transcript;
verdicts below are the actionable core.)*

## Master verdicts

### 🟢 ADAPT-NOW (execution-graded, CPU/Metal-feasible, importable task+oracle)
| Benchmark | Source | Why | Expected 27-35B band |
|---|---|---|---|
| **DS-1000** | HF `xlangai/DS-1000` (CC-BY-SA-4.0) | 1,000 tasks, execution+surface-form oracle, seconds/task, no Docker. One process per problem (TF/matplotlib global state). | 35-50% (frontier ~60-75%) |
| **AlgoTune** | github.com/oripress/AlgoTune (MIT-ish) | 154 tasks "beat SciPy" — structurally contamination-proof (no hidden answers). CPU-only. Harmonic-mean of same-machine speedup ratios; warmup + min-of-10 timing. NeurIPS'25, Epoch-mirrored. | ~1.1-1.3× (frontier 2.05×) |
| **SUPER-Masked** | HF `allenai/super` (Apache-2.0) | 152 checkpointed sub-scenarios of "make a real research repo work" — partial credit keeps small models off the floor; ~2-3¢/task CPU by design. | 10-25% (GPT-4o 46.1%) |

### 🟡 SECOND WAVE
- **DABStep** (HF `adyen/DABstep`, CC-BY-4.0): deterministic exact-match financial analytics over CSVs. GATE: verify how much of the hard-split answer key is public.
- **ScienceAgentBench** (HF `osunlp/ScienceAgentBench`, MIT/CC-BY): 102 tasks, use the 2026-04-30 verified release, adapt the NON-visualization partition only (GPT-4o judges figures).
- **CORE-Bench-Hard** (HF `siegelz/core-bench`, MIT/MIT): deterministic numeric report.json match; Hard tier avoids privileged DinD; mirror the Princeton capsule host.
- **Metal-Sci** (github.com/vicgalle/metal-sci-kernels, LICENSE UNVERIFIED = blocking): the ONLY kernel benchmark that runs on Apple Silicon — roofline-anchored `achieved/ceiling` scoring, cross-chip comparable. NO open-weight entry exists; being first is an uncontested differentiator. 12 tasks, single-author preprint — fit, not prestige.
- **LAB-Bench** (HF `futurehouse/lab-bench`): trivial MCQ smoke test; best contamination hygiene (20% private holdout + canary).

### 🔴 SKIP, blocker named
MLE-bench full/Lite (440GB RAM/3.3TB/GPU; salvage `mlebench grade-sample`) · RE-Bench (1-6×H100/env) ·
METR HCAST (suite withheld BY DESIGN — steal the method instead) · PaperBench (LLM-judge + CUDA) ·
GDPval (human+remote-only grader; mine `rubric_json`) · KernelBench/v2/X/TritonBench/FastKernels
(CUDA/Triton do not compile to Metal — cannot even build) · KernelBot/GPU MODE (remote MI300/H100
grading AND non-OSI license restricting AI training) · SWE-Perf (noise/signal 43×; 11/140 gold
patches reproduce — measurement invalid) · EffiBench/Mercury/Enamel/EvalPerf (6.11% signal +
LeetCode contamination) · MLAgentBench/MLGym/MLE-Dojo (GPU training per task) · BixBench
(LLM-judge core, 24-48h runs) · RExBench (12 tasks, 27B≈0.0) · PostTrainBench (10h×H100/task) ·
DSBench (NON-COMMERCIAL license — legal blocker for us) · GSO (frontier <5%, 27B≈0; LLM hack-detector in official score).

## STEAL THESE DESIGNS (regardless of adapters)
1. **METR's 50% time horizon**: logistic fit of success-probability vs log(human task duration), over OUR OWN task set. The field's converged long-horizon metric; the method is free even though their suite is withheld. → fits our verdicts + act ledgers TODAY.
2. **KernelBench `fast_p`**: correctness hard-gate × tunable speedup threshold, swept as a curve. Plus 5-random-input/1e-02 tolerance gates.
3. **Metal-Sci roofline anchoring**: score = achieved / hardware-derived ceiling (per-chip sysctl lookup) → cross-machine comparable; + a NEVER-fed-back held-out size to catch overfit.
4. **AlgoTune timing discipline**: one untimed warmup, min-of-10 via perf_counter_ns — min suppresses contention instead of averaging it in.
5. **SWE-Effi cost-normalized metrics**: tokens+time-normalized scoring is the axis where a $0-marginal-cost local model STRUCTURALLY WINS vs frontier APIs (Claude Code 40.5% @ 0.48M tokens vs Copilot 56% @ 3.9M). Our leaderboard story.
6. **SUPER's checkpointed partial credit** — dense signal for small models.
7. **LAB-Bench contamination pattern**: 20% private holdout + canary string.
8. **AstaBench validation**: AI2's meta-suite independently chose DS-1000 + SUPER-Expert + CORE-Bench-Hard as its Code&Execution category — convergent confirmation of our Tier-1 shortlist. Cost-per-problem is a FIRST-CLASS scored axis there.

## Cross-cutting laws (from the 2026 reliability audits)
- **Never let an absolute wall-clock from someone else's machine enter a score.** Anchor to a hardware roofline or a same-process ratio. (SWE-Perf: 92% of its own gold patches fail to reproduce across 4 machines; std/signal 43×. arXiv 2607.01211.)
- **LLM-judge oracles invert the value proposition for a local model** — filter mixed-verifier benchmarks to their deterministic subset.
- **Model size is not the kernel barrier; grading hardware is** (Meta's 8B KernelLLM beats GPT-4o on KernelBench-Triton).
- **Shadow-evaluation critique (arXiv 2607.27191)**: agents have saturated research ENGINEERING while failing research JUDGMENT — the gap long-horizon evals should measure. Read before designing ours.
- Benchmark-proliferation phase (11+ new entries in 6 months) → anchor on established execution-graded sets; don't chase the new ones.
