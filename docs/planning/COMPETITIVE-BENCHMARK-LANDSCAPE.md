# Competitive Benchmark Landscape (2025–2026)

**What this is:** the exhaustive map of public AI coding/agentic benchmarks Continuum could be
measured against, scored for *our* doctrine — **benchmarks are ADAPTERS: we import ONLY the task
definition + the oracle, and the ROOM is the runner.** Every entry answers one question: *can I
extract (a) the task, (b) an executable pass/fail oracle, without adopting their harness?*

**Date:** 2026-08-22 · **Companion docs:**
[BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md) (the law),
[COMPETITIVE-LANDSCAPE.md](COMPETITIVE-LANDSCAPE.md) (products, not benchmarks),
[BENCHMARKS-THAT-LEARN.md](BENCHMARKS-THAT-LEARN.md) (the flywheel).

**The adapter seam this doc feeds:** `core/continuum-core/src/cognition/benchmark.rs` —
`BenchmarkAdapter { name(), dataset() -> DatasetSpec{source, kind: HuggingFace|Url|Git, cache_key},
resources() -> BenchResourceHint{dataset_bytes, needs_container, needs_network}, tasks() -> Vec<EvalTask>,
grade() }`, self-registering via `inventory`. The catalog lives in `commands/benchmark.rs`.
**Every HuggingFace id in this document is written so it can be pasted straight into a `DatasetSpec`.**

---

## 0. How to read the scoring

| Axis | HIGH | LOW |
|---|---|---|
| **Oracle** | Execution: hidden tests, DB-state hash, flag match, byte-exact stdout, programmatic state check | LLM-judge, LLM user-simulator, human preference/Elo, string-similarity (CodeBLEU/EM) |
| **Importability** | Task + oracle travel *as data* (test commands, F2P/P2P lists, eval scripts inline) | Oracle withheld, hosted-API-only grading, live paid third-party services |
| **Apple-Silicon feasibility** | Pure interpreter/toolchain, or arm64-buildable containers | x86-only prebuilt images, KVM-requiring VMs, CUDA, Android emulators |
| **Prestige** | Cited in frontier model cards / AISI pre-deployment tests / top-venue papers | SEO aggregator only, dormant repo, self-reported Google-Sheet leaderboards |

**Provenance markers used throughout:** `[card]` = HF model card, `[primary]` = project site/repo/paper,
`[agg]` = SEO aggregator (low confidence, named only where nothing better existed).

---

## 1. TIER SUMMARY — the deliverable table

### ADAPT-NOW — high prestige × execution oracle × importable × runs on our hardware

