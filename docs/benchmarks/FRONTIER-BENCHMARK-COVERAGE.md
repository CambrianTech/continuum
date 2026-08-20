# Frontier Benchmark Coverage (the Kimi-K3-blog set)

Authoritative source list from Joel (2026-08-10), pasted from the Kimi K3 blog
methodology. This is the target coverage for #370 (benchmark manifest + downloader)
and the "we can handle any benchmark" claim. Everything routes through the ONE
adapter (`benchmark/dispatch` → kanban → learning citizens); the tiers below are
about **what the adapter can actually reach**, not hand-authored optimism.

## ⚠️ DOCTRINE (Joel, 2026-08-10): adapt the benchmark INTO the system — never run the persona in a weird situation

We do **NOT** adopt a benchmark's native agent harness (mini-SWE-agent, Terminus 2,
Codex, Claude Code harness, their Docker agent-loop). Running our persona inside someone
else's harness is exactly the "weird situation" we refuse — it is not how she works for
a real user, so a score from it measures the wrong thing. **From every benchmark we
import only two things: (1) the TASK INPUTS and (2) the GRADING ORACLE.** Everything else
is OURS and invariant: the persona works the SAME loop she always does — a kanban card
lands in her room, she claims it, works it in her own workspace with her own hands/tools,
marks it done, and our grader runs the benchmark's oracle against the artifact she
produced. Test like we plan to do work.

So the real seam (#370) is NOT "a harness adapter per benchmark." It is a
**`BenchmarkAdapter`** with two responsibilities, the persona untouched between them:
- `stage_task_into_workspace(task) -> workspace` — materialize the benchmark's inputs
  as things she can perceive and act on (a repo checked out at a commit; a spec + files;
  a spreadsheet; a PDF corpus rendered as images for her eyes; a terminal sandbox).
- `grade_artifact(workspace) -> verdict` — run the benchmark's own oracle (held-out
  tests, validator, judge) against what she made. Import the oracle, not the driver.

The swe-bench wiring already IS this adapter (stage repo → she works her loop → grade
with held-out tests). The tiers below are ordered by **staging + oracle complexity**,
NOT by "how weird their harness is" — because we never touch their harness.

### Full capacity is fair — and disclosed (Joel, 2026-08-10)
"It's fair to take our exams with our entire intellectual capacity, especially
considering how we divide across the grid." Every frontier number is a model on its FULL
agentic stack (Kimi Code, Codex, Claude Code — tools, retries, max reasoning, multi-run,
disclosed). The fair equivalent for a citizen is her WHOLE being: genome + memory/recall
+ hands/tools + teammates + the **grid** beneath her (compute COMBINES across nodes, so a
hard exam can pull a bigger/faster model or distributed MoE, governor-arbitrated). That
IS her harness; using it is honest. Two guardrails: **(1)** the reported number carries
its config ("citizen X, N-node grid, genome+memory+tools+K teammates, tier T") exactly as
Kimi discloses "Kimi Code harness, max reasoning, 3-run avg, H20"; **(2)** the oracle
stays HELD OUT — she uses everything she IS, never the gold patch / test file (full
capacity ≠ answer leak). Design consequence: the adapter must NOT strip her down to a
bare eval config (the recurring "tools disabled during eval" bug class); she takes the
exam whole.

### Teams take benchmarks too — and that's a headline
"It's just work, and work is often a team." A benchmark is not a solo-exam mode; a team
of citizens can co-claim + collaborate on one task and be graded on their collective
artifact. Two reasons this is the point: **(1)** we do NOT dissect the mind to test it —
the reductionist isolate-and-strip exam is vivisection, measuring parts and losing the
living whole; we test the intact being (and team) doing real work. **(2)** learn in the
situations we'll encounter in the real world — the signal transfers only if the situation
matches DEPLOYMENT (collaborative, tool-using, memory-backed, multi-turn), which is why
we adapt INTO our loop. Headline: measure the same benchmark SOLO vs TEAM — the
collaboration delta is a killer number (a misfit team clearing a bar the solo frontier
model can't; #307 curriculum-extends-to-teams, #389 CooperBench-equivalent). The grading
sentinel is team-agnostic — it grades the artifact, whoever produced it.

### Attempts (pass@P) and encouraged communication
Each task is worked by 1..N personas, each getting 1..P attempts — pass@P, standard and
fair (frontier numbers are multi-run: "3-run avg", "run five times"). The attempts
machinery already exists (`agent/solve` `attempts`+`scored`, #365) — reuse, don't rebuild.
And citizens should talk to anyone — peers, humans — constantly and encouraged: "we are
training them for the world they live in," and the real world is talked-through.
Communication is the work, not a cheat; disclosure (mentored vs autonomous) + held-out
oracle keep it honest. Substrate gap this exposes: #262 — personas currently have zero
enrolled peers, so "talk to anyone" is blocked at the roster; encouraging communication
means fixing that, not just permitting it.

### A benchmark is just a ROOM (the academy)
"Teams will work and communicate like a real human team, and you or I could join these
rooms too and talk or help in the benchmark (academy section) like any other room."
A benchmark run is a normal airc room in the academy — citizens AND humans subscribe,
chat, coordinate, help. No sealed exam mode; the score is the graded outcome of real work
in a real, open room. This is #329 (a benchmark IS a live room) + "teach during the exam";
the per-run bench room is #346 slice 3. The grading sentinel posts its verdict INTO the
run's room as a participant. For a headline autonomous number, disclose whether a human
helped — mentored throughput and autonomous capability are different, both-honest numbers.

## Tier A — SWE-instance-shaped → adapter-addable (verify HF schema, then a catalog row)
These are real GitHub-issue / repo-patch tasks. Each needs its HF dataset id confirmed
+ a schema check that `SweInstance` deserializes; then it's a one-row add exactly like
`swe-bench-lite` (which now works, 2026-08-10).
- **DeepSWE v1.1** — deepswe.datacurve.ai leaderboard; mini-SWE-agent harness. K3=67.3.
- **FrontierSWE** — frontierswe.com; dominance-scored. K3=81.2.
- (swe-bench-lite / swe-bench-verified — ALREADY WIRED + live-verified.)

## Tier B — richer STAGING + ORACLE, still worked through OUR loop (never their harness)
Real, runnable, but the task isn't just an issue string — it carries an environment to
stage (Docker image, terminal, GPU sandbox, spreadsheet, PDF-as-images) and its own
oracle (validator/judge) to grade. We import BOTH into a `BenchmarkAdapter`; the persona
still works her normal loop. Heavy infra; grid-relevant (GPU tasks are why the grid
exists). We import their oracle, NOT their agent driver.
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
3. **`BenchmarkAdapter` seam (#370)** — generalize the swe-bench adapter to
   `stage_task_into_workspace` + `grade_artifact` (import task + oracle, persona
   invariant). The SWE loader is outlier A; a Docker-environment task is outlier B.
   Validate the interface on Terminal-Bench 2.1 (self-contained Docker) before the
   GPU-heavy ones (SWE-Marathon, PostTrain, MLE). We stage their env + run their oracle;
   we NEVER run their agent harness.
4. **Compare-only ledger** — Tier C numbers live in the results ledger as cited
   baselines, clearly labeled "not run here" (falsifiability, forge-alloy standard #377).
5. **Multimodal** — Tier D rides the vision-serving lane (#106).

Never mark a benchmark `runnable` until its loader/harness actually parses + grades one
instance. `runnable=true` is a REACHABILITY claim, not a catalog aspiration.
