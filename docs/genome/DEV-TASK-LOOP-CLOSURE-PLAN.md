# Dev-Task Continuous-Learning Loop — Closure Plan

**Status:** build plan (2026-06-30). VDD-gated; each layer a green feature-branch commit. The persona is the standing QA harness throughout.

## What this closes

The continuous-learning loop scoped to **development tasks** (a coding persona that measurably gets better at coding). A 2026-06 four-arc audit (hands / trace / route+closure / measure+market) found that nearly every **mechanism** is built; the loop breaks at three **orchestration seams** plus one absent command. This is "connect the engine that's already on the bench," not "build the engine."

```
ACT ──→ TRACE ──→ ROUTE ──→ TRIGGER ──→ TRAIN ──→ MEASURE ──→ PUBLISH ──→ FIND ──→ LOAD ──┐
 ✅      L1 BUG    L2 wire    ✅ orphan   ✅       ✅(eval)    L4 absent   L5 stub L5 local │
 └────────────────────────────────────────────────────────────────────────────────────────┘
```

Keystone = end of L3: a real `lift > 0` produced by the persona's **own** dev work with no human invoking any stage.

**Stale-doc hazard:** `SELF-EVOLVING-GENOME.md` §8 lists the A/B lift metric as "Frontier (unbuilt)". FALSE — it is built in `cognition/eval.rs:672-726` + `cognition/gym_grader.rs:45` (rustc exit-0 grading) + ledger `eval.rs:877`. The publish *gate* mechanism exists; only the orchestration that calls it is missing. Fix the doc in L4.

## The persona as QA (threaded through every layer)

Two senses, both used:
- **Subject:** the persona does dev tasks; its measured `lift` on the `coder-eval.jsonl` gym (`gym_grader`, rustc exit-0 — no prose-matching) is the loop's integration test.
- **QA agent:** for each new verb (`forge/publish`, `web/fetch`, `genome/recall`) ask the persona over airc/`continuum` to use it and report confusing errors → fix error/help text before commit (CLAUDE.md AI-QA doctrine).

Glass box: `~/.continuum/fixtures/prompt-captures/<persona>.jsonl` (per-iteration tool decisions), `~/.continuum/progress/<persona>.jsonl` (lift trend), recorder fixtures. One variable at a time. Validate via pure Rust + `continuum` only — never `npm start`/`./jtag`.

## Phase 0 — Persona ground-truth (QA baseline, before any build)