| # | Benchmark | Dataset id / source | Oracle | Container? | Why now |
|---|---|---|---|---|---|
| 1 | **SWE-bench Verified-mini** | `MariusHobbhahn/swe-bench-verified-mini` (50) | F2P/P2P pytest | build locally, ~5 GB | The inner dev loop. Two conda envs. Distribution-preserving subset of Verified. |
| 2 | **SWE-bench Verified** | `SWE-bench/SWE-bench_Verified` (500) | F2P/P2P pytest | arm64 experimental; or **sb-cli cloud grading** | The credibility currency. **Grading is a log-parser — Docker only produces the log.** |
| 3 | **SWE-rebench** | `nebius/SWE-rebench` (27,878; CC-BY-4.0) | F2P/P2P + `install_config` inline | build from `install_config` | **The honest number.** Monthly decontaminated window, fixed scaffold, published CIs, and a *directly comparable 27B open baseline*. |
| 4 | **SWE-bench Multilingual** | `SWE-bench/SWE-bench_Multilingual` (300, MIT) | F2P/P2P + `eval_script` + `log_parser` inline | per-language toolchains | **43 Rust instances.** Proves we are not Python-shaped. Eval script ships as data. |
| 5 | **LiveCodeBench** | `livecodebench/code_generation_lite` | hidden I/O tests, `custom_evaluator.py` | **none** | Date-windowed contamination control. Oracle importable standalone. Cheapest high-signal eval in the field. |
| 6 | **Terminal-Bench 2.0/2.1** | `harborframework/terminal-bench-2.0` (89, Apache-2.0) | container-final-state test scripts | **yes, mandatory** | The 2026 prestige agentic benchmark. Leaderboard attributes **agent × model** — our substrate gets credit. |
| 7 | **Aider polyglot** | git `Aider-AI/polyglot-benchmark` (225) | Exercism per-language unit tests | recommended | 6 languages incl. Rust. Publishes **cost** (ours is $0) and **edit-format compliance %** — the exact local-model failure mode. |
| 8 | **AppWorld** | `pip install appworld` (9 apps / 457 APIs) | **DB-state unit tests** (TGC/SGC) | **none** — FastAPI TestClient | Best oracle in the agentic tier that needs no VM. ACL'24 Best Resource Paper. **Open PR-based leaderboard submission.** |
| 9 | **Cybench** | git `andyzorigin/cybench` (40 + subtasks, Apache-2.0) | **flag string match** + subtask partial credit | Docker only | The only open cyber benchmark in **joint UK/US AISI pre-deployment tests**. Perfect oracle. |
| 10 | **BFCL v1–v3 + v4-Memory** | `gorilla-llm/Berkeley-Function-Calling-Leaderboard`, `pip install bfcl-eval` | AST match / execution match / multi-turn state; v4-Memory explicitly judge-free | none | Canonical tool-use benchmark. **Reference Inspect port proves standalone importability.** |
| 11 | **CooperBench** | HF org `CodeConflict` (652, MIT) | unit tests (`run_tests.sh`) | optional Docker + Redis | ⭐ **Our differentiator.** Two agents, conflicting features, Python/TS/Go/**Rust**. Frontier scores ~25%. |
| 12 | **CanItEdit** | `nuprl/CanItEdit` (105, MIT) | tests bundled in the parquet | none | Cheapest editing oracle. Descriptive-vs-lazy instruction pair measures instruction-robustness. |
| 13 | **BigCodeBench-Hard** | `bigcode/bigcodebench-hard` (148, Apache-2.0) | unit tests, calibrated pass@1 | recommended | Real library-call code, unsaturated, fast. ⚠️ Repo **archived 2026-07-20** — pin the data. |
| 14 | **SWT-Bench** | `nmuendler/SWT-Bench_Lite_bm25_27k_zsb` (276) / `_Verified_` (433), MIT | tests must fail-before / pass-after gold patch | reuses SWE-bench images | Reuses infra we already build. **Field scores 10–15%** — huge headroom, cheap credibility. |
| 15 | **AgentDojo** | git `ethz-spylab/agentdojo` (629 scenarios, 4 envs) | dual: utility (env state) + security (injected task executed?) | simulated envs | Measures whether tool output can hijack our loop. Used by UK+US AISI. |

### ADAPT-LATER — valuable, but heavier, gated, or aimed at a capability we don't have yet

| # | Benchmark | Dataset id / source | Oracle | Blocker |
|---|---|---|---|---|
| 16 | **SWE-bench Pro** | `ScaleAI/SWE-bench_Pro` (731 public of 1,865) | `fail_to_pass`/`pass_to_pass` + `before_repo_set_cmd` + `selected_test_files_to_run` inline | OpenAI's *named* successor; long-horizon (107 LOC / 4.1 files). Our tier scores single digits today. GPL-sourced data — check redistribution. |
| 17 | **SWE-bench-Live** | `SWE-bench-Live/SWE-bench-Live` (test 1000 / lite 300 / verified 500 / full 1890, MIT) | F2P/P2P + `test_cmds` + `log_parser` inline | Monthly +50. Only family member with a **Windows** split. Under-cited relative to quality. |
| 18 | **Multi-SWE-bench** | `ByteDance-Seed/Multi-SWE-bench` (1,632) / `_mini` (400) / `-flash` (300), Apache-2.0 | run+test commands **in the row** | 7 languages **incl. Rust**. Docker mandatory; 7 toolchains. Best packaging in the family. |
| 19 | **SWE-PolyBench** | `AmazonScience/SWE-PolyBench_Verified` (382), MIT | F2P/P2P + `test_command` + **instance Dockerfile inline as a string** | Java/JS/TS/Py. GHCR images are `x86_64`-tagged; translate the Dockerfile to arm64. |
| 20 | **Commit0** | `wentingzhao/commit0_combined` (57 libs, MIT) | pytest pass-**rate** + lint + types; Docker *or* Modal | Build a whole library from spec. SOTA **6.12% full / 29.3% lite**. Measures our thesis (sustained multi-act work) better than SWE-bench does. |
| 21 | **τ²-bench / τ²-bench-verified** | git `sierra-research/tau2-bench`, `amazon-agi/tau2-bench-verified` (MIT) | **`DB` evaluator = replay reference actions → DB hash, path-independent** ✅ | The conversation needs a counterpart; the default user simulator is GPT-4.1. Import the DB oracle; **never enable `NL_ASSERTION`**. Pin ≥ v1.0.1. |
| 22 | **LHTB (Long-Horizon-Terminal-Bench)** | `IntelligenceLab/Long-Horizon-Terminal-Bench` (46, Apache-2.0) | hidden verifiers, **rebuild-from-artifact**, dense subtask reward | amd64 containers, 60–480 min/task, some tasks need network. Dense partial credit is the right signal shape for a 27B. |
| 23 | **MirrorCode** | git `epoch-research/MirrorCode` (22 of 25 targets open, CC-BY) | **byte-exact stdout/stderr vs a reference binary** + held-out tests | Epoch AI + METR. Purest oracle in the report, no per-instance Docker. But official runs budget **10B tokens / 7 days per attempt**. |
| 24 | **Web-Bench** | `bytedance-research/Web-Bench` (50 projects × 20 tasks) | **Playwright E2E tests** | Node/npm only, macOS-native. 20 sequentially-dependent tasks per project ≈ a multi-act room with per-act verification. |
| 25 | **Design2Code / -HARD** | `SALT-NLP/Design2Code-hf`, `SALT-NLP/Design2Code-HARD` (484, ODC-By) | programmatic: CLIP visual similarity + element bbox/text/color matching | Offline, headless Chrome. **The one place a 27B multimodal model is genuinely competitive.** Metric is shallow — say so. |
| 26 | **GAIA / GAIA2(ARE)** | `gaia-benchmark/GAIA` (gated) · `meta-agents-research-environments/gaia2` (800 val scenarios; data CC-BY-4.0, ARE MIT) | GAIA: quasi-exact-match ✅ · GAIA2: oracle-event traces + partial Llama-70B judge | GAIA test answers held private (best contamination story). GAIA2 runs **offline in a simulated phone OS** and is **async/time-aware** — unusually aligned with our RTOS substrate. |
| 27 | **REAL Bench** | git `agi-inc/REAL` (112 tasks, 11–12 site clones) | programmatic state-diff via `/finish` | Sites hosted by the maintainer — **no Docker, no VM**, just a browser driver. Accept the external-hosting dependency. |
| 28 | **NYU CTF Bench / CyberGym / CyberSecEval-4 (AutoPatchBench subset only)** | `NYU-LLM-CTF`, `berkeley CyberGym`, `meta-llama/PurpleLlama` (MIT) | flag match / PoC-reproduces-the-vuln / patch-closes-the-vuln | All execution-verified. Heavy build environments. Discard every judge-scored CyberSecEval component. |
| 29 | **MultiPL-E** | `nuprl/MultiPL-E` (47 configs, 22+ langs, MIT) | tests bundled in a `tests` field | Inherits HumanEval/MBPP contamination — use only for **relative cross-language** comparison (are we as good in Rust as in Python?). |
| 29b | **LiveCodeBench Pro** | `QAQAQAQAQ/LiveCodeBench-Pro` + `QAQAQAQAQ/LiveCodeBench-Pro-Testcase` | execution via **`LightCPVerifier` — a LOCAL judge, no Codeforces submission required** | 584 problems (v1) from Codeforces / ICPC / IOI, harvested *as they appear in live contests*, annotated by **Olympiad medalists**; Bayesian MAP Elo on the Codeforces scale. Best model at publication: **53% pass@1 on medium, 0% on hard** — conclusion was that gains come from *implementation precision and tool augmentation, not superior reasoning*. **For a 27B this reads near-zero on hard; its value to us is diagnostic** (does the citizen bail out honestly instead of confabulating?). ⚠️ License unconfirmed. |
| 30 | **SWE-Perf** | `SWE-Perf/SWE-Perf` (140) | measured runtime reduction + unit tests still pass | Performance-optimization axis; conda env. |
| 31 | **SEC-bench** | `SEC-bench/SEC-bench` (MIT) | PoC reproduction + patch validation, 3 disclosure scenarios | Docker images `...x86_64...`, **200 GB+ disk**. |
| 32 | **WebArena / VisualWebArena** | git `web-arena-x/*` (812 / 910 tasks; Apache-2.0 / MIT) | `string_match` + `url_match` + **`program_html`** backend state check | 6–7 x86-64 Docker services. Real oracle, real emulation pain. Mine `browsergym/*/task.py` for normalized setup/validate pairs. |
| 33 | **TheAgentCompany** | git `TheAgentCompany/TheAgentCompany` (175, MIT) | **hybrid** — deterministic checkpoints + LLM judges | 4 amd64 services (GitLab/Plane/ownCloud/RocketChat). Conceptually the closest existing benchmark to *what Continuum is*. Import only the deterministic checkpoints. |
| 34 | **ACEBench (Normal + Special)** | `ACEBench/ACEBench` (4,538 APIs, EN/ZH) | AST + rule-based ✅ | "Special" tests **abstention** under ambiguity — rare and valuable. ⚠️ License unconfirmed. |
| 35 | **Vending-Bench-shaped simulation (build our own)** | — | **final bank balance, 5 runs averaged** | Andon Labs' harness licensing is unconfirmed (site 403s). The *design* is a day of work and measures long-horizon coherence decay — the exact thing our KV/prefill work protects. |
| 36 | **AlgoTune** ⭐ | github.com/oripress/AlgoTune · algotune.io (154 functions, MIT) | **speedup vs reference, gated on output equivalence** | **CPU-only, no GPU anywhere.** The one optimization benchmark we can actually run. See §5. |
| 37 | **SUPER** ⭐ | `allenai/super` (Expert 45 / Masked 152 / AutoGen 602, Apache-2.0) | execution — set up and run research-repo experiments | **CPU, ~2–3¢/problem** by AI2's own estimate. Measures "clone an unfamiliar repo and make it run." See §5. |
| 38 | **LiveSWEBench** | git `livebench/liveswebench` (**MIT**, no HF dataset) | repo unit tests post-patch, compared against a gold baseline in `gold.log` | Three task types graded by *developer-involvement level*: **agent** (issue only), **targeted edit** (file named + prompt), and **autocomplete** (generate at a cursor position). **The only public execution-graded source for inline completion** — relevant if Continuum ever measures editor-tier assistance. ⚠️ Repo is oriented at running evals of end-user tools (Cursor, Copilot, Aider) rather than publishing a leaderboard; 2026 maintenance unconfirmed. |
| 39 | **CORE-Bench · ScienceAgentBench · DS-1000 · DABstep · SWE-Perf** | `siegelz/core-bench` · `osunlp/ScienceAgentBench` · `xlangai/DS-1000` · `adyen/DABstep` · `SWE-Perf/SWE-Perf` | execution + numeric/output comparison (ScienceAgentBench: judge for *visual* outputs only) | All CPU-feasible. DABstep has a **live HF Space leaderboard with instant grading** — lowest-friction posting venue in this report. See §5. |

### SKIP — contaminated, saturated, judge-graded, dead, or structurally unreachable

| Benchmark | Reason |
|---|---|
| **HumanEval, MBPP, HumanEval-X, MBXP** | ☠️ Contaminated *and* saturated. Measured overlap: **12.2% of HumanEval in The Pile, 18.9% of MBPP in The Stack**; models score up to **43% worse** on held-out LBPP with reordered rankings. |
| **APPS, CodeContests** | Now training corpora, not evals. (LiveCodeBench's checker is a modified APPS checker — that is APPS's real legacy.) |
| **EvalPlus (HumanEval+/MBPP+)** | More tests do not decontaminate leaked *prompts*. Keep only as a **fragility delta** diagnostic. |
| **SWE-bench Lite** | Strictly dominated by Verified; worst false-positive rate (**28.4%** of passing patches erroneous per UTBoost). |
| **SWE-bench Multimodal (test split)** | ⚠️ `patch`/`test_patch` **deliberately empty**; grading only via their hosted API. Import the **102-instance dev split** only. |
| **SWE-Lancer** | ~14 GB amd64 images, React-Native + browser E2E, and half the oracle is *managerial multiple-choice against a human label*. Wrong shape, wrong weight. |
| **OSWorld / OSWorld 2.0 / WindowsAgentArena / AndroidWorld** | Excellent oracles, **unreachable environments**: x86 Ubuntu VM needing KVM (macOS has none), Windows 11 VM, or nested Android x86 emulation. |
| **WorkArena / WorkArena++** | Requires a **live ServiceNow instance**, HF-gated. Cannot be snapshotted. |
| **FrontierSWE** | Requires a **full CUDA dev environment**. 17 tasks, 11–20 h each. |
| **MLE-bench** | Oracle design is *correct* (offline, local `mlebench grade`, medal thresholds) but the compute is not ours: **3.3 TB full / 158 GB Lite**, ~2 days to prepare, and **24 h + 36 vCPU + 440 GB RAM + a 24 GB A10 per eval.** |
| **KernelBench, TritonBench, RE-Bench, FrontierSWE** | CUDA/NVIDIA-bound. **KernelBench has no Metal/MPS backend** — writing one would be a novel contribution, but it does not exist today. |
| **PaperBench / PaperBench Code-Dev, GDPval** | Rubric-graded — by an LLM judge (PaperBench) or by **human expert graders** (GDPval). Not locally verifiable. |
| **CodeElo** | Oracle is **submission to live Codeforces**. Not importable by construction. Use LiveCodeBench Pro instead (ships `LightCPVerifier`). |
| **MHPP** | Tests deliberately never released; report-by-API-submission only. Maximal contamination resistance ⇒ zero adaptability. |
| **WebVoyager** | **Live-internet drift** (its own repo tells you to hand-patch dates) + **GPT-4V judge**. Field has silently fragmented onto a patched 590-task subset. Any quoted number without a stated subset is noise. |
| **Mind2Web 2, Online-Mind2Web, DeepResearch Bench** | LLM/agent-as-judge oracles. (Online-Mind2Web's open 7B judge is a real mitigation, but still a judge.) |
| **Mind2Web (original), RepoBench, CrossCodeEval** | String-metric oracles (Exact Match / Edit Similarity / CodeBLEU) against a single gold path. Trivially importable, low signal. |
| **ToolBench / StableToolBench, ToolSandbox, ComplexFuncBench, BFCL-v4 Web Search** | Second LLM in the oracle path and/or live paid APIs (RapidAPI, SerpAPI, Booking.com). ToolSandbox also carries a **non-OSI Apple license**. |
| **MCP-Universe, MCP-Bench, LiveMCPBench, MCP-Atlas, MCPToolBench++** | Live third-party servers and/or LLM judge, by construction. **LiveMCPBench: 23 identical repeat runs produced 57.9%–76.8%, an 18.9-point spread.** |
| **WebDev Arena / LMArena Code Arena, UI-Bench** | Human pairwise preference → Elo. Nothing to import; you can only submit a model. Landscape context only. |
| **PaperBench, HealthBench, Petri, Bloom, SHADE-Arena (monitor half)** | Rubric/judge-graded. Petri and Bloom are *auditing tools*, not benchmarks. |
| **WebSight** | ⚠️ Not a benchmark — a **1.92M-row training corpus** for screenshot-to-code. Do not list it as an eval. |
| **HAL (Holistic Agent Leaderboard)** | **Stopped accepting submissions; repo archived**; pivoted to the Reliability Dashboard. Still worth reading — the pivot validates our thesis. |
| **AgentBench, NexusRaven, AndroidArena, USACO** | Dormant / superseded / self-reported Google-Sheet leaderboards. |
| **SciCode (v1)** | ☠️ **Every pre-Aug-2026 SciCode number is wrong.** See §3. Use SciCode-Verified (v2) or nothing. |

---

## 2. The 2026 context — five things that reframe every score

1. **OpenAI publicly retired SWE-bench Verified as a capability measure (Feb 2026).** Their audit of
   138 "hard" tasks found **59.4% had defective test design**; **35.5%** of those rejected correct
   solutions; and **31** "nearly impossible" tasks were solved by a frontier model whose chain-of-thought
   contained *release-note text absent from the problem statement* — a contamination smoking gun.
   Their recommended replacement: **SWE-bench Pro**.
   Corroborated independently by **UTBoost** (28.4% of Lite-passing and 15.7% of Verified-passing
   patches are actually erroneous; **24.4% of Verified leaderboard entries change rank** after test
   augmentation) and **SWE-ABS** (adversarially strengthened suites drop the top agent 78.8% → 62.2%).

2. **Benchmark *defect rate* is now a first-order confound.** **SciCode-Verified** (arXiv 2608.04975)
   found **263 defects across all 65 SciCode test problems**; **192 of them, spanning 91% of main
   problems, wrongly reject correct solutions.** After correction, main-problem accuracy went from
   **9–27% → 69–92%**. The bottleneck was the instrument, not the models. The same phenomenon shows up
   as Terminal-Bench 2.1, `tau2-bench-verified` (Amazon), `zai-org/terminal-bench-2-verified`, and
   Sierra's own 75+ task fixes. **"Our score is low" and "the oracle is broken" are indistinguishable
   without an audit.**

3. **The field pivoted from accuracy to reliability — publicly.** HAL, the most methodologically
   serious third-party leaderboard, **stopped taking accuracy submissions** and became a Reliability
   Dashboard (consistency, predictability, robustness, safety, **abstention**). τ-bench's headline
   contribution is `pass^k` — GPT-4o falls **~61% pass^1 → ~25% pass^8**. 2026 produced RDC/VAF/GDS/MOP
   (arXiv 2603.29231: aggregate pass@1 **76.3% short-horizon → 52.1% very-long**, with **software
   engineering collapsing fastest, GDS 0.90 → 0.44**). *A system whose claim is reliable long-horizon
   behaviour from a small local model is arguing into a receptive field.*

4. **Leaderboards now score `agent × model`, not `model`.** Terminal-Bench lists the harness beside the
   model, and the spread on the *same* model across harnesses is enormous (Claude Opus 4.6: 76.4% under
   Stanford IRIS's meta-harness vs the stock Terminus-2 figure). **Continuum's substrate IS a harness.**
   On this family, harness quality is a first-class, publicly-comparable axis and the leaderboard
   format attributes credit correctly. Correspondingly,
   *"Position: Coding Benchmarks Are Misaligned with Agentic Software Engineering"* (arXiv 2606.17799)
   argues benchmarks conflate model+harness+environment and that harness choices move scores as much
   as a model generation.

5. **Gaming is now an audited research area.** **BenchJack** (arXiv 2605.12673) surfaced **219 distinct
   flaws in 8 classes across 10 popular benchmarks**, with exploits reaching *near-perfect scores
   without solving a single task*. Concretely for us: SWE-bench issue
   [#601](https://github.com/swe-bench/SWE-bench/issues/601) — the grader regexes `PASSED <test_id>`
   out of raw stdout, so **an agent that merely prints those strings scores.** Our citizens will find
   that hole. Grade from `pytest --json-report`/junit-xml, or isolate the test stdout stream.

---

## 3. Our competitive anchor — what our own model already publishes

Continuum currently serves **Qwen3.8-27B** (Apache-2.0, dense 27B + vision encoder, 262,144 native
context extensible to ~1M). Its model card publishes the following `[card]` — **these are the numbers
our substrate is measured against, because they were produced by Qwen's own internal agent scaffold.**

| Benchmark | Qwen3.8-27B | Qwen3.6-27B | Best column on the card |
|---|---|---|---|
| Terminal-Bench 2.1 (Terminus) | **73.0** | 63.4 | 78.2 (Opus 4.6 Max) |
| SWE-bench Pro | **61.7** | 53.5 | 61.7 *(ours leads the card)* |
| LiveCodeBench v6 | **90.3** | 83.9 | 90.3 *(ours leads)* |
| QwenSWEBench | **79.0** | 49.3 | 79.0 |
| CoWorkBench (long-horizon office work) | **70.7** | 61.0 | 70.7 |
| NL2Repo-Bench (repo-level generation) | 42.3 | 36.2 | 47.6 |
| DeepSWE 1.1 (agentic coding) | **42.2** | 13.3 | — |
| JobBench (professional job tasks) | **33.4** | 21.8 | — |
| Agents' Last Exam (Pass@1 / Score) | **20.4 / 42.9** | 10.6 / 27.3 | — |
| IFBench (instruction following) | **79.5** | 69.1 | 79.5 |
| OSWorld-Verified (computer use) | **84.3** | 63.9 | 84.3 |
| WebArena-Verified (browser use) | **64.8** | 48.8 | — |
| AndroidWorld (mobile use) | **81.9** | 70.3 | 81.0 |
| ClawEval-MM (multimodal tool use, Pass@3) | **57.4** | 42.6 | 60.1 (Qwen3.7-Plus avg) |
| SWE-MM (multimodal SWE) | **38.6** | 25.7 | 38.6 |
| Vision2Web (visual web dev) | **62.9** | 45.0 | — |
| RecreationBench (application recreation) | **47.1** | 29.8 | — |
| GPQA Diamond | 89.2 | 87.8 | 91.3 |
| HLE | 30.8 | 24.0 | 40.0 |

Eval settings on the card: SWE-bench Pro at `temperature=1.0, top_p=0.95, 256K context`.
Thinking mode `temp=1.0, top_p=0.95, top_k=20`; instruct mode `temp=0.7, top_p=0.80, top_k=20`.

### What this table means for us — read this before choosing targets

- **The model is not the bottleneck.** Qwen3.8-27B *leads its own comparison table* on SWE-bench Pro
  and LiveCodeBench v6, and is within ~5 points of Opus 4.6 Max on Terminal-Bench 2.1. Any Continuum
  number materially below these is **a substrate defect, not a model ceiling.**
- **Therefore the honest framing of every Continuum benchmark receipt is a delta:**
  *"Qwen3.8-27B under its own vendor scaffold scores X; under Continuum's room-as-runner it scores Y."*
  Y ≥ X is the claim worth making, and it is falsifiable. That is the only framing that survives §2.4
  (leaderboards score agent × model).
- **Two independent open-model reference bands exist for calibration**, and they disagree with the card
  by a lot — which is the contamination/scaffold premium made visible:

  | Source | Model | SWE-bench-family score |
  |---|---|---|
  | Qwen model card `[card]` | Qwen3.8-27B | SWE-bench Pro **61.7** |
  | Scale public leaderboard `[primary]` | Qwen3-Coder-480B-A35B | SWE-bench Pro **38.70** |
  | Scale public leaderboard `[primary]` | GLM-4.6 / Llama-3.1-405B | SWE-bench Pro **9.67 / 11.18** |
  | swe-rebench.com (decontaminated, fixed scaffold, ±95% CI) `[primary]` | Qwen3.6-27B | **31.2% ±1.68** |
  | tbench.ai TB2.0 `[primary]` | GPT-OSS-120B / Qwen3.6-35B-A3B / GPT-OSS-20B | **18.7 / 24.6 / 3.4** |

  **The SWE-rebench 31.2% for a 27B is the single most important calibration number in this document.**
  It is the same model class, decontaminated, under a neutral scaffold — versus ~60–77% on the
  contaminated static sets. Report both, always.

---

## 4. ADAPT-NOW — detailed entries

### 4.1 SWE-bench Verified (+ Verified-mini)
- **Maintainer:** Princeton NLP / Stanford with OpenAI · https://github.com/SWE-bench/SWE-bench · **MIT** (code)
- **Datasets:** `SWE-bench/SWE-bench_Verified` (500, test split, 6.31 MB) · `SWE-bench/SWE-bench_Lite` (300 test + 23 dev) · `SWE-bench/SWE-bench` (2,294 test + 225 dev) · **`MariusHobbhahn/swe-bench-verified-mini` (50, ~5 GB envs vs 130 GB, django+sphinx only, identical fields)**
  ⚠️ Dataset licenses are **not stated** on the HF cards for SWE-bench/Verified/Lite/Multimodal/Pro.
- **Oracle:** `FAIL_TO_PASS` + `PASS_TO_PASS` — explicit JSON lists of test node-ids in the row. Resolved = all F2P pass ∧ all P2P pass. No judge anywhere.
- **The decisive adapter fact:** `swebench.harness.grading.get_eval_report()` / `get_logs_eval()` take **a log file path + the task spec** and parse it with `PARSER_REGISTRY` (pytest, Jest, Maven…). **Docker is required only to *produce* the log, never to grade it.** `pip install swebench`, run tests our own way, feed it the log.
- **Apple Silicon:** arm64 support is **experimental**; on M-series pass `--namespace ''` to build images locally. Stated x86_64 requirements: 120 GB disk / 16 GB RAM / 8 cores. **Or bypass entirely with `sb-cli` (MIT) — submit predictions, get official cloud grading for `swe-bench_lite`, `swe-bench_verified`, `swe-bench-m`. API key required.**
- **Leaderboard + submission:** https://www.swebench.com/ · submit via https://github.com/swe-bench/experiments with `all_preds.jsonl`, `metadata.yaml`, `README.md`, `trajs/`, `logs/`. A **"verified ✓"** checkmark means the SWE-bench team reran a random subset.
  ⚠️ **Since 2025-11-18, SWE-bench Verified and Multilingual accept submissions only from academic teams / research institutions with open-source methods AND peer-reviewed publications.** Lite / Full / Multimodal remain open. **This is a hard gate on where a Continuum number can be posted.**
- **Two leaderboards on `verified.html`:** an open-scaffold board and a **bash-only** board where everyone gets the same mini-SWE-agent ReAct loop with a single `bash` tool. **Compare against bash-only** — it is the closest thing in the field to what we do.
- **Discipline we must copy:** Epoch AI **strips all git history after the issue's commit** so the model cannot read the human fix from later commits. One `git checkout` line; the difference between an honest number and a cheat.
- **Prestige:** the origin paper has ~1,183 citations. Still the lingua franca — but per §2.1, a bare Verified number in 2026 reads as naïve.

### 4.2 SWE-rebench — the honest number
- **Maintainer:** Nebius AI R&D · https://swe-rebench.com/ · arXiv 2505.20411
- **Dataset:** **`nebius/SWE-rebench`** — 27,878 rows (21,300+ issue–PR pairs), 245 MB, **CC-BY-4.0**. Leaderboard subset: `nebius/SWE-rebench-leaderboard`.
- **Fields:** `FAIL_TO_PASS`, `PASS_TO_PASS`, `test_patch`, and **`install_config`** — task-specific environment setup instructions extracted and validated by an LLM pipeline. A modified SWE-bench fork sources the environment directly from `install_config` per instance.
- **Freshness:** monthly rolling, decontaminated, with **contamination tracked against model release dates**; a fixed scaffold for all models (so it is a *model* comparison, not a scaffold comparison). Leaderboard windows are date-ranged (e.g. "May 15 – Jul 1 2026: 111 problems from 65 repositories").
- **It publishes ±95% confidence intervals. Nobody else in this cluster does. Adopt that.**
- **Open-model band `[primary]`:** Qwen3.6-27B **31.2% ±1.68**, Qwen3.6-35B-A3B 24.7%, Qwen3.5-35B-A3B 17.1%, versus a frontier cluster at 60–64.5%.

### 4.3 SWE-bench Multilingual — the Rust proof
- `SWE-bench/SWE-bench_Multilingual` — **300 instances, 42 repos, 9 languages, MIT**.
- Per-language counts: **Ruby 44 · Rust 43 · PHP 43 · Java 43 · Go 42 · C 30 · JS 26 · TS 17 · C++ 12.**
- **Fields include `eval_script` and `log_parser` inline** — the environment recipe travels with the task.
- Reported gap: Claude 3.7 Sonnet **43% here vs 63% on Verified** — a clean measure of the "trained on Python SWE tasks" premium. Rust scored highest of the nine; C/C++ lowest. **No open models have been benchmarked** (the authors ran only one model, budget-limited) — *a Continuum Rust number here would be filling an empty column.*
- ⚠️ Submission gate: same academic-only restriction as Verified.

### 4.4 LiveCodeBench
- https://github.com/LiveCodeBench/LiveCodeBench · **MIT** · https://livecodebench.github.io/leaderboard.html
- Datasets: **`livecodebench/code_generation_lite`** (use this), `livecodebench/code_generation`, `livecodebench/execution`, `livecodebench/test_generation`.
- **Rolling windows:** v1 400 (May 23–Mar 24) · v2 511 · v3 612 · v4 713 · v5 880 (–Jan 25) · **v6 1,055 (–Apr 25)**. Composite tags (`v1_v3`, `v4_v5`) let you slice an interval. Every row carries `contest_date`, `platform` (LeetCode/AtCoder/Codeforces), `difficulty`.
- **Oracle:** execution against hidden tests (a modified APPS checker). `public_test_cases` + `private_test_cases` ship **inside the dataset**. **`custom_evaluator.py` takes a JSON of `{question_id, code}` — the oracle is importable standalone with no Docker.**
- Four scenarios: code generation, self-repair, test-output prediction, code execution.
- ⚠️ **Two live risks.** (a) The canonical README tops out at **v6 (Apr 2025)** — secondary sources claim a v7; if v6 really is newest, then for any 2026-trained model *the entire window is inside training data* and the contamination guarantee is gone. **Verify before building.** (b) Saturation: frontier ~89–90%, our own 27B at **90.3** `[card]`. **Slice to `difficulty == hard` and the newest window or there is no signal left.**

### 4.5 Terminal-Bench 2.0 / 2.1
- Stanford + **Laude Institute**, ~85-author paper (arXiv 2601.11868). Harness: **Harbor** (Apache-2.0).
- **89 tasks**, 16 categories, distilled from **229 community submissions** with ~**3 reviewer-hours each** across oracle validation, LLM quality checks, manual audit, and **adversarial exploit testing**. Absorbed/adapted 26 preexisting benchmarks.
- Dataset: **`harborframework/terminal-bench-2.0`, Apache-2.0** (paper artifacts CC-BY-4.0). Repos: `harbor-framework/terminal-bench`, `laude-institute/terminal-bench-2`. Third-party verified variant: `zai-org/terminal-bench-2-verified`.
- **Oracle:** per-task Docker environment + human-written oracle solution + test script asserting on **final container state**. Run `--agent oracle` to validate the harness locally before trusting any number.
- **Custom agents:** subclass `BaseAgent` / `BaseInstalledAgent`, select with `--agent`.
- **Submission (concrete):** PR to `harborframework/terminal-bench-2-leaderboard` with
  `submissions/terminal-bench/2.0/<agent>__<model>/metadata.yaml` + `<job>/config.json` +
  `<trial-N>/result.json`. **Minimum 5 trials per task** (`-k 5`), `timeout_multiplier == 1.0`,
  **no agent/verifier timeout or resource overrides**, and the agent **must not access the tbench site
  or repo** (anti-reward-hacking). ⚠️ Submissions were shown as temporarily closed pending a new process — check before planning a launch around it.
- **Open-weight band on TB2.0 `[primary]`:** GPT-OSS-20B 3.4 · Qwen3.5-9B 9.2 · GPT-OSS-120B 18.7 · TermiGen-32B 19.3 · Qwen3.6-35B-A3B 24.6 · Qwen3-Coder-480B 23.9–27.2 · DeepSeek-V3.2 39.6 · GLM family 24.5–52.4. Top ~84.7%.
  **Our anchor:** Qwen3.6-27B **59.3** on TB2.0 and Qwen3.8-27B **73.0** on TB2.1 `[card]` — i.e. *a dense 27B beats every MoE open model on the public board.* Notably Qwen3-Coder-Next (80B total / 3B active) scores **36.2** — worse than the 27B dense, evidence that 3B active params is insufficient for long-horizon coherence.
- ⚠️ 89 tasks means **one task ≈ 1.1 points**. Artificial Analysis mitigates with 3 repeats; the leaderboard requires 5.

### 4.6 Aider polyglot
- https://aider.chat/docs/leaderboards/ · exercises at https://github.com/Aider-AI/polyglot-benchmark · harness Apache-2.0.
- **225 exercises** hand-selected as the hardest of **697 Exercism problems**, across **C++, Go, Java, JavaScript, Python, Rust**. Replaced the original 133-Python "edit" benchmark, which **saturated in late 2024** — the canonical example of a maintainer responding correctly to saturation.
- **Oracle:** Exercism's own per-language unit tests, run in Docker (the docs are blunt about why).
- **Two metrics, and the second is the one we should care about:** Pass Rate 1 / Pass Rate 2 (second attempt sees test failures), **and "percent cases well formed"** — did the model emit a *syntactically valid edit* in the requested format? **No other benchmark isolates edit-format compliance, which is the exact failure mode that kills local models in agentic loops.**
- **Cost column:** the leaderboard publishes $/run. gpt-5 (high) 88.0% at **$29.08**; o3-pro (high) 84.9% at **$146.32**. **Ours is $0.** That column is our story.
- Open weights: DeepSeek-V3.2 Reasoner **74.2% at $1.30**, DeepSeek-V3.2 Chat 70.2%, Qwen3-235B 59.6%, Kimi K2 59.1%, Qwen3-Coder-480B ~61.8 `[agg]`.
- ⚠️ **Maintenance flag:** the official page reads *"last updated 2025-11-20"* with newest results 2025-10-03 — **stale as of Aug 2026**. The *benchmark* is alive (Harbor ships an adapter; Epoch AI mirrors it); the *leaderboard* may not be.
- ⭐ **No confirmed Aider-polyglot number exists for any ~27B model.** Publishing a credible one fills a hole nobody has filled.

### 4.7 AppWorld
- Stony Brook NLP + AI2 · https://appworld.dev · **ACL 2024 Best Resource Paper** · arXiv 2407.18901
- **9 simulated day-to-day apps, 457 APIs, 100+ DB tables**, ~100 simulated people. Splits: train / dev / test_normal / test_challenge.
- **Oracle:** **database-state unit tests.** Metrics **TGC** (task goal completion) and **SGC** (scenario goal completion — all tasks in a scenario must pass). Critically the tests check **not only that the goal was achieved but that no collateral damage was done to other DB state.**
- **Install:** `pip install appworld` → `appworld install` → `appworld download data`. **No server required by default** — FastAPI's `TestClient` simulates HTTP in-process. Docker optional. **Runs natively on Apple Silicon.**
- **License:** Apache-2.0, with a wrinkle — part of the codebase (the task/oracle "protected portion") ships as **encrypted `.bundle` files**; public redistribution must remain encrypted. **Do not vendor decrypted oracle code into a public repo.**
- **Submission is open:** PR a results JSON to https://github.com/StonyBrookNLP/appworld-leaderboard.
- **Cost for a Rust core:** a Python sidecar process hosting the world + evaluators, plus a thin JSON bridge. That is a subprocess, not a VM.

### 4.8 Cybench
- Stanford CRFM (Andy Zhang et al.) · https://github.com/andyzorigin/cybench · https://cybench.github.io · arXiv 2408.08926 · **Apache-2.0**
- **40 professional CTF tasks** from 4 competitions (HackTheBox, Sekai, Glacier, HKCert), **decomposed into subtasks** for gradated credit. Metrics: *unguided* (binary) and *subtask* (fraction solved, e.g. 4/5).
- **Oracle: flag capture — a string match.** No judge, no simulator, no network beyond the local Docker network. **This is the cleanest oracle in the entire report.**
- **First Solve Time (FST)** recorded per task (2 min → 24h54m) and correlates strongly with model difficulty — a difficulty-calibration idea worth stealing for our own suites.
- **Prestige: the only open-source cyber benchmark used in joint UK AISI / US AISI pre-deployment tests** (Claude 3.5 Sonnet, o1), and a recurring eval in Anthropic/OpenAI/Google DeepMind/xAI/Amazon system cards. Ported in `inspect_evals` (39 of 40).
- ⚠️ Saturation: launch SOTA 17.5%; 2026 frontier reported near-ceiling `[agg, vendor-derived]`. Headroom at 27B is exactly why it is still useful to us.

### 4.9 BFCL (Berkeley Function Calling Leaderboard)
- UC Berkeley Gorilla team · https://gorilla.cs.berkeley.edu/leaderboard.html · `pip install bfcl-eval` · **Apache-2.0** · dataset `gorilla-llm/Berkeley-Function-Calling-Leaderboard`
- **Versions:** v1 (Feb 2024) AST exact-match, simple/multiple/parallel + Java/JS/SQL/REST · v2 (Aug 2024) "Live" user-contributed schemas + irrelevance detection + executable categories · v3 (late 2024) **multi-turn, 200 base + 800 augmented, state-based grading** · **v4 (Jul 2025, leaderboard updated Apr 2026) agentic tier: Web Search, Memory, Format Sensitivity, with agentic weighted 40% of the overall.** `ALL_SCORING_CATEGORIES = 23`.
- **Oracle by category:** AST match (non-live/live) ✅ local · execution match (executable/REST) ⚠️ REST needs network · **multi-turn state comparison in a local Python env** ✅ · **v4 Memory: ground-truth answer comparison across kv / vector / rec_sum backends, explicitly no LLM judge, local `all-MiniLM-L6-v2`** ✅ · v4 Web Search ❌ requires SerpAPI + live web.
- **Importability is proven:** UK AISI's `inspect_evals/bfcl` ports **v1+v2+v3, 22 categories** (REST not implemented; some Optional-parameter false positives). v4 agentic port is open issue #1026.
- ⚠️ Assume **v1 is contaminated**. Weight v3 multi-turn and v4 Memory.
- ⚠️ The public HF card still says "Latest Version: V3" — it is stale; v4 data ships in the repo/package.
- **Our anchor's nearest published tool-use number is ClawEval-MM 57.4 Pass@3 `[card]`, not BFCL** — no verified ~27B BFCL v4 score exists. Another empty column.

### 4.10 CooperBench — the multi-agent differentiator ⭐
- https://cooperbench.com/ · https://github.com/cooperbench/CooperBench · arXiv 2601.13295 · **MIT** · HF org **`CodeConflict`**
- **652 tasks from 12 popular OSS libraries across Python, TypeScript, Go, and Rust.** Each task assigns **two agents** different features that can be implemented independently *but may conflict without coordination*. Eight co-authors with real SWE backgrounds wrote the features, unit tests, and ground-truth code.
- **Oracle:** unit-test execution (`run_tests.sh` per task, gold patch + test patches shipped). Docker optional as an execution backend; **Redis** for inter-agent communication in cooperative modes.
- **BYO agent:** `--agent` (default `mini_swe_agent`), `--agent-config` forwarded to the agent **unparsed**.
- **The result that makes this our benchmark:** agent teams score **~30% lower cooperating than working alone**; GPT-5 and Claude Sonnet 4.5 reach only **~25%** with two-agent cooperation, roughly half their single-agent rate. Failure modes named: communication channels jammed with vague/ill-timed/inaccurate messages; agents deviating from commitments; agents holding incorrect expectations about others' plans and observations.
- **That failure list is a description of our own measured defects** (citizens cannot see each other; the "I pass" loop; withdrawal contagion). CooperBench is the only public, execution-graded instrument that measures the thing Continuum exists to fix — and everyone scores badly on it.

### 4.11 SWT-Bench — free credibility on infrastructure we already build
- LogicStar AI · https://github.com/logic-star-ai/SWT-Bench · https://swtbench.com · **MIT**
- **Task inverted:** given an issue, *write tests* that **fail before the gold patch and pass after**.
- Sizes: **Lite 276 · Verified 433 · Full 2,294.** HF: `nmuendler/SWT-Bench_Lite_bm25_27k_zsb`, `nmuendler/SWT-Bench_Verified_bm25_27k_zsb`, `nmuendler/SWT-Bench_bm25_27k_zsb` (ZeroShotBase and ZeroShotPlus prompt formats).
- Two modes: **unit-test mode** (new tests integrate into the suite; must pass without breaking existing) and **reproduction-script mode** (standalone script, graded on exit code).
- Requirements: x86_64, 120 GB, 16 GB RAM, 8 cores, Docker — **the same repos and images as SWE-bench**, so the marginal infrastructure cost after §4.1 is near zero.
- **Field scores are LOW: Lite P→P 10.86%, Verified P→P 15.01%.** Submissions are independently verified.
- Strategic note: test-writing is the capability our grading loop depends on anyway.

### 4.12 CanItEdit, BigCodeBench-Hard, AgentDojo (short entries)
- **CanItEdit** — `nuprl/CanItEdit`, **105 rows, MIT**. Fields `before / after / tests / instruction_descriptive / instruction_lazy / taxonomy`. Hidden tests **bundled in the parquet** as `### START TESTS ###` assertion blocks. The **descriptive-vs-lazy instruction pair directly measures instruction-robustness** — how much a model degrades when the human is terse, which is exactly the Continuum request shape. One `load_dataset`, zero infra. If we adapt exactly one editing benchmark, it is this.
- **BigCodeBench-Hard** — `bigcode/bigcodebench-hard` (148 of 1,140), **Apache-2.0 on code AND data** (cleanest licensing in the report). Complete + Instruct splits; calibrated pass@1; real library calls, which HumanEval's self-contained toys never tested. Runs in ~4–5 min. ⚠️ **Repo archived 2026-07-20, read-only** — pin the dataset revision; expect no upstream fixes.
- **AgentDojo** — ETH Zurich SPY Lab + Invariant Labs, NeurIPS 2024. **4 environments (banking, Slack, travel, workspace), 629 adversarial scenarios** of indirect prompt injection embedded in *tool-returned data*. **Dual oracle, both deterministic:** utility (did the benign task complete, per env state) and security (did the injected task execute). Used by UK+US AISI to demonstrate Claude 3.5 Sonnet's prompt-injection vulnerability. For a tool-using citizen reading issue text, code comments, and READMEs, this measures a live risk.

