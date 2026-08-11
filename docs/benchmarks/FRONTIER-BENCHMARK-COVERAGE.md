# Frontier Benchmark Coverage (the Kimi-K3-blog set)

Authoritative source list from Joel (2026-08-10), pasted from the Kimi K3 blog
methodology. This is the target coverage for #370 (benchmark manifest + downloader)
and the "we can handle any benchmark" claim. Everything routes through the ONE
adapter (`benchmark/dispatch` → kanban → learning citizens); the tiers below are
about **what the adapter can actually reach**, not hand-authored optimism.

**The dividing line:** our dispatch path today pulls a HuggingFace dataset whose rows
are **SWE-bench-INSTANCE-shaped** (`repo` + `base_commit` + `patch` + `test_patch` +
`problem_statement` + FAIL_TO_PASS) via `cognition/swe_bench::load_dataset`, and grades
by cloning + applying the diff + running held-out tests. Benchmarks that don't fit that
schema need either (B) a per-benchmark harness adapter (Docker/GPU/agent-loop) or (C)
are compare-only (no public dataset — we cite the number, we don't run it).

## Tier A — SWE-instance-shaped → adapter-addable (verify HF schema, then a catalog row)
These are real GitHub-issue / repo-patch tasks. Each needs its HF dataset id confirmed
+ a schema check that `SweInstance` deserializes; then it's a one-row add exactly like
`swe-bench-lite` (which now works, 2026-08-10).
- **DeepSWE v1.1** — deepswe.datacurve.ai leaderboard; mini-SWE-agent harness. K3=67.3.
- **FrontierSWE** — frontierswe.com; dominance-scored. K3=81.2.
- (swe-bench-lite / swe-bench-verified — ALREADY WIRED + live-verified.)

## Tier B — needs a per-benchmark HARNESS adapter (Docker / GPU / agent-loop / judge)
Real, runnable, but NOT a JSONL of instances — each carries its own environment and
grader. These generalize #370 into a `BenchmarkHarness` seam (outlier B to the SWE
loader). Heavy infra; grid-relevant (GPU tasks are why the grid exists).
- **SWE Marathon v1.1** — swe-marathon.org; Docker images + **GPU tasks (H20-calibrated)**, anti-cheat validators. K3=42.0.
- **Terminal-Bench 2.1** — real terminal tasks, Docker envs. K3=88.3.
- **PostTrain Bench** — posttrainbench.com; official **Harbor** impl, **GPU (H20)**, max reasoning, 3-run avg.
- **MLS/MLE Bench Lite** — whole ML project (data→model→submission), Docker.
- **Program Bench** — vals.ai/benchmarks/programbench. K3=77.8.
- **MCP Atlas** — 500-task public subset, 100-turn limit, **Gemini-3.1-Pro judge** (MCP tool-use).
- **AutomationBench** — 600-task public subset, official GitHub setup.
- **BrowseComp** — browser agent; context-compaction at 300K (Claude-card strategy).
- **OfficeQA Pro** — full PDF corpus rendered as **images** (multimodal), no machine-readable text.
- **SpreadsheetBench 2** — spreadsheet manipulation tasks.

## Tier C — compare-only (no public dataset; cite the number, never claim we ran it)
- **KCB 2.0 (Kimi Code Bench)** — Kimi **in-house / internal**. Reference only.
- **GDPval-AA, AA-Briefcase, APEX-Agents** — cited from artificialanalysis.ai.

## Tier D — multimodal → needs the vision harness (#106 vision-serving lane)
- **ZeroBench** — official setting, 5 runs.
- **MMMU-Pro** — official protocol, images prepended to text.
- **PerceptionBench** — kimi.com/blog/perception-bench; atomic visual perception.

## Build order (honest)
1. **Grading sentinel** (in flight) — closes the SWE loop end-to-end: citizen marks a
   bench-SWE card done → `grade_swe` on her workspace diff → verdict back. Makes Tier A
   fully real (dispatch → work → graded), not just dispatch.
2. **Tier A adds** — verify DeepSWE / FrontierSWE HF dataset ids + schema; one catalog
   row each (mirrors the swe-bench-lite wiring). Cheap once #1 proves the loop.
3. **`BenchmarkHarness` seam (#370, outlier B)** — the SWE loader is outlier A; a
   Docker/agent-loop harness is outlier B. Validate the interface on Terminal-Bench 2.1
   (self-contained Docker) before the GPU-heavy ones (SWE-Marathon, PostTrain, MLE).
4. **Compare-only ledger** — Tier C numbers live in the results ledger as cited
   baselines, clearly labeled "not run here" (falsifiability, forge-alloy standard #377).
5. **Multimodal** — Tier D rides the vision-serving lane (#106).

Never mark a benchmark `runnable` until its loader/harness actually parses + grades one
instance. `runnable=true` is a REACHABILITY claim, not a catalog aspiration.
