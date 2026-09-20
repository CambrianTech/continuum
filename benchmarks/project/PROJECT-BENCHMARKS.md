# Project-tier benchmarks — developing the personas against real apps, websites, projects

`uu benchmark/list` catalogs 21 benchmarks. The function/program tier runs locally
today (humaneval-rs, hard-rs, frontier-rs, coder-eval — rustc compile+run). This doc
is the **whole-app / website / project tier**: the yardsticks the frontier labs
report on, and exactly what we develop the personas against.

The runner is `benchmarks/project/run_project.py` — one flow (fetch → set up a real
workspace → point the persona's HANDS at it via `cognition/eval --workspace_root` →
capture her artifact → grade → append to `RESULTS.jsonl`). Every benchmark is a small
ADAPTER (fetch/setup/prompt/grade); adding one is a plug-in, not a fork.

```
python3 benchmarks/project/run_project.py --list
python3 benchmarks/project/run_project.py --benchmark commit0 --instance <lib-repo-url>
python3 benchmarks/project/run_project.py --benchmark swe-bench-lite --instance astropy__astropy-14365
```

## Status per benchmark

| benchmark | what she builds | adapter | to run for real, needs |
|---|---|---|---|
| **swe-bench-lite / verified** | a patch fixing a real repo bug | ✅ wired (`SweBenchAdapter`) | Docker + official `swebench` scorer (clone→edit→diff works without it) |
| **commit0** | an entire library from spec + tests | ✅ wired (`Commit0Adapter`) | the target lib repo + its pip deps; its own pytest is the grader (local, no Docker) |
| **terminal-bench** | end-to-end terminal tasks | ▫ adapter TODO | their task harness (Docker per task); maps cleanly to code/shell + recovery loop |
| **design2code** | a webpage from a screenshot | ▫ adapter TODO | the screenshot dataset + a headless-browser visual scorer; pairs with Screenshotter (#94) |
| **swe-lancer** | whole paid freelance features | ▫ adapter TODO | Docker + their end-to-end test harness (heavy; grid-tier) |
| **mle-bench** | an end-to-end Kaggle project | ▫ adapter TODO | Docker + GPU + competition data (grid-tier) |
| **webarena / visualwebarena** | operate a real website | ▫ adapter TODO | the self-hosted site stack (Docker compose: shop/forum/gitlab) + a browser tool |
| **appworld** | control 9 apps via APIs | ▫ adapter TODO | their app-server stack + an API tool surface |

## The honest split

- **Local now**: commit0 (pytest), swe-bench clone→edit→diff (scoring needs Docker).
  These we develop against on this machine today.
- **Grid-tier**: swe-lancer, mle-bench, webarena, appworld need Docker stacks / live
  web servers / GPUs / browser+API tool surfaces. That infra is what the hardware grid
  brings — the adapters are the small remaining glue, declared in `.requires` so the
  runner reports honestly instead of pretending.
- **Our own games tier**: no public "build a playable game" benchmark exists (they
  measure PLAYING). Our Conway / Snake project cards are that tier — a suite we define.

## The loop this enables

Point a persona (or a PAIR, over airc rooms) at a project instance → she works it with
memory + tools + the recovery loop across many turns → the artifact is graded → the
number lands durably in `RESULTS.jsonl` → the README re-renders. Every solved project
+ its traces is LoRA curriculum. Scale models × instances = the definitive-vs-SOTA proof.