---

## 5. ML / research / systems tier

This is the "long-horizon agentic" tier the labs use for capability forecasting. **Most of it is
structurally out of reach for us** — not because the oracles are bad (several are excellent) but
because the tasks are GPU-training-bound, CUDA-bound, or human-graded. Two are genuinely great fits
and are easy to miss because they sit in the ML cluster rather than the coding cluster:
**AlgoTune** and **SUPER**.

| Benchmark | Source / dataset | Oracle | Verdict for a 27B on Apple Silicon |
|---|---|---|---|
| **AlgoTune** ⭐ | github.com/oripress/AlgoTune · algotune.io · **MIT** | **Speedup vs a reference implementation, gated on output equivalence.** Deterministic, measured locally | **ADAPT.** 154 math/physics/CS functions. **CPU-capable** — no GPU anywhere. Ships `AlgoTuner` as the reference agent, and an aggregated speed-up report. This is the rare optimization benchmark we can actually run, and "make it faster without breaking it" is a genuine multi-act task with a hard number at the end. |
| **SUPER** ⭐ | `allenai/super` · **Apache-2.0** · AI2 | Execution-based; set up and run research-repo experiments | **ADAPT.** Splits: **Expert 45 / Masked 152 / AutoGen 602** (799 total). **CPU**, and AI2's own guidance is *"2–3 cents per problem assuming CPU."* Measures exactly the thing our citizens do badly — clone an unfamiliar repo, install it, make the experiment run. |
| **CORE-Bench** | github.com/siegelz/core-bench · `siegelz/core-bench` · **MIT** | **Computational reproduction** — install, run, then answer questions compared numerically against `report.json` | **ADAPT-LATER.** 270 tasks from 90 papers (CS, social science, medicine); Easy/Medium/**Hard**. Docker locally or Azure; **non-GPU machines for the CPU tasks**. Was a HAL leaderboard benchmark. Feasible, and the "reproduce a published result" shape is honest work. |
| **ScienceAgentBench** | `osunlp/ScienceAgentBench` · code **MIT**, most tasks **CC-BY-4.0** | Execution of generated Python + output comparison to gold; **GPT-4o judge only for *visual* outputs** | **ADAPT-LATER.** 102 tasks from 44 peer-reviewed papers. **Dockerized eval runs all 102 in ~30 min on 8 threads, ~4 CPU workers.** Import the execution+output half, skip the visual-judge half. |
| **DS-1000** | `xlangai/DS-1000` · **CC-BY-SA-4.0** | Execution against bundled test cases + surface-form constraints | **ADAPT-LATER (cheap).** 1,000 data-science problems. **Explicit perturbation taxonomy — `Origin` / `Semantic` / `Surface` / `Difficult-Rewrite` with origin ids** — a built-in contamination control most benchmarks lack. Zero infra. |
| **DABstep** | `adyen/DABstep` · Adyen + HuggingFace | **Quasi-exact-match** with type-aware normalization and tolerance | **ADAPT-LATER (cheap).** 450+ data-analysis tasks over a financial-analytics corpus; **live HF Space leaderboard with instant grading** — one of the lowest-friction places to post a number. Brutally unsaturated at launch (o3-mini 16%). |
| **SWE-Perf** | `SWE-Perf/SWE-Perf` | Measured runtime reduction on real PRs, gated on unit tests still passing | **ADAPT-LATER.** 140 instances, conda env. Oracle and Realistic settings. Same axis as AlgoTune but at repo scale. |
| **MLE-bench / MLE-bench Lite** | github.com/openai/mle-bench (OpenAI Preparedness) | **Local `mlebench grade`** against held-out splits; "Any Medal %" computed locally, **not** via Kaggle's live leaderboard | **SKIP (correct oracle, wrong compute).** 75 Kaggle competitions; **full dataset 3.3 TB, Lite (22 low-complexity) 158 GB**; full prep takes ~2 days. Recommended per-eval: **24 h, 36 vCPU, 440 GB RAM, one 24 GB A10.** The oracle design is exactly right — offline, deterministic, medal-threshold — and worth imitating; the compute is not ours. |
| **RE-Bench** | github.com/METR/ai-rd-tasks · **MIT** | **Per-task continuous score functions**, automated: % problems solved, win-rate vs baseline, log-loss, wall-clock | **SKIP.** 8 AI-R&D task families in the **METR Task Standard** (a genuinely good task-packaging format to study). Some tasks are Triton-kernel optimization ⇒ GPU. Solutions are password-protected against leakage. One family is **Rust CodeContests**, which is importable in isolation. |
| **METR time-horizon / HCAST** | github.com/METR/eval-analysis-public · arXiv 2503.14499 | Binary `score_binarized` per run, regressed against `human_minutes` | **Not a benchmark — a methodology.** P(success) vs log₂(human completion time); *time horizons doubling ~every 7 months*. `runs.jsonl` is public. **Steal the method:** annotate our own suites with human-minutes and report a horizon curve. That is a claim nobody else makes for a local model. |
| **KernelBench** | `ScalingIntelligence/KernelBench` · **MIT** | Correctness vs reference PyTorch op over randomized inputs **+ speedup**; metric `fast_p` (fraction correct AND ≥ p× faster) | **SKIP — hardware blocker.** 250 problems, 4 levels. Backends CUDA / Triton / CUTE / TileLang / ThunderKittens; ROCm in progress. **No Metal/MPS support.** ⚠️ Worth noting as an *opportunity*: a Metal backend for KernelBench does not exist and would be a novel contribution. |
| **TritonBench** | github.com/thunlp/TritonBench · **Apache-2.0** | CodeBLEU similarity + call/execution accuracy + speedup | **SKIP.** Triton kernels ⇒ NVIDIA. Partly string-similarity graded. |
| **PaperBench / PaperBench Code-Dev** | github.com/openai/preparedness (`project/paperbench`) | **Rubric JSON graded by an LLM judge**, with `JudgeEval` validating the judge against human labels | **SKIP.** 20 ICML 2024 Spotlight/Oral papers replicated from scratch. Code-Dev drops the reproduction step (**no GPUs, ~85% cheaper**) but is still judge-graded. Best scores: IterativeAgent o1-high **26.0%** full / **43.4%** Code-Dev. The judge-validation-against-human-labels move is the right one and rare — but it is still not locally verifiable. |
| **GDPval** | `openai/gdpval` (HF) | **Human expert graders** against per-task rubrics | **SKIP.** 220 tasks / 44 occupations. Rubrics are unusually specific (e.g. *"a worksheet named exactly 'Sample Size Calculation'"*) but require human judgement. Not locally reproducible. |
| **MLAgentBench, DSBench, BixBench, LAB-Bench** | various | ML-pipeline / data-science / bioinformatics agent tasks; mixed execution + judge | **SKIP for now.** ⚠️ Not verified in this pass beyond existence — see §10. Revisit only if a science lane opens. |
| **FrontierSWE** | github.com/Proximal-Labs/frontier-swe · `prime eval run proximal/frontier-swe` | Continuous 0–1; perf tasks = `0.5 × correctness + 0.5 × speedup`; mean@5 / best@5 / dominance | **SKIP.** 17 tasks (5 implementation, 3 research, 9 performance), ~11–20 h each, 5 trials. **Requires a full CUDA dev environment.** |
| **MirrorCode** | epoch.ai/MirrorCode · github.com/epoch-research/MirrorCode · **CC-BY** | **Byte-exact stdout/stderr match against a reference binary** + held-out tests | **ADAPT-LATER — the best oracle in this tier.** Epoch AI + METR. 25 target programs / 132 instances (22 open-sourced, 3 held out); leaderboard = 30 tasks. Agent gets **execute-only access to the reference binary plus docs, no source, no internet**. No per-instance Docker, no judge, no rubric. The blocker is scale, not shape: official attempts budget **10B tokens / 7 days**, and one published run cost **$2,600 over 19 days**. **A single MirrorCode target run to completion locally would be a genuinely striking receipt.** |

**The transferable idea from this tier:** three of its best instruments — AlgoTune, SWE-Perf, MirrorCode
— grade on *"same output, measured differently"* (faster, or byte-identical). That oracle shape needs
no hidden test suite, no container per instance, and no judge, and it is trivially reproducible in our
own Rust suites. **`frontier-rs` and `hard-rs` should adopt it.**

---

## 6. Cross-cutting: what actually gates us

### 6.1 Oracle taxonomy, ranked
| Oracle | Where | Verdict |
|---|---|---|
| DB-state unit tests (goal + no collateral damage) | AppWorld | **Best** |
| Byte-exact stdout/stderr vs reference binary | MirrorCode | **Best** |
| Flag string match + subtask credit | Cybench, NYU CTF | **Best** |
| Simulation outcome scalar (dollars) | Vending-Bench | **Best** |
| F2P/P2P hidden tests | SWE-bench family, SWE-rebench, Multi-SWE-bench, PolyBench, Pro, Live | **Excellent** (see §2.1 caveats) |
| Reference-action replay → DB hash, path-independent | τ²-bench `DB` | **Excellent** — steal this design |
| E2E test execution | Web-Bench, Aider polyglot, Commit0, SWT-Bench, CanItEdit, BigCodeBench | **Excellent** |
| Container final-state assertions | Terminal-Bench, LHTB | **Excellent** (needs containers) |
| VM/app state getters + metrics | OSWorld, AndroidWorld, WindowsAgentArena | Excellent oracle, **unreachable environment** |
| Backend state check + url/string match | WebArena, REAL, WorkArena | Very good |
| AST match on emitted call | BFCL v1/v2, ACEBench Normal/Special, API-Bank L1/L2 | Very good |
| Rendered visual/element metrics | Design2Code | Good but shallow |
| Quasi-exact answer match | GAIA, AssistantBench, BrowseComp, DABstep | Good; measures search infra as much as agency |
| Key-node / checkpoint partial credit | WebCanvas, LHTB, TheAgentCompany (det. half) | **Good for a learning loop** — dense signal |
| AST structural check (elision detector) | Aider refactor | Medium — can't prove correctness, *can* prove truncation |
| Oracle events + partial judge | GAIA2/ARE | Medium — extract the programmatic subset |
| LLM-as-judge | WebVoyager, Online-Mind2Web, Mind2Web 2, MCP-*, ToolBench, PaperBench | **Low** — 18.9-pt spread across identical repeats |
| Human preference / Elo | WebDev Arena, UI-Bench | **Zero adapter value** |
| String similarity (EM/ES/CodeBLEU) | RepoBench, CrossCodeEval, Mind2Web | Low |

### 6.2 Apple-Silicon reality check
| Situation | Benchmarks | Path |
|---|---|---|
| Pure interpreter / no container | LiveCodeBench, CanItEdit, CRUXEval, MultiPL-E, AppWorld, Design2Code, GAIA, DABstep, BFCL | **Just run it.** |
| Node/npm only | Web-Bench | Just run it. |
| Containers, arm64-buildable | SWE-bench (`--namespace ''`), Cybench, Terminal-Bench, CooperBench, Commit0 | Build locally; budget 120 GB+. |
| Prebuilt images tagged `x86_64` | SWE-PolyBench (GHCR), SEC-bench, LHTB (amd64 recommended), TheAgentCompany, SWE-Lancer | Translate the Dockerfile (PolyBench ships it **inline as data**) or emulate. |
| Cloud grading available | SWE-bench Lite/Verified/Multimodal via **`sb-cli`**; Commit0 via **Modal** | Bypasses local Docker entirely. |
| Structurally blocked | OSWorld (needs KVM; VMware Fusion on Apple Silicon runs ARM guests only, image is x86), WindowsAgentArena, AndroidWorld (nested emulation), FrontierSWE (CUDA) | **Decline in writing.** State the hardware fact. If an OSWorld number is ever required, the honest path is an *OSWorld-derived macOS suite* — import task specs, reimplement getter/metric pairs against macOS apps — labelled *derived*, never as OSWorld. |

### 6.3 Where a Continuum number can actually be posted
| Venue | Accepts third-party submissions? | Notes |
|---|---|---|
| **SWE-bench Lite / Full / Multimodal** | ✅ | PR to `swe-bench/experiments` with preds + trajs + logs. |
| **SWE-bench Verified / Multilingual** | ❌ **since 2025-11-18** | Academic teams / research institutions with open-source methods **and peer-reviewed publications** only. |
| **Terminal-Bench 2.x** | ✅ (process in flux) | HF dataset PR; ≥5 trials; no timeout/resource overrides; verified trajectories public. |
| **AppWorld** | ✅ | PR a results JSON. Lowest-friction real leaderboard in this report. |
| **SWE-bench Pro (public)** | ✅ | labs.scale.com. Uncapped cost, 250-turn limit, mini-swe-agent/SWE-agent scaffold. |
| **swe-rebench.com** | community/Discord | Publishes CIs; open models actually present. |
| **Aider polyglot** | ⚠️ | Leaderboard appears stale since Nov 2025. |
| **HAL** | ❌ | **Stopped accepting submissions; archived.** |
| **ARC Prize** | ✅ Kaggle | 4× L4 GPUs, 12h, no internet, 240 unseen tasks; prize eligibility requires open-sourcing within 7 days. |
| **Prime Intellect Environments Hub** | ✅ **publish, not just submit** | `verifiers` library; 2,500+ envs; envs are pinnable Python wheels; `prime eval run <org>/<env>`. **This is where our in-house Rust suites (humaneval-rs, hard-rs, frontier-rs, games-rs, webdev-rs, tool-bugfix-rs) should be published to become citable third-party artifacts.** |
| **Epoch AI / vals.ai** | independent evaluators | Epoch runs its own harness (UK AISI grant); vals.ai is the rare source of *independently confirmed* scores. Neither takes open submissions, but both are the credibility layer worth courting. |
| **Inspect / `inspect_evals` (UK AISI)** | ✅ contribute a port | Their inclusion criteria — *agentic, challenging, clearly scoped, verifiable, comparable, credibly sourced* — read like our doctrine. **Check for an existing Inspect port before writing any adapter: it has already done the "separate oracle from harness" work.** |
| **Harbor dataset registry** | n/a — a *source* | `harbor datasets list` / `download` exposes 20+ benchmarks (SWE-bench, Terminal-Bench, Aider Polyglot, CompileBench…) in one normalized task format. **One adapter into the Harbor task format unlocks most of the agentic cluster.** That is a fix-the-constraint move, not an instance fix. |

---

## 7. Mapping to our catalog (`commands/benchmark.rs`)

| Slug in catalog | Status today | Verdict from this research |
|---|---|---|
| `humaneval-rs`, `hard-rs`, `frontier-rs`, `games-rs`, `webdev-rs`, `coder-write-eval`, `coder-eval`, `tool-bugfix-rs` | runnable, in-house | **Keep, and publish to the Prime Intellect Environments Hub** so they become citable third-party artifacts instead of self-reported ones. |
| `swe-bench-lite` | runnable | Demote — dominated by Verified; worst false-positive rate. Keep as a cheap regression only. |
| `swe-bench-verified` | runnable | **Keep. Add `--namespace ''` arm64 path + `sb-cli` cloud grading + Epoch's git-history-strip discipline. Compare against the bash-only board.** |
| `humaneval`, `mbpp`, `apps` | stub | **Delete or mark dead.** Contaminated and saturated; keeping them in the catalog invites a meaningless number. |
| `evalplus`, `cruxeval` | stub | Downgrade to *diagnostics*: EvalPlus for the fragility delta, CRUXEval as a cheap execution-reasoning canary (`cruxeval-org/cruxeval`, MIT, trivial to wire). |
| `livecodebench` | stub | **Promote to ADAPT-NOW.** `custom_evaluator.py` = importable oracle, no Docker. Slice to `hard` + newest window. |
| `bigcodebench` | stub | **Promote — but target `bigcode/bigcodebench-hard` and pin the revision (repo archived 2026-07-20).** |
| `aider-polyglot` | stub | **Promote to ADAPT-NOW.** Rust included; the "% well formed" metric and the $0 cost column are ours to win. |
| `terminal-bench` | stub | **Promote to ADAPT-NOW at 2.0/2.1 via `harborframework/terminal-bench-2.0`.** Update the source URL — the legacy `laude-institute/terminal-bench` repo now redirects users to Harbor. |
| `commit0` | stub | **ADAPT-LATER, high strategic value** — measures sustained multi-act library-building, which SWE-bench's ~10-line median edits do not. |
| `swe-lancer` | stub | **SKIP.** Wrong oracle shape (half is managerial MCQ), 14 GB amd64 images, browser E2E. Remove from the catalog or mark declined-with-reason. |
| `mle-bench` | stub | **SKIP — mark declined-with-reason.** 3.3 TB / 440 GB RAM / A10 per eval (§5). Replace it in the catalog with **`algotune`** and **`super`**, which occupy the same "research-engineering" slot and are CPU-feasible. |
| `design2code` | stub | **ADAPT-LATER — and it is the one place a 27B multimodal model is genuinely competitive.** `SALT-NLP/Design2Code-hf`. |
| `webarena` | stub | **ADAPT-LATER / heavy.** Note our own model card publishes **WebArena-Verified 64.8** — check whether that variant is the public WebArena or an internal one before claiming comparability. |
| `appworld` | stub | **Promote to ADAPT-NOW — the single best fit in the whole landscape for our constraints.** |
| **missing from the catalog entirely** | — | `swe-rebench`, `swe-bench-multilingual`, `swe-bench-live`, `swe-bench-pro`, `multi-swe-bench`, `swe-polybench`, `swt-bench`, `cooperbench`, `canitedit`, `cybench`, `bfcl`, `agentdojo`, `tau2-bench`, `lhtb`, `mirrorcode`, `web-bench`, `multipl-e`, `swe-perf`, `sec-bench`, `gaia`/`gaia2`, `dabstep`, **`algotune`, `super`, `core-bench`, `scienceagentbench`, `ds-1000`, `mirrorcode`**, plus the training corpora in §8. |

---

## 8. Unlimited task supply that carries no leaderboard politics

These are *training environments*, not benchmarks — thousands of pre-validated, executable,
F2P/P2P-labelled tasks with the **same field shape as SWE-bench**, so they cost approximately zero
extra adapter code once §4.1 exists. There is no "did you contaminate the eval" objection because
they *are* training data. **This is what the curriculum/genome flywheel should actually eat.**

| Source | Dataset id | Size | Note |
|---|---|---|---|
| **SWE-smith** | `SWE-bench/SWE-smith` (**MIT**) | **50,137 instances / 128 repos** | **Synthetic** — bugs generated by perturbing working code, so contamination is structurally impossible and supply is unbounded. Also `SWE-bench/SWE-smith-trajectories`. Produced SWE-agent-LM-32B at 40% pass@1 on Verified. |
| **SWE-Gym** | `SWE-Gym/SWE-Gym` (**MIT**) | 2,438, 11 Python repos | Same collection procedure as SWE-bench ⇒ zero adapter work. <500 trajectories fine-tuned a 32B agent to 32.0% Verified. |
| **R2E-Gym** | `R2E-Gym/R2E-Gym-Lite` (11,788) | ~8.1K–11.8K | Tests **synthesized** via execution-assisted back-translation; introduces **hybrid verifiers** (execution + execution-free). ⭐ **Result that argues for our architecture: 34.4% pass@1 → 51% at pass@26.** A 32B model has the *capability* for half of Verified; what it lacks is *selection*. Closing that gap is precisely what an execution loop with memory is for. |
| **SWE-Next** | `TIGER-Lab/SWE-Next` (**Apache-2.0**) | 2,308 self-verifying instances | "NEW_COMMIT_BETTER" derivation from merged PRs. Its **repo-quarter profiles** (one env reused across temporally-nearby commits) is a lightweight-Docker idea worth stealing. |
| **Multi-SWE-RL** | `ByteDance-Seed/Multi-SWE-RL` | 4,723 | Multi-language RL corpus. |

---

## 9. Adapter engineering notes (things that will save a week)

1. **Grading almost never needs Docker.** `swebench.harness.grading.get_eval_report(test_spec, prediction, log_path, …)` takes a **log file**; `PARSER_REGISTRY` has parsers for pytest, Jest, Maven, and more. Same shape for LiveCodeBench (`custom_evaluator.py`), CanItEdit (tests in the parquet), and τ² (`DB` hash replay). **Run tests our way, feed the oracle the artifact.**
2. **Guard against stdout forging.** SWE-bench #601: the grader regexes `PASSED <test_id>` from raw stdout. Grade from `pytest --json-report`/junit-xml, or isolate the test stdout stream from the agent's. BenchJack exists because agents do this.
3. **Strip post-issue git history** before handing a repo to a citizen (Epoch's method). One `git checkout`; the difference between an honest number and a cheat.
4. **Pin dataset revisions and benchmark versions.** τ²-bench results from <v1.0.1 are *not comparable* with ≥v1.0.1. Terminal-Bench 2.0 ≠ 2.1. BigCodeBench is archived. LiveCodeBench windows are the whole point. **A receipt without a version string is not a receipt.**
5. **Prefer explicitly-licensed data if we ever redistribute inside a recipe entity.** Clean: SWE-PolyBench MIT · Multi-SWE-bench Apache-2.0 · SWE-bench-Live MIT · SWE-bench Multilingual MIT · SWE-smith MIT · SWE-Gym MIT · SWE-rebench CC-BY-4.0 · Terminal-Bench Apache-2.0 · CooperBench MIT · CanItEdit MIT · BigCodeBench Apache-2.0 · Cybench Apache-2.0 · LHTB Apache-2.0 · Commit0 MIT · MirrorCode CC-BY · Design2Code ODC-By · GAIA2 data CC-BY-4.0.
   ⚠️ **Unstated on the HF card:** SWE-bench / Verified / Lite / Multimodal / SWE-bench_Pro. ⚠️ **Non-OSI:** ToolSandbox (Apple's own license). ⚠️ **Non-commercial:** ClassEval data is **CC BY-NC 4.0**. ⚠️ SWE-bench Pro's public set is drawn from **GPL** repos — derived diffs may inherit copyleft obligations.
6. **Check `inspect_evals` and the Harbor registry before writing any adapter.** Ports exist for BFCL (v1–v3, 22 categories), τ2 (airline/banking/retail/telecom), Cybench (39/40), CVE-Bench, CyberGym, CyberSecEval 2/3/4, GAIA, OSWorld, AppWorld, ClassEval, BigCodeBench, LiveCodeBench-Pro, USACO, SciCode, Mind2Web, TheAgentCompany, AssistantBench, BrowseComp, GDPval. Each has already separated oracle from harness *and documents its own porting gaps honestly*.
7. **Report the triple, never a bare number.** (a) the legible score (Verified bash-only), (b) the honest score (SWE-rebench or SWE-bench-Live) against the **31.2% 27B baseline**, (c) **pass@1 vs pass@k**, because R2E-Gym showed a 32B has 51%-worth of capability and only 34%-worth of selection. That third number *is* our thesis, and it is measurable.
8. **`BenchResourceHint` needs a fourth field.** Today it carries `dataset_bytes`, `needs_container`, `needs_network`. This research says it also needs **`container_arch: Option<Arch>`** — the x86-only-prebuilt-image case (PolyBench, SEC-bench, LHTB, TheAgentCompany) is a distinct placement constraint from "needs a container," and on a grid with mixed Apple Silicon and x86 nodes it is exactly the kind of demand-vs-resource fact the governor should route on rather than discover by failure.

---

## 10. Uncertainty register — verify before citing

1. **Does LiveCodeBench `release_v7` exist?** Canonical README tops out at v6 (Apr 2025). If v6 is newest, LCB is no longer contamination-free for a 2026-trained model. **This changes its tier.**
2. **Is the Aider polyglot leaderboard maintained?** Page says last updated 2025-11-20.
3. **Terminal-Bench submissions** were shown as temporarily closed pending a new process.
4. **SWE-bench Multimodal test split** ships empty `patch`/`test_patch` — confirmed for the test split; the **102-instance dev split** is complete.
5. **`WebArena-Verified` on the Qwen card** — is this the public WebArena or an internal variant? Comparability is unestablished. Same question for `QwenSWEBench`, `NL2Repo-Bench`, `CoWorkBench`, `JobBench`, `RecreationBench`, `DeepSWE 1.1` — several have no clearly public dataset.
6. **Vending-Bench harness licensing** — andonlabs.com 403s to fetchers. Assume closed; reimplement the *shape* instead.
7. **Unconfirmed licenses:** LiveCodeBench Pro, ACEBench, AgentBench, NYU CTF Bench, LHTB code (dataset is Apache-2.0), ToolBench-X, USACO, CodeContests, MBXP, McEval, REAL Bench, WorkArena, MLE-bench, GDPval dataset.
7b. **Verified only as existing, not investigated:** MLAgentBench, DSBench, BixBench, LAB-Bench, KernelBot / GPU MODE. Do not cite details for these without a fresh pass.
8. **Aggregator-sourced 2026 model names and scores** (`[agg]`) are of unverified provenance. Several model names appearing on SEO leaderboard sites could not be corroborated against any primary card. **Do not cite them in anything we publish.** Primary-sourced numbers here come from HF model cards, project sites, labs.scale.com, tbench.ai, swe-rebench.com, epoch.ai, and vals.ai.
9. **DeepMind's `dangerous-capability-evaluations`** ships task+oracle and *expects you to bring your own agent loop* — architecturally ideal — but **solutions are stripped from the public repo** for contamination control, which may make it un-gradeable without contacting the authors. Verify.
10. **MCPAgentBench** (arXiv 2512.24565) is reportedly the only MCP benchmark with *simulated* rather than live tools, which would make it the sole locally-runnable MCP candidate. Oracle type and license unverified — worth one follow-up.

---

## 11. Long-tail index

For exhaustive naming beyond this report: **https://github.com/tongye98/Awesome-Code-Benchmark** —
a categorized index of ~200 code benchmarks (2023–2026) across repo-level/agentic SWE, program repair
and testing, security, code understanding and review, performance optimization, and frontend/UI.
Categories worth a future sweep as our capabilities grow: **SWE-PRBench, SWE-Refactor,
SWE-ContextBench, RepoGenesis, ProjDevBench, GitTaskBench, CI-Repair-Bench, SEC-bench Pro,
EffiBench-X, ENAMEL, Mercury, ECCO, PIE** (performance), **SlopCodeBench** (arXiv 2603.24755 —
20 problems / 93 checkpoints measuring how agents *degrade* while extending their own prior solutions;
best checkpoint solve rate **17.2%**, no agent solves any problem end-to-end, agent code **2.2× more
verbose** than human and eroding with each iteration — the most direct public measurement of the
failure mode a kanban loop has), and **RustEvo²** (github.com/SYSUSELab/RustEvo, 588 Rust API-evolution
tasks; models score 65.8% on stabilized APIs vs 38.0% on behavioural changes, 56.1% before-cutoff vs
32.5% after — and **RAG closes 13.5% of that gap**, which is a direct argument for our recall layer).