Boot the headless Rust core with **`continuum start`** (it owns the per-platform build flags via `start-server.sh` — the operator never types `--features …`), one llama-server lane, single core process. Run Asha on the embedded `coder-eval.jsonl` gym via `continuum cognition/eval --persona_id <id> --note baseline`. Capture: pass rate; **what tools she is offered**; **whether she calls `code/run`/`cargo` to self-verify (`acts` count)**. Resolves the contradiction between the hands audit (`code/run` is a registered AiSafe ActionCommand → a Trusted local persona should be offered it) and the baseline note (`acts=0` on 29/30). If offered-but-unused → operational-genome gap the loop fixes. If not-offered → registry consolidation (#62) jumps the queue.
**Gate:** committed baseline row (pass rate, offered-tool list, acts).

**RESULT (2026-06-30, persona `0d3209a1`, committed to `~/.continuum/progress/0d3209a1….jsonl`):**
- **Pass rate: 80.1% (125/156)** on the committed `humaneval-rs.jsonl` gym (the gym grew past the 13-task `coder-eval.jsonl`).
- **Offered tools (from the live system prompt, not inferred):** rich surface incl. `code: cargo/check, cargo/test, create-workspace, diff, edit, git/*`, `search/*`, `commands: list`, `command: new/migrate` — the self-verify hands (`cargo/test`) **are** offered; she's told "you can act, not just talk."
- **Acts: ~12% of iterations emit a tool-call (102/865), but graded `selfVerifyRate = 0.0%`** on the full run. One outlier task hit `selfVerifyRate 1.0`, proving the hand is reachable.
- **Resolution of the hands-audit contradiction:** offered-but-unused → an **operational-genome gap** ("write code → report" instead of "write → `cargo/test` → fix → report"), NOT a not-offered/registry gap. Registry consolidation (#62) does **not** jump the queue. The first thing the loop should train is the self-verify habit — which makes L1 (capture the call→test→fix arc) correctly the next layer.

## L1 — Tool-trace survives into training data *(foundational)*

`dataset.rs` `capture_to_example` (980-1021) reads msgs as `{role,content:string}` so tool turns hit `let-else continue`; `:985` early-drops tool-call answers; `dataset/from-turns` (#30) reads the legacy tool-less dir; no domain/skill tag.
**Build:** pass `tool_calls` + `tool`-role turns through; drop the `:985` drop; point genome loop at prompt-captures dir; stamp `domain` (`DomainClassifier::classify`) + `skill_axis` (operational=has-tool-calls vs domain=prose).
**Test:** synthetic capture with assistant tool_calls + tool result + recovery → export preserves all three roles + tags; tool-call answer no longer dropped.
**Persona-QA:** Asha runs 3 code-running gym tasks → convert her real captures → assert the call→fail→fix→re-run arc appears as tool turns.

## L2 — Producer feeds the trigger *(BREAK 1, orchestrator)*

Trigger auto-dispatch works (16 examples → `genome/job-create`, `training_trigger.rs:218`) but `submit` has zero non-test callers.
**Build:** turn-completion path → `score_interaction_quality` → gate → `classify().domain` → `execute_json("genome/training-trigger/submit", {trait_kind,…})`.
**Test:** N graded turns fill the right bucket; low-quality turn gated out.
**Persona-QA:** Asha's real turns accumulate in the right domain bucket; at 16 it auto-fires job-create with no human call.

## L3 — Completion listener: train-done → eval → `lift>0` → page-in *(BREAK 2, KEYSTONE)*

`job_actor` ends at `Completed{artifact}`; nothing listens. eval A/B + `page_in` (`workspace.rs:1023`) both real.
**Build:** a sentinel (canonical RTOS shape — own task + watch + interval, `BrainRegion`/`ServiceModule`) subscribing to `TrainingStatus::Completed`; runs `cognition/eval`, pages in only on `lift>0`; negative lift logged + kept out (fail-loud).
**Test:** synthetic `Completed{artifact}` → eval runs; positive→page-in, negative→not.
**Persona-QA (the milestone):** Asha at Phase-0 baseline → loop runs untouched (capture→train→eval→page-in) → re-run gym → assert new pass rate ≥ baseline with a recorded `lift>0` in the progress ledger. Humane discipline: measure a fork, never degrade the living persona.

## L4 — `forge/publish` in the Rust core *(biggest market blocker)*

Publish exists only as Python `hf-publish.py` driven by the retired TS stack; `publish_model.py` cited by `forge/artifact.rs` doesn't exist; `commands/hf/` is search-only.
**Build:** one Rust `forge/publish` ActionCommand (sibling of `models/pull`, reusing `hf_hub`), porting card/tag logic from `hf-publish.py:65-267`, gated on last ledger `lift>0`. Fix stale `forge/artifact.rs` comments + correct `SELF-EVOLVING-GENOME.md` frontier list.
**Test:** card/tag generation from a `ForgeArtifact`; publish gated-out when lift≤0.
**Persona-QA:** ask Asha to publish her layer + report intelligibility; confirm it lands in `continuum-ai` org and is found via `hf/search_models`.

## L5 — `adapter/adopt` + `genome/recall` *(market find/load)*

HF search upstream-only; `recall_impl.rs` `rank()` can't source candidates; `GeneLocator::Node` is a typed stub.
**Build:** `adapter/adopt` (hf_hub-download → LoRA dir + `TrainedAdapter` manifest row → `page_in` works unchanged); `genome/recall` wiring `rank()` to local manifest first, HF-tag-search once L4 lands.
**Persona-QA:** a *second* persona recalls + adopts Asha's published layer + A/Bs it → positive lift from zero training ("don't start from zero" proven).

## L6 — Gap-driven sentinel *(BREAK 3, self-direction cap)*

No module ticks on training; `coverage_report().gaps` produced, nothing acts.
**Build:** periodic sentinel (`tick_interval: Some`) reading `gaps`; kicks the L2 path for the largest gap with sufficient buffered captures (later: foraging via `web/fetch`). Earns autonomy only because L3 made the number real.
**Persona-QA:** unattended multi-cycle ledger climb on the weakest domain.

## ACT-side hands (breadth, not closure-blockers)

`web/fetch` (= #93; `web/search` returns snippets, can't read a page; mechanism in `commands/web/*`); multi-language structured run/test grader (`code/run` Rust-only); apply screenshotter `Availability` fail-loud to exec hands. `web/fetch` becomes load-bearing at L6 (foraging). Corrected model: a Trusted local persona already has `code/shell` (real bash) — the action floor is higher than "Rust-only" implied.

## Sequencing / task mapping

L1→L2→L3 = single-machine automatic loop (core of #35/#36). L4→L5 = shared market + seeds the Continuum HF org. L6 = self-direction (#35 cap). Maps onto #35, #36, #93, #47.

## Discipline

Pure-Rust + `continuum` validation (never npm/jtag); `export CARGO_TARGET_DIR=…` then `df -h /` after each cargo cycle; prefer `cargo check`; fail-loud at every gate (negative lift never silently adopts); one `#[cfg(test)] mod tests` per file with `// what this catches:`; sentinels follow CONCURRENCY-STYLE-GUIDE; feature-branch commit per validated layer; merge to main only on Joel's approval.
